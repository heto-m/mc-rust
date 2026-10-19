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
const MULTI_TINT_TABLES: u32 = 0x40000000u;
// Set in the tint kind passed to the fragment stage: draw without alpha cutout.
const TINT_OPAQUE: u32 = 0x100u;
const MODEL_FLAG_OPAQUE: u32 = 1u;
const MODEL_FLAG_UP_SHADE: u32 = 2u;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) @interpolate(flat) layer: u32,
    @location(3) @interpolate(flat) tint: vec2<u32>,
    @location(4) cell: vec3<f32>,
    // Fog distances, then vanilla's chunk fade-in visibility (0 draws the fog colour, 1 the terrain).
    @location(5) fog_distance: vec3<f32>,
}

fn face_shade(face: u32) -> f32 {
    var shades = array<f32, 6>(g.shade_b.y, g.shade_b.x, g.shade_a.y, g.shade_a.x, g.shade_a.w, g.shade_a.z);
    return shades[face];
}

fn lightmap_at(block: u32, sky: u32) -> vec3<f32> {
    return lightmap[sky * 16u + block].xyz;
}

fn sky_level(packed: u32) -> f32 {
    return clamp(f32((packed >> 8u) & 255u) / 16.0, 0.0, 15.0);
}

fn light_color(packed: u32) -> vec3<f32> {
    return light_levels(sky_level(packed), clamp(f32(packed & 255u) / 16.0, 0.0, 15.0));
}

