//! Dynamic light sources from mods (held torches, burning mobs) lighting the terrain in the shader.

use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Vec3};

use crate::camera::Frustum;

/// Sources the terrain shader evaluates per frame.
pub const MAX_GPU_DYNAMIC_LIGHTS: usize = 64;
/// Blocks at which a level 15 source fades to nothing; LambDynamicLights' falloff.
pub const DYNAMIC_LIGHT_RADIUS: f32 = 7.75;
/// The light grid: `GRID`³ camera-relative cells of `GRID_CELL` blocks, each with a bit per source
/// that reaches into it, so a fragment only evaluates the sources near it.
pub(crate) const GRID: usize = 16;
pub(crate) const GRID_CELLS: usize = GRID * GRID * GRID;
const GRID_CELL: f32 = 16.0;
const GRID_HALF: f32 = GRID as f32 * GRID_CELL / 2.0;

const _: () = assert!(
    MAX_GPU_DYNAMIC_LIGHTS == 64,
    "the grid holds two mask words per cell"
);

/// A point (`start == end`) or segment light in world space with a block light level of 0–15.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicLight {
    pub start: DVec3,
    pub end: DVec3,
    pub level: f32,
}

impl DynamicLight {
    pub fn point(at: DVec3, level: f32) -> Self {
        Self {
            start: at,
            end: at,
            level,
        }
    }
}

