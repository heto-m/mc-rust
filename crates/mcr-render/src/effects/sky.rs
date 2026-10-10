use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Vec3};

use super::random::JavaRandom;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct FxVertex {
    pub pos: [f32; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
}

impl FxVertex {
    pub(crate) fn new(pos: [f32; 3], uv: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            pos: [pos[0], pos[1], pos[2], 1.0],
            uv: [uv[0], uv[1], 0.0, 0.0],
            color,
        }
    }
}

const STAR_COUNT: u32 = 1500;
const STAR_SEED: i64 = 10842;
const SUNRISE_STEPS: u32 = 16;

/// Offsets into the static sky vertex buffer: triangle vertices first, then quads of four vertices.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SkyLayout {
    pub sunrise_vertices: u32,
    pub sun_quad: u32,
    pub moon_quad: u32,
    pub star_quad: u32,
    pub star_quads: u32,
    pub end_quad: u32,
}

pub(crate) fn build_static() -> (Vec<FxVertex>, SkyLayout) {
    let mut v = Vec::new();
    let white = [1.0; 4];
    let ring = |i: u32| {
        let a = i as f32 * std::f32::consts::TAU / SUNRISE_STEPS as f32;
        let (s, c) = a.sin_cos();
        FxVertex::new(
            [s * 120.0, c * 120.0, -c * 40.0],
            [0.0; 2],
            [1.0, 1.0, 1.0, 0.0],
        )
    };
    let center = FxVertex::new([0.0, 100.0, 0.0], [0.0; 2], white);
    for i in 0..SUNRISE_STEPS {
        v.extend([center, ring(i), ring(i + 1)]);
    }
    let sunrise_vertices = v.len() as u32;
    debug_assert_eq!(sunrise_vertices % 4, 0);

    let sun_quad = v.len() as u32 / 4;
    let corners = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
    for [x, z] in corners {
        v.push(FxVertex::new(
            [x, 0.0, z],
            [(x + 1.0) / 2.0, (z + 1.0) / 2.0],
            white,
        ));
    }
    let moon_quad = v.len() as u32 / 4;
    for [x, z] in corners {
        v.push(FxVertex::new(
            [x, 0.0, z],
            [1.0 - (x + 1.0) / 2.0, 1.0 - (z + 1.0) / 2.0],
            white,
        ));
    }

    let star_quad = v.len() as u32 / 4;
    let mut rng = JavaRandom::new(STAR_SEED);
    let mut star_quads = 0;
    for _ in 0..STAR_COUNT {
        let x = rng.next_f32() * 2.0 - 1.0;
        let y = rng.next_f32() * 2.0 - 1.0;
        let z = rng.next_f32() * 2.0 - 1.0;
        let size = 0.15 + rng.next_f32() * 0.1;
        let len_sq = x * x + y * y + z * z;
        if len_sq <= 0.010_000_001 || len_sq >= 1.0 {
            continue;
        }
        let center = Vec3::new(x, y, z).normalize() * 100.0;
        let roll = (rng.next_f64() * f64::from(std::f32::consts::PI) * 2.0) as f32;
        let forward = -center.normalize();
        let left = Vec3::Y.cross(forward).normalize_or_zero();
        let up = forward.cross(left);
        let rotation = Mat3::from_cols(left, up, forward) * Mat3::from_rotation_z(-roll);
        for (sx, sy) in [(1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0)] {
            let p = rotation * Vec3::new(sx * size, sy * size, 0.0) + center;
            v.push(FxVertex::new(p.to_array(), [0.0; 2], white));
        }
        star_quads += 1;
    }

    let end_quad = v.len() as u32 / 4;
    let shade = 40.0 / 255.0;
    for face in 0..6 {
        let pose = match face {
            1 => glam::Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2),
            2 => glam::Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            3 => glam::Mat4::from_rotation_x(std::f32::consts::PI),
            4 => glam::Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2),
            5 => glam::Mat4::from_rotation_z(-std::f32::consts::FRAC_PI_2),
            _ => glam::Mat4::IDENTITY,
        };
        for (p, uv) in [
            ([-100.0, -100.0, -100.0], [0.0, 0.0]),
            ([-100.0, -100.0, 100.0], [0.0, 16.0]),
            ([100.0, -100.0, 100.0], [16.0, 16.0]),
            ([100.0, -100.0, -100.0], [16.0, 0.0]),
        ] {
            let p = pose.transform_point3(Vec3::from(p));
            v.push(FxVertex::new(p.to_array(), uv, [shade, shade, shade, 1.0]));
        }
    }
    (
        v,
        SkyLayout {
            sunrise_vertices,
            sun_quad,
            moon_quad,
            star_quad,
            star_quads,
            end_quad,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_field_is_deterministic_and_bounded() {
        let (a, la) = build_static();
        let (b, lb) = build_static();
        assert_eq!(a.len(), b.len());
        assert_eq!(la.star_quads, lb.star_quads);
        assert!(la.star_quads > 600 && la.star_quads <= STAR_COUNT);
        assert!(a.iter().all(|v| v.pos.iter().all(|c| c.is_finite())));
    }
}
