@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var<storage, read> sections: array<SectionMeta>;
@group(0) @binding(2) var<storage, read_write> cube_cmds: array<DrawIndexedIndirect>;
@group(0) @binding(3) var<storage, read_write> cube_draws: array<DrawData>;
@group(0) @binding(4) var<storage, read_write> model_cmds: array<DrawIndexedIndirect>;
@group(0) @binding(5) var<storage, read_write> model_draws: array<DrawData>;
// Early cube, early model, late cube, late model.
@group(0) @binding(6) var<storage, read_write> counters: array<atomic<u32>, 4>;
// Per slot: nonzero when the section passed the occlusion test last frame.
@group(0) @binding(7) var<storage, read_write> visibility: array<u32>;
@group(0) @binding(8) var hiz: texture_2d<f32>;
// Bit per slot: reachable through the section visibility graph.
@group(0) @binding(9) var<storage, read> reachable: array<u32>;

const FLAG_OCCLUSION: u32 = 1u;
const FLAG_GRAPH: u32 = 2u;

fn graph_reachable(i: u32) -> bool {
    return (g.params.y & FLAG_GRAPH) == 0u || (reachable[i >> 5u] >> (i & 31u) & 1u) != 0u;
}

// Model quads reach one block below and two above the 16^3 cell grid.
const PAD_LO: f32 = 1.0;
const PAD_HI: f32 = 2.0;

fn in_frustum(lo: vec3<f32>, hi: vec3<f32>) -> bool {
    for (var i = 0; i < 5; i++) {
        let p = g.planes[i];
        let far = select(lo, hi, p.xyz >= vec3<f32>(0.0));
        if dot(p.xyz, far) + p.w < 0.0 {
            return false;
        }
    }
    return true;
}

// Terrain is fully fogged once max(horizontal distance, |dy|) reaches the render-distance fog end.
fn beyond_fog(lo: vec3<f32>, hi: vec3<f32>) -> bool {
    let end = g.fog.w;
    if !(end > 0.0) {
        return false;
    }
    let near = clamp(vec3<f32>(0.0), lo, hi);
    return max(length(near.xz), abs(near.y)) >= end;
}

// Conservative: true unless every depth texel under the box's screen rectangle is nearer than the
// box's nearest point. The pyramid holds the farthest (smallest reverse-Z) depth per texel.
fn hiz_visible(lo: vec3<f32>, hi: vec3<f32>) -> bool {
    var ndc_min = vec2<f32>(1.0e9);
    var ndc_max = vec2<f32>(-1.0e9);
    var z_near = 0.0;
    for (var c = 0u; c < 8u; c++) {
        let p = select(lo, hi, vec3<bool>((c & 1u) != 0u, (c & 2u) != 0u, (c & 4u) != 0u));
        let clip = g.view_proj * vec4<f32>(p, 1.0);
        if clip.w <= g.cull.w {
            return true;
        }
        let ndc = clip.xyz / clip.w;
        ndc_min = min(ndc_min, ndc.xy);
        ndc_max = max(ndc_max, ndc.xy);
        z_near = max(z_near, ndc.z);
    }
    let size = g.cull.xy;
    let px_lo = clamp(vec2<f32>(ndc_min.x * 0.5 + 0.5, 0.5 - ndc_max.y * 0.5) * size, vec2<f32>(0.0), size - 1.0);
    let px_hi = clamp(vec2<f32>(ndc_max.x * 0.5 + 0.5, 0.5 - ndc_min.y * 0.5) * size, vec2<f32>(0.0), size - 1.0);
    let a = vec2<u32>(px_lo);
    let b = vec2<u32>(px_hi);
    let span = max(b.x - a.x, b.y - a.y) + 1u;
    // Level m texels cover 2^(m+1) depth pixels; pick the finest level where the rectangle spans at most 2x2.
    let shift = max(32u - countLeadingZeros(span - 1u), 1u);
    let level = min(shift - 1u, u32(g.cull.z) - 1u);
    let s = level + 1u;
    let t0 = vec2<i32>(a >> vec2<u32>(s));
    let t1 = vec2<i32>(b >> vec2<u32>(s));
    let dims = vec2<i32>(textureDimensions(hiz, level)) - 1;
    let q0 = min(t0, dims);
    let q1 = min(t1, dims);
    let d = min(
        min(textureLoad(hiz, q0, level).x, textureLoad(hiz, vec2<i32>(q1.x, q0.y), level).x),
        min(textureLoad(hiz, vec2<i32>(q0.x, q1.y), level).x, textureLoad(hiz, q1, level).x),
    );
    return z_near >= d;
}

