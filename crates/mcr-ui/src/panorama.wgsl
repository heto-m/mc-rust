struct Globals {
    view_proj: mat4x4<f32>,
    srgb_target: vec4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var cube: texture_cube<f32>;
@group(0) @binding(2) var cube_sampler: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) dir: vec3<f32>,
}

@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.clip = globals.view_proj * vec4<f32>(pos, 1.0);
    out.dir = pos;
    return out;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(cube, cube_sampler, in.dir);
    var rgb = c.rgb;
    if globals.srgb_target.x > 0.5 {
        rgb = srgb_to_linear(rgb);
    }
    return vec4<f32>(rgb, c.a);
}