// Bilinear in 1/16-level units, like sampling vanilla's filtered 16x16 lightmap texture.
fn light_levels(sky: f32, block: f32) -> vec3<f32> {
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

// One quad corner before it becomes vertex output; `rel` is camera-relative.
struct Corner {
    rel: vec3<f32>,
    uv: vec2<f32>,
    layer: u32,
    tint: vec2<u32>,
    cell: vec3<f32>,
    light: u32,
    shade: f32,
    normal: vec3<f32>,
    // Seconds since start when the section was first uploaded.
    born: f32,
}

fn face_normal(face: u32) -> vec3<f32> {
    var n = vec3<f32>(0.0);
    n[face >> 1u] = select(-1.0, 1.0, (face & 1u) == 0u);
    return n;
}

fn cube_corner(vi: u32, slot: u32) -> Corner {
    let draw = cube_draws[slot];
    let q = decode_cube(cube_quads[draw.first_quad + (vi >> 2u)]);

    // The index buffer emits corners (0, 1, 2), (2, 1, 3); quads split along vanilla's v0-v2 diagonal
    // instead swap corner pairs, which keeps the winding.
    let flip = q.face == 0u || q.face == 2u || q.face == 5u;
    let c = select(vi & 3u, (vi & 3u) ^ 1u, flip);
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

    var out: Corner;
    out.rel = draw.origin + p;
    // Texture orientation per face matches vanilla's default UVs (u/v mirrored where the face is seen from behind).
    let mirror_u = q.face == 0u || q.face == 5u;
    let mirror_v = axis != 1u || q.face == 3u;
    let u = select(cu, 1.0 - cu, mirror_u);
    let v = select(cv, 1.0 - cv, mirror_v);
    out.uv = rotate_uv(vec2<f32>(u * f32(q.width), v * f32(q.height)), q.rotation);
    out.layer = q.layer;
    out.tint = vec2<u32>(q.tint | select(0u, TINT_OPAQUE, q.opaque), draw.tint_base);
    out.cell = p + nudge;
    out.light = q.light[c];
    out.shade = (0.2 + 0.2 * f32(q.ao[c])) * face_shade(q.face);
    out.normal = nudge * -2.0;
    out.born = bitcast<f32>(draw.pad1);
    return out;
}

fn model_corner(vi: u32, slot: u32) -> Corner {
    let draw = model_draws[slot];
    let q = model_quads[draw.first_quad + (vi >> 2u)];
    let c = vi & 3u;
    let p = decode_model_corner(q, c);
    let shade_face = decode_model_shade_face(q);

    var out: Corner;
    out.rel = draw.origin + p;
    out.uv = decode_model_uv(q, c);
    out.layer = decode_model_layer(q);
    let flags = decode_model_flags(q);
    out.tint = vec2<u32>(decode_model_tint(q) | select(0u, TINT_OPAQUE, (flags & MODEL_FLAG_OPAQUE) != 0u), draw.tint_base);
    out.cell = model_cell(q) + vec3<f32>(0.5, 0.0, 0.5);
    out.light = decode_model_light(q, c);
    out.shade = f32(decode_model_ao(q, c)) / 63.0 * face_shade(shade_face);
    if (flags & MODEL_FLAG_UP_SHADE) != 0u {
        out.shade *= face_shade(2u);
    }
    out.normal = face_normal(shade_face);
    out.born = bitcast<f32>(draw.pad1);
    return out;
}

fn corner_out(c: Corner) -> VsOut {
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(c.rel, 1.0);
    out.uv = c.uv;
    out.layer = c.layer;
    out.tint = c.tint;
    out.cell = c.cell;
    out.color = light_color(c.light) * c.shade;
    let fog = fog_distances(c.rel);
    out.fog_distance = vec3<f32>(fog, section_visibility(c.born, fog.x));
    return out;
}

// Fully visible within 16 blocks, fading in over the option's time beyond 32.
fn section_visibility(born: f32, distance: f32) -> f32 {
    var faded = 1.0;
    if g.fade.y > 0.0 {
        faded = clamp((g.fade.x - born) / g.fade.y, 0.0, 1.0);
    }
    return mix(1.0, faded, clamp((distance - 16.0) / 16.0, 0.0, 1.0));
}


@vertex
fn vs_cube(@builtin(vertex_index) vi: u32, @builtin(instance_index) slot: u32) -> VsOut {
    return corner_out(cube_corner(vi, slot));
}

@vertex
fn vs_model(@builtin(vertex_index) vi: u32, @builtin(instance_index) slot: u32) -> VsOut {
    return corner_out(model_corner(vi, slot));
}

fn tint_color(tint: u32, base: u32, cell: vec3<f32>) -> vec3<f32> {
    let kind = tint & 63u;
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
        var table = base & ~MULTI_TINT_TABLES;
        if (base & MULTI_TINT_TABLES) != 0u {
            // Word y of the first table names the table for blocks at that height.
            let y = u32(clamp(floor(cell.y), 0.0, 15.0));
            table += (tints[table + y] >> 24u) * 1024u;
        }
        packed = tints[table + (kind - 1u) * 256u + z * 16u + x];
    }
    return vec3<f32>(f32(packed & 255u), f32((packed >> 8u) & 255u), f32((packed >> 16u) & 255u)) / 255.0;
}

// Vanilla's `sampleNearest`: texel centres stay sharp under linear filtering, with a pixel-wide blend
// at texel edges.
fn sample_sharp(t: texture_2d_array<f32>, uv: vec2<f32>, l: i32, dx: vec2<f32>, dy: vec2<f32>, pixel: vec2<f32>) -> vec4<f32> {
    let texel_screen = max(sqrt(dx * dx + dy * dy), vec2<f32>(1e-8));
    let coords = uv / pixel;
    let center = round(coords) - 0.5;
    let offset = clamp((coords - center - 0.5) * pixel / texel_screen + 0.5, vec2<f32>(0.0), vec2<f32>(1.0));
    return textureSampleGrad(t, blocks_sampler, (center + offset) * pixel, l, dx, dy);
}

