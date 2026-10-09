use std::path::{Path, PathBuf};

use mcr_nbt::{Compound, Limits, List};

use crate::{AnvilError, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct LevelSummary {
    pub dir: PathBuf,
    pub name: String,
    pub last_played_ms: i64,
    pub game_type: i32,
    pub hardcore: bool,
    pub allow_commands: bool,
    pub data_version: Option<i32>,
    pub version_name: Option<String>,
    pub spawn: Option<[i32; 3]>,
    pub player_pos: Option<[f64; 3]>,
}

impl LevelSummary {
    pub fn read(dir: &Path) -> Result<Self> {
        let path = dir.join("level.dat");
        let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => AnvilError::NotAWorld(dir.display().to_string()),
            _ => e.into(),
        })?;
        let limits = Limits {
            max_depth: 512,
            max_bytes: 64 * 1024 * 1024,
        };
        let (_, root) = mcr_nbt::read_disk(&bytes, limits)?;
        let data = root
            .get_compound("Data")
            .ok_or(AnvilError::Malformed("level.dat without Data"))?;
        Ok(Self::from_data(dir, data))
    }

    pub fn from_data(dir: &Path, data: &Compound) -> Self {
        let fallback = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let hardcore = data.get_bool("hardcore").or_else(|| {
            data.get_compound("difficulty_settings")?
                .get_bool("hardcore")
        });
        let spawn = data
            .get_compound("spawn")
            .and_then(|s| match s.get_int_array("pos") {
                Some(&[x, y, z]) => Some([x, y, z]),
                _ => None,
            })
            .or_else(|| {
                Some([
                    data.get_i32("SpawnX")?,
                    data.get_i32("SpawnY")?,
                    data.get_i32("SpawnZ")?,
                ])
            });
        let player_pos = data
            .get_compound("Player")
            .and_then(|p| match p.get_list("Pos") {
                Some(List::Double(v)) if v.len() == 3 => Some([v[0], v[1], v[2]]),
                _ => None,
            });
        Self {
            dir: dir.to_path_buf(),
            name: data.get_str("LevelName").map_or(fallback, str::to_owned),
            last_played_ms: data.get_i64("LastPlayed").unwrap_or(0),
            game_type: data.get_i32("GameType").unwrap_or(0),
            hardcore: hardcore.unwrap_or(false),
            allow_commands: data.get_bool("allowCommands").unwrap_or(false),
            data_version: data.get_i32("DataVersion"),
            version_name: data
                .get_compound("Version")
                .and_then(|v| v.get_str("Name"))
                .map(str::to_owned),
            spawn,
            player_pos,
        }
    }
}

/// Worlds under `saves`, newest first; folders without a readable `level.dat` are skipped.
pub fn list_worlds(saves: &Path) -> Result<Vec<LevelSummary>> {
    let mut worlds = Vec::new();
    for entry in std::fs::read_dir(saves)? {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }
        match LevelSummary::read(&path) {
            Ok(summary) => worlds.push(summary),
            Err(AnvilError::NotAWorld(_)) => {}
            Err(e) => tracing::warn!(world = %path.display(), error = %e, "unreadable world"),
        }
    }
    worlds.sort_by(|a, b| {
        b.last_played_ms
            .cmp(&a.last_played_ms)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(worlds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_world(dir: &Path, name: &str, played: i64) {
        std::fs::create_dir_all(dir).unwrap();
        let mut data = Compound::new();
        data.insert("LevelName", name);
        data.insert("LastPlayed", played);
        data.insert("GameType", 1);
        data.insert("allowCommands", 1i8);
        data.insert("DataVersion", 5023);
        let mut diff = Compound::new();
        diff.insert("hardcore", 1i8);
        data.insert("difficulty_settings", diff);
        let mut version = Compound::new();
        version.insert("Name", "26.3");
        data.insert("Version", version);
        let mut spawn = Compound::new();
        spawn.insert("pos", mcr_nbt::Value::IntArray(vec![1, 64, -3]));
        data.insert("spawn", spawn);
        let mut player = Compound::new();
        player.insert("Pos", List::Double(vec![0.5, 70.0, 2.5]));
        data.insert("Player", player);
        let mut root = Compound::new();
        root.insert("Data", data);
        let bytes = mcr_nbt::write_disk("", &root, mcr_nbt::Compression::Gzip).unwrap();
        std::fs::write(dir.join("level.dat"), bytes).unwrap();
    }

    #[test]
    fn summary_and_sorted_listing() {
        let root = std::env::temp_dir().join(format!("mcr-anvil-saves-{}", std::process::id()));
        write_world(&root.join("old"), "Old", 100);
        write_world(&root.join("new"), "New", 200);
        std::fs::create_dir_all(root.join("stray")).unwrap();
        std::fs::write(root.join("file.txt"), b"x").unwrap();
        let worlds = list_worlds(&root).unwrap();
        let names: Vec<_> = worlds.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["New", "Old"]);
        let w = &worlds[0];
        assert_eq!(w.game_type, 1);
        assert!(w.hardcore && w.allow_commands);
        assert_eq!(w.data_version, Some(5023));
        assert_eq!(w.version_name.as_deref(), Some("26.3"));
        assert_eq!(w.spawn, Some([1, 64, -3]));
        assert_eq!(w.player_pos, Some([0.5, 70.0, 2.5]));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_level_dat_is_not_a_world() {
        let dir = std::env::temp_dir().join(format!("mcr-anvil-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(matches!(
            LevelSummary::read(&dir),
            Err(AnvilError::NotAWorld(_))
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
