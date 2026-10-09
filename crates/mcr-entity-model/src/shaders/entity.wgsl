struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
}

struct Instance {
    bone_base: u32,
    layer: u32,
    uv_scale: vec2<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> bones: array<mat4x4<f32>>;
@group(0) @binding(2) var<storage, read> instances: array<Instance>;
@group(0) @binding(3) var entity_textures: texture_2d_array<f32>;
@group(0) @binding(4) var entity_sampler: sampler;

struct VertexIn {
    @location(0) pos: vec3<f32>,
    @location(1) bone: u32,
    @location(2) normal: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @builtin(instance_index) instance: u32,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) shade: f32,
    @location(2) @interpolate(flat) layer: u32,
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    let inst = instances[v.instance];
    let m = bones[inst.bone_base + v.bone];
    let world = m * vec4<f32>(v.pos, 1.0);
    let n = normalize((m * vec4<f32>(v.normal, 0.0)).xyz);
    var out: VertexOut;
    out.clip = globals.view_proj * world;
    out.uv = v.uv * inst.uv_scale;
    out.shade = 0.55 + 0.45 * max(dot(n, normalize(globals.light_dir.xyz)), 0.0);
    out.layer = inst.layer;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let c = textureSample(entity_textures, entity_sampler, in.uv, in.layer);
    if c.a < 0.1 {
        discard;
    }
    return vec4<f32>(c.rgb * in.shade, 1.0);
}
