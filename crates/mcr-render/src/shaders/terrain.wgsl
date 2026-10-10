@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var blocks_sampler: sampler;
@group(0) @binding(2) var blocks0: texture_2d_array<f32>;
@group(0) @binding(3) var blocks1: texture_2d_array<f32>;
@group(0) @binding(4) var blocks2: texture_2d_array<f32>;
@group(0) @binding(5) var blocks3: texture_2d_array<f32>;
// Indexed `sky * 16 + block`.
@group(0) @binding(6) var<uniform> lightmap: array<vec4<f32>, 256>;
// Indexed by packed layer: x = current layer, y = next layer | mix << 16.
@group(0) @binding(7) var<storage, read> anim: array<vec2<u32>>;

@group(1) @binding(0) var<storage, read> cube_quads: array<vec4<u32>>;
@group(1) @binding(1) var<storage, read> model_quads: array<array<u32, 10>>;
@group(1) @binding(2) var<storage, read> cube_draws: array<DrawData>;
@group(1) @binding(3) var<storage, read> model_draws: array<DrawData>;
@group(1) @binding(4) var<storage, read> tints: array<u32>;

const NO_TINT_TABLE: u32 = 0xffffffffu;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) @interpolate(flat) layer: u32,
    @location(3) @interpolate(flat) tint: vec2<u32>,
    @location(4) cell: vec3<f32>,
    @location(5) fog_distance: vec2<f32>,
}

fn face_shade(face: u32) -> f32 {
    var shades = array<f32, 6>(g.shade_b.y, g.shade_b.x, g.shade_a.y, g.shade_a.x, g.shade_a.w, g.shade_a.z);
    return shades[face];
}

fn lightmap_at(block: u32, sky: u32) -> vec3<f32> {
    return lightmap[sky * 16u + block].xyz;
}

// Bilinear in 1/16-level units, like sampling vanilla's filtered 16x16 lightmap texture.
fn light_color(packed: u32) -> vec3<f32> {
    let sky = clamp(f32((packed >> 8u) & 255u) / 16.0, 0.0, 15.0);
    let block = clamp(f32(packed & 255u) / 16.0, 0.0, 15.0);
    let s0 = u32(floor(sky));
    let b0 = u32(floor(block));
    let s1 = min(s0 + 1u, 15u);
    let b1 = min(b0 + 1u, 15u);
    let fs = sky - floor(sky);
    let fb = block - floor(block);
    let low = mix(lightmap_at(b0, s0), lightmap_at(b1, s0), fb);
    let high = mix(lightmap_at(b0, s1), lightmap_at(b1, s1), fb);
    return mix(low, high, fs);
}

fn cube_corner_index(v: u32, flip: bool) -> u32 {
    var seq: array<u32, 6>;
    if flip {
        seq = array<u32, 6>(0u, 3u, 1u, 0u, 2u, 3u);
    } else {
        seq = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    }
    return seq[v];
}

fn rotate_uv(uv_in: vec2<f32>, orientation: u32) -> vec2<f32> {
    var uv = uv_in;
    if (orientation & 4u) != 0u {
        uv = vec2<f32>(-uv.x, uv.y);
    }
    switch orientation & 3u {
        case 1u: {
            return vec2<f32>(-uv.y, uv.x);
        }
        case 2u: {
            return -uv;
        }
        case 3u: {
            return vec2<f32>(uv.y, -uv.x);
        }
        default: {
            return uv;
        }
    }
}

fn fog_distances(rel: vec3<f32>) -> vec2<f32> {
    return vec2<f32>(length(rel), max(length(rel.xz), abs(rel.y)));
}

