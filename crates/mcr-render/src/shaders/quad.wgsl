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
    opaque: bool,
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
    d.opaque = (q.y >> 31u) != 0u;
    d.light = vec4<u32>(q.z & 0xffffu, q.z >> 16u, q.w & 0xffffu, q.w >> 16u);
    return d;
}

alias ModelQuad = array<u32, 24>;

// Model quads read straight from `model_quads` by quad index, so a corner loads only its own words.
fn model_cell(i: u32) -> vec3<f32> {
    let w20 = model_quads[i][20];
    let w23 = model_quads[i][23];
    return vec3<f32>(f32((w23 >> 24u) & 15u), f32(w20 >> 28u), f32(w23 >> 28u));
}

// Section-local block units.
fn decode_model_corner(i: u32, c: u32) -> vec3<f32> {
    let k = c * 3u;
    return vec3<f32>(bitcast<f32>(model_quads[i][k]), bitcast<f32>(model_quads[i][k + 1u]), bitcast<f32>(model_quads[i][k + 2u]));
}

fn decode_model_uv(i: u32, c: u32) -> vec2<f32> {
    let k = 12u + c * 2u;
    return vec2<f32>(bitcast<f32>(model_quads[i][k]), bitcast<f32>(model_quads[i][k + 1u]));
}

fn decode_model_layer(i: u32) -> u32 {
    return model_quads[i][20] & 0xffffu;
}

fn decode_model_shade_face(i: u32) -> u32 {
    return (model_quads[i][20] >> 16u) & 7u;
}

fn decode_model_flags(i: u32) -> u32 {
    return (model_quads[i][20] >> 19u) & 7u;
}

fn decode_model_tint(i: u32) -> u32 {
    return (model_quads[i][20] >> 22u) & 63u;
}

fn decode_model_light(i: u32, c: u32) -> u32 {
    return (model_quads[i][21u + (c >> 1u)] >> (16u * (c & 1u))) & 0xffffu;
}

fn decode_model_ao(i: u32, c: u32) -> u32 {
    return (model_quads[i][23] >> (6u * c)) & 63u;
}
