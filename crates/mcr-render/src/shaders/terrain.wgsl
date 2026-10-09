@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var blocks_sampler: sampler;
@group(0) @binding(2) var blocks0: texture_2d_array<f32>;
@group(0) @binding(3) var blocks1: texture_2d_array<f32>;
@group(0) @binding(4) var blocks2: texture_2d_array<f32>;
@group(0) @binding(5) var blocks3: texture_2d_array<f32>;

@group(1) @binding(0) var<storage, read> cube_quads: array<vec2<u32>>;
@group(1) @binding(1) var<storage, read> model_quads: array<array<u32, 8>>;
@group(1) @binding(2) var<storage, read> cube_draws: array<DrawData>;
@group(1) @binding(3) var<storage, read> model_draws: array<DrawData>;
@group(1) @binding(4) var<storage, read> tints: array<u32>;

const NO_TINT_TABLE: u32 = 0xffffffffu;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) brightness: f32,
    @location(2) @interpolate(flat) layer: u32,
    @location(3) @interpolate(flat) tint: vec2<u32>,
    @location(4) cell: vec3<f32>,
}

fn face_shade(face: u32) -> f32 {
    var shades = array<f32, 6>(0.6, 0.6, 1.0, 0.5, 0.8, 0.8);
    return shades[face];
}

fn light_factor(light: u32) -> f32 {
    let sky = f32(light >> 4u) / 15.0 * g.sun.x;
    let block = f32(light & 15u) / 15.0;
    return 0.12 + 0.88 * max(sky, block);
}

// Vanilla's per-corner shade average: 0.2 for full-collision neighbours, 1.0 otherwise.
fn ao_factor(ao: u32) -> f32 {
    var levels = array<f32, 4>(0.4, 0.6, 0.8, 1.0);
    return levels[ao];
}

fn corner_index(v: u32, flip: bool, diag03: bool) -> u32 {
    var seq: array<u32, 6>;
    if diag03 {
        if flip {
            seq = array<u32, 6>(0u, 3u, 1u, 0u, 2u, 3u);
        } else {
            seq = array<u32, 6>(0u, 1u, 3u, 0u, 3u, 2u);
        }
    } else {
        if flip {
            seq = array<u32, 6>(0u, 2u, 1u, 2u, 3u, 1u);
        } else {
            seq = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
        }
    }
    return seq[v];
}

@vertex
fn vs_cube(@builtin(vertex_index) vi: u32, @builtin(instance_index) slot: u32) -> VsOut {
    let draw = cube_draws[slot];
    let q = decode_cube(cube_quads[draw.first_quad + vi / 6u]);

    let flip = q.face == 0u || q.face == 2u || q.face == 5u;
    let diag03 = q.ao.x + q.ao.w > q.ao.y + q.ao.z;
    let c = corner_index(vi % 6u, flip, diag03);
    let cu = f32(c & 1u);
    let cv = f32(c >> 1u);

    let axis = q.face >> 1u;
    var u_axis = 0u;
    var v_axis = 1u;
    if axis == 0u {
        u_axis = 2u;
        v_axis = 1u;
    } else if axis == 1u {
        u_axis = 0u;
        v_axis = 2u;
    }
    var p = vec3<f32>(q.pos);
    var nudge = vec3<f32>(0.0);
    if (q.face & 1u) == 0u {
        p[axis] += 1.0;
        nudge[axis] = -0.5;
    } else {
        nudge[axis] = 0.5;
    }
    p[u_axis] += cu * f32(q.width);
    p[v_axis] += cv * f32(q.height);

    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(draw.origin + p, 1.0);
    // Texture orientation per face matches vanilla's default UVs (u/v mirrored where the face is seen from behind).
    let mirror_u = q.face == 0u || q.face == 5u;
    let mirror_v = axis != 1u || q.face == 3u;
    let u = select(cu, 1.0 - cu, mirror_u);
    let v = select(cv, 1.0 - cv, mirror_v);
    out.uv = vec2<f32>(u * f32(q.width), v * f32(q.height));
    out.layer = q.layer;
    out.tint = vec2<u32>(q.tint, draw.tint_base);
    out.cell = p + nudge;
    out.brightness = face_shade(q.face) * ao_factor(q.ao[c]) * light_factor(q.light);
    return out;
}

@vertex
fn vs_model(@builtin(vertex_index) vi: u32, @builtin(instance_index) slot: u32) -> VsOut {
    let draw = model_draws[slot];
    let q = model_quads[draw.first_quad + vi / 6u];

    var seq = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    let c = seq[vi % 6u];
    let p = decode_model_corner(q[c]);
    let ao = (q[7] >> (2u * c)) & 3u;

    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(draw.origin + p, 1.0);
    out.uv = decode_model_uv(vec2<u32>(q[4], q[5]), c);
    out.layer = q[6] & 0xffffu;
    out.tint = vec2<u32>((q[7] >> 8u) & 63u, draw.tint_base);
    out.cell = vec3<f32>(f32((q[7] >> 14u) & 15u) + 0.5, 0.0, f32((q[7] >> 18u) & 15u) + 0.5);
    out.brightness = face_shade((q[6] >> 24u) & 7u) * ao_factor(ao) * light_factor((q[6] >> 16u) & 255u);
    return out;
}

fn tint_color(kind: u32, base: u32, cell: vec3<f32>) -> vec3<f32> {
    if kind == 0u {
        return vec3<f32>(1.0);
    }
    var packed: u32;
    if kind >= 8u {
        packed = tints[kind - 8u];
    } else if base == NO_TINT_TABLE {
        return vec3<f32>(1.0);
    } else {
        let x = u32(clamp(floor(cell.x), 0.0, 15.0));
        let z = u32(clamp(floor(cell.z), 0.0, 15.0));
        packed = tints[base + (kind - 1u) * 256u + z * 16u + x];
    }
    return vec3<f32>(f32(packed & 255u), f32((packed >> 8u) & 255u), f32((packed >> 16u) & 255u)) / 255.0;
}

fn sample_blocks(uv: vec2<f32>, layer: u32, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    let l = i32(layer & 0x3fffu);
    switch layer >> 14u {
        case 1u: {
            return textureSampleGrad(blocks1, blocks_sampler, uv, l, dx, dy);
        }
        case 2u: {
            return textureSampleGrad(blocks2, blocks_sampler, uv, l, dx, dy);
        }
        case 3u: {
            return textureSampleGrad(blocks3, blocks_sampler, uv, l, dx, dy);
        }
        default: {
            return textureSampleGrad(blocks0, blocks_sampler, uv, l, dx, dy);
        }
    }
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let t = sample_blocks(in.uv, in.layer, dx, dy);
    if t.a < 0.5 {
        discard;
    }
    return vec4<f32>(t.rgb * tint_color(in.tint.x, in.tint.y, in.cell) * in.brightness, 1.0);
}

@fragment
fn fs_translucent(in: VsOut) -> @location(0) vec4<f32> {
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let t = sample_blocks(in.uv, in.layer, dx, dy);
    return vec4<f32>(t.rgb * tint_color(in.tint.x, in.tint.y, in.cell) * in.brightness, t.a);
}
