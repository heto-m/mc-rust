use std::fmt::Write as _;
use std::io;
use std::path::Path;

use crate::bindings::{KeyBindings, MAPPINGS, mapping_index};
use crate::keys::InputKey;
use crate::options::{MODEL_PARTS, Options, SOUND_CATEGORIES};
use crate::value::OptionValue;

/// `options.txt`: typed options and key bindings, plus every other `key:value` line kept verbatim.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OptionsFile {
    pub options: Options,
    pub bindings: KeyBindings,
    extra: Vec<(String, String)>,
}

impl OptionsFile {
    /// Parses vanilla `options.txt` text. Bad lines and values are skipped with a warning.
    pub fn parse(text: &str) -> Self {
        let mut options = Options::default();
        let mut inputs = KeyBindings::default().inputs().to_vec();
        let mut extra: Vec<(String, String)> = Vec::new();

        for line in text.lines() {
            if line.is_empty() {
                continue;
            }
            let Some((key, raw)) = line.split_once(':') else {
                tracing::warn!(line, "skipping option line without ':'");
                continue;
            };
            if let Some(name) = key.strip_prefix("key_")
                && let Some(i) = mapping_index(name)
            {
                inputs[i] = InputKey::from_name(raw).unwrap_or_else(|| {
                    tracing::warn!(key, raw, "unknown key name, unbinding");
                    InputKey::Unbound
                });
                continue;
            }
            if options.apply(key, raw) || options.apply_sound_or_part(key, raw) {
                continue;
            }
            match extra.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = raw.to_owned(),
                None => extra.push((key.to_owned(), raw.to_owned())),
            }
        }

        Self {
            options,
            bindings: KeyBindings::from_parts(inputs),
            extra,
        }
    }

    /// Parses raw file bytes. Invalid UTF-8 is replaced rather than rejected.
    pub fn parse_bytes(bytes: &[u8]) -> Self {
        Self::parse(&String::from_utf8_lossy(bytes))
    }

    /// Writes vanilla-format text: options, key bindings, sound volumes, model parts, then unknown keys.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let mut line = |key: &str, value: &str| {
            let _ = writeln!(out, "{key}:{value}");
        };
        for (key, value) in self.options.entries() {
            line(key, &value);
        }
        for (i, mapping) in MAPPINGS.iter().enumerate() {
            let name = format!("key_{}", mapping.name);
            line(&name, self.bindings.input(i).name());
        }
        for (i, category) in SOUND_CATEGORIES.iter().enumerate() {
            let name = format!("soundCategory_{category}");
            line(&name, &self.options.sound_volumes[i].format());
        }
        for (i, part) in MODEL_PARTS.iter().enumerate() {
            let name = format!("modelPart_{part}");
            line(&name, &self.options.model_parts[i].format());
        }
        for (key, value) in &self.extra {
            line(key, value);
        }
        out
    }

    /// Reads `path`. A missing file yields defaults, as vanilla does on first launch.
    pub fn load(path: &Path) -> io::Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Self::parse_bytes(&bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, self.to_text())
    }

    /// Unrecognized `key:value` pairs in file order, kept for round trips.
    pub fn extra(&self) -> impl Iterator<Item = (&str, &str)> {
        self.extra.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}
