use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::Path;

use serde_json::{Map, Value, json};

pub const FILE_NAME: &str = "mcr-mods.json";
pub const MIN_HEAP_MIB: u32 = 1024;
pub const MAX_HEAP_MIB: u32 = 12288;
/// What mcr's process needs next to the Java heap (native memory, JVM non-heap) plus the system's room.
const OUTSIDE_HEAP_MIB: u64 = 4608;
/// The smallest automatic heap, what a small mod set gets.
pub const DEFAULT_HEAP_MIB: u32 = 2048;
const MAX_FILE_BYTES: u64 = 1 << 20;
const MAX_ENTRIES: usize = 4096;
const MAX_ID_LEN: usize = 128;

/// The largest heap on a machine with `total_mib` of RAM: three quarters of it, at most
/// [`MAX_HEAP_MIB`].
pub fn max_heap_mib(total_mib: Option<u64>) -> u32 {
    let cap = total_mib.map_or(u64::from(MAX_HEAP_MIB), |t| t * 3 / 4 / 256 * 256);
    u32::try_from(cap.min(u64::from(MAX_HEAP_MIB)))
        .unwrap_or(MAX_HEAP_MIB)
        .max(MIN_HEAP_MIB)
}

/// The Java heap for `mods` enabled client mods. Live heap after a full GC grows by about 4.8 MiB a
/// mod from 510 MiB (Echoes of the Sift, 12 mods: 568 MB; Medieval Immersed, 102 mods: about 1 GB);
/// the heap is 2.3 times that in 256 MiB steps, at least [`DEFAULT_HEAP_MIB`], and leaves
/// [`OUTSIDE_HEAP_MIB`] of the machine to mcr's process and the system.
pub fn auto_heap_mib(mods: usize, total_mib: Option<u64>) -> u32 {
    let live_kib = 510 * 1024 + 4915 * mods as u64;
    let heap = (live_kib * 23 / 10).div_ceil(256 * 1024) * 256;
    let room = total_mib.map_or(u64::MAX, |t| t.saturating_sub(OUTSIDE_HEAP_MIB) / 256 * 256);
    let heap = heap.min(room).min(u64::from(max_heap_mib(total_mib)));
    u32::try_from(heap)
        .unwrap_or(MAX_HEAP_MIB)
        .max(DEFAULT_HEAP_MIB)
}

/// A stored heap the user chose. Files from before the automatic heap always held 2048 without
/// `heap_explicit`; that reads as automatic.
pub fn stored_heap_mib(jvm: &Value) -> Option<u32> {
    let mib = jvm["heap_mib"].as_u64()?;
    let explicit = jvm["heap_explicit"].as_bool().unwrap_or(mib != 2048);
    explicit.then(|| {
        u32::try_from(mib)
            .unwrap_or(MAX_HEAP_MIB)
            .clamp(MIN_HEAP_MIB, MAX_HEAP_MIB)
    })
}

/// Mod manager state: which mods are enabled by id, the Java heap, and the crash fallback flag.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModSettings {
    enabled: BTreeMap<String, bool>,
    pub disabled_all: bool,
    /// `None` is automatic, from the mod count.
    heap_mib: Option<u32>,
}

