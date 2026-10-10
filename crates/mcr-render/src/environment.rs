//! Lighting and atmosphere inputs: the lightmap, directional shade, sky and fog colours.

pub type Rgb = [f32; 3];

const DAY_TICKS: f32 = 24000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Skybox {
    #[default]
    Overworld,
    None,
    End,
}

/// What the shader needs to relight and fog existing geometry. The defaults are overworld noon.
#[derive(Debug, Clone, PartialEq)]
pub struct Environment {
    /// Multiplies sky light (`visual/sky_light_factor`): 1 by day, 0.24 at night in the overworld.
    pub sky_factor: f32,
    pub sky_light_color: Rgb,
    pub ambient_color: Rgb,
    pub block_light_tint: Rgb,
    /// 1.4 plus the block-light flicker.
    pub block_factor: f32,
    /// The brightness (gamma) option, 0 to 1.
    pub brightness: f32,
    pub night_vision: f32,
    pub night_vision_color: Rgb,
    pub darkness_scale: f32,
    pub boss_darkening: f32,
    /// Directional shade: down, up, north, south, west, east.
    pub cardinal: [f32; 6],
    pub fog_color: Rgb,
    pub sky_color: Rgb,
    /// Spherical environmental fog (vanilla: 0 and 1024 in the overworld, so effectively off).
    pub fog_start: f32,
    pub fog_end: f32,
    /// Render distance in blocks; fog fades over the last tenth (4 to 64 blocks).
    pub render_distance: f32,
    /// Where the sky disc is fully fogged.
    pub sky_fog_end: f32,
    pub skybox: Skybox,
    /// Radians; the sun is overhead at 0 and the moon opposite it.
    pub sun_angle: f32,
    pub moon_angle: f32,
    pub star_angle: f32,
    /// `MoonPhase` index: 0 full moon, 4 new moon.
    pub moon_phase: u8,
    pub star_brightness: f32,
    /// RGBA; alpha scales the horizon glow and is zero outside dawn and dusk.
    pub sunrise_color: [f32; 4],
    /// RGBA; alpha is the cloud opacity.
    pub cloud_color: [f32; 4],
    /// `None` hides clouds (nether, end).
    pub cloud_height: Option<f32>,
    pub rain_level: f32,
    pub thunder_level: f32,
    /// Eye height below which the dark void disc is drawn (sea level).
    pub horizon_height: f32,
}

pub const CARDINAL_DEFAULT: [f32; 6] = [0.5, 1.0, 0.8, 0.8, 0.6, 0.6];
pub const CARDINAL_NETHER: [f32; 6] = [0.9, 0.9, 0.8, 0.8, 0.6, 0.6];

impl Default for Environment {
    fn default() -> Self {
        Self {
            sky_factor: 1.0,
            sky_light_color: [1.0; 3],
            ambient_color: hex(0x0a0a0a),
            block_light_tint: hex(0xffd88c),
            block_factor: 1.4,
            brightness: 0.5,
            night_vision: 0.0,
            night_vision_color: hex(0x999999),
            darkness_scale: 0.0,
            boss_darkening: 0.0,
            cardinal: CARDINAL_DEFAULT,
            fog_color: hex(0xc0d8ff),
            sky_color: hex(0x78a7ff),
            fog_start: 0.0,
            fog_end: 1024.0,
            render_distance: 192.0,
            sky_fog_end: 192.0,
            skybox: Skybox::Overworld,
            sun_angle: 0.0,
            moon_angle: std::f32::consts::PI,
            star_angle: 0.0,
            moon_phase: 0,
            star_brightness: 0.0,
            sunrise_color: [1.0, 0.9, 0.2, 0.0],
            cloud_color: [1.0, 1.0, 1.0, 0.8],
            cloud_height: Some(192.33),
            rain_level: 0.0,
            thunder_level: 0.0,
            horizon_height: 63.0,
        }
    }
}

