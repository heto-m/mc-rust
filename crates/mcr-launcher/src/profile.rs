use std::path::{Path, PathBuf};

use mcr_settings::mods::{MAX_HEAP_MIB, MIN_HEAP_MIB, stored_heap_mib};
use serde_json::{Value, json};

use crate::clean;

pub const DEFAULT_PROFILE_ID: &str = "latest-release";
/// The version value that follows the newest supported release.
pub const LATEST_RELEASE: &str = "latest-release";
pub const MAX_NAME: usize = 64;
pub const MAX_PATH_BYTES: usize = 1024;
pub const MAX_VERSION_ID: usize = 64;
pub const MAX_ID: usize = 32;
pub const MIN_SIDE: u32 = 320;
pub const MAX_SIDE: u32 = 7680;

/// Item ids drawn as profile icons; the first is the default.
pub const ICONS: &[&str] = &[
    "grass_block",
    "crafting_table",
    "furnace",
    "chest",
    "bookshelf",
    "tnt",
    "oak_log",
    "cobblestone",
    "diamond_block",
    "emerald_block",
    "gold_block",
    "redstone_block",
    "netherrack",
    "end_stone",
    "glowstone",
    "carved_pumpkin",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let ok = |side: u32| (MIN_SIDE..=MAX_SIDE).contains(&side);
        (ok(width) && ok(height)).then_some(Self { width, height })
    }

    /// `1280x720`.
    pub fn parse(text: &str) -> Option<Self> {
        let (w, h) = text.trim().split_once(['x', 'X'])?;
        Self::new(w.trim().parse().ok()?, h.trim().parse().ok()?)
    }
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

/// One installation: which version runs, where it keeps its files, and with which mods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub icon: String,
    /// A version id or [`LATEST_RELEASE`].
    pub version: String,
    /// `None` is the default `<data dir>/client`.
    pub game_dir: Option<PathBuf>,
    /// `None` is vanilla: mod mode stays off.
    pub mods: Option<PathBuf>,
    /// `None` is automatic, from the enabled mod count.
    pub heap_mib: Option<u32>,
    pub resolution: Option<Resolution>,
    pub last_played: u64,
}

impl Profile {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            icon: ICONS[0].to_owned(),
            version: LATEST_RELEASE.to_owned(),
            game_dir: None,
            mods: None,
            heap_mib: None,
            resolution: None,
            last_played: 0,
        }
    }

    pub fn latest_release() -> Self {
        Self::new(DEFAULT_PROFILE_ID, "Latest release")
    }

    pub fn is_default(&self) -> bool {
        self.id == DEFAULT_PROFILE_ID
    }

    pub fn follows_latest(&self) -> bool {
        self.version == LATEST_RELEASE
    }

    pub fn game_dir_in(&self, data_dir: &Path) -> PathBuf {
        self.game_dir
            .clone()
            .unwrap_or_else(|| data_dir.join("client"))
    }

    /// Fixes up every field to the same bounds the file reader applies.
    pub fn sanitize(&mut self) {
        self.name = clean(&self.name, MAX_NAME).trim().to_owned();
        if self.name.is_empty() {
            self.name = "Unnamed".to_owned();
        }
        if !ICONS.contains(&self.icon.as_str()) {
            self.icon = ICONS[0].to_owned();
        }
        if !valid_version_id(&self.version) {
            self.version = LATEST_RELEASE.to_owned();
        }
        self.heap_mib = self.heap_mib.map(|h| h.clamp(MIN_HEAP_MIB, MAX_HEAP_MIB));
        self.game_dir = self.game_dir.take().filter(|p| valid_path(p));
        self.mods = self.mods.take().filter(|p| valid_path(p));
    }

    pub(crate) fn from_json(v: &Value) -> Option<Self> {
        let id = v["id"].as_str().filter(|id| valid_id(id))?;
        let mut profile = Self::new(id, v["name"].as_str().unwrap_or(""));
        if let Some(icon) = v["icon"].as_str() {
            profile.icon = icon.to_owned();
        }
        if let Some(version) = v["version"].as_str() {
            profile.version = version.to_owned();
        }
        profile.game_dir = path_field(&v["game_dir"]);
        profile.mods = path_field(&v["mods"]);
        profile.heap_mib = stored_heap_mib(v);
        profile.resolution = v["width"]
            .as_u64()
            .zip(v["height"].as_u64())
            .and_then(|(w, h)| Resolution::new(u32::try_from(w).ok()?, u32::try_from(h).ok()?));
        profile.last_played = v["last_played"].as_u64().unwrap_or(0);
        profile.sanitize();
        Some(profile)
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut v = json!({
            "id": self.id,
            "name": self.name,
            "icon": self.icon,
            "version": self.version,
            "last_played": self.last_played,
        });
        if let Some(heap) = self.heap_mib {
            v["heap_mib"] = json!(heap);
            v["heap_explicit"] = json!(true);
        }
        if let Some(dir) = &self.game_dir {
            v["game_dir"] = json!(dir.to_string_lossy());
        }
        if let Some(dir) = &self.mods {
            v["mods"] = json!(dir.to_string_lossy());
        }
        if let Some(r) = self.resolution {
            v["width"] = json!(r.width);
            v["height"] = json!(r.height);
        }
        v
    }
}

