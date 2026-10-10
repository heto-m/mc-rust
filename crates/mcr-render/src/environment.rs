//! Lighting and atmosphere inputs: the lightmap, directional shade, sky and fog colours.

pub type Rgb = [f32; 3];

const DAY_TICKS: f32 = 24000.0;

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
fn track<const N: usize>(keys: &[(f32, [f32; N]); 4], time: f32) -> [f32; N] {
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
    /// The overworld at `day_time` ticks (`gametime % 24000` for the default clock), with rain and thunder
    /// levels in 0..=1. Keyframes are the vanilla `day` timeline.
    pub fn overworld(day_time: u64, rain: f32, thunder: f32) -> Self {
        let t = (day_time % 24000) as f32;
        let light_factor = track(
            &[
                (730.0, [1.0]),
                (11270.0, [1.0]),
                (13140.0, [0.24]),
                (22860.0, [0.24]),
            ],
            t,
        )[0];
        let light_color = track(
            &[
                (730.0, [1.0, 1.0, 1.0]),
                (11270.0, [1.0, 1.0, 1.0]),
                (13140.0, hex(0x7a7aff)),
                (22860.0, hex(0x7a7aff)),
            ],
            t,
        );
        let night = hex(0x0f0f16);
        let fog_mul = track(
            &[
                (133.0, [1.0; 3]),
                (11867.0, [1.0; 3]),
                (13670.0, night),
                (22330.0, night),
            ],
            t,
        );
        let sky_mul = track(
            &[
                (133.0, [1.0; 3]),
                (11867.0, [1.0; 3]),
                (13670.0, [0.0; 3]),
                (22330.0, [0.0; 3]),
            ],
            t,
        );
        let base = Self::default();
        let mut sky: Rgb = std::array::from_fn(|i| base.sky_color[i] * sky_mul[i]);
        if rain > 0.0 {
            let m = 1.0 - rain * 0.5;
            sky = [sky[0] * m, sky[1] * m, sky[2] * (1.0 - rain * 0.4)];
        }
        if thunder > 0.0 {
            sky = sky.map(|c| c * (1.0 - thunder * 0.5));
        }
        let fog: Rgb = std::array::from_fn(|i| base.fog_color[i] * fog_mul[i]);
        let mut env = Self {
            sky_factor: light_factor,
            sky_light_color: light_color,
            sky_color: sky,
            fog_color: fog,
            ..base
        };
        env.mix_fog_with_sky();
        env
    }

    pub fn nether() -> Self {
        Self {
            sky_factor: 0.0,
            sky_light_color: hex(0x7a7aff),
            ambient_color: hex(0x302821),
            cardinal: CARDINAL_NETHER,
            fog_color: hex(0x330808),
            sky_color: hex(0x330808),
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
