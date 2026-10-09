@group(0) @binding(0) var<storage, read> cube_in: array<vec2<u32>>;
@group(0) @binding(1) var<storage, read> model_in: array<array<u32, 8>>;
@group(0) @binding(2) var<storage, read_write> cube_out: array<array<u32, 12>>;
@group(0) @binding(3) var<storage, read_write> model_out: array<array<i32, 32>>;

@compute @workgroup_size(1)
fn decode(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i < arrayLength(&cube_in) {
        let d = decode_cube(cube_in[i]);
        cube_out[i] = array<u32, 12>(d.pos.x, d.pos.y, d.pos.z, d.face, d.width, d.height, d.layer,
            d.ao.x, d.ao.y, d.ao.z, d.ao.w, d.light);
    }
    if i < arrayLength(&model_in) {
        let q = model_in[i];
        var o: array<i32, 32>;
        for (var c = 0u; c < 4u; c++) {
            let p = decode_model_corner(q[c]) * 32.0;
            o[c * 3u] = i32(round(p.x));
            o[c * 3u + 1u] = i32(round(p.y));
            o[c * 3u + 2u] = i32(round(p.z));
            let uv = decode_model_uv(vec2<u32>(q[4], q[5]), c) * 16.0;
            o[12u + c * 2u] = i32(round(uv.x));
            o[13u + c * 2u] = i32(round(uv.y));
        }
        o[20] = i32(q[6] & 0xffffu);
        o[21] = i32((q[6] >> 16u) & 255u);
        o[22] = i32((q[6] >> 24u) & 7u);
        o[23] = i32(q[6] >> 27u);
        for (var c = 0u; c < 4u; c++) {
            o[24u + c] = i32((q[7] >> (2u * c)) & 3u);
        }
        model_out[i] = o;
    }
}