fn emit(s: SectionMeta, lo: vec3<f32>, late: bool) {
    var counts = array<u32, 6>(s.counts_a.x, s.counts_a.y, s.counts_a.z, s.counts_a.w, s.counts_b.x, s.counts_b.y);
    let hi = lo + vec3<f32>(16.0);
    // Camera sits at the origin in this space; a face plane at p is visible from x > p.
    var facing = array<bool, 6>(
        lo.x + 1.0 < 0.0, hi.x - 1.0 > 0.0,
        lo.y + 1.0 < 0.0, hi.y - 1.0 > 0.0,
        lo.z + 1.0 < 0.0, hi.z - 1.0 > 0.0,
    );
    var offset = s.bases.x;
    for (var d = 0u; d < 6u; d++) {
        let n = counts[d];
        if n > 0u && facing[d] {
            var cmd: u32;
            var slot: u32;
            if late {
                let k = atomicAdd(&counters[2], 1u);
                cmd = g.params.z + k;
                slot = atomicLoad(&counters[0]) + k;
            } else {
                slot = atomicAdd(&counters[0], 1u);
                cmd = slot;
            }
            cube_cmds[cmd] = DrawIndexedIndirect(n * 6u, 1u, 0u, 0, slot);
            cube_draws[slot] = DrawData(lo, offset, s.bases.w, 0u, s.counts_b.z, 0u);
        }
        offset += n;
    }

    if s.bases.z > 0u {
        var cmd: u32;
        var slot: u32;
        if late {
            let k = atomicAdd(&counters[3], 1u);
            cmd = g.params.w + k;
            slot = atomicLoad(&counters[1]) + k;
        } else {
            slot = atomicAdd(&counters[1], 1u);
            cmd = slot;
        }
        model_cmds[cmd] = DrawIndexedIndirect(s.bases.z * 6u, 1u, 0u, 0, slot);
        model_draws[slot] = DrawData(lo, s.bases.y, s.bases.w, 0u, s.counts_b.z, 0u);
    }
}

// Draws sections that are in the frustum and were visible last frame (or all of them without
// occlusion culling).
@compute @workgroup_size(64)
fn cull_early(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= g.params.x {
        return;
    }
    let s = sections[i];
    if (s.origin.w & 1) == 0 {
        return;
    }
    if (g.params.y & FLAG_OCCLUSION) != 0u && visibility[i] == 0u {
        return;
    }
    if !graph_reachable(i) {
        return;
    }
    let lo = vec3<f32>(s.origin.xyz - g.cam_int.xyz) - g.cam_frac.xyz;
    if !in_frustum(lo - PAD_LO, lo + (16.0 + PAD_HI)) || beyond_fog(lo - PAD_LO, lo + (16.0 + PAD_HI)) {
        return;
    }
    emit(s, lo, false);
}

// Runs after the early draws filled the depth pyramid: tests every frustum-visible section, draws the
// ones the early pass skipped, and records visibility for the next frame.
@compute @workgroup_size(64)
fn cull_late(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= g.params.x {
        return;
    }
    let s = sections[i];
    if (s.origin.w & 1) == 0 {
        visibility[i] = 0u;
        return;
    }
    let lo = vec3<f32>(s.origin.xyz - g.cam_int.xyz) - g.cam_frac.xyz;
    let box_lo = lo - PAD_LO;
    let box_hi = lo + (16.0 + PAD_HI);
    var visible = graph_reachable(i) && in_frustum(box_lo, box_hi) && !beyond_fog(box_lo, box_hi);
    if visible {
        visible = hiz_visible(box_lo, box_hi);
    }
    let was = visibility[i] != 0u;
    if visible && !was {
        emit(s, lo, true);
    }
    visibility[i] = select(0u, 1u, visible);
}