@vertex
fn vs_cube(@builtin(vertex_index) vi: u32, @builtin(instance_index) slot: u32) -> VsOut {
    let draw = cube_draws[slot];
    let q = decode_cube(cube_quads[draw.first_quad + vi / 6u]);

    // Quads split along vanilla's v0-v2 diagonal; the same faces wind the other way.
    let flip = q.face == 0u || q.face == 2u || q.face == 5u;
    let c = cube_corner_index(vi % 6u, flip);
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
    let rel = draw.origin + p;
    out.clip = g.view_proj * vec4<f32>(rel, 1.0);
    // Texture orientation per face matches vanilla's default UVs (u/v mirrored where the face is seen from behind).
    let mirror_u = q.face == 0u || q.face == 5u;
    let mirror_v = axis != 1u || q.face == 3u;
    let u = select(cu, 1.0 - cu, mirror_u);
    let v = select(cv, 1.0 - cv, mirror_v);
    out.uv = rotate_uv(vec2<f32>(u * f32(q.width), v * f32(q.height)), q.rotation);
    out.layer = q.layer;
    out.tint = vec2<u32>(q.tint, draw.tint_base);
    out.cell = p + nudge;
    let ao = 0.2 + 0.2 * f32(q.ao[c]);
    out.color = light_color(q.light[c]) * (ao * face_shade(q.face));
    out.fog_distance = fog_distances(rel);
    return out;
}

@vertex
fn vs_model(@builtin(vertex_index) vi: u32, @builtin(instance_index) slot: u32) -> VsOut {
    let draw = model_draws[slot];
    let q = model_quads[draw.first_quad + vi / 6u];

    var seq = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    let c = seq[vi % 6u];
    let p = decode_model_corner(q, c);
    let cell = model_cell(q);

    var out: VsOut;
    let rel = draw.origin + p;
    out.clip = g.view_proj * vec4<f32>(rel, 1.0);
    out.uv = decode_model_uv(q, c);
    out.layer = decode_model_layer(q);
    out.tint = vec2<u32>(decode_model_tint(q), draw.tint_base);
    out.cell = cell + vec3<f32>(0.5, 0.0, 0.5);
    let ao = f32(decode_model_ao(q, c)) / 63.0;
    out.color = light_color(decode_model_light(q, c)) * (ao * face_shade(decode_model_shade_face(q)));
    out.fog_distance = fog_distances(rel);
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

fn sample_animated(uv: vec2<f32>, layer: u32, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    let e = anim[layer];
    var t = sample_blocks(uv, e.x, dx, dy);
    let m = e.y >> 16u;
    if m != 0u {
        t = mix(t, sample_blocks(uv, e.y & 0xffffu, dx, dy), f32(m) / 65535.0);
    }
    return t;
}

fn linear_fog(d: f32, start: f32, end: f32) -> f32 {
    if d <= start {
        return 0.0;
    }
    if d >= end {
        return 1.0;
    }
    return (d - start) / (end - start);
}

fn apply_fog(color: vec3<f32>, d: vec2<f32>) -> vec3<f32> {
    let f = max(linear_fog(d.x, g.fog.x, g.fog.y), linear_fog(d.y, g.fog.z, g.fog.w));
    return mix(color, g.fog_color.rgb, f * g.fog_color.a);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let t = sample_animated(in.uv, in.layer, dx, dy);
    if t.a < 0.5 {
        discard;
    }
    let c = t.rgb * tint_color(in.tint.x, in.tint.y, in.cell) * in.color;
    return vec4<f32>(apply_fog(c, in.fog_distance), 1.0);
}

@fragment
fn fs_translucent(in: VsOut) -> @location(0) vec4<f32> {
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let t = sample_animated(in.uv, in.layer, dx, dy);
    if t.a < 0.1 {
        discard;
    }
    let c = t.rgb * tint_color(in.tint.x, in.tint.y, in.cell) * in.color;
    return vec4<f32>(apply_fog(c, in.fog_distance), t.a);
}

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

@vertex
fn vs_sky(@builtin(vertex_index) vi: u32) -> SkyOut {
    var corners = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var out: SkyOut;
    out.clip = vec4<f32>(corners[vi], 0.0, 1.0);
    out.ndc = corners[vi];
    return out;
}

// The sky disc sits 16 blocks above the eye and fades to the fog colour with distance, like vanilla's.
@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let p = g.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let dir = normalize(p.xyz / p.w);
    var k = 1.0;
    if dir.y > 0.0 {
        k = clamp(16.0 / dir.y / max(g.sky_color.w, 1.0), 0.0, 1.0);
    }
    return vec4<f32>(mix(g.sky_color.rgb, g.fog_color.rgb, k), 1.0);
}
