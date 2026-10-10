use super::random::JavaRandom;
use super::sky::FxVertex;

const HEIGHT_LIMIT: i32 = 1 << 20;
pub const MAX_WEATHER_RADIUS: u32 = 10;
pub const MIN_WEATHER_RADIUS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precipitation {
    None,
    Rain,
    Snow,
}

impl Precipitation {
    /// Vanilla's rule once a biome has precipitation: cold enough (adjusted temperature below 0.15)
    /// means snow.
    pub fn from_temperature(has_precipitation: bool, temperature: f32) -> Self {
        match (has_precipitation, temperature < 0.15) {
            (false, _) => Self::None,
            (true, true) => Self::Snow,
            (true, false) => Self::Rain,
        }
    }
}

/// World queries for precipitation. Called for each column around the camera every frame, so they must
/// be cheap lookups.
pub trait WeatherSource {
    /// Top of the motion-blocking heightmap (the first free y above the highest blocking block).
    fn height(&self, x: i32, z: i32) -> i32;
    fn precipitation(&self, x: i32, y: i32, z: i32) -> Precipitation;
    /// Packed `(block, sky)` light levels at a position.
    fn light(&self, _x: i32, _y: i32, _z: i32) -> (u8, u8) {
        (0, 15)
    }
}

pub(crate) struct WeatherInput<'a> {
    pub camera: [f64; 3],
    pub game_time: u64,
    pub partial: f32,
    pub radius: u32,
    pub intensity: f32,
    pub lightmap: &'a [[f32; 4]],
}

pub(crate) struct Columns {
    pub quads: Vec<FxVertex>,
    pub rain_quads: u32,
    pub snow_quads: u32,
}

struct Column {
    x: i32,
    z: i32,
    bottom: i32,
    top: i32,
    u: f32,
    v: f32,
    light: [f32; 3],
}