pub fn hex(rgb: u32) -> Rgb {
    [
        (rgb >> 16 & 255) as f32 / 255.0,
        (rgb >> 8 & 255) as f32 / 255.0,
        (rgb & 255) as f32 / 255.0,
    ]
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Base colours the day timeline multiplies; biomes override sky and fog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverworldBase {
    pub sky: Rgb,
    pub fog: Rgb,
    /// RGBA; alpha is the cloud opacity.
    pub cloud: [f32; 4],
    pub cloud_height: f32,
}

impl Default for OverworldBase {
    fn default() -> Self {
        Self {
            sky: hex(0x78a7ff),
            fog: hex(0xc0d8ff),
            cloud: [1.0, 1.0, 1.0, 0.8],
            cloud_height: 192.33,
        }
    }
}

const STAR_KEYS: &[(f32, [f32; 1])] = &[
    (92.0, [0.037]),
    (627.0, [0.0]),
    (11373.0, [0.0]),
    (11732.0, [0.016]),
    (11959.0, [0.044]),
    (12399.0, [0.143]),
    (12729.0, [0.258]),
    (13228.0, [0.5]),
    (22772.0, [0.5]),
    (23032.0, [0.364]),
    (23356.0, [0.225]),
    (23758.0, [0.101]),
];

/// `visual/sunrise_sunset_color` keyframes as (tick, [r, g, b, a]) bytes.
const SUNRISE_RAW: &[(f32, [u8; 4])] = &[
    (71.0, [0xef, 0xa3, 0x33, 0x5f]),
    (310.0, [0xf5, 0xba, 0x33, 0x29]),
    (565.0, [0xfb, 0xd4, 0x33, 0x06]),
    (730.0, [0xff, 0xe5, 0x33, 0x00]),
    (11270.0, [0xff, 0xe5, 0x33, 0x00]),
    (11397.0, [0xfc, 0xd8, 0x33, 0x04]),
    (11522.0, [0xf9, 0xcb, 0x33, 0x0f]),
    (11690.0, [0xf5, 0xba, 0x33, 0x29]),
    (11929.0, [0xef, 0xa3, 0x33, 0x5f]),
    (12243.0, [0xe7, 0x87, 0x33, 0xb1]),
    (12358.0, [0xe4, 0x7e, 0x33, 0xcc]),
    (12512.0, [0xe0, 0x72, 0x33, 0xe9]),
    (12613.0, [0xdd, 0x6b, 0x33, 0xf6]),
    (12732.0, [0xda, 0x63, 0x33, 0xfe]),
    (12841.0, [0xd7, 0x5c, 0x33, 0xfe]),
    (13035.0, [0xd2, 0x51, 0x33, 0xec]),
    (13252.0, [0xcc, 0x47, 0x33, 0xc1]),
    (13775.0, [0xbe, 0x37, 0x33, 0x36]),
    (13888.0, [0xbb, 0x35, 0x33, 0x1f]),
    (14039.0, [0xb7, 0x33, 0x33, 0x09]),
    (14192.0, [0xb3, 0x33, 0x33, 0x00]),
    (21807.0, [0xb2, 0x33, 0x33, 0x00]),
    (21961.0, [0xb7, 0x33, 0x33, 0x09]),
    (22112.0, [0xbb, 0x35, 0x33, 0x1f]),
    (22225.0, [0xbe, 0x37, 0x33, 0x36]),
    (22748.0, [0xcc, 0x47, 0x33, 0xc1]),
    (22965.0, [0xd2, 0x51, 0x33, 0xec]),
    (23159.0, [0xd7, 0x5c, 0x33, 0xfe]),
    (23272.0, [0xda, 0x63, 0x33, 0xfe]),
    (23488.0, [0xe0, 0x72, 0x33, 0xe9]),
    (23642.0, [0xe4, 0x7e, 0x33, 0xcc]),
    (23757.0, [0xe7, 0x87, 0x33, 0xb1]),
];

fn sunrise_keys() -> Vec<(f32, [f32; 4])> {
    SUNRISE_RAW
        .iter()
        .map(|&(t, c)| (t, c.map(|v| f32::from(v) / 255.0)))
        .collect()
}

fn blend_to_gray(c: Rgb, brightness: f32, factor: f32) -> Rgb {
    let gray = (c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11) * brightness;
    srgb_lerp(factor, c, [gray; 3])
}

/// Vanilla's `cubic_bezier [0.362, 0.241, 0.638, 0.759]` easing of the day cycle.
fn bezier_ease(x: f32) -> f32 {
    const P: [f32; 4] = [0.362, 0.241, 0.638, 0.759];
    let at = |t: f32, a: f32, b: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
    };
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if at(mid, P[0], P[2]) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    at((lo + hi) / 2.0, P[1], P[3])
}

