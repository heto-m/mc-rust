@fragment
fn fs_oit_accumulate_dyn(in: VsOutDyn) -> @location(0) vec4<f32> {
    let t = translucent_texel(in.uv, in.layer, in.tint, in.cell);
    let rgb = apply_faded_fog(t.rgb * dyn_color(in), in.fog_distance);
    return oit_accumulated(in.clip, vec4<f32>(rgb, t.a));
}