pub(crate) fn build(input: &WeatherInput, src: &dyn WeatherSource) -> Columns {
    let radius = input.radius.clamp(MIN_WEATHER_RADIUS, MAX_WEATHER_RADIUS) as i32;
    let camera = input.camera.map(|c| {
        if c.is_finite() {
            c.clamp(-3.0e7, 3.0e7)
        } else {
            0.0
        }
    });
    let [cx, cy, cz] = camera;
    let (bx, by, bz) = (cx.floor() as i32, cy.floor() as i32, cz.floor() as i32);
    let wrapped = (input.game_time & 131_071) as i32;
    let partial = if input.partial.is_finite() {
        input.partial.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let intensity = if input.intensity.is_finite() {
        input.intensity.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let time = wrapped as f32 + partial;
    let mut rain = Vec::new();
    let mut snow = Vec::new();
    for z in bz - radius..=bz + radius {
        for x in bx - radius..=bx + radius {
            let terrain = src.height(x, z).clamp(-HEIGHT_LIMIT, HEIGHT_LIMIT);
            let y0 = (by - radius).max(terrain);
            let y1 = (by + radius).max(terrain);
            if y1 == y0 {
                continue;
            }
            let kind = src.precipitation(x, by, z);
            if kind == Precipitation::None {
                continue;
            }
            let seed = (x
                .wrapping_mul(x)
                .wrapping_mul(3121)
                .wrapping_add(x.wrapping_mul(45_238_971)))
                ^ (z.wrapping_mul(z)
                    .wrapping_mul(418_711)
                    .wrapping_add(z.wrapping_mul(13_761)));
            let mut rng = JavaRandom::new(i64::from(seed));
            let (block, sky) = src.light(x, by.max(terrain), z);
            let (block, sky) = (u32::from(block.min(15)), u32::from(sky.min(15)));
            let (u, v, light) = if kind == Precipitation::Rain {
                let tick_offset = (x
                    .wrapping_mul(x)
                    .wrapping_mul(3121)
                    .wrapping_add(x.wrapping_mul(45_238_971))
                    .wrapping_add(z.wrapping_mul(z).wrapping_mul(418_711))
                    .wrapping_add(z.wrapping_mul(13_761))
                    & 0xff) as f32;
                let speed = 3.0 + rng.next_f32();
                let offset = -(wrapped as f32 + tick_offset + partial) / 32.0 * speed;
                (0.0, offset % 32.0, (block, sky))
            } else {
                let u = (rng.next_f64() + f64::from(time * 0.01) * rng.next_gaussian()) as f32;
                let v = (rng.next_f64() + f64::from(time) * rng.next_gaussian() * 0.001) as f32;
                let v_off = -(((input.game_time & 511) as f32) + partial) / 512.0;
                (u, v + v_off, ((block * 3 + 15) / 4, (sky * 3 + 15) / 4))
            };
            let lm = input
                .lightmap
                .get((light.1 * 16 + light.0) as usize)
                .copied()
                .unwrap_or([1.0; 4]);
            let column = Column {
                x,
                z,
                bottom: y0,
                top: y1,
                u,
                v,
                light: [lm[0], lm[1], lm[2]],
            };
            if kind == Precipitation::Rain {
                rain.push(column);
            } else {
                snow.push(column);
            }
        }
    }
    let mut quads = Vec::with_capacity((rain.len() + snow.len()) * 4);
    let mut emit = |cols: &[Column], max_alpha: f32| {
        let radius_sq = (radius * radius) as f32;
        for c in cols {
            let rx = (f64::from(c.x) + 0.5 - cx) as f32;
            let rz = (f64::from(c.z) + 0.5 - cz) as f32;
            let d2 = rx * rx + rz * rz;
            let t = (d2 / radius_sq).min(1.0);
            let alpha = (max_alpha + (0.5 - max_alpha) * t) * intensity;
            let (dx, dz) = ((c.x - bx) as f32, (c.z - bz) as f32);
            let len = (dx * dx + dz * dz).sqrt();
            let (hx, hz) = if len < 1e-3 {
                (0.5, 0.0)
            } else {
                (-dz / len / 2.0, dx / len / 2.0)
            };
            let top = (f64::from(c.top) - cy) as f32;
            let bottom = (f64::from(c.bottom) - cy) as f32;
            let v0 = c.bottom as f32 * 0.25 + c.v;
            let v1 = c.top as f32 * 0.25 + c.v;
            let color = [c.light[0], c.light[1], c.light[2], alpha];
            quads.extend([
                FxVertex::new([rx - hx, top, rz - hz], [c.u, v0], color),
                FxVertex::new([rx + hx, top, rz + hz], [c.u + 1.0, v0], color),
                FxVertex::new([rx + hx, bottom, rz + hz], [c.u + 1.0, v1], color),
                FxVertex::new([rx - hx, bottom, rz - hz], [c.u, v1], color),
            ]);
        }
    };
    emit(&rain, 1.0);
    emit(&snow, 0.8);
    Columns {
        quads,
        rain_quads: rain.len() as u32,
        snow_quads: snow.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Flat(Precipitation);

    impl WeatherSource for Flat {
        fn height(&self, _: i32, _: i32) -> i32 {
            60
        }
        fn precipitation(&self, _: i32, _: i32, _: i32) -> Precipitation {
            self.0
        }
    }

    fn input(radius: u32) -> WeatherInput<'static> {
        WeatherInput {
            camera: [0.5, 64.0, 0.5],
            game_time: 100,
            partial: 0.5,
            radius,
            intensity: 1.0,
            lightmap: &[],
        }
    }

    #[test]
    fn radius_is_clamped() {
        let c = build(&input(99), &Flat(Precipitation::Rain));
        assert_eq!(c.rain_quads, 21 * 21);
        assert_eq!(c.snow_quads, 0);
        assert_eq!(c.quads.len(), 21 * 21 * 4);
    }

    #[test]
    fn snow_uses_snow_columns() {
        let c = build(&input(5), &Flat(Precipitation::Snow));
        assert_eq!(c.snow_quads, 11 * 11);
        assert_eq!(c.rain_quads, 0);
    }

    #[test]
    fn none_draws_nothing() {
        assert!(
            build(&input(5), &Flat(Precipitation::None))
                .quads
                .is_empty()
        );
    }

    #[test]
    fn temperature_threshold() {
        assert_eq!(
            Precipitation::from_temperature(true, 0.0),
            Precipitation::Snow
        );
        assert_eq!(
            Precipitation::from_temperature(true, 0.8),
            Precipitation::Rain
        );
        assert_eq!(
            Precipitation::from_temperature(false, 0.0),
            Precipitation::None
        );
    }
}

#[cfg(test)]
mod hostile_tests {
    use super::*;

    struct Wild;

    impl WeatherSource for Wild {
        fn height(&self, _: i32, _: i32) -> i32 {
            i32::MAX
        }
        fn precipitation(&self, _: i32, _: i32, _: i32) -> Precipitation {
            Precipitation::Rain
        }
        fn light(&self, _: i32, _: i32, _: i32) -> (u8, u8) {
            (255, 255)
        }
    }

    #[test]
    fn hostile_inputs_stay_bounded() {
        for camera in [[f64::NAN; 3], [f64::INFINITY, -1e300, f64::MAX]] {
            let c = build(
                &WeatherInput {
                    camera,
                    game_time: u64::MAX,
                    partial: f32::NAN,
                    radius: u32::MAX,
                    intensity: f32::INFINITY,
                    lightmap: &[],
                },
                &Wild,
            );
            assert!(c.quads.len() <= 21 * 21 * 4);
            assert!(c.quads.iter().all(|v| v.pos.iter().all(|p| p.is_finite())));
        }
    }
}
