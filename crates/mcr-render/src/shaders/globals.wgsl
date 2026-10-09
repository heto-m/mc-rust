struct Globals {
    view_proj: mat4x4<f32>,
    planes: array<vec4<f32>, 5>,
    cam_int: vec4<i32>,
    cam_frac: vec4<f32>,
    params: vec4<u32>,
    sun: vec4<f32>,
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
}

struct DrawIndirect {
    vertex_count: u32,
    instance_count: u32,
    first_vertex: u32,
    first_instance: u32,
}
