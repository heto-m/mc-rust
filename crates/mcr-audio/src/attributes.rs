//! The `audio/*` environment attributes of a dimension type or biome, parsed from synced registry NBT.

use mcr_nbt::{Compound, List, Value};

pub const MAX_MUSIC_DELAY: u32 = 1_728_000;
const MAX_ADDITIONS: usize = 8;
const MAX_SEARCH_EXTENT: i32 = 32;
const MAX_OFFSET: f64 = 64.0;
const MIN_MOOD_DELAY: i32 = 1;
const MAX_MOOD_DELAY: i32 = 1_000_000;

const BACKGROUND_MUSIC: &str = "minecraft:audio/background_music";
const AMBIENT_SOUNDS: &str = "minecraft:audio/ambient_sounds";
const MUSIC_VOLUME: &str = "minecraft:audio/music_volume";
const FIREFLY_BUSH_SOUNDS: &str = "minecraft:audio/firefly_bush_sounds";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Music {
    pub sound: String,
    pub min_delay: u32,
    pub max_delay: u32,
    pub replace_current: bool,
}

impl Music {
    fn new(sound: &str, min_delay: u32, max_delay: u32, replace_current: bool) -> Self {
        Self {
            sound: sound.to_owned(),
            min_delay,
            max_delay,
            replace_current,
        }
    }

    pub fn menu() -> Self {
        Self::new("music.menu", 20, 600, true)
    }

    pub fn creative() -> Self {
        Self::new("music.creative", 12_000, 24_000, false)
    }

    pub fn game() -> Self {
        Self::new("music.game", 12_000, 24_000, false)
    }

    pub fn credits() -> Self {
        Self::new("music.credits", 0, 0, true)
    }

    pub fn end_boss() -> Self {
        Self::new("music.dragon", 0, 0, true)
    }

    pub fn end() -> Self {
        Self::new("music.end", 6_000, 24_000, true)
    }

