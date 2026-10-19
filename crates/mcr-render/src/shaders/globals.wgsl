struct Globals {
    view_proj: mat4x4<f32>,
    planes: array<vec4<f32>, 5>,
    cam_int: vec4<i32>,
    cam_frac: vec4<f32>,
    params: vec4<u32>,
    sun: vec4<f32>,
    inv_view_proj: mat4x4<f32>,
    // rgb, strength
    fog_color: vec4<f32>,
    // environmental start/end, render-distance start/end
    fog: vec4<f32>,
    // rgb, sky fog end distance
    sky_color: vec4<f32>,
    // directional shade: down, up, north, south
    shade_a: vec4<f32>,
    // west, east
    shade_b: vec4<f32>,
    // viewport width, height, depth pyramid levels, near plane
    cull: vec4<f32>,
    // seconds since start, section fade-in seconds
    fade: vec4<f32>,
}

struct SectionMeta {
    origin: vec4<i32>,
    bases: vec4<u32>,
    counts_a: vec4<u32>,
    counts_b: vec4<u32>,
}

struct DrawData {
    origin: vec3<f32>,
    first_quad: u32,
    tint_base: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
}

struct DrawIndexedIndirect {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}
