use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use zip::ZipArchive;

pub const MAX_JARS: usize = 512;
pub const MAX_ENTRIES: usize = 200_000;
pub const MAX_JSON_BYTES: u64 = 1 << 20;
pub const MAX_NESTED: usize = 256;
pub const MAX_NESTED_TOTAL: usize = 1024;
const MAX_NESTED_DEPTH: u32 = 2;
pub const MAX_NESTED_JAR_BYTES: u64 = 32 << 20;
pub const MAX_NESTED_TOTAL_BYTES: u64 = 64 << 20;
pub const MAX_ICON_BYTES: u64 = 512 << 10;
pub const MAX_ICON_DIM: u32 = 256;
const MAX_DEPS: usize = 256;
const MAX_LIST: usize = 256;
const MAX_NAME: usize = 64;
const MAX_VERSION: usize = 64;
const MAX_DESCRIPTION: usize = 512;
const MAX_ID: usize = 64;
const MAX_ERROR: usize = 160;
const MAX_TOML_BYTES: u64 = 256 << 10;

const FABRIC: &str = "fabric.mod.json";
const JARJAR: &str = "META-INF/jarjar/metadata.json";
const JAR_MANIFEST: &str = "META-INF/MANIFEST.MF";
const QUILT: &str = "quilt.mod.json";
const NEOFORGE: &str = "META-INF/neoforge.mods.toml";
const FORGE: &str = "META-INF/mods.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Both,
    Client,
    Server,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub id: String,
    /// Alternatives; the dependency holds when any of them matches.
    pub range: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nested {
    pub id: String,
    pub version: String,
    pub provides: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModKind {
    Fabric,
    Quilt,
    NeoForge,
    Forge,
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModInfo {
    pub path: PathBuf,
    pub file_name: String,
    pub kind: ModKind,
    /// The mod id for Fabric jars, the file name otherwise.
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub icon: Option<Arc<Icon>>,
    pub side: Side,
    pub depends: Vec<Dependency>,
    pub recommends: Vec<Dependency>,
    pub breaks: Vec<Dependency>,
    pub provides: Vec<String>,
    pub nested: Vec<Nested>,
    pub warnings: Vec<String>,
    /// The NeoForge reading of a jar that also carries `neoforge.mods.toml` (multi-loader builds).
    pub neoforge: Option<Box<ModInfo>>,
}

impl ModInfo {
    fn bare(path: &Path, file_name: &str, kind: ModKind) -> Self {
        Self {
            path: path.to_owned(),
            file_name: file_name.to_owned(),
            kind,
            id: file_name.to_owned(),
            name: clean(file_name.trim_end_matches(".jar"), MAX_NAME),
            version: String::new(),
            description: String::new(),
            icon: None,
            side: Side::Both,
            depends: Vec::new(),
            recommends: Vec::new(),
            breaks: Vec::new(),
            provides: Vec::new(),
            nested: Vec::new(),
            warnings: Vec::new(),
            neoforge: None,
        }
    }

    pub fn is_fabric(&self) -> bool {
        self.kind == ModKind::Fabric
    }

    /// Carries a manifest for each loader, so it fits a set of either.
    pub fn is_multi_loader(&self) -> bool {
        self.neoforge.is_some()
    }

    /// A jar of a loader mcr can boot.
    pub fn is_loadable(&self) -> bool {
        matches!(self.kind, ModKind::Fabric | ModKind::NeoForge)
    }
}

#[derive(Debug, thiserror::Error)]
enum ScanError {
    #[error("not a readable jar: {0}")]
    Zip(String),
    #[error("too many entries in the jar")]
    TooManyEntries,
    #[error("{0} is larger than the {1} byte limit")]
    TooLarge(&'static str, u64),
    #[error("{FABRIC}: {0}")]
    Manifest(String),
    #[error("no {FABRIC} (not a Fabric mod)")]
    NotAMod,
}

/// Strips control, format and `§` characters and bounds the length, since the text is drawn as-is.
pub fn clean(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| {
            !c.is_control()
                && *c != '§'
                && !matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}' | '\u{00AD}')
        })
        .take(max)
        .collect()
}

/// Lists `*.jar` files in `dir` (a missing folder is an empty list), reading only manifests.
pub fn scan_dir(dir: &Path) -> Vec<ModInfo> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jar")))
        .collect();
    paths.sort();
    if paths.len() > MAX_JARS {
        tracing::warn!(
            count = paths.len(),
            "too many jars in the mods folder; scanning the first {MAX_JARS}"
        );
        paths.truncate(MAX_JARS);
    }
    paths.iter().map(|p| scan_jar(p)).collect()
}

