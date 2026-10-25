@group(0) @binding(0) var<storage, read> cube_in: array<vec4<u32>>;
@group(0) @binding(1) var<storage, read> model_quads: array<ModelQuad>;
@group(0) @binding(2) var<storage, read_write> cube_out: array<array<u32, 20>>;
@group(0) @binding(3) var<storage, read_write> model_out: array<array<i32, 40>>;

@compute @workgroup_size(1)
fn decode(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i < arrayLength(&cube_in) {
        let d = decode_cube(cube_in[i]);
        cube_out[i] = array<u32, 20>(d.pos.x, d.pos.y, d.pos.z, d.face, d.width, d.height, d.layer,
            d.ao.x, d.ao.y, d.ao.z, d.ao.w, d.light.x, d.light.y, d.light.z, d.light.w, d.tint, d.rotation,
            0u, 0u, 0u);
    }
    if i < arrayLength(&model_quads) {
        let q = i;
        var o: array<i32, 40>;
        for (var c = 0u; c < 4u; c++) {
            let p = decode_model_corner(q, c);
            o[c * 3u] = bitcast<i32>(p.x);
            o[c * 3u + 1u] = bitcast<i32>(p.y);
            o[c * 3u + 2u] = bitcast<i32>(p.z);
            let uv = decode_model_uv(q, c);
            o[12u + c * 2u] = bitcast<i32>(uv.x);
            o[13u + c * 2u] = bitcast<i32>(uv.y);
            o[24u + c] = i32(decode_model_light(q, c));
            o[28u + c] = i32(decode_model_ao(q, c));
        }
        o[20] = i32(decode_model_layer(q));
        o[21] = i32(decode_model_shade_face(q));
        o[22] = i32(decode_model_flags(q));
        o[23] = i32(decode_model_tint(q));
        let cell = model_cell(q);
        o[32] = i32(cell.x);
        o[33] = i32(cell.y);
        o[34] = i32(cell.z);
        model_out[i] = o;
    }
}
