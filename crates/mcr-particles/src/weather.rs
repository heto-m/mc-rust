//! Rain splash particles and rain sound cues, ticked once per client tick while it rains.

use mcr_render::glam::DVec3;
use mcr_render::{MAX_WEATHER_RADIUS, MIN_WEATHER_RADIUS, Precipitation, WeatherSource};

use crate::rng::Rng;
use crate::{Fluid, ParticleEngine, ParticleWorld};

/// A rain loop sound to play at `pos`; `above` selects the muffled "rain above" variant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RainSound {
    pub pos: [i32; 3],
    pub above: bool,
}

#[derive(Debug)]
pub struct RainTicker {
    rng: Rng,
    sound_time: u32,
}

impl Default for RainTicker {
    fn default() -> Self {
        Self {
            rng: Rng::new(0x1234_5678_9abc_def1),
            sound_time: 0,
        }
    }
}

impl RainTicker {
    /// Spawns rain (or smoke over lava) particles on the tops of columns near the camera and returns a
    /// sound cue when one is due. `rain_level` is the vanilla rain level; `radius` the weather radius.
    pub fn tick(
        &mut self,
        engine: &mut ParticleEngine,
        camera: DVec3,
        rain_level: f32,
        radius: u32,
        source: &dyn WeatherSource,
        world: &dyn ParticleWorld,
    ) -> Option<RainSound> {
        if !(rain_level.is_finite() && rain_level > 0.0) || !camera.is_finite() {
            return None;
        }
        let rain_level = rain_level.min(1.0);
        let radius = radius.clamp(MIN_WEATHER_RADIUS, MAX_WEATHER_RADIUS) as i32;
        let diameter = 2 * radius + 1;
        let count = (0.225 * (diameter * diameter) as f32 * rain_level * rain_level) as i32;
        let cam = camera
            .floor()
            .clamp(DVec3::splat(-3.0e7), DVec3::splat(3.0e7));
        let (cx, cy, cz) = (cam.x as i32, cam.y as i32, cam.z as i32);
        let mut last = None;
        for _ in 0..count {
            let x = cx + self.rng.below(diameter as u32) as i32 - radius;
            let z = cz + self.rng.below(diameter as u32) as i32 - radius;
            let y = source.height(x, z);
            if y <= -2048 || y > cy + 10 || y < cy - 10 {
                continue;
            }
            if source.precipitation(x, y, z) != Precipitation::Rain {
                continue;
            }
            let below = [x, y - 1, z];
            last = Some(below);
            let (fx, fz) = (self.rng.f64(), self.rng.f64());
            let mut boxes = Vec::new();
            world.collision_boxes(below[0], below[1], below[2], &mut boxes);
            let block_top = boxes
                .iter()
                .filter(|b| {
                    fx as f32 >= b[0] && fx as f32 <= b[3] && fz as f32 >= b[2] && fz as f32 <= b[5]
                })
                .map(|b| b[4])
                .fold(0.0f32, f32::max);
            let fluid = world.fluid(below[0], below[1], below[2]);
            let top = fluid.map_or(0.0, |(_, h)| h).max(block_top);
            let name = if matches!(fluid, Some((Fluid::Lava, _))) {
                "smoke"
            } else {
                "rain"
            };
            engine.add_simple(
                name,
                DVec3::new(
                    f64::from(x) + fx,
                    f64::from(below[1]) + f64::from(top),
                    f64::from(z) + fz,
                ),
                DVec3::ZERO,
            );
        }
        let at = last?;
        let due = self.rng.below(3) < self.sound_time;
        self.sound_time += 1;
        if !due {
            return None;
        }
        self.sound_time = 0;
        let covered = source.height(cx, cz) > cy;
        Some(RainSound {
            pos: at,
            above: at[1] > cy + 1 && covered,
        })
    }
}
