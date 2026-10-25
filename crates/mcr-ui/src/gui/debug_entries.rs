//! The debug screen's entries and profiles (`DebugScreenEntries`, `DebugScreenEntryList`): which
//! lines and renderers show always, only with the F3 overlay, or never.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

const MAX_STATUSES: usize = 512;
const MAX_ID_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryStatus {
    AlwaysOn,
    InOverlay,
    Never,
}

impl EntryStatus {
    fn name(self) -> &'static str {
        match self {
            Self::AlwaysOn => "alwaysOn",
            Self::InOverlay => "inOverlay",
            Self::Never => "never",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "alwaysOn" => Self::AlwaysOn,
            "inOverlay" => Self::InOverlay,
            "never" => Self::Never,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugProfile {
    Default,
    Performance,
}

impl DebugProfile {
    pub const ALL: [Self; 2] = [Self::Default, Self::Performance];

    pub fn lang_key(self) -> &'static str {
        match self {
            Self::Default => "debug.options.profile.default",
            Self::Performance => "debug.options.profile.performance",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Performance => "performance",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }

    /// `DebugScreenEntries.PROFILES`.
    fn statuses(self) -> &'static [(&'static str, EntryStatus)] {
        use EntryStatus::{AlwaysOn, InOverlay};
        match self {
            Self::Default => &[
                ("3d_crosshair", InOverlay),
                ("game_version", InOverlay),
                ("tps", InOverlay),
                ("fps", InOverlay),
                ("memory", InOverlay),
                ("system_specs", InOverlay),
                ("player_position", InOverlay),
                ("player_section_position", InOverlay),
                ("simple_performance_impactors", InOverlay),
            ],
            Self::Performance => &[
                ("tps", InOverlay),
                ("fps", AlwaysOn),
                ("gpu_utilization", InOverlay),
                ("memory", InOverlay),
                ("simple_performance_impactors", InOverlay),
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Text,
    Renderer,
}

impl Category {
    pub fn lang_key(self) -> &'static str {
        match self {
            Self::Text => "debug.options.category.text",
            Self::Renderer => "debug.options.category.renderer",
        }
    }
}

/// An entry this client implements; `reduced_ok` entries stay visible under reduced debug info.
#[derive(Clone, Copy, Debug)]
pub struct EntryDef {
    pub id: &'static str,
    pub category: Category,
    pub reduced_ok: bool,
}

const fn entry(id: &'static str, category: Category, reduced_ok: bool) -> EntryDef {
    EntryDef {
        id,
        category,
        reduced_ok,
    }
}

/// Sorted by category, then id: the options screen lists them so, and F3 text is written in id order.
pub const ENTRIES: [EntryDef; 30] = [
    entry("biome", Category::Text, false),
    entry("chunk_render_stats", Category::Text, true),
    entry("chunk_source_stats", Category::Text, true),
    entry("day_count", Category::Text, false),
    entry("detailed_memory", Category::Text, true),
    entry("entity_render_stats", Category::Text, true),
    entry("fps", Category::Text, true),
    entry("game_version", Category::Text, true),
    entry("gpu_utilization", Category::Text, true),
    entry("heightmap", Category::Text, false),
    entry("light_levels", Category::Text, false),
    entry("looking_at_block_state", Category::Text, false),
    entry("looking_at_block_tags", Category::Text, false),
    entry("looking_at_entity", Category::Text, false),
    entry("looking_at_entity_tags", Category::Text, false),
    entry("looking_at_fluid_state", Category::Text, false),
    entry("looking_at_fluid_tags", Category::Text, false),
    entry("memory", Category::Text, true),
    entry("particle_render_stats", Category::Text, false),
    entry("player_position", Category::Text, false),
    entry("player_section_position", Category::Text, true),
    entry("player_speed", Category::Text, false),
    entry("post_effects", Category::Text, false),
    entry("simple_performance_impactors", Category::Text, true),
    entry("sound_cache", Category::Text, true),
    entry("sound_mood", Category::Text, false),
    entry("system_specs", Category::Text, true),
    entry("tps", Category::Text, true),
    entry("chunk_borders", Category::Renderer, false),
    entry("entity_hitboxes", Category::Renderer, false),
];

pub fn entry_def(id: &str) -> Option<&'static EntryDef> {
    ENTRIES.iter().find(|e| e.id == id)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugEntries {
    profile: Option<DebugProfile>,
    statuses: BTreeMap<String, EntryStatus>,
}

impl Default for DebugEntries {
    fn default() -> Self {
        Self::with_profile(DebugProfile::Default)
    }
}

impl DebugEntries {
    pub fn with_profile(profile: DebugProfile) -> Self {
        Self {
            profile: Some(profile),
            statuses: profile
                .statuses()
                .iter()
                .map(|(id, s)| ((*id).to_owned(), *s))
                .collect(),
        }
    }

    /// The profile in use, or `None` once an entry was edited.
    pub fn profile(&self) -> Option<DebugProfile> {
        self.profile
    }

    pub fn load_profile(&mut self, profile: DebugProfile) {
        *self = Self::with_profile(profile);
    }

    pub fn status(&self, id: &str) -> EntryStatus {
        self.statuses.get(id).copied().unwrap_or(EntryStatus::Never)
    }

    pub fn set_status(&mut self, id: &str, status: EntryStatus) {
        if id.len() > MAX_ID_BYTES
            || (!self.statuses.contains_key(id) && self.statuses.len() >= MAX_STATUSES)
        {
            return;
        }
        self.profile = None;
        self.statuses.insert(id.to_owned(), status);
    }

    /// `toggleStatus`: returns whether the entry shows afterwards.
    pub fn toggle(&mut self, id: &str, overlay_visible: bool) -> bool {
        let (next, on) = match self.statuses.get(id) {
            Some(EntryStatus::AlwaysOn) => (EntryStatus::Never, false),
            Some(EntryStatus::InOverlay) if overlay_visible => (EntryStatus::Never, false),
            Some(EntryStatus::Never) if overlay_visible => (EntryStatus::InOverlay, true),
            Some(EntryStatus::InOverlay | EntryStatus::Never) | None => {
                (EntryStatus::AlwaysOn, true)
            }
        };
        self.set_status(id, next);
        on
    }

    /// Whether the entry draws: always on, or in the overlay while it is up, and allowed under reduced info.
    pub fn enabled(&self, id: &str, overlay_visible: bool, reduced: bool) -> bool {
        let shown = match self.status(id) {
            EntryStatus::AlwaysOn => true,
            EntryStatus::InOverlay => overlay_visible,
            EntryStatus::Never => false,
        };
        shown && entry_def(id).is_some_and(|e| e.reduced_ok || !reduced)
    }

    pub fn any_text_enabled(&self, overlay_visible: bool, reduced: bool) -> bool {
        ENTRIES
            .iter()
            .any(|e| e.category == Category::Text && self.enabled(e.id, overlay_visible, reduced))
    }

    /// `debug-profile.json`: `{"profile": ..}` while a profile is in use, else `{"custom": {..}}`.
    pub fn to_json(&self) -> String {
        let mut root = Map::new();
        match self.profile {
            Some(p) => {
                root.insert("profile".into(), p.name().into());
            }
            None => {
                let custom: Map<String, Value> = self
                    .statuses
                    .iter()
                    .map(|(id, s)| (qualified(id), s.name().into()))
                    .collect();
                root.insert("custom".into(), custom.into());
            }
        }
        Value::Object(root).to_string()
    }

    /// Reads `debug-profile.json`; a bad file is `None` and the caller falls back to the default profile.
    pub fn parse(json: &str) -> Option<Self> {
        let root: Value = serde_json::from_str(json).ok()?;
        if let Some(name) = root.get("profile") {
            return DebugProfile::from_name(name.as_str()?).map(Self::with_profile);
        }
        let custom = root.get("custom")?.as_object()?;
        let mut statuses = BTreeMap::new();
        for (id, status) in custom.iter().take(MAX_STATUSES) {
            let id = id.strip_prefix("minecraft:").unwrap_or(id);
            if id.len() <= MAX_ID_BYTES {
                statuses.insert(id.to_owned(), EntryStatus::from_name(status.as_str()?)?);
            }
        }
        Some(Self {
            profile: None,
            statuses,
        })
    }
}

fn qualified(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_sorted_by_category_then_id() {
        assert!(
            ENTRIES
                .windows(2)
                .all(|w| (w[0].category, w[0].id) < (w[1].category, w[1].id))
        );
    }

    #[test]
    fn the_default_profile_matches_vanilla() {
        let d = DebugEntries::default();
        assert_eq!(d.profile(), Some(DebugProfile::Default));
        for id in ["game_version", "fps", "tps", "memory", "player_position"] {
            assert_eq!(d.status(id), EntryStatus::InOverlay, "{id}");
        }
        assert_eq!(d.status("biome"), EntryStatus::Never);
        assert!(d.enabled("fps", true, false));
        assert!(!d.enabled("fps", false, false));
    }

    #[test]
    fn the_performance_profile_keeps_fps_on_without_the_overlay() {
        let p = DebugEntries::with_profile(DebugProfile::Performance);
        assert!(p.enabled("fps", false, false));
        assert!(!p.enabled("game_version", true, false));
        assert!(p.any_text_enabled(false, false));
    }

    #[test]
    fn editing_leaves_the_profile_and_toggle_follows_the_overlay() {
        let mut d = DebugEntries::default();
        assert!(d.toggle("entity_hitboxes", false));
        assert_eq!(d.status("entity_hitboxes"), EntryStatus::AlwaysOn);
        assert_eq!(d.profile(), None);
        assert!(!d.toggle("entity_hitboxes", true));
        assert!(d.toggle("entity_hitboxes", true));
        assert_eq!(d.status("entity_hitboxes"), EntryStatus::InOverlay);
        assert!(d.toggle("entity_hitboxes", false));
        assert_eq!(d.status("entity_hitboxes"), EntryStatus::AlwaysOn);
        d.load_profile(DebugProfile::Default);
        assert_eq!(d.profile(), Some(DebugProfile::Default));
    }

    #[test]
    fn reduced_debug_info_hides_the_world_entries() {
        let mut d = DebugEntries::default();
        d.set_status("player_position", EntryStatus::AlwaysOn);
        d.set_status("fps", EntryStatus::AlwaysOn);
        assert!(!d.enabled("player_position", true, true));
        assert!(d.enabled("fps", true, true));
        assert!(!d.enabled("not_an_entry", true, false));
    }

    #[test]
    fn json_round_trips_and_keeps_entries_this_client_lacks() {
        assert_eq!(
            DebugEntries::parse(r#"{"profile":"performance"}"#),
            Some(DebugEntries::with_profile(DebugProfile::Performance))
        );
        assert_eq!(
            DebugEntries::default().to_json(),
            r#"{"profile":"default"}"#
        );
        let custom =
            r#"{"custom":{"minecraft:fps":"alwaysOn","minecraft:sound_mood":"inOverlay"}}"#;
        let d = DebugEntries::parse(custom).unwrap();
        assert_eq!(d.status("fps"), EntryStatus::AlwaysOn);
        assert_eq!(d.status("sound_mood"), EntryStatus::InOverlay);
        assert_eq!(DebugEntries::parse(&d.to_json()), Some(d));
    }

    #[test]
    fn bad_files_are_refused() {
        for text in [
            "",
            "[]",
            "{}",
            r#"{"profile":"x"}"#,
            r#"{"custom":{"a":"b"}}"#,
            r#"{"custom":[]}"#,
        ] {
            assert_eq!(DebugEntries::parse(text), None, "{text}");
        }
    }

    #[test]
    fn the_status_table_is_bounded() {
        let mut d = DebugEntries::default();
        for i in 0..2 * MAX_STATUSES {
            d.set_status(&format!("e{i}"), EntryStatus::AlwaysOn);
        }
        assert!(d.statuses.len() <= MAX_STATUSES);
    }
}
