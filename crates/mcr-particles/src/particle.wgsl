struct Uniforms {
    view_proj: mat4x4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    fog_color: vec4<f32>,
    fog: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;

struct Instance {
    @location(0) pos_size: vec4<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) roll: vec4<f32>,
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // Spherical and cylindrical distance, as vanilla's particle fog.
    @location(2) fog_distance: vec2<f32>,
}

@vertex
fn vs(@builtin(vertex_index) vi: u32, inst: Instance) -> Out {
    var corners = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, 1.0));
    let c = corners[vi];
    let cs = cos(inst.roll.x);
    let sn = sin(inst.roll.x);
    let rotated = vec2<f32>(c.x * cs - c.y * sn, c.x * sn + c.y * cs);
    let q = inst.roll.yzw;
    var offset = u.right.xyz * rotated.x + u.up.xyz * rotated.y;
    if q.x > 1.5 {
        // Turns about the vertical axis only; the camera's right vector is always horizontal.
        offset = u.right.xyz * rotated.x + vec3<f32>(0.0, 1.0, 0.0) * rotated.y;
    } else if dot(q, q) > 0.0 {
        let w = sqrt(max(0.0, 1.0 - dot(q, q)));
        let v = vec3<f32>(c.x, c.y, 0.0);
        offset = v + 2.0 * cross(q, cross(q, v) + w * v);
    }
    let world = inst.pos_size.xyz + offset * inst.pos_size.w;
    var o: Out;
    o.clip = u.view_proj * vec4<f32>(world, 1.0);
    let t = vec2<f32>((c.x + 1.0) * 0.5, 1.0 - (c.y + 1.0) * 0.5);
    o.uv = mix(inst.uv.xy, inst.uv.zw, t);
    o.color = inst.color;
    o.fog_distance = vec2<f32>(length(world), max(length(world.xz), abs(world.y)));
    return o;
}

fn linear_fog(d: f32, start: f32, end: f32) -> f32 {
    if d <= start {
        return 0.0;
    }
    if d >= end {
        return 1.0;
    }
    return (d - start) / (end - start);
}

fn fogged(rgb: vec3<f32>, d: vec2<f32>) -> vec3<f32> {
    let f = max(linear_fog(d.x, u.fog.x, u.fog.y), linear_fog(d.y, u.fog.z, u.fog.w));
    return mix(rgb, u.fog_color.rgb, f * u.fog_color.a);
}

@fragment
fn fs_opaque(in: Out) -> @location(0) vec4<f32> {
    let t = textureSample(atlas, smp, in.uv);
    if t.a * in.color.a < 0.1 {
        discard;
    }
    return vec4<f32>(fogged(t.rgb * in.color.rgb, in.fog_distance), 1.0);
}

@fragment
fn fs_blend(in: Out) -> @location(0) vec4<f32> {
    let t = textureSample(atlas, smp, in.uv);
    if t.a * in.color.a < 0.1 {
        discard;
    }
    return vec4<f32>(fogged(t.rgb * in.color.rgb, in.fog_distance), t.a * in.color.a);
}
