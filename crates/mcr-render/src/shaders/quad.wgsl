struct CubeQuad {
    pos: vec3<u32>,
    face: u32,
    width: u32,
    height: u32,
    layer: u32,
    ao: vec4<u32>,
    light: vec4<u32>,
    tint: u32,
    rotation: u32,
}

fn decode_cube(q: vec4<u32>) -> CubeQuad {
    var d: CubeQuad;
    d.pos = vec3<u32>(q.x & 31u, (q.x >> 5u) & 31u, (q.x >> 10u) & 31u);
    d.face = (q.x >> 15u) & 7u;
    d.width = ((q.x >> 18u) & 15u) + 1u;
    d.height = ((q.x >> 22u) & 15u) + 1u;
    d.tint = q.x >> 26u;
    d.layer = q.y & 0xffffu;
    d.ao = vec4<u32>((q.y >> 16u) & 7u, (q.y >> 19u) & 7u, (q.y >> 22u) & 7u, (q.y >> 25u) & 7u);
    d.rotation = (q.y >> 28u) & 7u;
    d.light = vec4<u32>(q.z & 0xffffu, q.z >> 16u, q.w & 0xffffu, q.w >> 16u);
    return d;
}

alias ModelQuad = array<u32, 10>;

fn model_cell(q: ModelQuad) -> vec3<f32> {
    return vec3<f32>(f32((q[0] >> 27u) & 15u), f32((q[1] >> 27u) & 15u), f32((q[2] >> 27u) & 15u));
}

// Block units inside the section: base cell plus 1/128 block offsets biased by -1 block.
fn decode_model_corner(q: ModelQuad, c: u32) -> vec3<f32> {
    let w = q[c];
    let bits = vec3<u32>(w & 511u, (w >> 9u) & 511u, (w >> 18u) & 511u);
    return vec3<f32>(bits) / 128.0 - vec3<f32>(1.0) + model_cell(q);
}

fn decode_model_uv(q: ModelQuad, c: u32) -> vec2<f32> {
    let p = q[4u + (c >> 1u)] >> (16u * (c & 1u));
    return vec2<f32>(f32(p & 255u), f32((p >> 8u) & 255u)) / 16.0;
}

fn decode_model_layer(q: ModelQuad) -> u32 {
    return q[6] & 0xffffu;
}

fn decode_model_shade_face(q: ModelQuad) -> u32 {
    return (q[6] >> 16u) & 7u;
}

fn decode_model_flags(q: ModelQuad) -> u32 {
    return (q[6] >> 19u) & 31u;
}

fn decode_model_tint(q: ModelQuad) -> u32 {
    return (q[6] >> 24u) & 63u;
}

fn decode_model_light(q: ModelQuad, c: u32) -> u32 {
    return (q[7u + (c >> 1u)] >> (16u * (c & 1u))) & 0xffffu;
}

fn decode_model_ao(q: ModelQuad, c: u32) -> u32 {
    return (q[9] >> (6u * c)) & 63u;
}
