struct FullOut {
    @builtin(position) clip: vec4<f32>,
}

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> FullOut {
    var corners = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return FullOut(vec4<f32>(corners[vi], 0.0, 1.0));
}

@fragment
fn fs_bounds_copy(in: FullOut) -> @location(0) vec4<f32> {
    return textureLoad(oit_bounds, vec2<i32>(in.clip.xy), 0);
}

struct CullOut {
    @location(0) bounds: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

// Fully opaque translucent fragments end the range and enter the depth buffer, so later stages skip
// what they hide.
@fragment
fn fs_bounds_cull(in: FullOut) -> CullOut {
    let b = textureLoad(oit_bounds, vec2<i32>(in.clip.xy), 0);
    let opaque = oit_linear_depth(b.a);
    return CullOut(vec4<f32>(b.r, min(b.g, opaque), b.b, b.a), b.a);
}

@fragment
fn fs_composite(in: FullOut) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.clip.xy);
    let accum = textureLoad(oit_accum, pixel, 0);
    let t = oit_transmittance_at(pixel, 100000.0, 1.0);
    let coverage = 1.0 - select(t, 0.0, t < OIT_OPAQUE_TRANSMITTANCE);
    if coverage < 0.00001 && dot(accum.rgb, vec3<f32>(1.0)) < 0.00001 {
        discard;
    }
    let scale = select(1.0, coverage / accum.a, accum.a > 0.00001);
    return vec4<f32>(accum.rgb * scale, coverage);
}