pub fn scan_jar(path: &Path) -> ModInfo {
    let file_name = clean(
        &path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        MAX_NAME * 2,
    );
    match std::fs::File::open(path) {
        Ok(file) => scan_reader(path, &file_name, std::io::BufReader::new(file)),
        Err(e) => invalid(path, &file_name, &e.to_string()),
    }
}

fn invalid(path: &Path, file_name: &str, why: &str) -> ModInfo {
    ModInfo::bare(path, file_name, ModKind::Invalid(clean(why, MAX_ERROR)))
}

pub fn scan_reader<R: Read + Seek>(path: &Path, file_name: &str, reader: R) -> ModInfo {
    let mut zip = match open_zip(reader) {
        Ok(z) => z,
        Err(e) => return invalid(path, file_name, &e.to_string()),
    };
    let text = match read_text(&mut zip, FABRIC, MAX_JSON_BYTES) {
        Ok(Some(text)) => text,
        Ok(None) => return other_loader(&mut zip, path, file_name),
        Err(e) => return invalid(path, file_name, &e.to_string()),
    };
    let mut info = ModInfo::bare(path, file_name, ModKind::Fabric);
    if let Err(e) = fill_fabric(&mut zip, &text, &mut info) {
        return invalid(path, file_name, &e.to_string());
    }
    if let Ok(Some(toml)) = read_text(&mut zip, NEOFORGE, MAX_TOML_BYTES) {
        let mut neoforge = ModInfo::bare(path, file_name, ModKind::NeoForge);
        if fill_neoforge(&mut zip, &toml, &mut neoforge).is_ok() {
            info.neoforge = Some(Box::new(neoforge));
        }
    }
    info
}

fn open_zip<R: Read + Seek>(reader: R) -> Result<ZipArchive<R>, ScanError> {
    let zip = ZipArchive::new(reader).map_err(|e| ScanError::Zip(e.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(ScanError::TooManyEntries);
    }
    Ok(zip)
}

fn read_named<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>, ScanError> {
    let Ok(mut file) = zip.by_name(name) else {
        return Ok(None);
    };
    if file.size() > limit {
        return Err(ScanError::TooLarge("an entry", limit));
    }
    let mut out = Vec::new();
    file.by_ref()
        .take(limit + 1)
        .read_to_end(&mut out)
        .map_err(|e| ScanError::Zip(e.to_string()))?;
    if out.len() as u64 > limit {
        return Err(ScanError::TooLarge("an entry", limit));
    }
    Ok(Some(out))
}

fn read_text<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Option<String>, ScanError> {
    Ok(read_named(zip, name, limit)?.map(|b| String::from_utf8_lossy(&b).into_owned()))
}

fn other_loader<R: Read + Seek>(zip: &mut ZipArchive<R>, path: &Path, file_name: &str) -> ModInfo {
    let kind = if zip.by_name(QUILT).is_ok() {
        ModKind::Quilt
    } else if zip.by_name(NEOFORGE).is_ok() {
        ModKind::NeoForge
    } else if zip.by_name(FORGE).is_ok() {
        ModKind::Forge
    } else if let Some(info) = neoforge_library(zip, path, file_name) {
        return info;
    } else {
        return invalid(path, file_name, &ScanError::NotAMod.to_string());
    };
    if kind == ModKind::NeoForge {
        return match read_text(zip, NEOFORGE, MAX_TOML_BYTES) {
            Ok(Some(text)) => {
                let mut info = ModInfo::bare(path, file_name, ModKind::NeoForge);
                match fill_neoforge(zip, &text, &mut info) {
                    Ok(()) => info,
                    Err(why) => invalid(path, file_name, &format!("{NEOFORGE}: {why}")),
                }
            }
            Ok(None) => invalid(path, file_name, &format!("{NEOFORGE} unreadable")),
            Err(e) => invalid(path, file_name, &e.to_string()),
        };
    }
    let mut info = ModInfo::bare(path, file_name, kind.clone());
    if kind == ModKind::Forge
        && let Ok(Some(text)) = read_text(zip, FORGE, MAX_TOML_BYTES)
    {
        if let Some(v) = toml_value(&text, "displayName") {
            info.name = clean(&v, MAX_NAME);
        }
        if let Some(v) = toml_value(&text, "version") {
            info.version = clean(&v, MAX_VERSION);
        }
    }
    info
}

fn manifest_attribute(manifest: &str, key: &str) -> Option<String> {
    manifest.lines().find_map(|l| {
        l.strip_prefix(key)?
            .strip_prefix(':')
            .map(|v| v.trim().to_owned())
    })
}

/// A library NeoForge loads from the mods folder on its manifest's `FMLModType` alone (no mods.toml).
fn neoforge_library<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    path: &Path,
    file_name: &str,
) -> Option<ModInfo> {
    let manifest = read_text(zip, JAR_MANIFEST, MAX_TOML_BYTES).ok()??;
    let kind = manifest_attribute(&manifest, "FMLModType")?;
    if !matches!(kind.as_str(), "GAMELIBRARY" | "LIBRARY") {
        return None;
    }
    let mut info = ModInfo::bare(path, file_name, ModKind::NeoForge);
    if let Some(title) = manifest_attribute(&manifest, "Specification-Title") {
        info.name = clean(&title, MAX_NAME);
    }
    if let Some(version) = manifest_attribute(&manifest, "Implementation-Version") {
        info.version = clean(&version, MAX_VERSION);
    }
    info.description = "library".to_owned();
    Some(info)
}

