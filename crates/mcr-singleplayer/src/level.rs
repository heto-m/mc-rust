use std::fs;
use std::io::Read;
use std::path::Path;

use mcr_nbt::{Compound, Compression, Limits, Value, read_disk, write_disk};

use crate::layout::{MAX_FOLDER_LISTING, valid_folder};
use crate::{Result, SpError};

const MAX_LEVEL_DAT: u64 = 32 << 20;
const LEVEL_LIMITS: Limits = Limits {
    max_depth: 512,
    max_bytes: 64 << 20,
};
const MAX_ICON: u64 = 1 << 20;
pub const ICON_SIZE: u32 = 64;
pub const MAX_NAME_CHARS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GameMode {
    #[default]
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl GameMode {
    pub fn from_id(id: i64) -> Self {
        match id {
            1 => Self::Creative,
            2 => Self::Adventure,
            3 => Self::Spectator,
            _ => Self::Survival,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Survival => "survival",
            Self::Creative => "creative",
            Self::Adventure => "adventure",
            Self::Spectator => "spectator",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Difficulty {
    Peaceful,
    Easy,
    #[default]
    Normal,
    Hard,
}

impl Difficulty {
    pub fn from_id(id: i64) -> Self {
        match id {
            0 => Self::Peaceful,
            1 => Self::Easy,
            3 => Self::Hard,
            _ => Self::Normal,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "peaceful" => Self::Peaceful,
            "easy" => Self::Easy,
            "normal" => Self::Normal,
            "hard" => Self::Hard,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Peaceful => "peaceful",
            Self::Easy => "easy",
            Self::Normal => "normal",
            Self::Hard => "hard",
        }
    }
}

/// What the world list and a launch need from `level.dat`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LevelInfo {
    pub name: String,
    pub last_played: i64,
    pub game_mode: GameMode,
    pub hardcore: bool,
    pub cheats: bool,
    pub difficulty: Difficulty,
    pub version: Option<String>,
    pub data_version: Option<i32>,
}

fn number(c: &Compound, key: &str) -> Option<i64> {
    match c.get(key)? {
        Value::Byte(v) => Some(i64::from(*v)),
        Value::Short(v) => Some(i64::from(*v)),
        Value::Int(v) => Some(i64::from(*v)),
        Value::Long(v) => Some(*v),
        _ => None,
    }
}

fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME_CHARS)
        .collect()
}

pub fn parse_level(bytes: &[u8]) -> Result<LevelInfo> {
    let (_, root) = read_disk(bytes, LEVEL_LIMITS)?;
    let data = root
        .get_compound("Data")
        .ok_or_else(|| SpError::data("level.dat has no Data compound"))?;
    let game_mode = GameMode::from_id(number(data, "GameType").unwrap_or(0));
    let settings = data.get_compound("difficulty_settings");
    let difficulty = settings
        .and_then(|s| s.get_str("difficulty"))
        .and_then(Difficulty::from_name)
        .or_else(|| number(data, "Difficulty").map(Difficulty::from_id))
        .unwrap_or_default();
    let hardcore = settings
        .and_then(|s| number(s, "hardcore"))
        .or_else(|| number(data, "hardcore"))
        .is_some_and(|v| v != 0);
    let cheats = number(data, "allowCommands").map_or(game_mode == GameMode::Creative, |v| v != 0);
    let version = data
        .get_compound("Version")
        .and_then(|v| v.get_str("Name"))
        .map(clean);
    let data_version = number(data, "DataVersion").and_then(|v| i32::try_from(v).ok());
    Ok(LevelInfo {
        name: clean(data.get_str("LevelName").unwrap_or_default()),
        last_played: number(data, "LastPlayed").unwrap_or(0),
        game_mode,
        hardcore,
        cheats,
        difficulty,
        version,
        data_version,
    })
}

fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(SpError::TooLarge(path.display().to_string(), max));
    }
    Ok(bytes)
}

pub fn read_level(world: &Path) -> Result<LevelInfo> {
    parse_level(&read_bounded(&world.join("level.dat"), MAX_LEVEL_DAT)?)
}

