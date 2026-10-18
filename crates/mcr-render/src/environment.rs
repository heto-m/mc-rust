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
    /// Where the sky disc is fully fogged; the `visual/sky_fog_end_distance` attribute until
    /// [`Self::apply_fog`] resolves it.
    pub sky_fog_end: f32,
    /// Where clouds are fully fogged; clouds also cap it at their range.
    pub cloud_fog_end: f32,
    pub water_fog_color: Rgb,
    pub water_fog_start: f32,
    pub water_fog_end: f32,
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
    /// Eye height below which the dark void disc is drawn: sea level, or the bottom of a flat world.
    pub horizon_height: f32,
    /// End sky flash: intensity, then the x and y angles in degrees.
    pub end_flash: Option<[f32; 3]>,
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
            cloud_fog_end: 2048.0,
            water_fog_color: hex(0x050533),
            water_fog_start: -8.0,
            water_fog_end: 96.0,
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
            end_flash: None,
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

/// `visual/*` environment attributes after the dimension and biome layers, before the timelines and
/// weather. The defaults are the attribute defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub sky: Rgb,
    pub fog: Rgb,
    /// RGBA; alpha is the cloud opacity, zero hides the clouds.
    pub cloud: [f32; 4],
    pub cloud_height: f32,
    pub sky_light_color: Rgb,
    pub sky_light_factor: f32,
    pub ambient: Rgb,
    pub block_light_tint: Rgb,
    pub night_vision_color: Rgb,
    pub fog_start: f32,
    pub fog_end: f32,
    pub sky_fog_end: f32,
    pub cloud_fog_end: f32,
    pub water_fog: Rgb,
    pub water_fog_start: f32,
    pub water_fog_end: f32,
    pub skybox: Skybox,
    pub cardinal: [f32; 6],
    /// The dimension runs the `minecraft:day` timeline.
    pub day_cycle: bool,
    /// `Level#canHaveWeather`: sky light, no ceiling, not the End.
    pub weather: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            sky: [0.0; 3],
            fog: [0.0; 3],
            cloud: [0.0; 4],
            cloud_height: 192.33,
            sky_light_color: [1.0; 3],
            sky_light_factor: 1.0,
            ambient: [0.0; 3],
            block_light_tint: hex(0xffd88c),
            night_vision_color: hex(0x999999),
            fog_start: 0.0,
            fog_end: 1024.0,
            sky_fog_end: 512.0,
            cloud_fog_end: 2048.0,
            water_fog: hex(0x050533),
            water_fog_start: -8.0,
            water_fog_end: 96.0,
            skybox: Skybox::Overworld,
            cardinal: CARDINAL_DEFAULT,
            day_cycle: false,
            weather: false,
        }
    }
}

impl Look {
    /// The vanilla overworld dimension type with plains on top.
    pub fn overworld() -> Self {
        Self {
            sky: hex(0x78a7ff),
            fog: hex(0xc0d8ff),
            cloud: [1.0, 1.0, 1.0, 0.8],
            ambient: hex(0x0a0a0a),
            day_cycle: true,
            weather: true,
            ..Self::default()
        }
    }

    /// The vanilla nether dimension type with nether wastes on top.
    pub fn nether() -> Self {
        Self {
            fog: hex(0x330808),
            sky_light_color: hex(0x7a7aff),
            sky_light_factor: 0.0,
            ambient: hex(0x302821),
            fog_start: 10.0,
            fog_end: 96.0,
            skybox: Skybox::None,
            cardinal: CARDINAL_NETHER,
            ..Self::default()
        }
    }

    pub fn end() -> Self {
        Self {
            fog: hex(0x181318),
            sky_light_color: hex(0xac60cd),
            sky_light_factor: 0.0,
            ambient: hex(0x3f473f),
            skybox: Skybox::End,
            ..Self::default()
        }
    }
}

/// What fills the camera's block (`Camera#getFluidInCamera`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FogKind {
    #[default]
    Atmospheric,
    Water,
    Lava,
    PowderSnow,
}