/// `${file.jarVersion}` stands for the jar manifest's `Implementation-Version`.
fn neoforge_version<R: Read + Seek>(zip: &mut ZipArchive<R>, version: &str) -> String {
    if !version.contains("${") {
        return clean(version, MAX_VERSION);
    }
    let jar_version = read_text(zip, JAR_MANIFEST, MAX_TOML_BYTES)
        .ok()
        .flatten()
        .and_then(|m| manifest_attribute(&m, "Implementation-Version"))
        .unwrap_or_else(|| "0".to_owned());
    clean(
        &version.replace("${file.jarVersion}", &jar_version),
        MAX_VERSION,
    )
}

fn neoforge_id(id: &str) -> bool {
    let mut chars = id.chars();
    (2..=MAX_ID).contains(&id.len())
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn bounded(deps: Vec<Dependency>) -> Vec<Dependency> {
    deps.into_iter()
        .filter(|d| d.id.len() <= MAX_ID)
        .take(MAX_DEPS)
        .map(|d| Dependency {
            id: d.id,
            range: d
                .range
                .into_iter()
                .take(MAX_LIST)
                .map(|r| clean(&r, 128))
                .collect(),
        })
        .collect()
}

fn fill_neoforge<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    text: &str,
    info: &mut ModInfo,
) -> Result<(), String> {
    let m = super::neoforge::parse_manifest(text)?;
    if !neoforge_id(&m.id) {
        return Err("invalid mod id".into());
    }
    info.version = neoforge_version(zip, &m.version);
    info.name = m
        .name
        .map(|n| clean(&n, MAX_NAME))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| clean(&m.id, MAX_NAME));
    info.id = m.id;
    info.description = clean(&m.description.replace('\n', " "), MAX_DESCRIPTION);
    info.depends = bounded(m.depends);
    info.breaks = bounded(m.breaks);
    if let Some(path) = m.icon.filter(|p| p.len() <= 256) {
        info.icon = match read_named(zip, &path, MAX_ICON_BYTES) {
            Ok(Some(bytes)) => decode_icon(&bytes).map(Arc::new),
            _ => None,
        };
    }
    let mut budget = NestedBudget {
        bytes: MAX_NESTED_TOTAL_BYTES,
        jars: MAX_NESTED_TOTAL,
    };
    read_jarjar(zip, info, &mut budget, MAX_NESTED_DEPTH);
    Ok(())
}

