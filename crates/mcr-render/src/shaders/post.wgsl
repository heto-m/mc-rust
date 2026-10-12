struct Params {
    // xy: input size, z: menu blur radius
    size: vec4f,
    a: vec4f,
    b: vec4f,
    c: vec4f,
    d: vec4f,
};

@group(0) @binding(0) var t_in: texture_2d<f32>;
@group(0) @binding(1) var t_aux: texture_2d<f32>;
@group(0) @binding(2) var s_in: sampler;
@group(0) @binding(3) var s_aux: sampler;
@group(0) @binding(4) var<uniform> p: Params;

const LUMA = vec3f(0.3, 0.59, 0.11);
const MAX_BLUR_RADIUS = 32.0;

struct VOut {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
    @location(1) warped: vec2f,
};

fn corner(i: u32) -> vec2f {
    return vec2f(f32((i << 1u) & 2u), f32(i & 2u));
}

// `uv` runs bottom-up like vanilla's screen quad; mcr's targets are stored top-down.
fn tap(t: texture_2d<f32>, s: sampler, uv: vec2f) -> vec4f {
    return textureSampleLevel(t, s, vec2f(uv.x, 1.0 - uv.y), 0.0);
}

@vertex
fn vs_quad(@builtin(vertex_index) i: u32) -> VOut {
    let uv = corner(i);
    var o: VOut;
    o.pos = vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
    o.uv = uv;
    o.warped = uv;
    return o;
}

@vertex
fn vs_rotscale(@builtin(vertex_index) i: u32) -> VOut {
    let uv = corner(i);
    let angle = radians(p.b.x);
    let cs = cos(angle);
    let sn = sin(angle);
    let turned = vec2f(uv.x * cs - uv.y * sn, uv.y * cs + uv.x * sn);
    var o: VOut;
    o.pos = vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
    o.uv = uv;
    o.warped = turned * p.a.xy + p.a.zw;
    return o;
}

@fragment
fn fs_box_blur(v: VOut) -> @location(0) vec4f {
    let step = p.a.xy / p.size.xy;
    let radius = min(select(p.size.z, round(p.a.z), p.a.z >= 0.5), MAX_BLUR_RADIUS);
    var sum = vec4f(0.0);
    // Linear filtering averages texel pairs, so taps step by two with a half-weight last tap.
    for (var k = 0.5 - radius; k <= radius; k += 2.0) {
        sum += tap(t_in, s_in, v.uv + step * k);
    }
    sum += tap(t_in, s_in, v.uv + step * radius) * 0.5;
    return sum / (radius + 0.5);
}

@fragment
fn fs_convolve(v: VOut) -> @location(0) vec4f {
    let c = tap(t_in, s_in, v.uv).rgb;
    let mixed = vec3f(dot(c, p.a.xyz), dot(c, p.b.xyz), dot(c, p.c.xyz));
    let luma = dot(mixed, LUMA);
    return vec4f((mixed - luma) * 1.8 + luma, 1.0);
}

@fragment
fn fs_bits(v: VOut) -> @location(0) vec4f {
    let cells = p.size.xy / p.a.y;
    let anchor = v.uv - fract(v.uv * cells) / cells;
    // Half a texel in, so nearest sampling picks the cell's first texel on every backend.
    let c = tap(t_in, s_in, anchor + 0.5 / p.size.xy).rgb;
    let levels = c - fract(c * p.a.x) / p.a.x;
    let luma = dot(levels, LUMA);
    return vec4f((levels - luma) * 1.5 + luma, 1.0);
}

@fragment
fn fs_invert(v: VOut) -> @location(0) vec4f {
    let c = tap(t_in, s_in, v.uv);
    return vec4f(mix(c.rgb, 1.0 - c.rgb, p.a.x), 1.0);
}

@fragment
fn fs_blit(v: VOut) -> @location(0) vec4f {
    return tap(t_in, s_in, v.uv) * p.a;
}

fn edge_fade(blur: vec4f, sharp: vec4f, at: f32, hard: f32, soft: f32) -> vec4f {
    return mix(blur, sharp, (hard - at) / (hard - soft));
}

@fragment
fn fs_spider(v: VOut) -> @location(0) vec4f {
    let w = v.warped;
    let clip = p.c;
    let fade = p.d;
    let blur = tap(t_aux, s_aux, v.uv);
    var out = tap(t_in, s_in, w);
    if w.x < clip.x || w.y < clip.y || w.x > clip.z || w.y > clip.w {
        out = blur;
    }
    if w.x < fade.x {
        out = edge_fade(blur, out, w.x, clip.x, fade.x);
    }
    if w.y < fade.y {
        out = edge_fade(blur, out, w.y, clip.y, fade.y);
    }
    if w.x > fade.z {
        out = edge_fade(blur, out, w.x, clip.z, fade.z);
    }
    if w.y > fade.w {
        out = edge_fade(blur, out, w.y, clip.w, fade.w);
    }
    return vec4f(out.rgb, 1.0);
}

@fragment
fn fs_sobel(v: VOut) -> @location(0) vec4f {
    let px = 1.0 / p.size.xy;
    let c = tap(t_in, s_in, v.uv);
    let l = tap(t_in, s_in, v.uv - vec2f(px.x, 0.0));
    let r = tap(t_in, s_in, v.uv + vec2f(px.x, 0.0));
    let u = tap(t_in, s_in, v.uv - vec2f(0.0, px.y));
    let d = tap(t_in, s_in, v.uv + vec2f(0.0, px.y));
    let edge = abs(c.a - l.a) + abs(c.a - r.a) + abs(c.a - u.a) + abs(c.a - d.a);
    let rgb = c.rgb * c.a + l.rgb * l.a + r.rgb * r.a + u.rgb * u.a + d.rgb * d.a;
    return vec4f(rgb * 0.2, clamp(edge, 0.0, 1.0));
}

@fragment
fn fs_outline_blur(v: VOut) -> @location(0) vec4f {
    let step = p.a.xy / p.size.xy;
    let sum = tap(t_in, s_in, v.uv - step * 1.5) + tap(t_in, s_in, v.uv + step * 0.5)
        + tap(t_in, s_in, v.uv + step * 2.0) * 0.5;
    // Alpha stays a sum, which widens the outline.
    return vec4f(sum.rgb / 2.5, sum.a);
}