/// Camera and mob effect state that picks the fog environment (`FogRenderer#setupFog`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FogInputs {
    pub kind: FogKind,
    /// Unit look direction.
    pub forward: [f32; 3],
    pub camera_y: f32,
    pub min_y: f32,
    /// `voidDarknessOnsetRange`: 1 in flat worlds, 32 otherwise.
    pub void_range: f32,
    /// Blindness ticks left (infinite for an endless effect).
    pub blindness: Option<f32>,
    /// Darkness blend factor, 0 to 1.
    pub darkness: Option<f32>,
    /// `LocalPlayer#getWaterVision`, 0 to 1.
    pub water_vision: f32,
    /// Night vision brightening of the fog, 0 unless night vision without darkness.
    pub night_vision: f32,
    pub spectator: bool,
    pub fire_resistance: bool,
    /// `AtmosphericFogEnvironment.rainFogMultiplier`.
    pub rain_fog: f32,
    /// A boss bar asks for world fog.
    pub boss_fog: bool,
}

impl Default for FogInputs {
    fn default() -> Self {
        Self {
            kind: FogKind::Atmospheric,
            forward: [0.0, 0.0, 1.0],
            camera_y: 64.0,
            min_y: -64.0,
            void_range: 32.0,
            blindness: None,
            darkness: None,
            water_vision: 0.0,
            night_vision: 0.0,
            spectator: false,
            fire_resistance: false,
            rain_fog: 0.0,
            boss_fog: false,
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

fn sunrise_keys() -> &'static [(f32, [f32; 4])] {
    static KEYS: std::sync::LazyLock<Vec<(f32, [f32; 4])>> = std::sync::LazyLock::new(|| {
        SUNRISE_RAW
            .iter()
            .map(|&(t, c)| (t, c.map(|v| f32::from(v) / 255.0)))
            .collect()
    });
    &KEYS
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

pub type Lightmap = [[f32; 4]; 256];

fn finite_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

fn mul3(a: Rgb, b: Rgb) -> Rgb {
    std::array::from_fn(|i| a[i] * b[i])
}

impl Environment {
    /// The overworld at `day_time` ticks of the overworld clock (unbounded; the moon phase uses the day
    /// count), with the vanilla rain level and thunder level (already scaled by rain).
    pub fn overworld(day_time: u64, rain: f32, thunder: f32) -> Self {
        Self::overworld_with(day_time, rain, thunder, OverworldBase::default())
    }

    /// Like [`Self::overworld`] with biome base colours, which the day timeline multiplies.
    pub fn overworld_with(day_time: u64, rain: f32, thunder: f32, colors: OverworldBase) -> Self {
        let look = Look {
            sky: colors.sky,
            fog: colors.fog,
            cloud: colors.cloud,
            cloud_height: colors.cloud_height,
            ..Look::overworld()
        };
        let mut env = Self::from_look(day_time, rain, thunder, &look);
        env.apply_fog(&FogInputs::default());
        env
    }

    /// The attributes of `look` run through the day timeline and weather layers when the dimension has
    /// them. Fog colour and distances stay the raw attributes until [`Self::apply_fog`].
    pub fn from_look(day_time: u64, rain: f32, thunder: f32, look: &Look) -> Self {
        let t = (day_time % 24000) as f32;
        let (rain, thunder) = if look.weather {
            let rain = finite_unit(rain);
            (rain, finite_unit(thunder).min(rain))
        } else {
            (0.0, 0.0)
        };
        let mut sky = look.sky;
        let mut fog = look.fog;
        let mut cloud: Rgb = [look.cloud[0], look.cloud[1], look.cloud[2]];
        let mut factor = finite_unit(look.sky_light_factor);
        let mut light = look.sky_light_color;
        let mut star = 0.0;
        let mut sunrise = [0.0; 4];
        let (mut sun_angle, mut moon_angle, mut star_angle, mut moon_phase) = (0.0, 0.0, 0.0, 0);
        if look.day_cycle {
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
            sky = mul3(sky, day_night([0.0; 3]));
            fog = mul3(fog, day_night(hex(0x0f0f16)));
            cloud = mul3(cloud, day_night(hex(0x191926)));
            factor *= track(
                &[
                    (730.0, [1.0]),
                    (11270.0, [1.0]),
                    (13140.0, [0.24]),
                    (22860.0, [0.24]),
                ],
                t,
            )[0];
            light = mul3(
                light,
                track(
                    &[
                        (730.0, [1.0; 3]),
                        (11270.0, [1.0; 3]),
                        (13140.0, hex(0x7a7aff)),
                        (22860.0, hex(0x7a7aff)),
                    ],
                    t,
                ),
            );
            star = track(STAR_KEYS, t)[0];
            sunrise = track(sunrise_keys(), t);
            let day_deg = |offset: f32| {
                offset + 360.0 * bezier_ease((t - 6000.0).rem_euclid(24000.0) / 24000.0)
            };
            sun_angle = day_deg(0.0).to_radians();
            moon_angle = day_deg(180.0).to_radians();
            star_angle = day_deg(0.0).to_radians();
            moon_phase = ((day_time / 24000) % 8) as u8;
        }

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

        let cloud_alpha = finite_unit(look.cloud[3]);
        Self {
            sky_factor: factor,
            sky_light_color: light,
            ambient_color: look.ambient,
            block_light_tint: look.block_light_tint,
            night_vision_color: look.night_vision_color,
            cardinal: look.cardinal,
            sky_color: sky,
            fog_color: fog,
            fog_start: finite_or(look.fog_start, 0.0),
            fog_end: finite_or(look.fog_end, 1024.0).max(0.0),
            sky_fog_end: finite_or(look.sky_fog_end, 512.0).max(0.0),
            cloud_fog_end: finite_or(look.cloud_fog_end, 2048.0).max(0.0),
            water_fog_color: look.water_fog,
            water_fog_start: finite_or(look.water_fog_start, -8.0),
            water_fog_end: finite_or(look.water_fog_end, 96.0).max(0.0),
            skybox: look.skybox,
            sun_angle,
            moon_angle,
            star_angle,
            moon_phase,
            star_brightness: star,
            sunrise_color: sunrise,
            cloud_color: [cloud[0], cloud[1], cloud[2], cloud_alpha],
            cloud_height: Some(look.cloud_height)
                .filter(|h| h.is_finite() && cloud_alpha > 0.0 && look.skybox == Skybox::Overworld),
            rain_level: rain,
            thunder_level: thunder,
            ..Self::default()
        }
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
        let mut env = Self::from_look(0, 0.0, 0.0, &Look::end());
        env.apply_fog(&FogInputs::default());
        env
    }

    pub fn nether() -> Self {
        let mut env = Self::from_look(0, 0.0, 0.0, &Look::nether());
        env.apply_fog(&FogInputs::default());
        env
    }

    pub fn set_render_distance_chunks(&mut self, chunks: u32) {
        self.render_distance = chunks as f32 * 16.0;
    }

    /// Cylindrical render-distance fog range (start, end).
    pub fn render_fog_range(&self) -> (f32, f32) {
        let span = (self.render_distance / 10.0).clamp(4.0, 64.0);
        (self.render_distance - span, self.render_distance)
    }

    /// `AtmosphericFogEnvironment#getBaseColor`: the fog attribute tinted toward a rising or setting sun
    /// in view, then mixed toward the weather-darkened sky for short sky fog distances.
    fn atmospheric_fog_color(&self, forward: [f32; 3]) -> Rgb {
        let mut fog = self.fog_color;
        let chunks = self.render_distance / 16.0;
        if chunks >= 4.0 && self.skybox == Skybox::Overworld {
            let sun_x = if self.sun_angle.sin() > 0.0 {
                -1.0
            } else {
                1.0
            };
            let looking = finite_or(forward[0], 0.0) * sun_x;
            let alpha = self.sunrise_color[3];
            if looking > 0.0 && alpha > 0.0 {
                let sun: Rgb = [
                    self.sunrise_color[0],
                    self.sunrise_color[1],
                    self.sunrise_color[2],
                ];
                fog = srgb_lerp(looking.min(1.0) * alpha, fog, sun);
            }
        }
        let mut sky = self.sky_color;
        if self.rain_level > 0.0 {
            let (rg, b) = (1.0 - self.rain_level * 0.5, 1.0 - self.rain_level * 0.4);
            sky = [sky[0] * rg, sky[1] * rg, sky[2] * b];
        }
        if self.thunder_level > 0.0 {
            sky = sky.map(|c| c * (1.0 - self.thunder_level * 0.5));
        }
        let sky_end = (self.sky_fog_end / 16.0).min(chunks);
        let mix = 1.0 - lerp(0.25, 1.0, (sky_end / 32.0).clamp(0.0, 1.0)).powf(0.25);
        srgb_lerp(mix, fog, sky)
    }

    /// `FogRenderer#computeFogColor` and `setupFog`: resolves fog colour, environmental fog and sky fog
    /// from the attributes for the camera's fluid and the player's effects. Call once per environment.
    pub fn apply_fog(&mut self, f: &FogInputs) {
        let rd = self.render_distance;
        let mut color = match f.kind {
            FogKind::Atmospheric => self.atmospheric_fog_color(f.forward),
            FogKind::Water => self.water_fog_color,
            FogKind::Lava => hex(0x991a00),
            FogKind::PowderSnow => hex(0x9fbacc),
        };
        let range = if f.void_range > 0.0 {
            f.void_range
        } else {
            32.0
        };
        let mut darkness = finite_unit((range + f.min_y - f.camera_y) / range);
        if let Some(left) = f.blindness {
            darkness = if left <= 19.0 {
                darkness.max(left / 20.0)
            } else {
                1.0
            };
        } else if let Some(blend) = f.darkness {
            darkness = darkness.max(finite_unit(blend));
        }
        if darkness > 0.0 && !matches!(f.kind, FogKind::Lava | FogKind::PowderSnow) {
            let b = (1.0 - darkness) * (1.0 - darkness);
            color = color.map(|c| c * b);
        }
        let boss = finite_unit(self.boss_darkening);
        if boss > 0.0 {
            color = lerp3(
                color,
                [color[0] * 0.7, color[1] * 0.6, color[2] * 0.6],
                boss,
            );
        }
        let brighten = finite_unit(if f.kind == FogKind::Water {
            f.water_vision
        } else {
            f.night_vision
        });
        if color.iter().all(|&c| c != 0.0) {
            let scale = 1.0 / color[0].max(color[1]).max(color[2]);
            color = lerp3(color, color.map(|c| c * scale), brighten);
        }
        self.fog_color = color;

        let sky_attr = self.sky_fog_end;
        let (start, end, sky, cloud) = match f.kind {
            FogKind::Lava | FogKind::PowderSnow if f.spectator => {
                (-8.0, rd * 0.5, rd * 0.5, rd * 0.5)
            }
            FogKind::Lava if f.fire_resistance => (0.0, 5.0, 5.0, 5.0),
            FogKind::Lava => (0.25, 1.0, 1.0, 1.0),
            FogKind::PowderSnow => (0.0, 2.0, 2.0, 2.0),
            _ if f.blindness.is_some() => {
                let left = f.blindness.unwrap_or(0.0);
                let d = if left.is_infinite() {
                    5.0
                } else {
                    lerp(rd, 5.0, (left / 20.0).clamp(0.0, 1.0))
                };
                (d * 0.25, d, d * 0.8, d * 0.8)
            }
            _ if f.darkness.is_some() => {
                let d = lerp(rd, 15.0, finite_unit(f.darkness.unwrap_or(0.0)));
                (d * 0.75, d, d, d)
            }
            FogKind::Water => {
                let end = self.water_fog_end * finite_unit(f.water_vision).max(0.25);
                (self.water_fog_start, end, end, end)
            }
            FogKind::Atmospheric => {
                let rain = finite_unit(f.rain_fog);
                let start = self.fog_start - 160.0 * rain;
                let end = (self.fog_end - 256.0 * rain).max(self.fog_end.min(96.0));
                let sky = rd.min(sky_attr);
                if f.boss_fog {
                    let end = end.min(96.0);
                    (start.min(10.0), end, end, end)
                } else {
                    (start, end, sky, self.cloud_fog_end)
                }
            }
        };
        self.fog_start = start;
        self.fog_end = end;
        self.sky_fog_end = sky;
        self.cloud_fog_end = cloud;
        let sky_blocked = f.blindness.is_some()
            || f.darkness.is_some()
            || matches!(f.kind, FogKind::Lava | FogKind::PowderSnow);
        if sky_blocked {
            self.skybox = Skybox::None;
            self.sky_color = self.fog_color;
            self.end_flash = None;
        }
    }

    /// The 16x16 lightmap, indexed `sky * 16 + block`, with `sky_scale` multiplying the sky factor.
    pub fn lightmap(&self, sky_scale: f32) -> Lightmap {
        let brightness = |l: f32| l / (4.0 - 3.0 * l);
        let night_vision = self.night_vision_color.map(|c| c * self.night_vision);
        let mut out = [[0.0; 4]; 256];
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
                c = c.map(|v| finite_or(v - self.darkness_scale, 0.0).clamp(0.0, 1.0));
                let max = c[0].max(c[1]).max(c[2]);
                if max > 0.0 {
                    let inverted = 1.0 - max;
                    let scaled = 1.0 - inverted.powi(4);
                    let not_gamma = c.map(|v| v * scaled / max);
                    c = lerp3(c, not_gamma, self.brightness);
                }
                out[sky * 16 + block] = [c[0], c[1], c[2], 1.0];
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

    fn fogged(kind: FogKind, tweak: impl FnOnce(&mut FogInputs)) -> Environment {
        let mut env = Environment::from_look(6000, 0.0, 0.0, &Look::overworld());
        env.set_render_distance_chunks(12);
        let mut f = FogInputs {
            kind,
            ..FogInputs::default()
        };
        tweak(&mut f);
        env.apply_fog(&f);
        env
    }

    #[test]
    fn night_vision_lifts_the_darkest_cell_and_darkness_pulls_it_down() {
        let base = Environment {
            brightness: 0.0,
            ..Environment::overworld(18000, 0.0, 0.0)
        };
        let dark = base.lightmap(1.0)[0];
        let seeing = Environment {
            night_vision: 1.0,
            ..base.clone()
        }
        .lightmap(1.0)[0];
        assert!((seeing[0] - 0.6).abs() < 1e-3, "{seeing:?}");
        assert!(seeing[0] > dark[0] * 10.0);
        let darkness = Environment {
            darkness_scale: 0.3,
            ..base.clone()
        }
        .lightmap(1.0);
        assert_eq!(darkness[0][0], 0.0);
        assert!(darkness[8 * 16][0] < base.lightmap(1.0)[8 * 16][0]);
    }

    #[test]
    fn fog_distances_follow_the_fog_environment() {
        let air = fogged(FogKind::Atmospheric, |_| {});
        assert_eq!(
            (air.fog_start, air.fog_end, air.sky_fog_end),
            (0.0, 1024.0, 192.0)
        );
        let lava = fogged(FogKind::Lava, |_| {});
        assert_eq!((lava.fog_start, lava.fog_end), (0.25, 1.0));
        assert_eq!(lava.fog_color, hex(0x991a00));
        assert_eq!(lava.skybox, Skybox::None);
        let resistant = fogged(FogKind::Lava, |f| f.fire_resistance = true);
        assert_eq!((resistant.fog_start, resistant.fog_end), (0.0, 5.0));
        let spectating = fogged(FogKind::Lava, |f| f.spectator = true);
        assert_eq!((spectating.fog_start, spectating.fog_end), (-8.0, 96.0));
        let snow = fogged(FogKind::PowderSnow, |_| {});
        assert_eq!((snow.fog_start, snow.fog_end), (0.0, 2.0));
        let blind = fogged(FogKind::Atmospheric, |f| f.blindness = Some(200.0));
        assert_eq!(
            (blind.fog_start, blind.fog_end, blind.sky_fog_end),
            (1.25, 5.0, 4.0)
        );
        assert_eq!(blind.fog_color, [0.0; 3]);
        assert_eq!(blind.cloud_fog_end, 4.0);
        assert_eq!(air.cloud_fog_end, 2048.0);
        assert_eq!(lava.cloud_fog_end, 1.0);
        let fading = fogged(FogKind::Atmospheric, |f| f.blindness = Some(10.0));
        assert!((fading.fog_end - (192.0 + (5.0 - 192.0) * 0.5)).abs() < 1e-3);
        let darkness = fogged(FogKind::Atmospheric, |f| f.darkness = Some(1.0));
        assert_eq!((darkness.fog_start, darkness.fog_end), (11.25, 15.0));
        let rain = fogged(FogKind::Atmospheric, |f| f.rain_fog = 1.0);
        assert_eq!((rain.fog_start, rain.fog_end), (-160.0, 768.0));
        let boss = fogged(FogKind::Atmospheric, |f| f.boss_fog = true);
        assert_eq!(
            (boss.fog_start, boss.fog_end, boss.sky_fog_end),
            (0.0, 96.0, 96.0)
        );
    }

    #[test]
    fn water_fog_uses_the_biome_and_water_vision() {
        let look = Look {
            water_fog: hex(0x041f33),
            ..Look::overworld()
        };
        let mut env = Environment::from_look(6000, 0.0, 0.0, &look);
        env.apply_fog(&FogInputs {
            kind: FogKind::Water,
            water_vision: 0.0,
            ..FogInputs::default()
        });
        assert_eq!(
            (env.fog_start, env.fog_end, env.sky_fog_end),
            (-8.0, 24.0, 24.0)
        );
        assert_eq!(env.fog_color, hex(0x041f33));
        let mut clear = Environment::from_look(6000, 0.0, 0.0, &look);
        clear.apply_fog(&FogInputs {
            kind: FogKind::Water,
            water_vision: 1.0,
            ..FogInputs::default()
        });
        assert_eq!(clear.fog_end, 96.0);
        assert!(
            (clear.fog_color[2] - 1.0).abs() < 1e-5,
            "{:?}",
            clear.fog_color
        );
    }

    #[test]
    fn void_and_night_vision_shift_the_fog_colour() {
        let deep = fogged(FogKind::Atmospheric, |f| f.camera_y = -80.0);
        assert_eq!(deep.fog_color, [0.0; 3]);
        let seeing = fogged(FogKind::Atmospheric, |f| f.night_vision = 1.0);
        let plain = fogged(FogKind::Atmospheric, |_| {});
        assert!(seeing.fog_color[2] > plain.fog_color[2] - 1e-6);
        assert!((seeing.fog_color.iter().cloned().fold(0.0, f32::max) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn biome_sky_colour_feeds_the_day_timeline() {
        let desert = Look {
            sky: hex(0x6eb1ff),
            ..Look::overworld()
        };
        let env = Environment::from_look(6000, 0.0, 0.0, &desert);
        assert_eq!(env.sky_color, hex(0x6eb1ff));
        let night = Environment::from_look(18000, 0.0, 0.0, &desert);
        assert_eq!(night.sky_color, [0.0; 3]);
    }

    #[test]
    fn nether_and_end_fog_mix_toward_their_black_sky() {
        let nether = Environment::nether();
        assert!(nether.fog_color[0] < hex(0x330808)[0]);
        assert_eq!((nether.fog_start, nether.fog_end), (10.0, 96.0));
        let end = Environment::end();
        assert!(end.fog_color[0] < hex(0x181318)[0]);
        assert_eq!(end.cloud_height, None);
    }

    #[test]
    fn sunrise_tints_fog_only_when_looking_at_the_sun() {
        let dawn = Environment::from_look(23500, 0.0, 0.0, &Look::overworld());
        let mut east = dawn.clone();
        east.apply_fog(&FogInputs {
            forward: [1.0, 0.0, 0.0],
            ..FogInputs::default()
        });
        let mut west = dawn;
        west.apply_fog(&FogInputs {
            forward: [-1.0, 0.0, 0.0],
            ..FogInputs::default()
        });
        assert!(east.fog_color[0] > west.fog_color[0]);
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