pub fn valid_id(id: &str) -> bool {
    (1..=MAX_ID).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Mojang's ids look like `26.3`, `26w14a`, `1.21.4-rc1` or `1.14 Pre-Release 1`.
pub fn valid_version_id(id: &str) -> bool {
    (1..=MAX_VERSION_ID).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_+ ".contains(&b))
}

fn valid_path(path: &Path) -> bool {
    let text = path.as_os_str();
    !text.is_empty()
        && text.len() <= MAX_PATH_BYTES
        && !text.to_string_lossy().chars().any(char::is_control)
}

fn path_field(v: &Value) -> Option<PathBuf> {
    let text = v.as_str()?;
    (text.len() <= MAX_PATH_BYTES && !text.trim().is_empty()).then(|| PathBuf::from(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_bounds_and_parse() {
        assert_eq!(Resolution::parse("1280x720"), Resolution::new(1280, 720));
        assert_eq!(
            Resolution::parse(" 854 X 480 ").unwrap().to_string(),
            "854x480"
        );
        for bad in ["", "1280", "0x0", "100000x720", "axb", "1280x719x1"] {
            assert!(Resolution::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn json_round_trip() {
        let mut p = Profile::new("abc", "Survival");
        p.icon = "tnt".into();
        p.version = "26.3".into();
        p.game_dir = Some("/games/survival".into());
        p.mods = Some("/games/mods".into());
        p.heap_mib = Some(3072);
        p.resolution = Resolution::new(1280, 720);
        p.last_played = 42;
        assert_eq!(Profile::from_json(&p.to_json()), Some(p));
    }

    #[test]
    fn hostile_fields_are_bounded() {
        let v = json!({
            "id": "x",
            "name": format!("\u{1b}[31m{}", "n".repeat(10_000)),
            "icon": "../../etc/passwd",
            "version": "bad\nid",
            "game_dir": "g".repeat(5000),
            "heap_mib": 1u64 << 40,
            "width": 1, "height": 1,
        });
        let p = Profile::from_json(&v).unwrap();
        assert_eq!(p.name.chars().count(), MAX_NAME);
        assert!(!p.name.contains('\u{1b}'));
        assert_eq!(p.icon, ICONS[0]);
        assert_eq!(p.version, LATEST_RELEASE);
        assert_eq!(p.game_dir, None);
        assert_eq!(p.heap_mib, Some(MAX_HEAP_MIB));
        let legacy = Profile::from_json(&json!({"id": "x", "heap_mib": 2048})).unwrap();
        assert_eq!(legacy.heap_mib, None, "the old default reads as automatic");
        assert_eq!(p.resolution, None);
        assert!(Profile::from_json(&json!({"id": "a b"})).is_none());
        assert!(Profile::from_json(&json!({"id": "x".repeat(33)})).is_none());
        assert_eq!(
            Profile::from_json(&json!({"id": "y"})).unwrap().name,
            "Unnamed"
        );
    }

    #[test]
    fn version_ids() {
        for ok in ["26.3", "26w14a", "1.21.4-rc1", "1.14 Pre-Release 1"] {
            assert!(valid_version_id(ok), "{ok}");
        }
        for bad in ["", "a/b", "x\u{0}", &"9".repeat(65)] {
            assert!(!valid_version_id(bad), "{bad}");
        }
    }
}