// Vanilla's rotated-grid supersampling, blended in as texels shrink below a pixel.
fn sample_rgss(t: texture_2d_array<f32>, uv: vec2<f32>, l: i32, dx: vec2<f32>, dy: vec2<f32>, pixel: vec2<f32>) -> vec4<f32> {
    let texel_screen = sqrt(dx * dx + dy * dy);
    let min_pixel = min(pixel.x, pixel.y);
    let blend = smoothstep(min_pixel, min_pixel * 2.0, max(texel_screen.x, texel_screen.y));
    let sharp = sample_sharp(t, uv, l, dx, dy, pixel);
    if blend <= 0.0 {
        return sharp;
    }
    let du = length(dx);
    let dv = length(dy);
    let lod = max(0.0, log2(sqrt(min(du, dv) * max(du, dv)) / min_pixel));
    let low = floor(lod);
    var offsets = array<vec2<f32>, 4>(vec2<f32>(0.125, 0.375), vec2<f32>(-0.125, -0.375), vec2<f32>(0.375, -0.125), vec2<f32>(-0.375, 0.125));
    var a = vec4<f32>(0.0);
    var b = vec4<f32>(0.0);
    for (var i = 0u; i < 4u; i++) {
        let p = uv + offsets[i] * pixel;
        a += textureSampleLevel(t, blocks_sampler, p, l, low);
        b += textureSampleLevel(t, blocks_sampler, p, l, low + 1.0);
    }
    return mix(sharp, mix(a, b, fract(lod)) * 0.25, blend);
}

// `FilterMode`, specialised per pipeline so the nearest path carries no filtering code.
override FILTER_MODE: u32 = 0u;

fn sample_group(t: texture_2d_array<f32>, uv: vec2<f32>, l: i32, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    let mode = FILTER_MODE;
    if mode == 0u {
        return textureSampleGrad(t, blocks_sampler, uv, l, dx, dy);
    }
    let pixel = 1.0 / vec2<f32>(textureDimensions(t));
    if mode == 1u {
        return sample_rgss(t, uv, l, dx, dy, pixel);
    }
    return sample_sharp(t, uv, l, dx, dy, pixel);
}

fn sample_blocks(uv: vec2<f32>, layer: u32, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    let l = i32(layer & 0x3fffu);
    switch layer >> 14u {
        case 1u: {
            return sample_group(blocks1, uv, l, dx, dy);
        }
        case 2u: {
            return sample_group(blocks2, uv, l, dx, dy);
        }
        case 3u: {
            return sample_group(blocks3, uv, l, dx, dy);
        }
        default: {
            return sample_group(blocks0, uv, l, dx, dy);
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

// Vanilla fades a section in toward the fog colour (`d.z` is its visibility), then fogs it; one blend.
fn apply_faded_fog(color: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    let f = max(linear_fog(d.x, g.fog.x, g.fog.y), linear_fog(d.y, g.fog.z, g.fog.w)) * g.fog_color.a;
    return mix(color, g.fog_color.rgb, 1.0 - d.z * (1.0 - f));
}

// Texture times tint; cutout below half alpha.
fn opaque_texel(uv: vec2<f32>, layer: u32, tint: vec2<u32>, cell: vec3<f32>) -> vec3<f32> {
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let t = sample_animated(uv, layer, dx, dy);
    if t.a < 0.5 && (tint.x & TINT_OPAQUE) == 0u {
        discard;
    }
    return t.rgb * tint_color(tint.x, tint.y, cell);
}

fn translucent_texel(uv: vec2<f32>, layer: u32, tint: vec2<u32>, cell: vec3<f32>) -> vec4<f32> {
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let t = sample_animated(uv, layer, dx, dy);
    if t.a < 0.1 {
        discard;
    }
    return vec4<f32>(t.rgb * tint_color(tint.x, tint.y, cell), t.a);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = opaque_texel(in.uv, in.layer, in.tint, in.cell) * in.color;
    return vec4<f32>(apply_faded_fog(c, in.fog_distance), 1.0);
}

@fragment
fn fs_translucent(in: VsOut) -> @location(0) vec4<f32> {
    let t = translucent_texel(in.uv, in.layer, in.tint, in.cell);
    return vec4<f32>(apply_faded_fog(t.rgb * in.color, in.fog_distance), t.a);
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
    if dir.y < 0.0 && g.sun.y > 0.5 {
        let void_k = clamp(4.0 / -dir.y / max(g.sky_color.w, 1.0), 0.0, 1.0);
        return vec4<f32>(mix(vec3<f32>(0.0), g.fog_color.rgb, void_k), 1.0);
    }
    return vec4<f32>(mix(g.sky_color.rgb, g.fog_color.rgb, k), 1.0);
}
