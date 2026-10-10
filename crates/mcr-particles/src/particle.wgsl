struct Uniforms {
    view_proj: mat4x4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;

struct Instance {
    @location(0) pos_size: vec4<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) roll: vec4<f32>,
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs(@builtin(vertex_index) vi: u32, inst: Instance) -> Out {
    var corners = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, 1.0));
    let c = corners[vi];
    let cs = cos(inst.roll.x);
    let sn = sin(inst.roll.x);
    let rotated = vec2<f32>(c.x * cs - c.y * sn, c.x * sn + c.y * cs);
    let world = inst.pos_size.xyz + (u.right.xyz * rotated.x + u.up.xyz * rotated.y) * inst.pos_size.w;
    var o: Out;
    o.clip = u.view_proj * vec4<f32>(world, 1.0);
    let t = vec2<f32>((c.x + 1.0) * 0.5, 1.0 - (c.y + 1.0) * 0.5);
    o.uv = mix(inst.uv.xy, inst.uv.zw, t);
    o.color = inst.color;
    return o;
}

@fragment
fn fs_opaque(in: Out) -> @location(0) vec4<f32> {
    let t = textureSample(atlas, smp, in.uv);
    if t.a * in.color.a < 0.1 {
        discard;
    }
    return vec4<f32>(t.rgb * in.color.rgb, 1.0);
}

@fragment
fn fs_blend(in: Out) -> @location(0) vec4<f32> {
    let t = textureSample(atlas, smp, in.uv);
    if t.a < 0.01 {
        discard;
    }
    return vec4<f32>(t.rgb * in.color.rgb, t.a * in.color.a);
}
