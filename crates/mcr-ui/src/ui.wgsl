struct Globals {
    scale: vec2<f32>,
    srgb_target: f32,
    pad: f32,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var page: texture_2d<f32>;
@group(1) @binding(1) var page_sampler: sampler;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = vec4<f32>(v.pos * globals.scale + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = v.uv;
    out.color = v.color;
    return out;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(page, page_sampler, in.uv) * in.color;
    var rgb = c.rgb;
    if globals.srgb_target > 0.5 {
        rgb = srgb_to_linear(rgb);
    }
    return vec4<f32>(rgb * c.a, c.a);
}