/// Changes the display name like vanilla's Edit World: the previous file is kept as `level.dat_old`.
pub fn rename(world: &Path, name: &str) -> Result<()> {
    let name = clean(name.trim());
    if name.is_empty() {
        return Err(SpError::data("the world name is empty"));
    }
    let path = world.join("level.dat");
    let bytes = read_bounded(&path, MAX_LEVEL_DAT)?;
    let (root_name, mut root) = read_disk(&bytes, LEVEL_LIMITS)?;
    let Some(Value::Compound(data)) = root.get_mut("Data") else {
        return Err(SpError::data("level.dat has no Data compound"));
    };
    data.insert("LevelName", Value::String(name));
    let out = write_disk(&root_name, &root, Compression::Gzip)?;
    let tmp = world.join("level.dat.mcr-tmp");
    fs::write(&tmp, out)?;
    fs::copy(&path, world.join("level.dat_old"))?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// A 64×64 RGBA `icon.png`, or `None` when it is missing, too big or not that size.
pub fn read_icon(path: &Path) -> Option<Vec<u8>> {
    let bytes = read_bounded(path, MAX_ICON).ok()?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    if info.width != ICON_SIZE || info.height != ICON_SIZE {
        return None;
    }
    let mut buf = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buf).ok()?;
    let pixels = &buf[..frame.buffer_size()];
    let n = (ICON_SIZE * ICON_SIZE) as usize;
    let rgba: Vec<u8> = match frame.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    (rgba.len() == n * 4).then_some(rgba)
}

/// `YYYY-MM-DD HH:MM` in local time (UTC where the platform offers no local time).
pub fn format_played(ms: i64) -> String {
    if ms <= 0 {
        return "-".into();
    }
    let secs = ms / 1000;
    #[cfg(unix)]
    {
        let t = secs as libc::time_t;
        // SAFETY: tm is plain data; localtime_r only writes into it and is thread-safe.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: both pointers are valid for the duration of the call.
        if !unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday,
                tm.tm_hour,
                tm.tm_min
            );
        }
    }
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60
    )
}

/// Howard Hinnant's days-to-civil conversion.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldSummary {
    pub folder: String,
    pub info: LevelInfo,
    pub icon: Option<Vec<u8>>,
}

/// Worlds under `saves`, newest first; folders without a readable `level.dat` are left out.
pub fn scan(saves: &Path) -> Vec<WorldSummary> {
    let Ok(entries) = fs::read_dir(saves) else {
        return Vec::new();
    };
    let mut worlds = Vec::new();
    for entry in entries.flatten().take(MAX_FOLDER_LISTING * 4) {
        if worlds.len() >= MAX_FOLDER_LISTING {
            break;
        }
        let Ok(folder) = entry.file_name().into_string() else {
            continue;
        };
        let path = entry.path();
        if !valid_folder(&folder) || !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        match read_level(&path) {
            Ok(mut info) => {
                if info.name.trim().is_empty() {
                    info.name.clone_from(&folder);
                }
                worlds.push(WorldSummary {
                    icon: read_icon(&path.join("icon.png")),
                    folder,
                    info,
                });
            }
            Err(e) => tracing::debug!("skipping world folder {folder}: {e}"),
        }
    }
    worlds.sort_by(|a, b| {
        b.info
            .last_played
            .cmp(&a.info.last_played)
            .then_with(|| a.folder.cmp(&b.folder))
    });
    worlds
}

