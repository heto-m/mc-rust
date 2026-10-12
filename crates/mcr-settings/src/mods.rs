use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::Path;

use serde_json::{Map, Value, json};

pub const FILE_NAME: &str = "mcr-mods.json";
pub const MIN_HEAP_MIB: u32 = 1024;
pub const MAX_HEAP_MIB: u32 = 4096;
pub const DEFAULT_HEAP_MIB: u32 = 2048;
const MAX_FILE_BYTES: u64 = 1 << 20;
const MAX_ENTRIES: usize = 4096;
const MAX_ID_LEN: usize = 128;

/// Mod manager state: which mods are enabled by id, the Java heap, and the crash fallback flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModSettings {
    enabled: BTreeMap<String, bool>,
    pub disabled_all: bool,
    heap_mib: u32,
}

impl Default for ModSettings {
    fn default() -> Self {
        Self {
            enabled: BTreeMap::new(),
            disabled_all: false,
            heap_mib: DEFAULT_HEAP_MIB,
        }
    }
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
        if let Some(mib) = root["jvm"]["heap_mib"].as_u64() {
            out.set_heap_mib(u32::try_from(mib).unwrap_or(MAX_HEAP_MIB));
        }
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
            "jvm": { "heap_mib": self.heap_mib },
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

    pub fn heap_mib(&self) -> u32 {
        self.heap_mib
    }

    pub fn set_heap_mib(&mut self, mib: u32) {
        self.heap_mib = mib.clamp(MIN_HEAP_MIB, MAX_HEAP_MIB);
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
        assert_eq!(ModSettings::default().heap_mib(), 2048);
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
        assert_eq!(s.heap_mib(), MIN_HEAP_MIB);
        s.set_heap_mib(u32::MAX);
        assert_eq!(s.heap_mib(), MAX_HEAP_MIB);
        let parsed = ModSettings::parse(r#"{"jvm":{"heap_mib":99999999999999}}"#);
        assert_eq!(parsed.heap_mib(), MAX_HEAP_MIB);
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
