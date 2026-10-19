//! What the player sees of the world's atmosphere: dimension and biome `visual/*` attributes, mob effects
//! that change vision, the fluid around the camera, rain fog, block-light flicker and End flashes.

use mcr_client::{ClientState, Registries};
use mcr_nbt::{Compound, List, Value};
use mcr_render::{CARDINAL_NETHER, Environment, FogInputs, FogKind, Look, Rgb, Skybox, srgb_lerp};
use mcr_settings::Options;
use mcr_world::{ChunkPos, ChunkStore};

const DIMENSION_TYPE: &str = "minecraft:dimension_type";
const BIOME: &str = "minecraft:worldgen/biome";
const TIMELINE: &str = "minecraft:timeline";
const WORLD_CLOCK: &str = "minecraft:world_clock";
const MAX_BLEND_SOURCES: usize = 32;
const KERNEL: [f64; 7] = [0.0, 1.0, 4.0, 6.0, 4.0, 1.0, 0.0];
const WATER_VISION_MAX: i32 = 600;
const DARKNESS_BLEND_TICKS: f32 = 22.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ColorOp {
    Set(Rgb),
    Add(Rgb),
    Subtract(Rgb),
    Multiply(Rgb),
    AlphaBlend([f32; 4]),
    BlendToGray { brightness: f32, factor: f32 },
}

