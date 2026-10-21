@fragment
fn fs_cloud_bounds(in: Out) -> @location(0) vec4<f32> {
    return oit_bounds_of(in.clip, cloud_color(in).a);
}

@fragment
fn fs_cloud_coefficients(in: Out) -> OitCoeffs {
    return oit_coefficients_of(in.clip, cloud_color(in).a);
}

@fragment
fn fs_cloud_accumulate(in: Out) -> @location(0) vec4<f32> {
    return oit_accumulated(in.clip, cloud_color(in));
}

@fragment
fn fs_weather_bounds(in: Out) -> @location(0) vec4<f32> {
    return oit_bounds_of(in.clip, weather_color(in).a);
}

@fragment
fn fs_weather_coefficients(in: Out) -> OitCoeffs {
    return oit_coefficients_of(in.clip, weather_color(in).a);
}

@fragment
fn fs_weather_accumulate(in: Out) -> @location(0) vec4<f32> {
    return oit_accumulated(in.clip, weather_color(in));
}

// The border adds light: it bounds depth but never absorbs, so its transmittance stage draws nothing.
@fragment
fn fs_border_bounds(in: Out) -> @location(0) vec4<f32> {
    return oit_bounds_of(in.clip, border_color(in).a);
}

@fragment
fn fs_border_coefficients(in: Out) -> OitCoeffs {
    discard;
    return OitCoeffs(vec4<f32>(0.0), vec4<f32>(0.0));
}

@fragment
fn fs_border_accumulate(in: Out) -> @location(0) vec4<f32> {
    return oit_accumulated_additive(in.clip, border_color(in));
}
