// improvedTransparency: vanilla's wavelet order-independent transparency. Each pixel stores its
// translucent absorbance over depth as 8 Haar wavelet coefficients (rank 2): one average plus a tree of
// differences splitting the pixel's translucent depth range into 8 bins.

@group(OIT_GROUP) @binding(0) var oit_bounds: texture_2d<f32>;
@group(OIT_GROUP) @binding(1) var oit_coeff0: texture_2d<f32>;
@group(OIT_GROUP) @binding(2) var oit_coeff1: texture_2d<f32>;
// x: near plane of the infinite reverse-Z projection.
@group(OIT_GROUP) @binding(3) var<uniform> oit_params: vec4<f32>;
@group(OIT_GROUP) @binding(4) var oit_accum: texture_2d<f32>;

const OIT_COEFFS: i32 = 8;
const OIT_RANK: i32 = 2;
const OIT_BINS: f32 = 8.0;
const OIT_OPAQUE_ALPHA: f32 = 0.99;
const OIT_OPAQUE_TRANSMITTANCE: f32 = 0.02;
// Bins spent on the first blocks behind the nearest translucent surface; the rest share what is left.
const OIT_PRECISE_RANGE: f32 = 10.0;
const OIT_COARSE_BINS: f32 = 2.0;

struct OitCoeffs {
    @location(0) c0: vec4<f32>,
    @location(1) c1: vec4<f32>,
}

fn oit_linear_depth(device: f32) -> f32 {
    return oit_params.x / max(device, 1e-30);
}

fn oit_absorbance(transmittance: f32) -> f32 {
    return clamp(-log(max(transmittance, 0.0001)), 0.0, 4.0);
}

fn oit_transmittance_of(absorbance: f32) -> f32 {
    return clamp(exp(-absorbance), 0.0001, 1.0);
}

// The fragment's place in its pixel's translucent depth range, 0 at the nearest surface.
fn oit_normalized_depth(frag: vec4<f32>) -> f32 {
    let b = textureLoad(oit_bounds, vec2<i32>(frag.xy), 0);
    let nearest = -b.r;
    let range = max(b.g - nearest, 0.0001);
    let within = clamp(oit_linear_depth(frag.z) - nearest, 0.0, range);
    let precise_share = (OIT_BINS - OIT_COARSE_BINS) / OIT_BINS;
    if precise_share * range <= OIT_PRECISE_RANGE {
        return within / range;
    }
    if within <= OIT_PRECISE_RANGE {
        return within / OIT_PRECISE_RANGE * precise_share;
    }
    return precise_share + (within - OIT_PRECISE_RANGE) / (range - OIT_PRECISE_RANGE) * (1.0 - precise_share);
}

// Depth bounds stage: nearest and farthest linear depth, nearest device depth and the nearest fully
// opaque one, merged with a max blend.
fn oit_bounds_of(frag: vec4<f32>, alpha: f32) -> vec4<f32> {
    if alpha < 0.01 {
        discard;
    }
    let linear = oit_linear_depth(frag.z);
    return vec4<f32>(-linear, linear, frag.z, select(0.0, frag.z, alpha > OIT_OPAQUE_ALPHA));
}

// Transmittance stage: this surface's absorbance as a step at its depth, added to the coefficients.
fn oit_coefficients_of(frag: vec4<f32>, alpha: f32) -> OitCoeffs {
    var c = array<f32, 8>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let absorbance = oit_absorbance(1.0 - alpha);
    let normalized = oit_normalized_depth(frag);
    let depth = normalized * (OIT_BINS - 1.0) / OIT_BINS;
    c[0] = absorbance * (1.0 - depth);
    var node = i32(floor(normalized * (OIT_BINS - 1.0))) + OIT_COEFFS;
    for (var level = 0; level <= OIT_RANK; level++) {
        let power = OIT_RANK - level;
        let parent = node >> 1u;
        let right = node & 1;
        let sign = f32(1 - 2 * right);
        let width = exp2(-f32(power));
        let height = exp2(f32(power) * 0.5);
        let offset = depth - width * f32(parent & ((1 << u32(power)) - 1));
        c[parent] = (f32(right) * width + sign * offset) * height * absorbance;
        node = parent;
    }
    return OitCoeffs(vec4<f32>(c[0], c[1], c[2], c[3]), vec4<f32>(c[4], c[5], c[6], c[7]));
}

// Absorbance in front of `normalized`, without this fragment's own `current` contribution.
fn oit_absorbance_at(coeffs: array<f32, 8>, normalized: f32, current: f32) -> f32 {
    var c = coeffs;
    var average = c[0];
    if average == 0.0 {
        return 0.0;
    }
    let bins = normalized * (OIT_BINS - 1.0);
    let depth = bins / OIT_BINS;
    average -= current * (1.0 - depth);
    var node_b = clamp(i32(floor(bins)), 0, OIT_COEFFS - 1);
    let has_a = node_b >= 1;
    var node_a = select(node_b, node_b - 1, has_a) + OIT_COEFFS;
    node_b += OIT_COEFFS;
    var sample_b = average;
    var sample_a = select(0.0, average, has_a);
    for (var level = 0; level <= OIT_RANK; level++) {
        let power = OIT_RANK - level;
        let width = exp2(-f32(power));
        let height = exp2(f32(power) * 0.5);
        let parent_b = node_b >> 1u;
        let right_b = node_b & 1;
        let sign_b = f32(1 - 2 * right_b);
        let offset = depth - width * f32(parent_b & ((1 << u32(power)) - 1));
        let own = (f32(right_b) * width + sign_b * offset) * height * current;
        let diff_b = c[parent_b] - own;
        sample_b -= height * diff_b * sign_b;
        node_b = parent_b;
        if has_a {
            let parent_a = node_a >> 1u;
            let diff_a = select(c[parent_a], diff_b, parent_a == parent_b);
            sample_a -= height * diff_a * f32(1 - 2 * (node_a & 1));
            node_a = parent_a;
        }
    }
    let t = select(fract(bins), 1.0, bins >= OIT_BINS);
    return mix(sample_a, sample_b, t);
}

fn oit_transmittance_at(pixel: vec2<i32>, normalized: f32, current: f32) -> f32 {
    let a = textureLoad(oit_coeff0, pixel, 0);
    let b = textureLoad(oit_coeff1, pixel, 0);
    let c = array<f32, 8>(a.x, a.y, a.z, a.w, b.x, b.y, b.z, b.w);
    return oit_transmittance_of(oit_absorbance_at(c, normalized, oit_absorbance(current)));
}

// Accumulate stage: premultiplied colour weighted by what lies in front of it.
fn oit_accumulated(frag: vec4<f32>, color: vec4<f32>) -> vec4<f32> {
    let t = oit_transmittance_at(vec2<i32>(frag.xy), oit_normalized_depth(frag), 1.0 - color.a);
    return vec4<f32>(color.rgb * color.a, color.a) * t;
}