impl ColorOp {
    fn apply(self, c: Rgb) -> Rgb {
        let clamp = |v: Rgb| v.map(|x| x.clamp(0.0, 1.0));
        match self {
            Self::Set(v) => v,
            Self::Add(v) => clamp(std::array::from_fn(|i| c[i] + v[i])),
            Self::Subtract(v) => clamp(std::array::from_fn(|i| c[i] - v[i])),
            Self::Multiply(v) => std::array::from_fn(|i| c[i] * v[i]),
            Self::AlphaBlend([a, r, g, b]) => {
                std::array::from_fn(|i| c[i] + ([r, g, b][i] - c[i]) * a)
            }
            Self::BlendToGray { brightness, factor } => {
                let gray = (c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11) * brightness;
                srgb_lerp(factor, c, [gray; 3])
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FloatOp {
    Set(f32),
    Add(f32),
    Subtract(f32),
    Multiply(f32),
    Minimum(f32),
    Maximum(f32),
    AlphaBlend { value: f32, alpha: f32 },
}

impl FloatOp {
    fn apply(self, v: f32) -> f32 {
        let out = match self {
            Self::Set(x) => x,
            Self::Add(x) => v + x,
            Self::Subtract(x) => v - x,
            Self::Multiply(x) => v * x,
            Self::Minimum(x) => v.min(x),
            Self::Maximum(x) => v.max(x),
            Self::AlphaBlend { value, alpha } => v + (value - v) * alpha,
        };
        if out.is_finite() { out } else { v }
    }
}

/// One attribute map (a dimension type's or a biome's) restricted to what the renderer reads.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Layer {
    sky: Option<ColorOp>,
    fog: Option<ColorOp>,
    water_fog: Option<ColorOp>,
    sky_light_color: Option<ColorOp>,
    ambient: Option<ColorOp>,
    block_light_tint: Option<ColorOp>,
    night_vision_color: Option<ColorOp>,
    cloud: Option<[f32; 4]>,
    cloud_height: Option<FloatOp>,
    sky_light_factor: Option<FloatOp>,
    fog_start: Option<FloatOp>,
    fog_end: Option<FloatOp>,
    sky_fog_end: Option<FloatOp>,
    cloud_fog_end: Option<FloatOp>,
    water_fog_start: Option<FloatOp>,
    water_fog_end: Option<FloatOp>,
}

fn number(v: &Value) -> Option<f32> {
    let n = match v {
        Value::Byte(n) => f64::from(*n),
        Value::Short(n) => f64::from(*n),
        Value::Int(n) => f64::from(*n),
        Value::Long(n) => *n as f64,
        Value::Float(n) => f64::from(*n),
        Value::Double(n) => *n,
        _ => return None,
    };
    let n = n as f32;
    n.is_finite().then_some(n)
}

fn channel(v: u32, shift: u32) -> f32 {
    ((v >> shift) & 255) as f32 / 255.0
}

/// `#rrggbb`, `#aarrggbb`, a packed int or a float list, as ARGB.
fn argb(v: &Value) -> Option<[f32; 4]> {
    match v {
        Value::String(s) => {
            let hex = s.strip_prefix('#')?;
            let n = u32::from_str_radix(hex, 16).ok()?;
            match hex.len() {
                6 => Some([1.0, channel(n, 16), channel(n, 8), channel(n, 0)]),
                8 => Some([channel(n, 24), channel(n, 16), channel(n, 8), channel(n, 0)]),
                _ => None,
            }
        }
        Value::Int(n) => {
            let n = *n as u32;
            Some([1.0, channel(n, 16), channel(n, 8), channel(n, 0)])
        }
        Value::List(List::Float(f)) if f.len() == 3 => Some([1.0, f[0], f[1], f[2]]),
        Value::List(List::Float(f)) if f.len() == 4 => Some([f[3], f[0], f[1], f[2]]),
        Value::List(List::Double(f)) if f.len() == 3 => {
            Some([1.0, f[0] as f32, f[1] as f32, f[2] as f32])
        }
        _ => None,
    }
    .filter(|c| c.iter().all(|x| x.is_finite()))
    .map(|c| c.map(|x| x.clamp(0.0, 1.0)))
}

fn rgb(v: &Value) -> Option<Rgb> {
    argb(v).map(|[_, r, g, b]| [r, g, b])
}

/// `(modifier, argument)` of an attribute entry; a plain value overrides.
fn modifier(v: &Value) -> (&str, &Value) {
    match v {
        Value::Compound(c) if c.contains_key("argument") => (
            c.get_str("modifier").unwrap_or("override"),
            c.get("argument").unwrap_or(v),
        ),
        other => ("override", other),
    }
}

fn color_op(v: &Value) -> Option<ColorOp> {
    let (op, arg) = modifier(v);
    Some(match op {
        "override" | "minecraft:override" => ColorOp::Set(rgb(arg)?),
        "add" => ColorOp::Add(rgb(arg)?),
        "subtract" => ColorOp::Subtract(rgb(arg)?),
        "multiply" => ColorOp::Multiply(rgb(arg)?),
        "alpha_blend" => ColorOp::AlphaBlend(argb(arg)?),
        "blend_to_gray" => {
            let Value::Compound(c) = arg else { return None };
            ColorOp::BlendToGray {
                brightness: c.get("brightness").and_then(number)?.clamp(0.0, 1.0),
                factor: c.get("factor").and_then(number)?.clamp(0.0, 1.0),
            }
        }
        _ => return None,
    })
}

fn float_op(v: &Value) -> Option<FloatOp> {
    let (op, arg) = modifier(v);
    Some(match op {
        "override" | "minecraft:override" => FloatOp::Set(number(arg)?),
        "add" => FloatOp::Add(number(arg)?),
        "subtract" => FloatOp::Subtract(number(arg)?),
        "multiply" => FloatOp::Multiply(number(arg)?),
        "minimum" => FloatOp::Minimum(number(arg)?),
        "maximum" => FloatOp::Maximum(number(arg)?),
        "alpha_blend" => {
            let Value::Compound(c) = arg else { return None };
            FloatOp::AlphaBlend {
                value: c.get("value").and_then(number)?,
                alpha: c.get("alpha").and_then(number)?.clamp(0.0, 1.0),
            }
        }
        _ => return None,
    })
}

impl Layer {
    fn parse(attributes: &Compound) -> Self {
        let get = |name: &str| attributes.get(&format!("minecraft:visual/{name}"));
        let color = |name: &str| get(name).and_then(color_op);
        let float = |name: &str| get(name).and_then(float_op);
        Self {
            sky: color("sky_color"),
            fog: color("fog_color"),
            water_fog: color("water_fog_color"),
            sky_light_color: color("sky_light_color"),
            ambient: color("ambient_light_color"),
            block_light_tint: color("block_light_tint"),
            night_vision_color: color("night_vision_color"),
            cloud: get("cloud_color").and_then(|v| argb(modifier(v).1)),
            cloud_height: float("cloud_height"),
            sky_light_factor: float("sky_light_factor"),
            fog_start: float("fog_start_distance"),
            fog_end: float("fog_end_distance"),
            sky_fog_end: float("sky_fog_end_distance"),
            cloud_fog_end: float("cloud_fog_end_distance"),
            water_fog_start: float("water_fog_start_distance"),
            water_fog_end: float("water_fog_end_distance"),
        }
    }

    fn of_entry(entry: &Compound) -> Self {
        entry
            .get_compound("attributes")
            .map(Self::parse)
            .unwrap_or_default()
    }

    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn apply(&self, look: &Look) -> Look {
        let c = |op: Option<ColorOp>, v: Rgb| op.map_or(v, |op| op.apply(v));
        let f = |op: Option<FloatOp>, v: f32| op.map_or(v, |op| op.apply(v));
        Look {
            sky: c(self.sky, look.sky),
            fog: c(self.fog, look.fog),
            water_fog: c(self.water_fog, look.water_fog),
            sky_light_color: c(self.sky_light_color, look.sky_light_color),
            ambient: c(self.ambient, look.ambient),
            block_light_tint: c(self.block_light_tint, look.block_light_tint),
            night_vision_color: c(self.night_vision_color, look.night_vision_color),
            cloud: self.cloud.map_or(look.cloud, |[a, r, g, b]| [r, g, b, a]),
            cloud_height: f(self.cloud_height, look.cloud_height),
            sky_light_factor: f(self.sky_light_factor, look.sky_light_factor).clamp(0.0, 1.0),
            fog_start: f(self.fog_start, look.fog_start),
            fog_end: f(self.fog_end, look.fog_end).max(0.0),
            sky_fog_end: f(self.sky_fog_end, look.sky_fog_end).max(0.0),
            cloud_fog_end: f(self.cloud_fog_end, look.cloud_fog_end).max(0.0),
            water_fog_start: f(self.water_fog_start, look.water_fog_start),
            water_fog_end: f(self.water_fog_end, look.water_fog_end).max(0.0),
            ..*look
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Spatial and partial-tick lerp of the positional attributes.
fn lerp_look(a: &Look, b: &Look, t: f32) -> Look {
    if t <= 0.0 {
        return *a;
    }
    if t >= 1.0 {
        return *b;
    }
    let c = |x: Rgb, y: Rgb| srgb_lerp(t, x, y);
    Look {
        sky: c(a.sky, b.sky),
        fog: c(a.fog, b.fog),
        water_fog: c(a.water_fog, b.water_fog),
        sky_light_color: c(a.sky_light_color, b.sky_light_color),
        ambient: c(a.ambient, b.ambient),
        block_light_tint: c(a.block_light_tint, b.block_light_tint),
        night_vision_color: c(a.night_vision_color, b.night_vision_color),
        cloud: std::array::from_fn(|i| lerp(a.cloud[i], b.cloud[i], t)),
        cloud_height: lerp(a.cloud_height, b.cloud_height, t),
        sky_light_factor: lerp(a.sky_light_factor, b.sky_light_factor, t),
        fog_start: lerp(a.fog_start, b.fog_start, t),
        fog_end: lerp(a.fog_end, b.fog_end, t),
        sky_fog_end: lerp(a.sky_fog_end, b.sky_fog_end, t),
        cloud_fog_end: lerp(a.cloud_fog_end, b.cloud_fog_end, t),
        water_fog_start: lerp(a.water_fog_start, b.water_fog_start, t),
        water_fog_end: lerp(a.water_fog_end, b.water_fog_end, t),
        ..*b
    }
}

/// The dimension type's fixed look: its attributes over the defaults, its skybox, cardinal light and
/// whether it runs the day timeline. Entries without data fall back to the vanilla dimension by name.
fn dimension_look(registries: &Registries, type_id: i32, name: &str) -> Look {
    let entry = registries.get(DIMENSION_TYPE).and_then(|r| r.get(type_id));
    let data = entry.and_then(|e| e.data.as_ref());
    let Some(data) = data else {
        let key = entry.map_or(name, |e| e.id.as_str());
        return match key {
            "minecraft:the_nether" => Look::nether(),
            "minecraft:the_end" => Look::end(),
            _ => Look::overworld(),
        };
    };
    let mut look = Layer::of_entry(data).apply(&Look::default());
    look.skybox = match data.get_str("skybox").unwrap_or("overworld") {
        "none" | "minecraft:none" => Skybox::None,
        "end" | "minecraft:end" => Skybox::End,
        _ => Skybox::Overworld,
    };
    if matches!(
        data.get_str("cardinal_light"),
        Some("nether" | "minecraft:nether")
    ) {
        look.cardinal = CARDINAL_NETHER;
    }
    let flag = |key: &str, default: bool| data.get_bool(key).unwrap_or(default);
    look.weather =
        flag("has_skylight", true) && !flag("has_ceiling", false) && look.skybox != Skybox::End;
    look.day_cycle = runs_day_timeline(registries, data.get("timelines"));
    look
}

fn runs_day_timeline(registries: &Registries, timelines: Option<&Value>) -> bool {
    let named = |id: i32| {
        registries
            .get(TIMELINE)
            .and_then(|r| r.get(id))
            .is_some_and(|e| e.id.as_str() == "minecraft:day")
    };
    match timelines {
        Some(Value::String(s)) => match s.strip_prefix('#') {
            Some(tag) => match registries.tag(TIMELINE, tag) {
                Some(ids) => ids.iter().any(|&id| named(id)),
                None => tag == "minecraft:in_overworld",
            },
            None => s == "minecraft:day",
        },
        Some(Value::List(List::String(list))) => list.iter().any(|s| s == "minecraft:day"),
        _ => false,
    }
}

fn clock_ticks(state: &ClientState, clock: &str) -> Option<i64> {
    let id = state.registries.get(WORLD_CLOCK)?.id_of(clock)?;
    let c = state.world.clocks.get(&i32::try_from(id).ok()?)?;
    Some(c.ticks as i64)
}

/// `BiomeManager#getNoiseBiomeAtQuart` on the client's columns; y is clamped into the world.
fn noise_biome(chunks: &ChunkStore, q: [i32; 3]) -> Option<u16> {
    let cfg = chunks.config();
    let min_q = cfg.min_y() >> 2;
    let max_q = min_q + (cfg.height() as i32 >> 2) - 1;
    let qy = q[1].clamp(min_q, max_q.max(min_q));
    let column = chunks.column(ChunkPos::new(q[0] >> 2, q[2] >> 2))?;
    let rel = (qy << 2) - cfg.min_y();
    let section = column.section((rel >> 4) as usize)?;
    Some(
        section
            .biome(
                (q[0] & 3) as usize,
                ((rel & 15) >> 2) as usize,
                (q[2] & 3) as usize,
            )
            .0,
    )
}

/// `java.util.Random`, as `SingleThreadedRandomSource` seeds the End flashes.
struct JavaRandom(u64);

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5_DEEC_E66D;
    const MASK: u64 = (1 << 48) - 1;

    fn new(seed: i64) -> Self {
        Self((seed as u64 ^ Self::MULTIPLIER) & Self::MASK)
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.0 = self.0.wrapping_mul(Self::MULTIPLIER).wrapping_add(0xB) & Self::MASK;
        (self.0 >> (48 - bits)) as i64 as i32
    }

    fn next_f32(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        if bound <= 0 {
            return 0;
        }
        if bound & -bound == bound {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let val = bits % bound;
            if bits.wrapping_sub(val).wrapping_add(bound - 1) >= 0 {
                return val;
            }
        }
    }
}

/// `EndFlashState`: one flash in a random part of every 600-tick interval of the dimension clock.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EndFlash {
    seed: Option<i64>,
    offset: i32,
    duration: i32,
    intensity: f32,
    old_intensity: f32,
    x_angle: f32,
    y_angle: f32,
}

impl EndFlash {
    pub fn tick(&mut self, clock: i64) {
        let seed = clock.div_euclid(600);
        if self.seed != Some(seed) {
            let mut r = JavaRandom::new(seed);
            r.next_f32();
            self.offset = r.next_int(201);
            let max = 380.min(600 - self.offset);
            self.duration = 100 + r.next_int(max - 100 + 1);
            self.x_angle = -60.0 + r.next_f32() * 70.0;
            self.y_angle = -180.0 + r.next_f32() * 360.0;
            self.seed = Some(seed);
        }
        self.old_intensity = self.intensity;
        let within = clock.rem_euclid(600) as i32;
        self.intensity = if within >= self.offset && within <= self.offset + self.duration {
            ((within - self.offset) as f32 * std::f32::consts::PI / self.duration as f32).sin()
        } else {
            0.0
        };
    }

    pub fn intensity(&self, partial: f32) -> f32 {
        lerp(self.old_intensity, self.intensity, partial)
    }

    pub fn started_this_tick(&self) -> bool {
        self.intensity > 0.0 && self.old_intensity <= 0.0
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct EffectIds {
    night_vision: Option<i32>,
    conduit_power: Option<i32>,
    blindness: Option<i32>,
    darkness: Option<i32>,
    fire_resistance: Option<i32>,
}

impl EffectIds {
    fn load() -> Self {
        let registry = mcr_data::v26_3().registry("minecraft:mob_effect");
        let id = |name: &str| registry.and_then(|r| r.id_of(name)).map(|id| id as i32);
        Self {
            night_vision: id("minecraft:night_vision"),
            conduit_power: id("minecraft:conduit_power"),
            blindness: id("minecraft:blindness"),
            darkness: id("minecraft:darkness"),
            fire_resistance: id("minecraft:fire_resistance"),
        }
    }
}

fn water_vision_at(time: i32) -> f32 {
    let t = time as f32;
    if t >= 600.0 {
        return 1.0;
    }
    let a = (t / 100.0).clamp(0.0, 1.0);
    let b = if t < 100.0 {
        0.0
    } else {
        ((t - 100.0) / 500.0).clamp(0.0, 1.0)
    };
    a * 0.6 + b * 0.4
}

/// Per-frame inputs of [`Vision::environment`].
pub struct VisionFrame<'a> {
    pub state: &'a ClientState,
    pub options: &'a Options,
    pub camera: [f64; 3],
    pub forward: [f32; 3],
    pub partial: f32,
    /// Game ticks since the previous frame.
    pub dt_ticks: f32,
    pub render_distance: i32,
    pub lightning: bool,
    pub boss_darkening: f32,
    pub boss_fog: bool,
}

/// Ticked state behind the world's look; see the module docs.
pub struct Vision {
    ids: EffectIds,
    ambient_light: f32,
    piglins_zombify: bool,
    key: Option<(i32, String, usize, usize)>,
    dimension: Look,
    biomes: Vec<Option<Box<Layer>>>,
    precipitation: Vec<bool>,
    plains: Option<u16>,
    look: (Look, Look),
    darkness: (f32, f32),
    water_vision_time: i32,
    flicker: f32,
    rng: u64,
    end_flash: Option<EndFlash>,
    rain_fog: f32,
    ticks: u32,
}

impl Default for Vision {
    fn default() -> Self {
        Self {
            ids: EffectIds::load(),
            ambient_light: 0.0,
            piglins_zombify: true,
            key: None,
            dimension: Look::overworld(),
            biomes: Vec::new(),
            precipitation: Vec::new(),
            plains: None,
            look: (Look::overworld(), Look::overworld()),
            darkness: (0.0, 0.0),
            water_vision_time: 0,
            flicker: 0.0,
            rng: 0x9E37_79B9_7F4A_7C15,
            end_flash: None,
            rain_fog: 0.0,
            ticks: 0,
        }
    }
}

impl Vision {
    fn refresh(&mut self, state: &ClientState) {
        let biomes = state.registries.get(BIOME);
        let dims = state.registries.get(DIMENSION_TYPE).map_or(0, |r| r.len());
        let key = (
            state.dimension.type_id,
            state.dimension.name.as_str().to_owned(),
            biomes.map_or(0, |r| r.len()),
            dims,
        );
        if self.key.as_ref() == Some(&key) {
            return;
        }
        self.dimension = dimension_look(
            &state.registries,
            state.dimension.type_id,
            state.dimension.name.as_str(),
        );
        self.biomes = biomes.map_or_else(Vec::new, |r| {
            r.entries
                .iter()
                .map(|e| {
                    e.data
                        .as_ref()
                        .map(Layer::of_entry)
                        .filter(|l| !l.is_empty())
                        .map(Box::new)
                })
                .collect()
        });
        self.precipitation = biomes.map_or_else(Vec::new, |r| {
            crate::effects::precipitation_table(r)
                .into_iter()
                .map(|(p, _)| p)
                .collect()
        });
        self.plains = biomes
            .and_then(|r| r.id_of("minecraft:plains"))
            .and_then(|id| u16::try_from(id).ok());
        self.end_flash = (self.dimension.skybox == Skybox::End).then(EndFlash::default);
        let entry = state
            .registries
            .get(DIMENSION_TYPE)
            .and_then(|r| r.get(state.dimension.type_id));
        let data = entry.and_then(|e| e.data.as_ref());
        let vanilla = entry.map_or(state.dimension.name.as_str(), |e| e.id.as_str());
        self.ambient_light = data
            .and_then(|d| d.get("ambient_light"))
            .and_then(number)
            .unwrap_or(match vanilla {
                "minecraft:the_nether" => 0.1,
                "minecraft:the_end" => 0.25,
                _ => 0.0,
            })
            .clamp(0.0, 1.0);
        self.piglins_zombify = data
            .and_then(|d| d.get_compound("attributes"))
            .and_then(|a| a.get("minecraft:gameplay/piglins_zombify"))
            .and_then(|v| number(modifier(v).1))
            .map_or(vanilla != "minecraft:the_nether", |v| v != 0.0);
        let look = self.dimension;
        self.look = (look, look);
        self.key = Some(key);
    }

    /// `EnvironmentAttributeProbe#tick`: Gaussian-weighted biome attributes around `pos`.
    fn blend(&self, chunks: &ChunkStore, pos: [f64; 3]) -> Look {
        self.blend_with(pos, |q| noise_biome(chunks, q))
    }

    fn blend_with(&self, pos: [f64; 3], biome_at: impl Fn([i32; 3]) -> Option<u16>) -> Look {
        let base = self.dimension;
        if self.biomes.iter().all(Option::is_none) || !pos.iter().all(|v| v.is_finite()) {
            return base;
        }
        let p = pos.map(|v| (v * 0.25 - 0.5).clamp(-1.0e8, 1.0e8));
        let i = p.map(|v| v.floor() as i32);
        let f: [f64; 3] = std::array::from_fn(|k| p[k] - f64::from(i[k]));
        let mut sources = [(0u16, 0.0f64); MAX_BLEND_SOURCES];
        let mut n = 0;
        for z in 0..6 {
            let wz = KERNEL[z + 1] + (KERNEL[z] - KERNEL[z + 1]) * f[2];
            for x in 0..6 {
                let wx = KERNEL[x + 1] + (KERNEL[x] - KERNEL[x + 1]) * f[0];
                for y in 0..6 {
                    let wy = KERNEL[y + 1] + (KERNEL[y] - KERNEL[y + 1]) * f[1];
                    let w = wx * wy * wz;
                    if w <= 0.0 {
                        continue;
                    }
                    let q = [
                        i[0] - 2 + x as i32,
                        i[1] - 2 + y as i32,
                        i[2] - 2 + z as i32,
                    ];
                    let Some(biome) = biome_at(q).or(self.plains) else {
                        continue;
                    };
                    match sources[..n].iter_mut().find(|s| s.0 == biome) {
                        Some(s) => s.1 += w,
                        None if n < MAX_BLEND_SOURCES => {
                            sources[n] = (biome, w);
                            n += 1;
                        }
                        None => {}
                    }
                }
            }
        }
        let apply = |biome: u16| match self.biomes.get(usize::from(biome)) {
            Some(Some(layer)) => layer.apply(&base),
            _ => base,
        };
        let mut out: Option<Look> = None;
        let mut total = 0.0;
        for &(biome, w) in &sources[..n] {
            let value = apply(biome);
            total += w;
            out = Some(match out {
                None => value,
                Some(prev) => lerp_look(&prev, &value, (w / total) as f32),
            });
        }
        out.unwrap_or(base)
    }

    fn effect(&self, state: &ClientState, id: Option<i32>) -> Option<i32> {
        state.player.effects.get(&id?).map(|e| e.duration_ticks)
    }

    /// One client tick at the camera position.
    pub fn tick(&mut self, state: &ClientState, camera: [f64; 3]) {
        self.refresh(state);
        self.ticks = self.ticks.wrapping_add(1);
        let blended = self.blend(&state.chunks, camera);
        self.look = (self.look.1, blended);

        self.darkness.0 = self.darkness.1;
        match self.effect(state, self.ids.darkness) {
            Some(left) => {
                let target = if left < 0 || left > DARKNESS_BLEND_TICKS as i32 {
                    1.0
                } else {
                    0.0
                };
                let step = 1.0 / DARKNESS_BLEND_TICKS;
                self.darkness.1 += (target - self.darkness.1).clamp(-step, step);
            }
            None => self.darkness = (0.0, 0.0),
        }

        let spectator = state.player.game_mode == mcr_protocol::types::game::GameType::Spectator;
        if state.player.physics.eye_in_water() {
            let speed = if spectator { 10 } else { 1 };
            self.water_vision_time = (self.water_vision_time + speed).min(WATER_VISION_MAX);
        } else {
            self.water_vision_time = (self.water_vision_time - 10).max(0);
        }

        let mut next = || {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            (self.rng >> 40) as f32 / (1u64 << 24) as f32
        };
        let (a, b, c, d) = (next(), next(), next(), next());
        self.flicker = (self.flicker + (a - b) * c * d * 0.1) * 0.9;

        if let Some(flash) = &mut self.end_flash {
            let clock = clock_ticks(state, "minecraft:the_end").unwrap_or(state.world.game_time);
            flash.tick(clock);
        }
    }

    /// The dimension type's `ambient_light`, for `Lightmap#getBrightness`.
    pub fn ambient_light(&self) -> f32 {
        self.ambient_light
    }

    pub fn piglins_zombify(&self) -> bool {
        self.piglins_zombify
    }

    pub fn end_flash_started(&self) -> bool {
        self.end_flash.is_some_and(|f| f.started_this_tick())
    }

    /// `LocalPlayer#getWaterVision`.
    pub fn water_vision(&self, state: &ClientState) -> f32 {
        if state.player.physics.eye_in_water() {
            water_vision_at(self.water_vision_time)
        } else {
            0.0
        }
    }

    pub(crate) fn fog_kind(state: &ClientState, camera: [f64; 3], forward: [f32; 3]) -> FogKind {
        let chunks = &state.chunks;
        if crate::effects::camera_in_water(chunks, camera) {
            return FogKind::Water;
        }
        let data = mcr_physics::PhysicsData::v26_3();
        let near = [
            camera[0] + f64::from(forward[0]) * 0.05,
            camera[1] + f64::from(forward[1]) * 0.05,
            camera[2] + f64::from(forward[2]) * 0.05,
        ];
        for p in [near, camera] {
            if mcr_physics::camera_in_lava(chunks, data, p) {
                return FogKind::Lava;
            }
            let [x, y, z] = p.map(|v| v.floor() as i32);
            let data = mcr_data::v26_3();
            let snow = chunks.block(x, y, z).is_some_and(|b| {
                data.block_state(b.0)
                    .is_some_and(|s| data.block_name(s.block) == Some("minecraft:powder_snow"))
            });
            if snow {
                return FogKind::PowderSnow;
            }
        }
        FogKind::Atmospheric
    }

    /// The frame's environment: attributes, timelines, weather, mob effects and the camera's fluid.
    pub fn environment(&mut self, f: &VisionFrame<'_>) -> Environment {
        let state = f.state;
        let partial = f.partial.clamp(0.0, 1.0);
        let look = lerp_look(&self.look.0, &self.look.1, partial);
        let w = &state.world;
        let time = if look.day_cycle {
            clock_ticks(state, "minecraft:overworld")
                .or_else(|| w.day_time())
                .unwrap_or(6000)
                .max(0) as u64
        } else {
            0
        };
        let mut env = Environment::from_look(time, w.rain_level, w.thunder_level, &look);
        env = env
            .with_lightning_flash(f.lightning && look.weather && !f.options.hide_lightning_flashes);
        env.set_render_distance_chunks(f.render_distance.max(2) as u32);
        env.boss_darkening = f.boss_darkening;
        env.block_factor = 1.4 + self.flicker;
        if state.dimension.is_flat {
            env.horizon_height = state.dimension.min_y as f32;
        }

        let options = f.options;
        let gamma = if options.gamma.is_finite() {
            options.gamma.clamp(0.0, 1.0) as f32
        } else {
            0.5
        };
        let dark_option = options.darkness_effect_scale.clamp(0.0, 1.0) as f32;
        let darkness_blend = lerp(self.darkness.0, self.darkness.1, partial);
        let darkness_mod = darkness_blend * dark_option;
        env.brightness = (gamma - darkness_mod).max(0.0);
        let pulse = ((self.ticks as f32 - partial) * std::f32::consts::PI * 0.025).cos();
        env.darkness_scale = (pulse * 0.45 * darkness_mod).max(0.0) * dark_option;

        let effects = &state.player.effects;
        let has = |id: Option<i32>| effects.get(&id?).map(|e| e.duration_ticks);
        let night_vision = has(self.ids.night_vision).map(|left| {
            if !(0..=200).contains(&left) {
                1.0
            } else {
                0.7 + ((left as f32 - partial) * std::f32::consts::PI * 0.2).sin() * 0.3
            }
        });
        let water_vision = self.water_vision(state);
        env.night_vision = match night_vision {
            Some(v) => v,
            None if water_vision > 0.0 && has(self.ids.conduit_power).is_some() => water_vision,
            None => 0.0,
        };

        if let Some(flash) = &self.end_flash
            && !options.hide_lightning_flashes
        {
            let intensity = flash.intensity(partial);
            env.sky_factor += if f.boss_fog {
                intensity / 3.0
            } else {
                intensity
            };
            env.end_flash = Some([intensity, flash.x_angle, flash.y_angle]);
        }

        self.update_rain_fog(state, f.camera, f.dt_ticks);
        let darkness_present = has(self.ids.darkness).is_some();
        let spectator = state.player.game_mode == mcr_protocol::types::game::GameType::Spectator;
        let inputs = FogInputs {
            kind: Self::fog_kind(state, f.camera, f.forward),
            forward: f.forward,
            camera_y: f.camera[1] as f32,
            min_y: state.dimension.min_y as f32,
            void_range: if state.dimension.is_flat { 1.0 } else { 32.0 },
            blindness: has(self.ids.blindness)
                .map(|left| if left < 0 { f32::INFINITY } else { left as f32 }),
            darkness: darkness_present.then_some(darkness_blend),
            water_vision,
            night_vision: if darkness_present {
                0.0
            } else {
                night_vision.unwrap_or(0.0)
            },
            spectator,
            fire_resistance: has(self.ids.fire_resistance).is_some(),
            rain_fog: self.rain_fog,
            boss_fog: f.boss_fog,
        };
        env.apply_fog(&inputs);
        env
    }

    /// `AtmosphericFogEnvironment#updateRainFogState`, smoothed by the frame's tick delta.
    fn update_rain_fog(&mut self, state: &ClientState, camera: [f64; 3], dt_ticks: f32) {
        let [x, y, z] = camera.map(|v| v.floor() as i32);
        let sky = crate::entities::light_at(&state.chunks, camera, 0.0)[1];
        let sky_mult = ((f32::from(sky) - 8.0) / 7.0).clamp(0.0, 1.0);
        let rains = crate::ambience::biome_at(state, [x, y, z])
            .and_then(|b| self.precipitation.get(b).copied())
            .unwrap_or(true);
        let rain = if self.dimension.weather {
            state.world.rain_level.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let target = rain * sky_mult * if rains { 1.0 } else { 0.5 };
        let step = (dt_ticks * 0.2).clamp(0.0, 1.0);
        self.rain_fog += (target - self.rain_fog) * step;
        if !self.rain_fog.is_finite() {
            self.rain_fog = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compound(snbt: &str) -> Compound {
        mcr_nbt::snbt::parse(snbt)
            .ok()
            .and_then(Value::into_compound)
            .expect("valid snbt")
    }

    #[test]
    fn biome_attributes_override_and_modify() {
        let swamp = Layer::parse(&compound(
            r##"{"minecraft:visual/sky_color":"#78a7ff","minecraft:visual/water_fog_color":"#232317","minecraft:visual/water_fog_end_distance":{argument:0.85f,modifier:"multiply"}}"##,
        ));
        let look = swamp.apply(&Look::overworld());
        assert_eq!(look.water_fog, mcr_render::hex(0x232317));
        assert!((look.water_fog_end - 96.0 * 0.85).abs() < 1e-3);
        assert_eq!(look.sky, mcr_render::hex(0x78a7ff));
    }

    fn layer(snbt: &str) -> Option<Box<Layer>> {
        Some(Box::new(Layer::parse(&compound(snbt))))
    }

    #[test]
    fn biome_sky_colour_blends_across_a_border() {
        let vision = Vision {
            biomes: vec![
                None,
                layer(r##"{"minecraft:visual/sky_color":"#ff0000"}"##),
                layer(r##"{"minecraft:visual/sky_color":"#0000ff"}"##),
            ],
            ..Vision::default()
        };
        let split = |q: [i32; 3]| Some(if q[0] >= 0 { 1 } else { 2 });
        let east = vision.blend_with([40.0, 64.0, 0.0], split);
        let west = vision.blend_with([-40.0, 64.0, 0.0], split);
        assert_eq!((east.sky, west.sky), ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
        let border = vision.blend_with([0.0, 64.0, 0.0], split);
        assert!(
            border.sky[0] > 0.2 && border.sky[2] > 0.2,
            "{:?}",
            border.sky
        );
        let unloaded = vision.blend_with([0.0, 64.0, 0.0], |_| None);
        assert_eq!(unloaded.sky, Look::overworld().sky);
    }

    fn state() -> ClientState {
        ClientState::new(&mcr_client::ClientConfig::default(), uuid::Uuid::nil())
    }

    fn frame<'a>(state: &'a ClientState, options: &'a Options) -> VisionFrame<'a> {
        VisionFrame {
            state,
            options,
            camera: [0.5, 80.0, 0.5],
            forward: [0.0, 0.0, 1.0],
            partial: 0.0,
            dt_ticks: 1.0,
            render_distance: 12,
            lightning: false,
            boss_darkening: 0.0,
            boss_fog: false,
        }
    }

    fn effect(name: &str, duration_ticks: i32) -> (i32, mcr_client::MobEffect) {
        let id = mcr_data::v26_3()
            .registry("minecraft:mob_effect")
            .and_then(|r| r.id_of(name))
            .unwrap() as i32;
        (
            id,
            mcr_client::MobEffect {
                amplifier: 0,
                duration_ticks,
                flags: 0,
            },
        )
    }

    #[test]
    fn flat_worlds_lower_the_horizon_to_the_world_bottom() {
        let mut s = state();
        let options = Options::default();
        let mut vision = Vision::default();
        assert_eq!(
            vision.environment(&frame(&s, &options)).horizon_height,
            63.0
        );
        s.dimension.is_flat = true;
        s.dimension.min_y = -64;
        assert_eq!(
            vision.environment(&frame(&s, &options)).horizon_height,
            -64.0
        );
    }

    #[test]
    fn effects_reach_the_lightmap_and_fog() {
        let mut s = state();
        let options = Options::default();
        let mut vision = Vision::default();
        let plain = vision.environment(&frame(&s, &options));
        assert_eq!(plain.night_vision, 0.0);
        s.player
            .effects
            .extend([effect("minecraft:night_vision", 1000)]);
        assert_eq!(vision.environment(&frame(&s, &options)).night_vision, 1.0);
        s.player
            .effects
            .extend([effect("minecraft:blindness", 100)]);
        let blind = vision.environment(&frame(&s, &options));
        assert_eq!(blind.fog_end, 5.0);
        assert_eq!(blind.skybox, Skybox::None);
        s.player.effects.clear();
        s.player.effects.extend([effect("minecraft:darkness", 200)]);
        for _ in 0..30 {
            vision.tick(&s, [0.5, 80.0, 0.5]);
        }
        let dark = vision.environment(&frame(&s, &options));
        assert_eq!(dark.fog_end, 15.0);
        assert!(dark.brightness < plain.brightness);
    }

    #[test]
    fn hostile_attribute_values_are_ignored() {
        let layer = Layer::parse(&compound(
            r##"{"minecraft:visual/sky_color":"#zzzzzz","minecraft:visual/fog_end_distance":{argument:"x",modifier:"multiply"},"minecraft:visual/fog_start_distance":{modifier:"pow",argument:2.0f}}"##,
        ));
        assert!(layer.is_empty());
        let nan = FloatOp::Multiply(f32::NAN).apply(5.0);
        assert_eq!(nan, 5.0);
    }

    #[test]
    fn end_flash_follows_its_interval() {
        let mut flash = EndFlash::default();
        let mut peak = 0.0f32;
        let mut lit = 0;
        for t in 0..600 {
            flash.tick(t);
            peak = peak.max(flash.intensity);
            lit += i32::from(flash.intensity > 0.0);
        }
        assert!(peak > 0.9 && peak <= 1.0);
        assert!((100..=380).contains(&lit), "{lit}");
        assert!((-60.0..=10.0).contains(&flash.x_angle));
    }

    #[test]
    fn java_random_matches_known_sequence() {
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next_int(10), 0);
        assert_eq!(r.next_int(10), 3);
        assert_eq!(r.next_int(16), 10);
        assert_eq!(r.next_int(201), 101);
    }

    #[test]
    fn end_flash_parameters_match_java_random() {
        let mut flash = EndFlash::default();
        flash.tick(3000);
        assert_eq!((flash.offset, flash.duration), (76, 198));
        assert!((flash.x_angle + 17.172_733).abs() < 1e-4);
    }

    #[test]
    fn water_vision_ramps_like_vanilla() {
        assert_eq!(water_vision_at(0), 0.0);
        assert!((water_vision_at(50) - 0.3).abs() < 1e-6);
        assert!((water_vision_at(100) - 0.6).abs() < 1e-6);
        assert!((water_vision_at(350) - 0.8).abs() < 1e-6);
        assert_eq!(water_vision_at(600), 1.0);
    }
}
