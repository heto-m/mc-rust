struct Globals {
    view_proj: mat4x4<f32>,
    light0: vec4<f32>,
    light1: vec4<f32>,
    params: vec4<f32>,
    fog: vec4<f32>,
    fog_color: vec4<f32>,
    // Indexed `sky * 16 + block`.
    lightmap: array<vec4<f32>, 256>,
}

struct Instance {
    bone_base: u32,
    layer: u32,
    uv_scale: vec2<f32>,
    tint: u32,
    flags: u32,
    uv_offset: vec2<f32>,
    origin: vec3<f32>,
    // Glow colour as RGBA8 with alpha 255, or 0 when the entity does not glow.
    outline: u32,
}

// The overlay texture's white row at u = 1 has alpha 0.25: three quarters white.
const WHITE_OVERLAY: f32 = 0.75;

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> bones: array<vec4<f32>>;
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
    @location(1) shade: vec3<f32>,
    @location(2) @interpolate(flat) layer: u32,
    @location(3) @interpolate(flat) tint: vec4<f32>,
    @location(4) @interpolate(flat) hurt: f32,
    @location(5) fog_dist: vec2<f32>,
    @location(6) @interpolate(flat) flash: f32,
}

fn unpack_rgba(c: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(c & 255u),
        f32((c >> 8u) & 255u),
        f32((c >> 16u) & 255u),
        f32((c >> 24u) & 255u),
    ) / 255.0;
}

fn lightmap_at(block: u32, sky: u32) -> vec3<f32> {
    return globals.lightmap[sky * 16u + block].xyz;
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    return transform(v);
}

fn transform(v: VertexIn) -> VertexOut {
    let inst = instances[v.instance];
    let at = (inst.bone_base + v.bone) * 3u;
    let r0 = bones[at];
    let r1 = bones[at + 1u];
    let r2 = bones[at + 2u];
    let p = vec3<f32>(dot(r0.xyz, v.pos) + r0.w, dot(r1.xyz, v.pos) + r1.w, dot(r2.xyz, v.pos) + r2.w);
    let world = vec4<f32>(p + inst.origin, 1.0);
    let n = normalize(vec3<f32>(dot(r0.xyz, v.normal), dot(r1.xyz, v.normal), dot(r2.xyz, v.normal)));
    var out: VertexOut;
    out.clip = globals.view_proj * world;
    out.uv = v.uv * inst.uv_scale + inst.uv_offset;
    let fullbright = (inst.flags & 1u) != 0u;
    let block = (inst.flags >> 8u) & 15u;
    let sky = (inst.flags >> 12u) & 15u;
    var lit = vec3<f32>(1.0);
    if !fullbright {
        let acc = max(dot(n, globals.light0.xyz), 0.0) + max(dot(n, globals.light1.xyz), 0.0);
        lit = vec3<f32>(min(acc * 0.6 + 0.4, 1.0)) * lightmap_at(block, sky);
    }
    out.shade = lit;
    out.layer = inst.layer;
    out.tint = unpack_rgba(inst.tint);
    out.hurt = f32((inst.flags >> 1u) & 1u);
    out.flash = f32((inst.flags >> 2u) & 1u);
    out.fog_dist = vec2<f32>(length(p + inst.origin), max(length((p + inst.origin).xz), abs(p.y + inst.origin.y)));
    return out;
}

@vertex
fn vs_outline(v: VertexIn) -> VertexOut {
    var out = transform(v);
    let outline = instances[v.instance].outline;
    out.tint = unpack_rgba(outline);
    if outline == 0u {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
    }
    return out;
}

@fragment
fn fs_outline(in: VertexOut) -> @location(0) vec4<f32> {
    let c = textureSample(entity_textures, entity_sampler, in.uv, in.layer);
    if c.a == 0.0 {
        discard;
    }
    return vec4<f32>(in.tint.rgb, 1.0);
}

fn fog_factor(d: f32, start: f32, end: f32) -> f32 {
    return clamp((d - start) / max(end - start, 0.001), 0.0, 1.0);
}

fn fogged(rgb: vec3<f32>, d: vec2<f32>) -> vec3<f32> {
    let f = max(fog_factor(d.x, globals.fog.x, globals.fog.y), fog_factor(d.y, globals.fog.z, globals.fog.w));
    return mix(rgb, globals.fog_color.rgb, f);
}

fn finish(rgb: vec3<f32>) -> vec3<f32> {
    if globals.params.y > 0.5 {
        return to_linear(rgb);
    }
    return rgb;
}

@fragment
fn fs_cutout(in: VertexOut) -> @location(0) vec4<f32> {
    let c = textureSample(entity_textures, entity_sampler, in.uv, in.layer);
    if c.a < 0.1 {
        discard;
    }
    var rgb = c.rgb * in.tint.rgb;
    rgb = mix(rgb, vec3<f32>(1.0, 0.0, 0.0), in.hurt * 0.3);
    rgb = mix(rgb, vec3<f32>(1.0), in.flash * WHITE_OVERLAY);
    return vec4<f32>(finish(fogged(rgb * in.shade, in.fog_dist)), 1.0);
}

@fragment
fn fs_blend(in: VertexOut) -> @location(0) vec4<f32> {
    let c = textureSample(entity_textures, entity_sampler, in.uv, in.layer);
    if c.a < 0.003 {
        discard;
    }
    var rgb = c.rgb * in.tint.rgb;
    rgb = mix(rgb, vec3<f32>(1.0, 0.0, 0.0), in.hurt * 0.3);
    rgb = mix(rgb, vec3<f32>(1.0), in.flash * WHITE_OVERLAY);
    return vec4<f32>(finish(fogged(rgb * in.shade, in.fog_dist)), c.a * in.tint.a);
}

@fragment
fn fs_additive(in: VertexOut) -> @location(0) vec4<f32> {
    let c = textureSample(entity_textures, entity_sampler, in.uv, in.layer);
    let f = max(fog_factor(in.fog_dist.x, globals.fog.x, globals.fog.y), fog_factor(in.fog_dist.y, globals.fog.z, globals.fog.w));
    return vec4<f32>(finish(c.rgb * in.tint.rgb), c.a * in.tint.a * (1.0 - f));
}
