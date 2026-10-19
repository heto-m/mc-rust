use rustc_hash::FxHashMap;
use serde::Deserialize;

use crate::error::AudioError;

const MAX_EVENT_DEPTH: u8 = 8;

/// `java.util.Random`, which vanilla's `RandomSource.create(seed)` uses for sound variant choice.
#[derive(Clone, Debug)]
pub struct JavaRandom {
    state: u64,
}

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5DEE_CE66D;
    const MASK: u64 = (1 << 48) - 1;

    pub fn new(seed: i64) -> Self {
        Self {
            state: (seed as u64 ^ Self::MULTIPLIER) & Self::MASK,
        }
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.state = self.state.wrapping_mul(Self::MULTIPLIER).wrapping_add(0xB) & Self::MASK;
        (self.state >> (48 - bits)) as i32
    }

    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }

    pub fn next_double(&mut self) -> f64 {
        let hi = i64::from(self.next(26)) << 27;
        (hi + i64::from(self.next(27))) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// `Mth.nextInt(random, min, max)`: inclusive, and `min` when the range is empty.
    pub fn next_int_between(&mut self, min: i32, max: i32) -> i32 {
        if min >= max {
            min
        } else {
            self.next_int(max - min + 1) + min
        }
    }

    pub fn next_int(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        if bound & (bound - 1) == 0 {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let value = bits % bound;
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSound {
    pub name: String,
    pub volume: f32,
    pub pitch: f32,
    pub stream: bool,
    pub preload: bool,
    pub attenuation_distance: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Event,
}

#[derive(Debug, Clone)]
struct Entry {
    kind: EntryKind,
    name: String,
    volume: f32,
    pitch: f32,
    weight: u32,
    stream: bool,
    preload: bool,
    attenuation_distance: i32,
}

#[derive(Debug, Clone, Default)]
struct Event {
    entries: Vec<Entry>,
    subtitle: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawEntry {
    Name(String),
    Full(RawSound),
}

#[derive(Deserialize)]
struct RawSound {
    name: String,
    #[serde(default = "one_f32")]
    volume: f32,
    #[serde(default = "one_f32")]
    pitch: f32,
    #[serde(default = "one_u32")]
    weight: u32,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    preload: bool,
    #[serde(default = "default_attenuation")]
    attenuation_distance: i32,
    #[serde(default, rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct RawEvent {
    #[serde(default)]
    sounds: Vec<RawEntry>,
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    subtitle: Option<String>,
}

fn one_f32() -> f32 {
    1.0
}

fn one_u32() -> u32 {
    1
}

fn default_attenuation() -> i32 {
    16
}

impl From<RawEntry> for Entry {
    fn from(raw: RawEntry) -> Self {
        match raw {
            RawEntry::Name(name) => Entry {
                kind: EntryKind::File,
                name,
                volume: 1.0,
                pitch: 1.0,
                weight: 1,
                stream: false,
                preload: false,
                attenuation_distance: 16,
            },
            RawEntry::Full(s) => Entry {
                kind: if s.kind.as_deref() == Some("event") {
                    EntryKind::Event
                } else {
                    EntryKind::File
                },
                name: s.name,
                volume: s.volume,
                pitch: s.pitch,
                weight: s.weight,
                stream: s.stream,
                preload: s.preload,
                attenuation_distance: s.attenuation_distance,
            },
        }
    }
}

pub fn strip_namespace(id: &str) -> &str {
    id.strip_prefix("minecraft:").unwrap_or(id)
}

#[derive(Debug, Default)]
pub struct SoundEvents {
    events: FxHashMap<String, Event>,
}

impl SoundEvents {
    pub fn parse(json: &str) -> Result<Self, AudioError> {
        let raw: FxHashMap<String, RawEvent> = serde_json::from_str(json)?;
        let mut events = Self::default();
        events.merge(raw);
        Ok(events)
    }

    /// Merges a resource pack's `sounds.json` for `namespace`, as vanilla layers pack sound definitions.
    pub fn merge_namespaced(&mut self, namespace: &str, json: &str) -> Result<(), AudioError> {
        let raw: FxHashMap<String, RawEvent> = serde_json::from_str(json)?;
        if namespace == "minecraft" {
            self.merge(raw);
            return Ok(());
        }
        let qualify = |name: String| {
            if name.contains(':') {
                name
            } else {
                format!("{namespace}:{name}")
            }
        };
        let raw = raw
            .into_iter()
            .map(|(id, mut event)| {
                for entry in &mut event.sounds {
                    match entry {
                        RawEntry::Name(n) => *n = qualify(std::mem::take(n)),
                        RawEntry::Full(s) => s.name = qualify(std::mem::take(&mut s.name)),
                    }
                }
                (format!("{namespace}:{id}"), event)
            })
            .collect();
        self.merge(raw);
        Ok(())
    }

    fn merge(&mut self, raw: FxHashMap<String, RawEvent>) {
        for (id, event) in raw {
            let slot = self.events.entry(id).or_default();
            if event.replace {
                slot.entries.clear();
            }
            if event.subtitle.is_some() {
                slot.subtitle = event.subtitle;
            }
            slot.entries
                .extend(event.sounds.into_iter().map(Entry::from));
        }
    }

    /// The event's `subtitle` translation key, as `WeighedSoundEvents#getSubtitle`.
    pub fn subtitle(&self, event: &str) -> Option<&str> {
        self.events.get(strip_namespace(event))?.subtitle.as_deref()
    }

    pub fn contains(&self, event: &str) -> bool {
        self.events.contains_key(strip_namespace(event))
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn preloads(&self) -> impl Iterator<Item = &str> {
        self.events
            .values()
            .flat_map(|e| &e.entries)
            .filter(|e| e.kind == EntryKind::File && e.preload)
            .map(|e| e.name.as_str())
    }

    fn event_weight(&self, event: &Event, depth: u8) -> u32 {
        event
            .entries
            .iter()
            .map(|e| self.entry_weight(e, depth))
            .sum()
    }

    fn entry_weight(&self, entry: &Entry, depth: u8) -> u32 {
        match entry.kind {
            EntryKind::File => entry.weight,
            EntryKind::Event if depth >= MAX_EVENT_DEPTH => 0,
            EntryKind::Event => self
                .events
                .get(strip_namespace(&entry.name))
                .map_or(0, |e| self.event_weight(e, depth + 1)),
        }
    }

    pub fn pick(&self, event: &str, rng: &mut JavaRandom) -> Option<ResolvedSound> {
        self.pick_in(self.events.get(strip_namespace(event))?, rng, 0)
    }

    fn pick_in(&self, event: &Event, rng: &mut JavaRandom, depth: u8) -> Option<ResolvedSound> {
        let total = self.event_weight(event, depth);
        if total == 0 {
            return None;
        }
        let mut index = rng.next_int(i32::try_from(total).ok()?);
        for entry in &event.entries {
            index -= i32::try_from(self.entry_weight(entry, depth)).ok()?;
            if index < 0 {
                return self.resolve(entry, rng, depth);
            }
        }
        None
    }

    fn resolve(&self, entry: &Entry, rng: &mut JavaRandom, depth: u8) -> Option<ResolvedSound> {
        match entry.kind {
            EntryKind::File => Some(ResolvedSound {
                name: entry.name.clone(),
                volume: entry.volume,
                pitch: entry.pitch,
                stream: entry.stream,
                preload: entry.preload,
                attenuation_distance: entry.attenuation_distance as f32,
            }),
            EntryKind::Event => {
                let target = self.events.get(strip_namespace(&entry.name))?;
                let mut inner = self.pick_in(target, rng, depth + 1)?;
                inner.volume *= entry.volume;
                inner.pitch *= entry.pitch;
                inner.stream |= entry.stream;
                Some(inner)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{
        "a": {"sounds": ["x/one", {"name": "x/two", "weight": 3, "volume": 0.5, "pitch": 0.8,
              "stream": true, "attenuation_distance": 32, "preload": true}]},
        "b": {"sounds": [{"name": "a", "type": "event", "volume": 0.5, "pitch": 2}]},
        "loop": {"sounds": [{"name": "loop", "type": "event"}]},
        "r": {"sounds": ["old"]}
    }"#;

    #[test]
    fn subtitles_parse_and_packs_override_them() {
        let mut e = SoundEvents::parse(
            r#"{"entity.pig.ambient": {"sounds": ["mob/pig/say1"], "subtitle": "subtitles.entity.pig.ambient"}, "quiet": {"sounds": ["q"]}}"#,
        )
        .unwrap();
        assert_eq!(
            e.subtitle("minecraft:entity.pig.ambient"),
            Some("subtitles.entity.pig.ambient")
        );
        assert_eq!(e.subtitle("quiet"), None);
        e.merge_namespaced("minecraft", r#"{"quiet": {"subtitle": "pack.quiet"}}"#)
            .unwrap();
        assert_eq!(e.subtitle("quiet"), Some("pack.quiet"));
        assert!(e.pick("quiet", &mut JavaRandom::new(0)).is_some());
    }

    fn spread(seed: i64) -> i64 {
        seed.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as i64)
    }

    fn events() -> SoundEvents {
        SoundEvents::parse(JSON).unwrap()
    }

    #[test]
    fn java_random_matches_reference_values() {
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next_int(100), 30);
        assert_eq!(r.next_int(100), 63);
        let mut r = JavaRandom::new(0);
        assert_eq!(r.next_int(10), 0);
        assert_eq!(r.next_int(16), 13);
    }

    #[test]
    fn bare_string_gets_defaults() {
        let ev = SoundEvents::parse(r#"{"a":{"sounds":["x/one"]}}"#).unwrap();
        let s = ev.pick("minecraft:a", &mut JavaRandom::new(1)).unwrap();
        assert_eq!(s.name, "x/one");
        assert_eq!(
            (s.volume, s.pitch, s.attenuation_distance),
            (1.0, 1.0, 16.0)
        );
        assert!(!s.stream && !s.preload);
    }

    #[test]
    fn same_seed_same_variant() {
        let ev = events();
        for seed in 0..50 {
            let a = ev.pick("a", &mut JavaRandom::new(seed));
            let b = ev.pick("a", &mut JavaRandom::new(seed));
            assert_eq!(a, b);
        }
    }

    #[test]
    fn weights_bias_selection() {
        let ev = events();
        let heavy = (0..4000)
            .filter(|&seed| {
                ev.pick("a", &mut JavaRandom::new(spread(seed)))
                    .unwrap()
                    .name
                    == "x/two"
            })
            .count();
        assert!((2800..3200).contains(&heavy), "{heavy}");
    }

    #[test]
    fn event_reference_multiplies_volume_and_pitch() {
        let ev = events();
        let s = (0..100)
            .map(|seed| ev.pick("b", &mut JavaRandom::new(seed)).unwrap())
            .find(|s| s.name == "x/two")
            .unwrap();
        assert!((s.volume - 0.25).abs() < 1e-6);
        assert!((s.pitch - 1.6).abs() < 1e-6);
        assert!(s.stream && s.preload);
        assert_eq!(s.attenuation_distance, 32.0);
    }

    #[test]
    fn unknown_and_cyclic_events_pick_nothing() {
        let ev = events();
        assert!(ev.pick("nope", &mut JavaRandom::new(1)).is_none());
        assert!(ev.pick("loop", &mut JavaRandom::new(1)).is_none());
    }

    #[test]
    fn replace_discards_earlier_entries() {
        let mut ev = SoundEvents::parse(JSON).unwrap();
        let raw: FxHashMap<String, RawEvent> =
            serde_json::from_str(r#"{"r":{"replace":true,"sounds":["new"]}}"#).unwrap();
        ev.merge(raw);
        assert_eq!(ev.pick("r", &mut JavaRandom::new(1)).unwrap().name, "new");
    }

    #[test]
    fn preloads_lists_flagged_files() {
        assert_eq!(events().preloads().collect::<Vec<_>>(), ["x/two"]);
    }

    #[test]
    fn pack_namespaces_qualify_events_and_files() {
        let mut ev = SoundEvents::parse(JSON).unwrap();
        ev.merge_namespaced("mypack", r#"{"boom":{"sounds":["fx/boom"]}}"#)
            .unwrap();
        ev.merge_namespaced("minecraft", r#"{"r":{"replace":true,"sounds":["mine"]}}"#)
            .unwrap();
        let boom = ev.pick("mypack:boom", &mut JavaRandom::new(1)).unwrap();
        assert_eq!(boom.name, "mypack:fx/boom");
        assert_eq!(ev.pick("r", &mut JavaRandom::new(1)).unwrap().name, "mine");
    }
}