fn reach(level: f32) -> f32 {
    level * DYNAMIC_LIGHT_RADIUS / 15.0
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuLight {
    /// Camera-relative start, level in `w`.
    start: [f32; 4],
    end: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuLights {
    count: [u32; 4],
    lights: [GpuLight; MAX_GPU_DYNAMIC_LIGHTS],
}

impl GpuLights {
    pub fn count(&self) -> usize {
        self.count[0] as usize
    }
}

impl GpuLight {
    /// Camera-relative box the light reaches.
    fn bounds(&self) -> (Vec3, Vec3) {
        let a = Vec3::from_slice(&self.start[..3]);
        let b = Vec3::from_slice(&self.end[..3]);
        let r = Vec3::splat(reach(self.start[3]));
        (a.min(b) - r, a.max(b) + r)
    }
}

fn grid_range(min: Vec3, max: Vec3) -> Option<([usize; 3], [usize; 3])> {
    if min.cmpge(Vec3::splat(GRID_HALF)).any() || max.cmplt(Vec3::splat(-GRID_HALF)).any() {
        return None;
    }
    let cell = |v: Vec3| {
        ((v + GRID_HALF) / GRID_CELL)
            .floor()
            .clamp(Vec3::ZERO, Vec3::splat(GRID as f32 - 1.0))
            .to_array()
            .map(|c| c as usize)
    };
    Some((cell(min), cell(max)))
}

impl Default for GpuLights {
    fn default() -> Self {
        Self::zeroed()
    }
}

fn segment_distance(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// The sources whose reach touches the frustum and the grid, nearest to the camera first,
/// camera-relative, and the grid's masks of them.
pub(crate) fn select(
    lights: &[DynamicLight],
    camera: DVec3,
    frustum: &Frustum,
    scratch: &mut Vec<(f32, GpuLight)>,
    out: &mut GpuLights,
    grid: &mut [[u32; 2]],
) {
    scratch.clear();
    for l in lights {
        if l.level.is_nan() || l.level <= 0.0 {
            continue;
        }
        let level = l.level.min(15.0);
        let a = (l.start - camera).as_vec3();
        let b = (l.end - camera).as_vec3();
        if !a.is_finite() || !b.is_finite() {
            continue;
        }
        let r = Vec3::splat(reach(level));
        let (min, max) = (a.min(b) - r, a.max(b) + r);
        if !frustum.intersects_aabb(min, max) || grid_range(min, max).is_none() {
            continue;
        }
        let distance = segment_distance(Vec3::ZERO, a, b);
        scratch.push((
            distance,
            GpuLight {
                start: [a.x, a.y, a.z, level],
                end: [b.x, b.y, b.z, 0.0],
            },
        ));
    }
    let keep = scratch.len().min(MAX_GPU_DYNAMIC_LIGHTS);
    if scratch.len() > keep {
        scratch.select_nth_unstable_by(keep, |x, y| x.0.total_cmp(&y.0));
    }
    grid.fill([0; 2]);
    for (i, (dst, (_, src))) in out.lights.iter_mut().zip(&scratch[..keep]).enumerate() {
        *dst = *src;
        let (min, max) = src.bounds();
        let Some((lo, hi)) = grid_range(min, max) else {
            continue;
        };
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                for x in lo[0]..=hi[0] {
                    if let Some(cell) = grid.get_mut(x + z * GRID + y * GRID * GRID) {
                        cell[i / 32] |= 1 << (i % 32);
                    }
                }
            }
        }
    }
    out.count = [keep as u32, 0, 0, 0];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;

    fn frustum() -> (Camera, Frustum) {
        let cam = Camera {
            position: DVec3::new(100.5, 70.0, -40.25),
            ..Camera::default()
        };
        let f = cam.frustum(16.0 / 9.0);
        (cam, f)
    }

    fn run_grid(lights: &[DynamicLight]) -> (GpuLights, Vec<[u32; 2]>) {
        let (cam, f) = frustum();
        let mut out = GpuLights::default();
        let mut grid = vec![[u32::MAX; 2]; GRID_CELLS];
        select(
            lights,
            cam.position,
            &f,
            &mut Vec::new(),
            &mut out,
            &mut grid,
        );
        (out, grid)
    }

    fn run(lights: &[DynamicLight]) -> GpuLights {
        run_grid(lights).0
    }

    #[test]
    fn grid_marks_the_cells_a_light_reaches() {
        let (cam, _) = frustum();
        let (out, grid) = run_grid(&[DynamicLight::point(
            cam.position + DVec3::new(0.0, 0.0, -20.0),
            15.0,
        )]);
        assert_eq!(out.count(), 1);
        let at = |p: Vec3| {
            let c = ((p + GRID_HALF) / GRID_CELL).floor().as_uvec3();
            grid[c.x as usize + c.z as usize * GRID + c.y as usize * GRID * GRID]
        };
        assert_eq!(at(Vec3::new(0.0, 0.0, -20.0)), [1, 0]);
        assert_eq!(at(Vec3::new(7.0, -7.0, -27.0)), [1, 0]);
        assert_eq!(at(Vec3::new(0.0, 0.0, 20.0)), [0, 0]);
        let marked = grid.iter().filter(|c| c[0] != 0).count();
        assert!((1..=8).contains(&marked), "{marked}");
    }

    #[test]
    fn sources_off_the_grid_are_dropped() {
        let (cam, _) = frustum();
        let out = run(&[DynamicLight::point(
            cam.position + DVec3::new(0.0, 0.0, -200.0),
            15.0,
        )]);
        assert_eq!(out.count(), 0);
    }

    #[test]
    fn camera_relative_and_level_in_w() {
        let (cam, _) = frustum();
        let out = run(&[DynamicLight::point(
            cam.position + DVec3::new(1.0, -1.5, -3.0),
            14.0,
        )]);
        assert_eq!(out.count(), 1);
        assert_eq!(out.lights[0].start, [1.0, -1.5, -3.0, 14.0]);
        assert_eq!(out.lights[0].end, [1.0, -1.5, -3.0, 0.0]);
    }

    #[test]
    fn drops_dark_invalid_and_out_of_view_sources() {
        let (cam, _) = frustum();
        let p = cam.position;
        let out = run(&[
            DynamicLight::point(p + DVec3::new(0.0, 0.0, -2.0), 0.0),
            DynamicLight::point(p + DVec3::new(0.0, 0.0, -2.0), f32::NAN),
            DynamicLight::point(DVec3::new(f64::NAN, 0.0, 0.0), 15.0),
            DynamicLight::point(p + DVec3::new(0.0, 0.0, 40.0), 15.0),
            DynamicLight::point(p + DVec3::new(0.0, 0.0, 5.0), 15.0),
        ]);
        assert_eq!(out.count(), 1, "the one behind but within reach stays");
        assert_eq!(out.lights[0].start[2], 5.0);
    }

    #[test]
    fn keeps_the_nearest_and_clamps_level() {
        let (cam, _) = frustum();
        let lights: Vec<_> = (0..200)
            .map(|i| {
                DynamicLight::point(
                    cam.position + DVec3::new(0.0, 0.0, -1.0 - f64::from(i) * 0.5),
                    99.0,
                )
            })
            .collect();
        let out = run(&lights);
        assert_eq!(out.count(), MAX_GPU_DYNAMIC_LIGHTS);
        let mut z: Vec<f32> = out.lights[..out.count()]
            .iter()
            .map(|l| -l.start[2])
            .collect();
        z.sort_by(f32::total_cmp);
        assert_eq!(z[0], 1.0);
        assert_eq!(z[MAX_GPU_DYNAMIC_LIGHTS - 1], 1.0 + 0.5 * 63.0);
        assert!(out.lights[..out.count()].iter().all(|l| l.start[3] == 15.0));
    }

    #[test]
    fn segment_distance_is_to_the_nearest_point() {
        let a = Vec3::new(-5.0, 0.0, 0.0);
        let b = Vec3::new(5.0, 0.0, 0.0);
        let near = |p: Vec3, a: Vec3, b: Vec3, want: f32| {
            let got = segment_distance(p, a, b);
            assert!((got - want).abs() < 1e-5, "{got} != {want}");
        };
        near(Vec3::new(0.0, 3.0, 0.0), a, b, 3.0);
        near(Vec3::new(8.0, 4.0, 0.0), a, b, 5.0);
        near(Vec3::new(1.0, 2.0, 0.0), a, a, 40f32.sqrt());
    }
}