fn finite_unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn mixf(t: f32, a: f32, b: f32) -> f32 {
    lerp(a, b, t)
}

fn lerp3(a: Rgb, b: Rgb, t: f32) -> Rgb {
    std::array::from_fn(|i| lerp(a[i], b[i], t))
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Lerp in linear light, like vanilla's `ARGB.srgbLerp`.
pub fn srgb_lerp(t: f32, a: Rgb, b: Rgb) -> Rgb {
    std::array::from_fn(|i| linear_to_srgb(lerp(srgb_to_linear(a[i]), srgb_to_linear(b[i]), t)))
}

/// Piecewise-linear value of a track that repeats every day. Keyframes are `(tick, value)`, ascending.
fn track<const N: usize>(keys: &[(f32, [f32; N])], time: f32) -> [f32; N] {
    let t = time.rem_euclid(DAY_TICKS);
    let last = keys.len() - 1;
    let (a, b, span, at) = if t < keys[0].0 {
        (
            last,
            0,
            keys[0].0 + DAY_TICKS - keys[last].0,
            t + DAY_TICKS - keys[last].0,
        )
    } else if t >= keys[last].0 {
        (
            last,
            0,
            keys[0].0 + DAY_TICKS - keys[last].0,
            t - keys[last].0,
        )
    } else {
        let i = keys.iter().rposition(|k| k.0 <= t).unwrap_or(0);
        (i, i + 1, keys[i + 1].0 - keys[i].0, t - keys[i].0)
    };
    let f = (at / span).clamp(0.0, 1.0);
    std::array::from_fn(|i| lerp(keys[a].1[i], keys[b].1[i], f))
}

impl Environment {
    /// The overworld at `day_time` ticks of the overworld clock (unbounded; the moon phase uses the day
    /// count), with the vanilla rain level and thunder level (already scaled by rain).
    pub fn overworld(day_time: u64, rain: f32, thunder: f32) -> Self {
        Self::overworld_with(day_time, rain, thunder, OverworldBase::default())
    }

    /// Like [`Self::overworld`] with biome base colours, which the day timeline multiplies.
    pub fn overworld_with(day_time: u64, rain: f32, thunder: f32, colors: OverworldBase) -> Self {
        let t = (day_time % 24000) as f32;
        let rain = finite_unit(rain);
        let thunder = finite_unit(thunder).min(rain);
        let night_fog = hex(0x0f0f16);
        let night_cloud = hex(0x191926);
        let day_night = |night: Rgb| {
            track(
                &[
                    (133.0, [1.0; 3]),
                    (11867.0, [1.0; 3]),
                    (13670.0, night),
                    (22330.0, night),
                ],
                t,
            )
        };
        let (fog_mul, sky_mul, cloud_mul) = (
            day_night(night_fog),
            day_night([0.0; 3]),
            day_night(night_cloud),
        );
        let mut factor = track(
            &[
                (730.0, [1.0]),
                (11270.0, [1.0]),
                (13140.0, [0.24]),
                (22860.0, [0.24]),
            ],
            t,
        )[0];
        let mut light = track(
            &[
                (730.0, [1.0; 3]),
                (11270.0, [1.0; 3]),
                (13140.0, hex(0x7a7aff)),
                (22860.0, hex(0x7a7aff)),
            ],
            t,
        );
        let mut star = track(STAR_KEYS, t)[0];
        let mut sunrise = track(&sunrise_keys(), t);
        let mut sky: Rgb = std::array::from_fn(|i| colors.sky[i] * sky_mul[i]);
        let mut fog: Rgb = std::array::from_fn(|i| colors.fog[i] * fog_mul[i]);
        let mut cloud: Rgb = std::array::from_fn(|i| colors.cloud[i] * cloud_mul[i]);

        struct Layer {
            level: f32,
            sky: (f32, f32),
            cloud: (f32, f32),
            light_alpha: f32,
            mul: Rgb,
        }
        let layers = [
            Layer {
                level: rain - thunder,
                sky: (0.6, 0.75),
                cloud: (0.24, 0.5),
                light_alpha: 0.3125,
                mul: [0.5, 0.5, 0.6],
            },
            Layer {
                level: thunder,
                sky: (0.24, 0.94),
                cloud: (0.095, 0.94),
                light_alpha: 0.527_343_75,
                mul: [0.25, 0.25, 0.3],
            },
        ];
        for l in layers.iter().filter(|l| l.level > 0.0) {
            sky = srgb_lerp(l.level, sky, blend_to_gray(sky, l.sky.0, l.sky.1));
            let fogged: Rgb = std::array::from_fn(|i| fog[i] * l.mul[i]);
            fog = srgb_lerp(l.level, fog, fogged);
            cloud = srgb_lerp(l.level, cloud, blend_to_gray(cloud, l.cloud.0, l.cloud.1));
            factor = mixf(l.level, factor, mixf(l.light_alpha, factor, 0.24));
            light = srgb_lerp(
                l.level,
                light,
                srgb_lerp(l.light_alpha, light, hex(0x7a7aff)),
            );
            star = mixf(l.level, star, 0.0);
            for (s, m) in sunrise.iter_mut().zip(l.mul) {
                *s = mixf(l.level, *s, *s * m);
            }
        }

        let day_deg =
            |offset: f32| offset + 360.0 * bezier_ease((t - 6000.0).rem_euclid(24000.0) / 24000.0);
        let mut env = Self {
            sky_factor: factor,
            sky_light_color: light,
            sky_color: sky,
            fog_color: fog,
            sun_angle: day_deg(0.0).to_radians(),
            moon_angle: day_deg(180.0).to_radians(),
            star_angle: day_deg(0.0).to_radians(),
            moon_phase: ((day_time / 24000) % 8) as u8,
            star_brightness: star,
            sunrise_color: sunrise,
            cloud_color: [cloud[0], cloud[1], cloud[2], colors.cloud[3]],
            cloud_height: Some(colors.cloud_height).filter(|h| h.is_finite()),
            rain_level: rain,
            thunder_level: thunder,
            ..Self::default()
        };
        env.mix_fog_with_sky();
        env
    }

    /// Lightning flash: the sky tints towards (0.8, 0.8, 1.0) and sky light jumps to full.
    pub fn with_lightning_flash(mut self, active: bool) -> Self {
        if active {
            self.sky_color = srgb_lerp(0.22, self.sky_color, [0.8, 0.8, 1.0]);
            self.sky_factor = 1.0;
        }
        self
    }

    pub fn end() -> Self {
        Self {
            sky_factor: 0.0,
            sky_light_color: hex(0xac60cd),
            ambient_color: hex(0x3f473f),
            fog_color: hex(0x181318),
            sky_color: [0.0; 3],
            skybox: Skybox::End,
            cloud_height: None,
            ..Self::default()
        }
    }

    pub fn nether() -> Self {
        Self {
            sky_factor: 0.0,
            sky_light_color: hex(0x7a7aff),
            ambient_color: hex(0x302821),
            cardinal: CARDINAL_NETHER,
            fog_color: hex(0x330808),
            sky_color: hex(0x330808),
            skybox: Skybox::None,
            cloud_height: None,
            fog_start: 10.0,
            fog_end: 96.0,
            ..Self::default()
        }
    }

    /// Blends the horizon fog colour towards the sky colour for short render distances.
    pub fn mix_fog_with_sky(&mut self) {
        let chunks = self.render_distance / 16.0;
        let sky_end = (self.sky_fog_end / 16.0).min(chunks);
        let mix = 1.0 - lerp(0.25, 1.0, (sky_end / 32.0).clamp(0.0, 1.0)).powf(0.25);
        self.fog_color = srgb_lerp(mix, self.fog_color, self.sky_color);
    }

    pub fn set_render_distance_chunks(&mut self, chunks: u32) {
        self.render_distance = chunks as f32 * 16.0;
        self.sky_fog_end = self.render_distance.min(512.0);
    }

    /// Cylindrical render-distance fog range (start, end).
    pub fn render_fog_range(&self) -> (f32, f32) {
        let span = (self.render_distance / 10.0).clamp(4.0, 64.0);
        (self.render_distance - span, self.render_distance)
    }

    /// The 16x16 lightmap, indexed `sky * 16 + block`, with `sky_scale` multiplying the sky factor.
    pub fn lightmap(&self, sky_scale: f32) -> Vec<[f32; 4]> {
        let brightness = |l: f32| l / (4.0 - 3.0 * l);
        let night_vision = self.night_vision_color.map(|c| c * self.night_vision);
        let mut out = Vec::with_capacity(256);
        for sky in 0..16 {
            for block in 0..16 {
                let (sky_level, block_level) = (sky as f32 / 15.0, block as f32 / 15.0);
                let sky_b = brightness(sky_level) * self.sky_factor * sky_scale;
                let block_b = brightness(block_level) * self.block_factor;
                let mut c: Rgb =
                    std::array::from_fn(|i| self.ambient_color[i].max(night_vision[i]));
                for (v, light) in c.iter_mut().zip(self.sky_light_color) {
                    *v += light * sky_b;
                }
                let parabola = (2.0 * block_level - 1.0) * (2.0 * block_level - 1.0);
                let block_color = lerp3(self.block_light_tint, [1.0; 3], 0.9 * parabola);
                for (v, light) in c.iter_mut().zip(block_color) {
                    *v += light * block_b;
                }
                c = lerp3(c, [c[0] * 0.7, c[1] * 0.6, c[2] * 0.6], self.boss_darkening);
                c = c.map(|v| (v - self.darkness_scale).clamp(0.0, 1.0));
                let max = c[0].max(c[1]).max(c[2]);
                if max > 0.0 {
                    let inverted = 1.0 - max;
                    let scaled = 1.0 - inverted.powi(4);
                    let not_gamma = c.map(|v| v * scaled / max);
                    c = lerp3(c, not_gamma, self.brightness);
                }
                out.push([c[0], c[1], c[2], 1.0]);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noon_is_bright_and_midnight_dark() {
        let noon = Environment::overworld(6000, 0.0, 0.0);
        let midnight = Environment::overworld(18000, 0.0, 0.0);
        assert_eq!(noon.sky_factor, 1.0);
        assert!((midnight.sky_factor - 0.24).abs() < 1e-6);
        assert_eq!(midnight.sky_color, [0.0; 3]);
        let day = noon.lightmap(1.0)[15 * 16];
        let night = midnight.lightmap(1.0)[15 * 16];
        assert!(day[0] > night[0] && day[2] > night[2]);
    }

    #[test]
    fn noon_sun_is_overhead_without_glow_or_stars() {
        let e = Environment::overworld(6000, 0.0, 0.0);
        assert!(e.sun_angle.abs() < 1e-4);
        assert!((e.moon_angle - std::f32::consts::PI).abs() < 1e-4);
        assert_eq!(e.star_brightness, 0.0);
        assert!(e.sunrise_color[3] < 0.01);
        let dusk = Environment::overworld(12500, 0.0, 0.0);
        assert!(dusk.sunrise_color[3] > 0.5 && dusk.star_brightness > 0.1);
    }

    #[test]
    fn moon_phase_advances_daily_and_wraps() {
        let phase = |d: u64| Environment::overworld(d * 24000 + 100, 0.0, 0.0).moon_phase;
        assert_eq!((phase(0), phase(1), phase(4), phase(8)), (0, 1, 4, 0));
    }

    #[test]
    fn rain_greys_sky_and_hides_stars() {
        let clear = Environment::overworld(18000, 0.0, 0.0);
        let rain = Environment::overworld(18000, 1.0, 0.0);
        assert!(clear.star_brightness > 0.4 && rain.star_brightness == 0.0);
        let day = Environment::overworld(6000, 0.0, 0.0);
        let wet = Environment::overworld(6000, 1.0, 0.0);
        assert!(wet.sky_color[2] < day.sky_color[2]);
        assert!(wet.cloud_color[0] < day.cloud_color[0]);
        assert!(wet.sky_factor < 1.0);
    }

    #[test]
    fn lightning_flash_brightens() {
        let night = Environment::overworld(18000, 1.0, 1.0);
        let flash = night.clone().with_lightning_flash(true);
        assert_eq!(flash.sky_factor, 1.0);
        assert!(flash.sky_color[2] > night.sky_color[2]);
    }

    #[test]
    fn dusk_interpolates_between_keyframes() {
        let dusk = Environment::overworld(12205, 0.0, 0.0);
        assert!(dusk.sky_factor < 1.0 && dusk.sky_factor > 0.24);
        let wrap = Environment::overworld(23500, 0.0, 0.0);
        assert!((wrap.sky_factor - 0.24).abs() < 1e-6 || wrap.sky_factor > 0.24);
    }

    #[test]
    fn block_light_keeps_brightness_at_night() {
        let night = Environment::overworld(18000, 0.0, 0.0).lightmap(1.0);
        let noon = Environment::overworld(6000, 0.0, 0.0).lightmap(1.0);
        let lit = |m: &[[f32; 4]]| m[15];
        assert_eq!(lit(&night), lit(&noon));
    }

    #[test]
    fn fully_dark_cell_is_ambient_only() {
        let env = Environment {
            brightness: 0.0,
            ..Environment::default()
        };
        let c = env.lightmap(1.0)[0];
        assert!((c[0] - 10.0 / 255.0).abs() < 1e-5);
    }

    #[test]
    fn render_fog_span_is_clamped() {
        let mut env = Environment::default();
        env.set_render_distance_chunks(12);
        assert_eq!(env.render_fog_range(), (192.0 - 19.2, 192.0));
        env.set_render_distance_chunks(2);
        assert_eq!(env.render_fog_range(), (28.0, 32.0));
    }
}

#[cfg(test)]
mod hostile_tests {
    use super::*;

    #[test]
    fn non_finite_levels_and_times_are_sanitised() {
        for (rain, thunder) in [
            (f32::NAN, f32::NAN),
            (f32::INFINITY, f32::NEG_INFINITY),
            (1e30, -1e30),
        ] {
            for time in [0, 6000, u64::MAX] {
                let e = Environment::overworld(time, rain, thunder);
                assert!((0.0..=1.0).contains(&e.rain_level));
                assert!(e.sky_color.iter().all(|c| c.is_finite()));
                assert!(e.sun_angle.is_finite() && e.moon_phase < 8);
                assert!(e.lightmap(1.0).iter().flatten().all(|c| c.is_finite()));
            }
        }
    }
}