    fn from_nbt(c: &Compound) -> Option<Self> {
        let delay = |key| {
            let v = c.get(key).and_then(number)?;
            Some(v.clamp(0.0, f64::from(MAX_MUSIC_DELAY)) as u32)
        };
        Some(Self {
            sound: sound_name(c.get("sound")?)?,
            min_delay: delay("min_delay")?,
            max_delay: delay("max_delay")?,
            replace_current: c.get_bool("replace_current_music").unwrap_or(false),
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackgroundMusic {
    pub default: Option<Music>,
    pub creative: Option<Music>,
    pub underwater: Option<Music>,
}

impl BackgroundMusic {
    pub fn select(&self, creative: bool, underwater: bool) -> Option<&Music> {
        if underwater && self.underwater.is_some() {
            return self.underwater.as_ref();
        }
        if creative && self.creative.is_some() {
            return self.creative.as_ref();
        }
        self.default.as_ref()
    }

    fn from_nbt(c: &Compound) -> Self {
        let part = |key| c.get_compound(key).and_then(Music::from_nbt);
        Self {
            default: part("default"),
            creative: part("creative"),
            underwater: part("underwater"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mood {
    pub sound: String,
    pub tick_delay: i32,
    pub block_search_extent: i32,
    pub offset: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Additions {
    pub sound: String,
    pub tick_chance: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AmbientSounds {
    pub loop_sound: Option<String>,
    pub mood: Option<Mood>,
    pub additions: Vec<Additions>,
}

impl AmbientSounds {
    fn from_nbt(c: &Compound) -> Self {
        let mood = c.get_compound("mood").and_then(|m| {
            Some(Mood {
                sound: sound_name(m.get("sound")?)?,
                tick_delay: (m.get("tick_delay").and_then(number)?)
                    .clamp(f64::from(MIN_MOOD_DELAY), f64::from(MAX_MOOD_DELAY))
                    as i32,
                block_search_extent: (m.get("block_search_extent").and_then(number)?)
                    .clamp(0.0, f64::from(MAX_SEARCH_EXTENT))
                    as i32,
                offset: sane(m.get("offset").and_then(number)?, MAX_OFFSET),
            })
        });
        let mut additions = Vec::new();
        let mut add = |a: &Compound| {
            let parsed = (|| {
                Some(Additions {
                    sound: sound_name(a.get("sound")?)?,
                    tick_chance: sane(a.get("tick_chance").and_then(number)?, 1.0).max(0.0),
                })
            })();
            if let Some(a) = parsed
                && additions.len() < MAX_ADDITIONS
            {
                additions.push(a);
            }
        };
        match c.get("additions") {
            Some(Value::Compound(a)) => add(a),
            Some(Value::List(List::Compound(list))) => list.iter().for_each(&mut add),
            _ => {}
        }
        Self {
            loop_sound: c.get("loop").and_then(sound_name),
            mood,
            additions,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioAttributes {
    pub background_music: Option<BackgroundMusic>,
    pub ambient_sounds: Option<AmbientSounds>,
    pub music_volume: Option<f32>,
    pub firefly_bush_sounds: Option<bool>,
}

impl AudioAttributes {
    pub fn from_nbt(attributes: &Compound) -> Self {
        Self {
            background_music: attribute(attributes, BACKGROUND_MUSIC)
                .and_then(Value::as_compound)
                .map(BackgroundMusic::from_nbt),
            ambient_sounds: attribute(attributes, AMBIENT_SOUNDS)
                .and_then(Value::as_compound)
                .map(AmbientSounds::from_nbt),
            music_volume: attribute(attributes, MUSIC_VOLUME)
                .and_then(number)
                .map(|v| sane(v, 1.0).max(0.0) as f32),
            firefly_bush_sounds: attribute(attributes, FIREFLY_BUSH_SOUNDS)
                .and_then(number)
                .map(|v| v != 0.0),
        }
    }

    /// Biome registry entry: its attributes only.
    pub fn from_registry_entry(entry: &Compound) -> Self {
        entry
            .get_compound("attributes")
            .map(Self::from_nbt)
            .unwrap_or_default()
    }

    /// A JSON file of the vanilla data (`worldgen/biome`, `dimension_type`) with an `attributes` object.
    pub fn from_json(bytes: &[u8]) -> Option<Self> {
        let json: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        match json_to_nbt(&json, 0)? {
            Value::Compound(c) => Some(Self::from_registry_entry(&c)),
            _ => None,
        }
    }

    /// The biome's value wins attribute by attribute over the dimension's (the `override` modifier).
    pub fn over(&self, dimension: &Self) -> Self {
        Self {
            background_music: self
                .background_music
                .clone()
                .or_else(|| dimension.background_music.clone()),
            ambient_sounds: self
                .ambient_sounds
                .clone()
                .or_else(|| dimension.ambient_sounds.clone()),
            music_volume: self.music_volume.or(dimension.music_volume),
            firefly_bush_sounds: self.firefly_bush_sounds.or(dimension.firefly_bush_sounds),
        }
    }
}

const MAX_JSON_DEPTH: u32 = 12;

fn json_to_nbt(json: &serde_json::Value, depth: u32) -> Option<Value> {
    use serde_json::Value as J;
    if depth > MAX_JSON_DEPTH {
        return None;
    }
    Some(match json {
        J::Bool(b) => Value::Byte(i8::from(*b)),
        J::Number(n) => match n.as_i64().and_then(|v| i32::try_from(v).ok()) {
            Some(i) => Value::Int(i),
            None => Value::Double(n.as_f64()?),
        },
        J::String(s) => Value::String(s.clone()),
        J::Array(items) => {
            let compounds: Vec<Compound> = items
                .iter()
                .filter_map(|i| match json_to_nbt(i, depth + 1) {
                    Some(Value::Compound(c)) => Some(c),
                    _ => None,
                })
                .collect();
            Value::List(if compounds.is_empty() {
                List::Empty
            } else {
                List::Compound(compounds)
            })
        }
        J::Object(map) => {
            let mut c = Compound::new();
            for (k, v) in map {
                if let Some(v) = json_to_nbt(v, depth + 1) {
                    c.insert(k.clone(), v);
                }
            }
            Value::Compound(c)
        }
        J::Null => return None,
    })
}

fn attribute<'a>(attributes: &'a Compound, key: &str) -> Option<&'a Value> {
    let value = attributes.get(key)?;
    match value {
        Value::Compound(c) if c.contains_key("modifier") || c.contains_key("argument") => {
            c.get("argument")
        }
        other => Some(other),
    }
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

fn sane(v: f64, limit: f64) -> f64 {
    if v.is_finite() {
        v.clamp(-limit, limit)
    } else {
        0.0
    }
}

fn sound_name(v: &Value) -> Option<String> {
    let name = match v {
        Value::String(s) => s.as_str(),
        Value::Compound(c) => c.get_str("sound_id")?,
        _ => return None,
    };
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    (!name.is_empty() && name.len() <= 256).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcr_nbt::snbt;

    fn parse(text: &str) -> AudioAttributes {
        let value = snbt::parse(text).expect("snbt");
        AudioAttributes::from_nbt(value.as_compound().expect("compound"))
    }

    #[test]
    fn dimension_cave_mood_and_music() {
        let a = parse(
            r#"{"minecraft:audio/ambient_sounds":{mood:{block_search_extent:8,offset:2.0d,sound:"minecraft:ambient.cave",tick_delay:6000}},
                "minecraft:audio/background_music":{creative:{max_delay:24000,min_delay:12000,sound:"minecraft:music.creative"},default:{max_delay:24000,min_delay:12000,sound:"minecraft:music.game"}}}"#,
        );
        let mood = a.ambient_sounds.unwrap().mood.unwrap();
        assert_eq!(mood.sound, "ambient.cave");
        assert_eq!((mood.tick_delay, mood.block_search_extent), (6000, 8));
        let music = a.background_music.unwrap();
        assert_eq!(music.select(false, false).unwrap().sound, "music.game");
        assert_eq!(music.select(true, false).unwrap().sound, "music.creative");
        assert_eq!(music.select(true, true).unwrap().sound, "music.creative");
    }

    #[test]
    fn json_form_matches_the_registry_form() {
        let json = br#"{"attributes":{"minecraft:audio/ambient_sounds":{"loop":"minecraft:ambient.nether_wastes.loop",
            "mood":{"block_search_extent":8,"offset":2.0,"sound":"minecraft:ambient.nether_wastes.mood","tick_delay":6000},
            "additions":{"sound":"minecraft:ambient.nether_wastes.additions","tick_chance":0.0111}},
            "minecraft:audio/background_music":{"default":{"max_delay":24000,"min_delay":12000,"sound":"minecraft:music.nether.nether_wastes"}}}}"#;
        let a = AudioAttributes::from_json(json).unwrap();
        let sounds = a.ambient_sounds.unwrap();
        assert_eq!(
            sounds.loop_sound.as_deref(),
            Some("ambient.nether_wastes.loop")
        );
        assert_eq!(sounds.mood.unwrap().tick_delay, 6000);
        assert_eq!(sounds.additions.len(), 1);
        assert_eq!(
            a.background_music.unwrap().default.unwrap().sound,
            "music.nether.nether_wastes"
        );
        assert!(AudioAttributes::from_json(b"not json").is_none());
    }

    #[test]
    fn modifier_form_unwraps_the_argument() {
        let a = parse(
            r#"{"minecraft:audio/background_music":{argument:{default:{max_delay:5,min_delay:1,sound:"a.b"}},modifier:"override"}}"#,
        );
        assert_eq!(a.background_music.unwrap().default.unwrap().sound, "a.b");
    }

    #[test]
    fn additions_accept_one_object_or_a_list() {
        let one = parse(
            r#"{"minecraft:audio/ambient_sounds":{additions:{sound:"x",tick_chance:0.5d},loop:"minecraft:l"}}"#,
        )
        .ambient_sounds
        .unwrap();
        assert_eq!(one.additions.len(), 1);
        assert_eq!(one.loop_sound.as_deref(), Some("l"));
        let many = parse(
            r#"{"minecraft:audio/ambient_sounds":{additions:[{sound:"x",tick_chance:0.5d},{sound:"y",tick_chance:2.0d}]}}"#,
        )
        .ambient_sounds
        .unwrap();
        assert_eq!(many.additions.len(), 2);
        assert_eq!(many.additions[1].tick_chance, 1.0);
    }

    #[test]
    fn hostile_numbers_are_clamped() {
        let a = parse(
            r#"{"minecraft:audio/ambient_sounds":{mood:{block_search_extent:2000000,offset:1.0e300d,sound:"s",tick_delay:-5}},
                "minecraft:audio/background_music":{default:{max_delay:2000000000,min_delay:-1,sound:"m"}}}"#,
        );
        let mood = a.ambient_sounds.unwrap().mood.unwrap();
        assert_eq!(mood.tick_delay, 1);
        assert_eq!(mood.block_search_extent, 32);
        assert_eq!(mood.offset, 64.0);
        let m = a.background_music.unwrap().default.unwrap();
        assert_eq!((m.min_delay, m.max_delay), (0, MAX_MUSIC_DELAY));
    }

    #[test]
    fn biome_overrides_dimension_per_attribute() {
        let dim = parse(
            r#"{"minecraft:audio/ambient_sounds":{mood:{block_search_extent:8,offset:2.0d,sound:"ambient.cave",tick_delay:6000}},
                "minecraft:audio/background_music":{default:{max_delay:2,min_delay:1,sound:"music.game"}}}"#,
        );
        let biome = parse(
            r#"{"minecraft:audio/background_music":{default:{max_delay:2,min_delay:1,sound:"music.nether"}}}"#,
        );
        let merged = biome.over(&dim);
        assert_eq!(
            merged.background_music.unwrap().default.unwrap().sound,
            "music.nether"
        );
        assert!(merged.ambient_sounds.unwrap().mood.is_some());
    }
}