impl ModSettings {
    /// Reads `{"mods": {"enabled": {id: bool}, "disabled_all": bool}, "jvm": {"heap_mib": n}}`.
    /// Anything malformed or over the bounds is skipped.
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        let Ok(root) = serde_json::from_str::<Value>(text) else {
            tracing::warn!("mod settings are not valid JSON; using defaults");
            return out;
        };
        if let Some(map) = root["mods"]["enabled"].as_object() {
            for (id, value) in map.iter().take(MAX_ENTRIES) {
                if let Some(on) = value.as_bool()
                    && id.len() <= MAX_ID_LEN
                {
                    out.enabled.insert(id.clone(), on);
                }
            }
        }
        out.disabled_all = root["mods"]["disabled_all"].as_bool().unwrap_or(false);
        out.heap_mib = stored_heap_mib(&root["jvm"]);
        out
    }

    pub fn to_text(&self) -> String {
        let enabled: Map<String, Value> = self
            .enabled
            .iter()
            .map(|(id, on)| (id.clone(), Value::Bool(*on)))
            .collect();
        let mut text = serde_json::to_string_pretty(&json!({
            "mods": { "enabled": enabled, "disabled_all": self.disabled_all },
            "jvm": match self.heap_mib {
                Some(mib) => json!({ "heap_mib": mib, "heap_explicit": true }),
                None => json!({}),
            },
        }))
        .unwrap_or_default();
        text.push('\n');
        text
    }

    /// A missing or oversized file yields defaults.
    pub fn load(path: &Path) -> Self {
        let mut text = String::new();
        let read = std::fs::File::open(path)
            .and_then(|f| f.take(MAX_FILE_BYTES + 1).read_to_string(&mut text));
        match read {
            Ok(n) if n as u64 <= MAX_FILE_BYTES => Self::parse(&text),
            Ok(_) => {
                tracing::warn!(path = %path.display(), "mod settings file is too large; ignoring it");
                Self::default()
            }
            Err(e) => {
                if e.kind() != io::ErrorKind::NotFound {
                    tracing::warn!(path = %path.display(), "cannot read mod settings: {e}");
                }
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, self.to_text())
    }

    /// The stored choice, or `default` for a mod never toggled.
    pub fn is_enabled(&self, id: &str, default: bool) -> bool {
        self.enabled.get(id).copied().unwrap_or(default)
    }

    pub fn set_enabled(&mut self, id: &str, on: bool) {
        let room = self.enabled.len() < MAX_ENTRIES || self.enabled.contains_key(id);
        if id.len() <= MAX_ID_LEN && room {
            self.enabled.insert(id.to_owned(), on);
        }
    }

    /// The heap the user chose, `None` when automatic.
    pub fn heap_mib(&self) -> Option<u32> {
        self.heap_mib
    }

    /// The chosen heap, or the automatic one for `mods` enabled client mods.
    pub fn heap_mib_for(&self, mods: usize, total_mib: Option<u64>) -> u32 {
        self.heap_mib
            .unwrap_or_else(|| auto_heap_mib(mods, total_mib))
    }

    pub fn set_heap_mib(&mut self, mib: u32) {
        self.heap_mib = Some(mib.clamp(MIN_HEAP_MIB, MAX_HEAP_MIB));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_empty_or_garbage() {
        for text in ["", "not json", "[]", "{\"mods\": 3}"] {
            assert_eq!(ModSettings::parse(text), ModSettings::default(), "{text}");
        }
        assert_eq!(ModSettings::default().heap_mib(), None);
        assert_eq!(ModSettings::default().heap_mib_for(0, None), 2048);
    }

    #[test]
    fn automatic_heap_grows_with_the_mod_set() {
        assert_eq!(auto_heap_mib(0, None), 2048);
        assert_eq!(auto_heap_mib(12, None), 2048, "Echoes of the Sift");
        assert_eq!(auto_heap_mib(102, None), 2304, "Medieval Immersed");
        assert_eq!(auto_heap_mib(300, None), 4608);
        assert_eq!(auto_heap_mib(400, None), 5632);
        assert_eq!(auto_heap_mib(100_000, None), MAX_HEAP_MIB);
        assert_eq!(
            auto_heap_mib(400, Some(15905)),
            5632,
            "fits a 16 GB machine"
        );
        assert_eq!(
            auto_heap_mib(400, Some(8192)),
            3584,
            "room for mcr's process"
        );
        assert_eq!(auto_heap_mib(2000, Some(65536)), MAX_HEAP_MIB);
        assert_eq!(
            auto_heap_mib(400, Some(2048)),
            2048,
            "never below the default"
        );
        assert_eq!(max_heap_mib(Some(15905)), 11776);
        assert_eq!(max_heap_mib(Some(65536)), MAX_HEAP_MIB);
        assert_eq!(max_heap_mib(Some(1024)), MIN_HEAP_MIB);
        assert_eq!(max_heap_mib(None), MAX_HEAP_MIB);
        let mut last = 0;
        for mods in 0..2000 {
            let heap = auto_heap_mib(mods, Some(64 * 1024));
            assert!(heap >= last && heap.is_multiple_of(256), "{mods}: {heap}");
            last = heap;
        }
    }

    #[test]
    fn a_chosen_heap_wins_and_the_old_default_reads_as_automatic() {
        let mut s = ModSettings::default();
        s.set_heap_mib(2048);
        let back = ModSettings::parse(&s.to_text());
        assert_eq!(back.heap_mib(), Some(2048));
        assert_eq!(back.heap_mib_for(400, None), 2048);
        let legacy = ModSettings::parse(r#"{"jvm":{"heap_mib":2048}}"#);
        assert_eq!(legacy.heap_mib(), None);
        assert_eq!(legacy.heap_mib_for(102, None), 2304);
        let old_choice = ModSettings::parse(r#"{"jvm":{"heap_mib":3072}}"#);
        assert_eq!(old_choice.heap_mib_for(400, None), 3072);
        assert_eq!(
            ModSettings::parse(&ModSettings::default().to_text()).heap_mib(),
            None
        );
    }

    #[test]
    fn round_trips() {
        let mut s = ModSettings::default();
        s.set_enabled("modmenu", false);
        s.set_enabled("fabric-api", true);
        s.disabled_all = true;
        s.set_heap_mib(3072);
        assert_eq!(ModSettings::parse(&s.to_text()), s);
        assert!(!s.is_enabled("modmenu", true));
        assert!(s.is_enabled("unknown", true));
        assert!(!s.is_enabled("unknown", false));
    }

    #[test]
    fn heap_is_clamped() {
        let mut s = ModSettings::default();
        s.set_heap_mib(1);
        assert_eq!(s.heap_mib(), Some(MIN_HEAP_MIB));
        s.set_heap_mib(u32::MAX);
        assert_eq!(s.heap_mib(), Some(MAX_HEAP_MIB));
        let parsed = ModSettings::parse(r#"{"jvm":{"heap_mib":99999999999999}}"#);
        assert_eq!(parsed.heap_mib(), Some(MAX_HEAP_MIB));
    }

    #[test]
    fn entries_are_bounded() {
        let mut s = ModSettings::default();
        for i in 0..MAX_ENTRIES + 10 {
            s.set_enabled(&format!("m{i}"), true);
        }
        assert_eq!(s.enabled.len(), MAX_ENTRIES);
        s.set_enabled("m0", false);
        assert!(!s.is_enabled("m0", true));
        s.set_enabled(&"x".repeat(MAX_ID_LEN + 1), true);
        assert_eq!(s.enabled.len(), MAX_ENTRIES);
    }

    #[test]
    fn non_bool_entries_are_skipped() {
        let s = ModSettings::parse(r#"{"mods":{"enabled":{"a":1,"b":false}}}"#);
        assert!(s.is_enabled("a", true));
        assert!(!s.is_enabled("b", true));
    }
}
