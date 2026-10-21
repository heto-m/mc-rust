struct Globals {
    view_proj: mat4x4<f32>,
    params: vec4<f32>,
    fog: vec4<f32>,
    fog_color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var tex0: texture_2d<f32>;
@group(1) @binding(1) var tex1: texture_2d<f32>;
@group(1) @binding(2) var samp: sampler;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) proj: vec4<f32>,
    @location(3) fog_dist: vec2<f32>,
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var out: VsOut;
    let clip = globals.view_proj * vec4<f32>(v.pos, 1.0);
    out.clip = clip;
    out.uv = v.uv;
    out.color = v.color;
    out.proj = vec4<f32>(clip.x * 0.5 + clip.w * 0.5, clip.y * 0.5 + clip.w * 0.5, clip.z, clip.w);
    out.fog_dist = vec2<f32>(length(v.pos), max(length(v.pos.xz), abs(v.pos.y)));
    return out;
}

fn fog_factor(d: f32, start: f32, end: f32) -> f32 {
    return clamp((d - start) / max(end - start, 0.001), 0.0, 1.0);
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

fn finish(rgb: vec3<f32>) -> vec3<f32> {
    if globals.params.y > 0.5 {
        return to_linear(rgb);
    }
    return rgb;
}

@fragment
fn fs_tex(in: VsOut) -> @location(0) vec4<f32> {
    return tex_color(in);
}

fn tex_color(in: VsOut) -> vec4<f32> {
    let c = textureSample(tex0, samp, in.uv) * in.color;
    if c.a < 0.004 {
        discard;
    }
    let f = max(fog_factor(in.fog_dist.x, globals.fog.x, globals.fog.y), fog_factor(in.fog_dist.y, globals.fog.z, globals.fog.w));
    return vec4<f32>(finish(mix(c.rgb, globals.fog_color.rgb, f)), c.a);
}

var<private> COLORS = array<vec3<f32>, 16>(
    vec3<f32>(0.022087, 0.098399, 0.110818),
    vec3<f32>(0.011892, 0.095924, 0.089485),
    vec3<f32>(0.027636, 0.101689, 0.100326),
    vec3<f32>(0.046564, 0.109883, 0.114838),
    vec3<f32>(0.064901, 0.117696, 0.097189),
    vec3<f32>(0.063761, 0.086895, 0.123646),
    vec3<f32>(0.084817, 0.111994, 0.166380),
    vec3<f32>(0.097489, 0.154120, 0.091064),
    vec3<f32>(0.106152, 0.131144, 0.195191),
    vec3<f32>(0.097721, 0.110188, 0.187229),
    vec3<f32>(0.133516, 0.138278, 0.148582),
    vec3<f32>(0.070006, 0.243332, 0.235792),
    vec3<f32>(0.196766, 0.142899, 0.214696),
    vec3<f32>(0.047281, 0.315338, 0.321970),
    vec3<f32>(0.204675, 0.390010, 0.302066),
    vec3<f32>(0.080955, 0.314821, 0.661491),
);

fn project(p: vec4<f32>) -> vec2<f32> {
    return p.xy / p.w;
}

fn portal_layer(p: vec4<f32>, layer: f32) -> vec4<f32> {
    let t = globals.params.x;
    let angle = (layer * layer * 4321.0 + layer * 9.0) * 2.0 * 0.017453292;
    let s = (4.5 - layer / 4.0) * 2.0;
    let c = cos(angle);
    let sn = sin(angle);
    var q = vec4<f32>(s * (p.x * c - p.y * sn), s * (p.x * sn + p.y * c), p.z, p.w);
    q.x = q.x + q.w * (17.0 / layer);
    q.y = q.y + q.w * ((2.0 + layer / 1.5) * (t * 1.5));
    return vec4<f32>(0.5 * q.x + 0.25 * q.w, 0.5 * q.y + 0.25 * q.w, q.z, q.w);
}

@fragment
fn fs_portal(in: VsOut) -> @location(0) vec4<f32> {
    var color = textureSample(tex0, samp, project(in.proj)).rgb * COLORS[0];
    let layers = i32(globals.params.z);
    for (var i = 0; i < layers; i = i + 1) {
        let uv = project(portal_layer(in.proj, f32(i + 1)));
        color = color + textureSample(tex1, samp, uv).rgb * COLORS[i];
    }
    return vec4<f32>(finish(color), 1.0);
}
