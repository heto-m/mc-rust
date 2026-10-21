fn blended(in: Out) -> vec4<f32> {
    let t = textureSample(atlas, smp, in.uv);
    let a = t.a * in.color.a;
    if a < 0.1 {
        discard;
    }
    return vec4<f32>(fogged(t.rgb * in.color.rgb, in.fog_distance), a);
}

@fragment
fn fs_oit_bounds(in: Out) -> @location(0) vec4<f32> {
    return oit_bounds_of(in.clip, blended(in).a);
}

@fragment
fn fs_oit_coefficients(in: Out) -> OitCoeffs {
    return oit_coefficients_of(in.clip, blended(in).a);
}

@fragment
fn fs_oit_accumulate(in: Out) -> @location(0) vec4<f32> {
    return oit_accumulated(in.clip, blended(in));
}