/// Removes a world folder; `folder` must be a plain name of a directory directly under `saves`.
pub fn delete_world(saves: &Path, folder: &str) -> Result<()> {
    if !valid_folder(folder) {
        return Err(SpError::Refused(format!("not a world folder: {folder}")));
    }
    let path = saves.join(folder);
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_dir() {
        return Err(SpError::Refused(format!("not a world folder: {folder}")));
    }
    fs::remove_dir_all(&path)?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn level_dat(data: Compound) -> Vec<u8> {
        let mut root = Compound::new();
        root.insert("Data", Value::Compound(data));
        write_disk("", &root, Compression::Gzip).unwrap()
    }

    pub fn modern(name: &str, last: i64) -> Compound {
        let mut data = Compound::new();
        data.insert("LevelName", name);
        data.insert("LastPlayed", Value::Long(last));
        data.insert("GameType", Value::Int(1));
        data.insert("allowCommands", Value::Byte(0));
        data.insert("DataVersion", Value::Int(4790));
        let mut version = Compound::new();
        version.insert("Name", "26.3");
        version.insert("Id", Value::Int(4790));
        data.insert("Version", Value::Compound(version));
        let mut settings = Compound::new();
        settings.insert("difficulty", "hard");
        settings.insert("hardcore", Value::Byte(0));
        settings.insert("locked", Value::Byte(0));
        data.insert("difficulty_settings", Value::Compound(settings));
        data
    }

    pub fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mcr-sp-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_743), (2026, 10, 17));
        assert_eq!(format_played(0), "-");
        assert_eq!(format_played(1_792_000_000_000).len(), 16);
    }

    #[test]
    fn modern_level_dat() {
        let info = parse_level(&level_dat(modern("My World", 1_700_000_000_000))).unwrap();
        assert_eq!(info.name, "My World");
        assert_eq!(info.last_played, 1_700_000_000_000);
        assert_eq!(info.game_mode, GameMode::Creative);
        assert_eq!(info.difficulty, Difficulty::Hard);
        assert!(!info.hardcore);
        assert!(!info.cheats);
        assert_eq!(info.version.as_deref(), Some("26.3"));
        assert_eq!(info.data_version, Some(4790));
    }

    #[test]
    fn old_level_dat_falls_back() {
        let mut data = Compound::new();
        data.insert("LevelName", "Old\u{7}");
        data.insert("Difficulty", Value::Byte(3));
        data.insert("hardcore", Value::Byte(1));
        data.insert("GameType", Value::Int(1));
        let info = parse_level(&level_dat(data)).unwrap();
        assert_eq!(info.name, "Old");
        assert_eq!(info.difficulty, Difficulty::Hard);
        assert!(info.hardcore);
        assert!(
            info.cheats,
            "creative without allowCommands defaults to cheats"
        );
    }

    #[test]
    fn hostile_level_dat_is_rejected() {
        assert!(parse_level(b"").is_err());
        assert!(parse_level(&[0x1f, 0x8b, 0, 0]).is_err());
        let no_data = write_disk("", &Compound::new(), Compression::Gzip).unwrap();
        assert!(parse_level(&no_data).is_err());
        let mut long = modern(&"n".repeat(10_000), 0);
        long.insert("GameType", Value::String("x".into()));
        let info = parse_level(&level_dat(long)).unwrap();
        assert_eq!(info.name.chars().count(), MAX_NAME_CHARS);
        assert_eq!(info.game_mode, GameMode::Survival);
    }

    #[test]
    fn scan_sorts_and_skips_broken() {
        let saves = temp_dir("scan");
        for (folder, last) in [("a", 10), ("b", 30), ("c", 20)] {
            fs::create_dir_all(saves.join(folder)).unwrap();
            fs::write(
                saves.join(folder).join("level.dat"),
                level_dat(modern(folder, last)),
            )
            .unwrap();
        }
        fs::create_dir_all(saves.join("broken")).unwrap();
        fs::write(saves.join("broken/level.dat"), b"nope").unwrap();
        fs::write(saves.join("usercache.json"), b"[]").unwrap();
        let worlds = scan(&saves);
        let folders: Vec<_> = worlds.iter().map(|w| w.folder.as_str()).collect();
        assert_eq!(folders, ["b", "c", "a"]);
        delete_world(&saves, "b").unwrap();
        assert!(!saves.join("b").exists());
        assert!(delete_world(&saves, "..").is_err());
        assert!(delete_world(&saves, "usercache.json").is_err());
        let _ = fs::remove_dir_all(&saves);
    }

    #[test]
    fn rename_keeps_old_copy() {
        let world = temp_dir("rename");
        fs::write(world.join("level.dat"), level_dat(modern("Before", 5))).unwrap();
        rename(&world, "  After ").unwrap();
        assert_eq!(read_level(&world).unwrap().name, "After");
        assert_eq!(
            parse_level(&fs::read(world.join("level.dat_old")).unwrap())
                .unwrap()
                .name,
            "Before"
        );
        assert!(rename(&world, "   ").is_err());
        let _ = fs::remove_dir_all(&world);
    }

    #[test]
    fn icons_must_be_64_square() {
        let dir = temp_dir("icon");
        let write = |name: &str, w: u32, h: u32| {
            let path = dir.join(name);
            let file = fs::File::create(&path).unwrap();
            let mut enc = png::Encoder::new(file, w, h);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            writer
                .write_image_data(&vec![7; (w * h * 3) as usize])
                .unwrap();
            path
        };
        let icon = read_icon(&write("ok.png", 64, 64)).unwrap();
        assert_eq!(icon.len(), 64 * 64 * 4);
        assert_eq!(&icon[..4], &[7, 7, 7, 255]);
        assert!(read_icon(&write("big.png", 128, 64)).is_none());
        assert!(read_icon(&dir.join("missing.png")).is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
