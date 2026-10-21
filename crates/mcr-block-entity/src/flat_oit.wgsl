@fragment
fn fs_tex_bounds(in: VsOut) -> @location(0) vec4<f32> {
    return oit_bounds_of(in.clip, tex_color(in).a);
}

@fragment
fn fs_tex_coefficients(in: VsOut) -> OitCoeffs {
    return oit_coefficients_of(in.clip, tex_color(in).a);
}

@fragment
fn fs_tex_accumulate(in: VsOut) -> @location(0) vec4<f32> {
    return oit_accumulated(in.clip, tex_color(in));
}
