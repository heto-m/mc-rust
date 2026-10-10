@group(0) @binding(0) var<uniform> g: Globals;

struct Fx {
    model: mat4x4<f32>,
    color: vec4<f32>,
    // xyz: cloud offset, w: cloud fog end
    extra: vec4<f32>,
    // x: texture layer
    misc: vec4<f32>,
}

struct FxVertex {
    pos: vec4<f32>,
    uv: vec4<f32>,
    color: vec4<f32>,
}

struct CloudVertex {
    pos: vec3<f32>,
    shade: f32,
}

@group(1) @binding(0) var<uniform> fx: Fx;
@group(1) @binding(1) var tex: texture_2d_array<f32>;
@group(1) @binding(2) var smp: sampler;
@group(1) @binding(3) var<storage, read> verts: array<FxVertex>;
@group(1) @binding(4) var<storage, read> cloud_verts: array<CloudVertex>;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) dist: f32,
}

fn emit(v: FxVertex, pos: vec3<f32>) -> Out {
    var o: Out;
    o.clip = g.view_proj * vec4<f32>(pos, 1.0);
    o.uv = v.uv.xy;
    o.color = v.color * fx.color;
    o.dist = length(pos.xz);
    return o;
}

fn quad_corner(vi: u32) -> u32 {
    var order = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    return (vi / 6u) * 4u + order[vi % 6u];
}

@vertex
fn vs_quad(@builtin(vertex_index) vi: u32) -> Out {
    let v = verts[quad_corner(vi)];
    return emit(v, (fx.model * vec4<f32>(v.pos.xyz, 1.0)).xyz);
}

@vertex
fn vs_tri(@builtin(vertex_index) vi: u32) -> Out {
    let v = verts[vi];
    return emit(v, (fx.model * vec4<f32>(v.pos.xyz, 1.0)).xyz);
}

@vertex
fn vs_cloud(@builtin(vertex_index) vi: u32) -> Out {
    let v = cloud_verts[quad_corner(vi)];
    let pos = v.pos + fx.extra.xyz;
    var o: Out;
    o.clip = g.view_proj * vec4<f32>(pos, 1.0);
    o.uv = vec2<f32>(0.0);
    o.color = vec4<f32>(v.shade * fx.color.rgb, fx.color.a);
    o.dist = length(pos);
    return o;
}

@fragment
fn fs_textured(in: Out) -> @location(0) vec4<f32> {
    let t = textureSample(tex, smp, in.uv, i32(fx.misc.x));
    if t.a < 0.01 {
        discard;
    }
    return t * in.color;
}

@fragment
fn fs_color(in: Out) -> @location(0) vec4<f32> {
    return in.color;
}

@fragment
fn fs_cloud(in: Out) -> @location(0) vec4<f32> {
    let fade = 1.0 - clamp(in.dist / fx.extra.w, 0.0, 1.0);
    return vec4<f32>(in.color.rgb, in.color.a * fade);
}

@fragment
fn fs_weather(in: Out) -> @location(0) vec4<f32> {
    let t = textureSample(tex, smp, in.uv, 0);
    if t.a < 0.01 {
        discard;
    }
    let f = clamp((in.dist - g.fog.z) / max(g.fog.w - g.fog.z, 0.001), 0.0, 1.0);
    let c = mix(t.rgb * in.color.rgb, g.fog_color.rgb, f * g.fog_color.a);
    return vec4<f32>(c, t.a * in.color.a);
}

@fragment
fn fs_destroy(in: Out) -> @location(0) vec4<f32> {
    let t = textureSample(tex, smp, in.uv, i32(in.color.a * 255.0 + 0.5));
    if t.a < 0.1 {
        discard;
    }
    return vec4<f32>(t.rgb, 1.0);
}
