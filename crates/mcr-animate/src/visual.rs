//! The `visual/*` environment attributes the scan reads, parsed from synced registry NBT.

use mcr_nbt::{Compound, List, Value};
use mcr_particles::PARTICLE_NAMES;
use mcr_protocol::types::particle::{ColorParticleKind, Particle};

pub const MAX_AMBIENT_PARTICLES: usize = 16;

const AMBIENT_PARTICLES: &str = "minecraft:visual/ambient_particles";
const DRIPSTONE_PARTICLE: &str = "minecraft:visual/default_dripstone_particle";
const WATER_EVAPORATES: &str = "minecraft:gameplay/water_evaporates";
const FIREFLY_BUSH_SOUNDS: &str = "minecraft:audio/firefly_bush_sounds";

const COMPLEX: [&str; 22] = [
    "block",
    "block_marker",
    "falling_dust",
    "dust_pillar",
    "block_crumble",
    "item",
    "vibration",
    "trail",
    "shriek",
    "effect",
    "instant_effect",
    "geyser",
    "geyser_plume",
    "geyser_base",
    "geyser_poof",
    "sculk_charge",
    "dragon_breath",
    "dust",
    "dust_color_transition",
    "entity_effect",
    "tinted_leaves",
    "flash",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Visual {
    pub ambient: Vec<(Particle, f32)>,
    pub dripstone: Option<Particle>,
    pub water_evaporates: Option<bool>,
    pub firefly_sounds: Option<bool>,
    ambient_set: bool,
    append: bool,
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Byte(n) => Some(f64::from(*n)),
        Value::Short(n) => Some(f64::from(*n)),
        Value::Int(n) => Some(f64::from(*n)),
        Value::Long(n) => Some(*n as f64),
        Value::Float(n) => Some(f64::from(*n)),
        Value::Double(n) => Some(*n),
        _ => None,
    }
}

fn unit(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn color(v: Option<&Value>) -> Option<i32> {
    match v? {
        Value::Int(n) => Some(*n & 0x00FF_FFFF),
        Value::List(List::Float(f)) if f.len() == 3 => {
            let c = |i: usize| (unit(f64::from(f[i])) * 255.0).round() as i32;
            Some(c(0) << 16 | c(1) << 8 | c(2))
        }
        _ => None,
    }
}

fn scale(c: &Compound) -> f32 {
    let s = c.get("scale").and_then(number).unwrap_or(1.0);
    if s.is_finite() {
        s.clamp(0.01, 4.0) as f32
    } else {
        1.0
    }
}

pub fn parse_particle(c: &Compound) -> Option<Particle> {
    let ty = c.get_str("type")?;
    let name = ty.strip_prefix("minecraft:").unwrap_or(ty);
    let id = PARTICLE_NAMES.iter().position(|n| *n == name)? as i32;
    match name {
        "dust" => Some(Particle::Dust {
            color: color(c.get("color")).unwrap_or(0xFF_0000),
            scale: scale(c),
        }),
        "dust_color_transition" => Some(Particle::DustColorTransition {
            from: color(c.get("from_color"))?,
            to: color(c.get("to_color"))?,
            scale: scale(c),
        }),
        "tinted_leaves" => Some(Particle::Color {
            kind: ColorParticleKind::TintedLeaves,
            argb: color(c.get("color"))? | 0xFF00_0000u32 as i32,
        }),
        n if COMPLEX.contains(&n) => None,
        _ => Some(Particle::Simple(id)),
    }
}

impl Visual {
    pub fn from_attributes(attributes: &Compound) -> Self {
        let mut out = Self::default();
        if let Some(value) = attributes.get(AMBIENT_PARTICLES) {
            out.ambient_set = true;
            let list = match value {
                Value::Compound(c) if c.contains_key("argument") => {
                    out.append = c.get_str("modifier") == Some("append");
                    c.get("argument")
                }
                other => Some(other),
            };
            if let Some(Value::List(List::Compound(entries))) = list {
                for entry in entries.iter().take(MAX_AMBIENT_PARTICLES) {
                    let parsed = entry.get_compound("particle").and_then(parse_particle);
                    let probability = entry.get("probability").and_then(number);
                    if let (Some(p), Some(prob)) = (parsed, probability) {
                        out.ambient.push((p, unit(prob) as f32));
                    }
                }
            }
        }
        out.dripstone = plain(attributes, DRIPSTONE_PARTICLE)
            .and_then(Value::as_compound)
            .and_then(parse_particle);
        out.water_evaporates = plain(attributes, WATER_EVAPORATES)
            .and_then(number)
            .map(|v| v != 0.0);
        out.firefly_sounds = plain(attributes, FIREFLY_BUSH_SOUNDS)
            .and_then(number)
            .map(|v| v != 0.0);
        out
    }

    pub fn from_registry_entry(entry: &Compound) -> Self {
        entry
            .get_compound("attributes")
            .map(Self::from_attributes)
            .unwrap_or_default()
    }

    /// The biome's value wins attribute by attribute over the dimension's; an `append` list follows it.
    pub fn over(&self, dimension: &Self) -> Self {
        let mut ambient = if !self.ambient_set {
            dimension.ambient.clone()
        } else if self.append {
            let mut both = dimension.ambient.clone();
            both.extend(self.ambient.iter().cloned());
            both
        } else {
            self.ambient.clone()
        };
        ambient.truncate(MAX_AMBIENT_PARTICLES);
        Self {
            ambient,
            dripstone: self
                .dripstone
                .clone()
                .or_else(|| dimension.dripstone.clone()),
            water_evaporates: self.water_evaporates.or(dimension.water_evaporates),
            firefly_sounds: self.firefly_sounds.or(dimension.firefly_sounds),
            ambient_set: true,
            append: false,
        }
    }
}

fn plain<'a>(attributes: &'a Compound, key: &str) -> Option<&'a Value> {
    match attributes.get(key)? {
        Value::Compound(c) if c.contains_key("modifier") || c.contains_key("argument") => {
            c.get("argument")
        }
        other => Some(other),
    }
}
