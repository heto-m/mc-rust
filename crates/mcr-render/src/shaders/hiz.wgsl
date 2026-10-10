// Each texel keeps the farthest depth of its 2x2 source footprint; reverse-Z makes that the minimum.
// Odd edges clamp to the last source texel, so a level-m texel covers exactly 2^(m+1) depth pixels.

@group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var dst: texture_storage_2d<r32float, write>;
@group(0) @binding(2) var src: texture_2d<f32>;

@compute @workgroup_size(8, 8)
fn from_depth(@builtin(global_invocation_id) gid: vec3<u32>) {
    let out = textureDimensions(dst);
    if gid.x >= out.x || gid.y >= out.y {
        return;
    }
    let last = vec2<i32>(textureDimensions(depth)) - 1;
    let p = vec2<i32>(gid.xy) * 2;
    let d = min(
        min(textureLoad(depth, min(p, last), 0), textureLoad(depth, min(p + vec2<i32>(1, 0), last), 0)),
        min(textureLoad(depth, min(p + vec2<i32>(0, 1), last), 0), textureLoad(depth, min(p + 1, last), 0)),
    );
    textureStore(dst, gid.xy, vec4<f32>(d, 0.0, 0.0, 0.0));
}

@compute @workgroup_size(8, 8)
fn downsample(@builtin(global_invocation_id) gid: vec3<u32>) {
    let out = textureDimensions(dst);
    if gid.x >= out.x || gid.y >= out.y {
        return;
    }
    let last = vec2<i32>(textureDimensions(src)) - 1;
    let p = vec2<i32>(gid.xy) * 2;
    let d = min(
        min(textureLoad(src, min(p, last), 0).x, textureLoad(src, min(p + vec2<i32>(1, 0), last), 0).x),
        min(textureLoad(src, min(p + vec2<i32>(0, 1), last), 0).x, textureLoad(src, min(p + 1, last), 0).x),
    );
    textureStore(dst, gid.xy, vec4<f32>(d, 0.0, 0.0, 0.0));
}
