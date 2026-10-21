// Variant terrain entry points that add mod dynamic lights; appended to terrain.wgsl.

struct DynLight {
    // Camera-relative; level in start.w.
    start: vec4<f32>,
    end: vec4<f32>,
}

struct DynLights {
    count: vec4<u32>,
    lights: array<DynLight, 64>,
}

@group(2) @binding(0) var<uniform> dyn_lights: DynLights;
// 16^3 camera-relative cells of 16 blocks around the camera: a bit per light that reaches the cell.
@group(2) @binding(1) var<storage, read> dyn_grid: array<vec2<u32>>;

const DYN_GRID: f32 = 16.0;
const DYN_CELL: f32 = 16.0;

// LambDynamicLights: a level 15 source fades to nothing 7.75 blocks away.
const DYN_FALLOFF: f32 = 15.0 / 7.75;

struct VsOutDyn {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) @interpolate(flat) layer: u32,
    @location(3) @interpolate(flat) tint: vec2<u32>,
    @location(4) cell: vec3<f32>,
    @location(5) fog_distance: vec3<f32>,
    @location(6) sample_pos: vec3<f32>,
    // Sky level, AO times face shade.
    @location(7) sky_shade: vec2<f32>,
}

fn dyn_out(c: Corner) -> VsOutDyn {
    let base = corner_out(c);
    var out: VsOutDyn;
    out.clip = base.clip;
    out.uv = base.uv;
    out.color = base.color;
    out.layer = base.layer;
    out.tint = base.tint;
    out.cell = base.cell;
    out.fog_distance = base.fog_distance;
    // Vanilla lights a face from the block in front of it.
    out.sample_pos = c.rel + c.normal * 0.5;
    out.sky_shade = vec2<f32>(sky_level(c.light), c.shade);
    return out;
}

@vertex
fn vs_cube_dyn(@builtin(vertex_index) vi: u32, @builtin(instance_index) slot: u32) -> VsOutDyn {
    return dyn_out(cube_corner(vi, slot));
}

@vertex
fn vs_model_dyn(@builtin(vertex_index) vi: u32, @builtin(instance_index) slot: u32) -> VsOutDyn {
    return dyn_out(model_corner(vi, slot));
}

fn dyn_light_at(p: vec3<f32>, i: u32) -> f32 {
    let l = dyn_lights.lights[i];
    let ab = l.end.xyz - l.start.xyz;
    let t = clamp(dot(p - l.start.xyz, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
    return l.start.w - distance(p, l.start.xyz + ab * t) * DYN_FALLOFF;
}

fn dyn_level(p: vec3<f32>) -> f32 {
    let c = floor(p / DYN_CELL + DYN_GRID * 0.5);
    if any(c < vec3<f32>(0.0)) || any(c >= vec3<f32>(DYN_GRID)) {
        return 0.0;
    }
    let cell = vec3<u32>(c);
    let mask = dyn_grid[cell.x + cell.z * 16u + cell.y * 256u];
    var level = 0.0;
    for (var w = 0u; w < 2u; w++) {
        var bits = mask[w];
        while bits != 0u {
            level = max(level, dyn_light_at(p, w * 32u + firstTrailingBit(bits)));
            bits &= bits - 1u;
        }
    }
    return min(level, 15.0);
}

// The lightmap grows with block light, so this is the lookup of max(block, dynamic).
fn dyn_color(in: VsOutDyn) -> vec3<f32> {
    let level = dyn_level(in.sample_pos);
    if level <= 0.0 {
        return in.color;
    }
    return max(in.color, light_levels(in.sky_shade.x, level) * in.sky_shade.y);
}

@fragment
fn fs_main_dyn(in: VsOutDyn) -> @location(0) vec4<f32> {
    let c = opaque_texel(in.uv, in.layer, in.tint, in.cell) * dyn_color(in);
    return vec4<f32>(apply_faded_fog(c, in.fog_distance), 1.0);
}

@fragment
fn fs_solid_dyn(in: VsOutDyn) -> @location(0) vec4<f32> {
    let c = solid_texel(in.uv, in.layer, in.tint, in.cell) * dyn_color(in);
    return vec4<f32>(apply_faded_fog(c, in.fog_distance), 1.0);
}

@fragment
fn fs_translucent_dyn(in: VsOutDyn) -> @location(0) vec4<f32> {
    let t = translucent_texel(in.uv, in.layer, in.tint, in.cell);
    return vec4<f32>(apply_faded_fog(t.rgb * dyn_color(in), in.fog_distance), t.a);
}
