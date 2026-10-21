@fragment
fn fs_blend_bounds(in: VertexOut) -> @location(0) vec4<f32> {
    return oit_bounds_of(in.clip, blend_color(in).a);
}

@fragment
fn fs_blend_coefficients(in: VertexOut) -> OitCoeffs {
    return oit_coefficients_of(in.clip, blend_color(in).a);
}

@fragment
fn fs_blend_accumulate(in: VertexOut) -> @location(0) vec4<f32> {
    return oit_accumulated(in.clip, blend_color(in));
}

// Additive parts bound depth but never absorb, so their transmittance stage draws nothing.
@fragment
fn fs_additive_bounds(in: VertexOut) -> @location(0) vec4<f32> {
    return oit_bounds_of(in.clip, additive_color(in).a);
}

@fragment
fn fs_additive_coefficients(in: VertexOut) -> OitCoeffs {
    discard;
    return OitCoeffs(vec4<f32>(0.0), vec4<f32>(0.0));
}

@fragment
fn fs_additive_accumulate(in: VertexOut) -> @location(0) vec4<f32> {
    return oit_accumulated_additive(in.clip, additive_color(in));
}
