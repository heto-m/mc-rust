@fragment
fn fs_oit_bounds(in: VsOut) -> @location(0) vec4<f32> {
    let t = translucent_texel(in.uv, in.layer, in.tint, in.cell);
    return oit_bounds_of(in.clip, t.a);
}

@fragment
fn fs_oit_coefficients(in: VsOut) -> OitCoeffs {
    let t = translucent_texel(in.uv, in.layer, in.tint, in.cell);
    return oit_coefficients_of(in.clip, t.a);
}

@fragment
fn fs_oit_accumulate(in: VsOut) -> @location(0) vec4<f32> {
    let t = translucent_texel(in.uv, in.layer, in.tint, in.cell);
    let rgb = apply_faded_fog(t.rgb * in.color, in.fog_distance);
    return oit_accumulated(in.clip, vec4<f32>(rgb, t.a));
}