/// Jar-in-jar mods NeoForge would load from `META-INF/jarjar`, so they satisfy dependencies.
fn read_jarjar<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    info: &mut ModInfo,
    budget: &mut NestedBudget,
    depth: u32,
) {
    let Ok(Some(text)) = read_text(zip, JARJAR, MAX_JSON_BYTES) else {
        return;
    };
    let Ok(root) = serde_json::from_str::<Value>(&text) else {
        return;
    };
    let paths: Vec<String> = root["jars"]
        .as_array()
        .map(|jars| {
            jars.iter()
                .filter_map(|j| j["path"].as_str())
                .filter(|p| p.len() <= 256)
                .take(MAX_NESTED)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    for file in paths {
        let Some(mut inner) = take_nested(zip, &file, info, budget) else {
            continue;
        };
        let Ok(Some(toml)) = read_text(&mut inner, NEOFORGE, MAX_TOML_BYTES) else {
            continue;
        };
        if let Ok(m) = super::neoforge::parse_manifest(&toml)
            && neoforge_id(&m.id)
        {
            let version = neoforge_version(&mut inner, &m.version);
            info.nested.push(Nested {
                id: m.id,
                version,
                provides: Vec::new(),
            });
            if depth > 1 {
                read_jarjar(&mut inner, info, budget, depth - 1);
            }
        }
    }
}

/// Opens one nested jar within the budget, or warns why it was skipped.
fn take_nested<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    file: &str,
    info: &mut ModInfo,
    budget: &mut NestedBudget,
) -> Option<ZipArchive<Cursor<Vec<u8>>>> {
    if budget.jars == 0 {
        info.warnings
            .push("too many nested jars; the rest were not read".to_owned());
        return None;
    }
    let size = zip.by_name(file).ok()?.size();
    if size > MAX_NESTED_JAR_BYTES || size > budget.bytes {
        info.warnings.push(format!(
            "nested jar {} was not read (too large)",
            clean(file, 80)
        ));
        return None;
    }
    budget.bytes -= size;
    budget.jars -= 1;
    let bytes = read_named(zip, file, MAX_NESTED_JAR_BYTES).ok()??;
    open_zip(Cursor::new(bytes)).ok()
}

/// The first `key = "value"` line; mods.toml is only read for display, never trusted.
fn toml_value(text: &str, key: &str) -> Option<String> {
    text.lines().take(2000).find_map(|line| {
        let rest = line
            .trim()
            .strip_prefix(key)?
            .trim_start()
            .strip_prefix('=')?;
        let value = rest.trim().trim_matches(|c| c == '"' || c == '\'');
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    (2..=MAX_ID).contains(&id.len())
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

struct Manifest {
    id: String,
    version: String,
    name: Option<String>,
    description: String,
    side: Side,
    depends: Vec<Dependency>,
    recommends: Vec<Dependency>,
    breaks: Vec<Dependency>,
    provides: Vec<String>,
    jars: Vec<String>,
    icon: Option<String>,
}

/// Fabric Loader accepts raw control characters (newlines) inside strings; serde_json does not.
fn parse_lenient(text: &str) -> Result<Value, ScanError> {
    serde_json::from_str(text).or_else(|strict| {
        let spaced: String = text
            .chars()
            .map(|c| if c.is_ascii_control() { ' ' } else { c })
            .collect();
        serde_json::from_str(&spaced).map_err(|_| ScanError::Manifest(strict.to_string()))
    })
}

fn parse_manifest(text: &str) -> Result<Manifest, ScanError> {
    let bad = |m: &str| ScanError::Manifest(m.to_owned());
    let root = parse_lenient(text)?;
    let id = root["id"].as_str().ok_or_else(|| bad("missing id"))?;
    if !valid_id(id) {
        return Err(bad("invalid mod id"));
    }
    let version = root["version"]
        .as_str()
        .ok_or_else(|| bad("missing version"))?;
    let side = match root["environment"].as_str() {
        Some("client") => Side::Client,
        Some("server") => Side::Server,
        _ => Side::Both,
    };
    let provides = root["provides"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter(|s| valid_id(s))
                .take(MAX_LIST)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let jars = root["jars"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|j| j["file"].as_str())
                .filter(|f| f.len() <= 256)
                .take(MAX_NESTED)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    Ok(Manifest {
        id: id.to_owned(),
        version: clean(version, MAX_VERSION),
        name: root["name"]
            .as_str()
            .map(|n| clean(n, MAX_NAME))
            .filter(|n| !n.is_empty()),
        description: clean(
            root["description"].as_str().unwrap_or_default(),
            MAX_DESCRIPTION,
        ),
        side,
        depends: dependencies(&root["depends"]),
        recommends: dependencies(&root["recommends"]),
        breaks: dependencies(&root["breaks"]),
        provides,
        jars,
        icon: pick_icon(&root["icon"]),
    })
}

fn dependencies(value: &Value) -> Vec<Dependency> {
    let Some(map) = value.as_object() else {
        return Vec::new();
    };
    map.iter()
        .take(MAX_DEPS)
        .filter(|(id, _)| id.len() <= MAX_ID)
        .map(|(id, range)| {
            let range = match range {
                Value::String(s) => vec![clean(s, 128)],
                Value::Array(a) => a
                    .iter()
                    .filter_map(Value::as_str)
                    .take(MAX_LIST)
                    .map(|s| clean(s, 128))
                    .collect(),
                _ => vec!["*".to_owned()],
            };
            Dependency {
                id: id.clone(),
                range,
            }
        })
        .collect()
}

/// The largest icon no bigger than the decode limit, or the only one.
fn pick_icon(value: &Value) -> Option<String> {
    let path = match value {
        Value::String(path) => Some(path.clone()),
        Value::Object(map) => map
            .iter()
            .filter_map(|(size, path)| Some((size.parse::<u32>().ok()?, path.as_str()?)))
            .filter(|(size, _)| *size <= MAX_ICON_DIM)
            .max_by_key(|(size, _)| *size)
            .map(|(_, path)| path.to_owned()),
        _ => None,
    };
    path.filter(|p| p.len() <= 256)
}

fn fill_fabric<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    text: &str,
    info: &mut ModInfo,
) -> Result<(), ScanError> {
    let m = parse_manifest(text)?;
    info.name = m.name.clone().unwrap_or_else(|| clean(&m.id, MAX_NAME));
    info.id = m.id.clone();
    info.version = m.version;
    info.description = m.description;
    info.side = m.side;
    info.depends = m.depends;
    info.recommends = m.recommends;
    info.breaks = m.breaks;
    info.provides = m.provides;
    if let Some(path) = &m.icon {
        info.icon = match read_named(zip, path, MAX_ICON_BYTES) {
            Ok(Some(bytes)) => decode_icon(&bytes).map(Arc::new),
            _ => None,
        };
    }
    read_nested(zip, &m.jars, info);
    Ok(())
}

struct NestedBudget {
    bytes: u64,
    jars: usize,
}

fn read_nested<R: Read + Seek>(zip: &mut ZipArchive<R>, files: &[String], info: &mut ModInfo) {
    let mut budget = NestedBudget {
        bytes: MAX_NESTED_TOTAL_BYTES,
        jars: MAX_NESTED_TOTAL,
    };
    read_nested_level(zip, files, info, &mut budget, MAX_NESTED_DEPTH);
}

fn read_nested_level<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    files: &[String],
    info: &mut ModInfo,
    budget: &mut NestedBudget,
    depth: u32,
) {
    for file in files {
        if budget.jars == 0 {
            info.warnings
                .push("too many nested jars; the rest were not read".to_owned());
            return;
        }
        let Some(mut inner) = take_nested(zip, file, info, budget) else {
            continue;
        };
        let Ok(Some(text)) = read_text(&mut inner, FABRIC, MAX_JSON_BYTES) else {
            continue;
        };
        if let Ok(m) = parse_manifest(&text) {
            info.nested.push(Nested {
                id: m.id,
                version: m.version,
                provides: m.provides,
            });
            if depth > 1 {
                read_nested_level(&mut inner, &m.jars, info, budget, depth - 1);
            }
        }
    }
}

/// Decodes a PNG of at most `MAX_ICON_DIM` square to RGBA8, or `None` for anything else.
pub fn decode_icon(bytes: &[u8]) -> Option<Icon> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let (w, h) = (reader.info().width, reader.info().height);
    if w == 0 || h == 0 || w > MAX_ICON_DIM || h > MAX_ICON_DIM {
        return None;
    }
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..frame.buffer_size()];
    let pixels = (w * h) as usize;
    let rgba = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    (rgba.len() == pixels * 4).then_some(Icon {
        width: w,
        height: h,
        rgba,
    })
}
