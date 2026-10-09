@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var<storage, read> sections: array<SectionMeta>;
@group(0) @binding(2) var<storage, read_write> cube_cmds: array<DrawIndirect>;
@group(0) @binding(3) var<storage, read_write> cube_draws: array<DrawData>;
@group(0) @binding(4) var<storage, read_write> model_cmds: array<DrawIndirect>;
@group(0) @binding(5) var<storage, read_write> model_draws: array<DrawData>;
@group(0) @binding(6) var<storage, read_write> counters: array<atomic<u32>, 2>;

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

// Seam for Hi-Z occlusion: later samples the previous-frame depth pyramid (bind group 1).
fn hiz_visible(lo: vec3<f32>, hi: vec3<f32>) -> bool {
    return true;
}

@compute @workgroup_size(64)
fn cull(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= g.params.x {
        return;
    }
    let s = sections[i];
    if (s.origin.w & 1) == 0 {
        return;
    }
    let lo = vec3<f32>(s.origin.xyz - g.cam_int.xyz) - g.cam_frac.xyz;
    let hi = lo + vec3<f32>(16.0);
    if !in_frustum(lo, hi) || !hiz_visible(lo, hi) {
        return;
    }

    var counts = array<u32, 6>(s.counts_a.x, s.counts_a.y, s.counts_a.z, s.counts_a.w, s.counts_b.x, s.counts_b.y);
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
            let slot = atomicAdd(&counters[0], 1u);
            cube_cmds[slot] = DrawIndirect(n * 6u, 1u, 0u, slot);
            cube_draws[slot] = DrawData(lo, offset);
        }
        offset += n;
    }

    if s.bases.z > 0u {
        let slot = atomicAdd(&counters[1], 1u);
        model_cmds[slot] = DrawIndirect(s.bases.z * 6u, 1u, 0u, slot);
        model_draws[slot] = DrawData(lo, s.bases.y);
    }
}
