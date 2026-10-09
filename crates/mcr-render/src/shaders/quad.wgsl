struct CubeQuad {
    pos: vec3<u32>,
    face: u32,
    width: u32,
    height: u32,
    layer: u32,
    ao: vec4<u32>,
    light: u32,
    tint: u32,
}

fn decode_cube(q: vec2<u32>) -> CubeQuad {
    var d: CubeQuad;
    d.pos = vec3<u32>(q.x & 31u, (q.x >> 5u) & 31u, (q.x >> 10u) & 31u);
    d.face = (q.x >> 15u) & 7u;
    d.width = ((q.x >> 18u) & 15u) + 1u;
    d.height = ((q.x >> 22u) & 15u) + 1u;
    d.layer = q.y & 0xffffu;
    d.ao = vec4<u32>((q.y >> 16u) & 3u, (q.y >> 18u) & 3u, (q.y >> 20u) & 3u, (q.y >> 22u) & 3u);
    d.light = q.y >> 24u;
    d.tint = q.x >> 26u;
    return d;
}

// Block units; model corner words hold 1/32 block steps biased by -1 block.
fn decode_model_corner(w: u32) -> vec3<f32> {
    let bits = vec3<u32>(w & 1023u, (w >> 10u) & 1023u, (w >> 20u) & 1023u);
    return vec3<f32>(bits) / 32.0 - vec3<f32>(1.0);
}

fn decode_model_uv(words: vec2<u32>, corner: u32) -> vec2<f32> {
    let p = words[corner >> 1u] >> (16u * (corner & 1u));
    return vec2<f32>(f32(p & 255u), f32((p >> 8u) & 255u)) / 16.0;
}
