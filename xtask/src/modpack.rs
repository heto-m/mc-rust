//! `cargo xtask modpack-fetch`: a CurseForge modpack zip into `.cache/packs/<slug>/`, split into the jars
//! mcr's client loads and the ones a server runs, with the pack's overrides installed into a game dir.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use mcr_modmeta::policy::{Policy, PolicyStatus};
use mcr_modmeta::resolve::{self, GAME_VERSION, HostEnv};
use mcr_modmeta::scan::{self, ModInfo, ModKind, Side};
use serde_json::{Value, json};

use crate::net::{USER_AGENT, download_to};
use crate::paths::Layout;

const MAX_MANIFEST: u64 = 1 << 20;
const MAX_FILES: usize = 1024;
const MAX_PROJECT_JSON: u64 = 32 << 20;
const MAX_OVERRIDE_ENTRIES: usize = 100_000;
const MAX_OVERRIDE_BYTES: u64 = 4 << 30;
const MAX_FILE_BYTES: u64 = 1 << 30;
const CFWIDGET: &str = "https://api.cfwidget.com";
const FORGECDN: &str = "https://edge.forgecdn.net/files";
/// Between requests to cfwidget, a free service.
const POLITE: Duration = Duration::from_millis(600);
const QUEUED_RETRIES: u32 = 8;
/// Shader packs need Iris, which mcr's renderer replaces.
const SKIPPED_OVERRIDES: &[&str] = &["shaderpacks"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub minecraft: String,
    pub loader: String,
    pub overrides: String,
    pub files: Vec<FileRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileRef {
    pub project: u64,
    pub file: u64,
    pub required: bool,
}

pub fn parse_manifest(text: &str) -> Result<Manifest> {
    let root: Value = serde_json::from_str(text).context("manifest.json is not JSON")?;
    if root["manifestType"].as_str() != Some("minecraftModpack") {
        bail!("not a CurseForge modpack manifest");
    }
    let s = |v: &Value| v.as_str().map(str::to_owned);
    let loader = root["minecraft"]["modLoaders"]
        .as_array()
        .and_then(|l| {
            l.iter()
                .find(|m| m["primary"].as_bool() == Some(true))
                .or_else(|| l.first())
        })
        .and_then(|m| s(&m["id"]))
        .context("manifest names no mod loader")?;
    let files = root["files"].as_array().context("manifest has no files")?;
    if files.len() > MAX_FILES {
        bail!("manifest lists {} files (limit {MAX_FILES})", files.len());
    }
    let files = files
        .iter()
        .map(|f| {
            Ok(FileRef {
                project: f["projectID"].as_u64().context("file without projectID")?,
                file: f["fileID"].as_u64().context("file without fileID")?,
                required: f["required"].as_bool().unwrap_or(true),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Manifest {
        name: s(&root["name"]).context("manifest has no name")?,
        version: s(&root["version"]).unwrap_or_default(),
        minecraft: s(&root["minecraft"]["version"]).context("manifest has no game version")?,
        loader,
        overrides: s(&root["overrides"]).unwrap_or_else(|| "overrides".into()),
        files,
    })
}

/// Lowercase ASCII letters and digits joined by single dashes.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.truncate(48);
    out.trim_end_matches('-').to_owned()
}

pub fn file_url(file: u64, name: &str) -> String {
    format!(
        "{FORGECDN}/{}/{}/{}",
        file / 1000,
        file % 1000,
        percent_encode(name)
    )
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A file name from the web, used as a path component.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':'])
        && !name.chars().any(char::is_control)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfFile {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfProject {
    pub title: String,
    /// "Mods", "Resource Packs", "Shaders", ...
    pub kind: String,
    pub files: HashMap<u64, CfFile>,
}

pub fn parse_project(text: &str) -> Result<CfProject> {
    let root: Value = serde_json::from_str(text).context("cfwidget answer is not JSON")?;
    let files = root["files"]
        .as_array()
        .context("cfwidget answer lists no files")?
        .iter()
        .filter_map(|f| {
            Some((
                f["id"].as_u64()?,
                CfFile {
                    name: f["name"].as_str()?.to_owned(),
                    size: f["filesize"].as_u64().unwrap_or(0),
                },
            ))
        })
        .collect();
    Ok(CfProject {
        title: root["title"].as_str().unwrap_or_default().to_owned(),
        kind: root["type"].as_str().unwrap_or_default().to_owned(),
        files,
    })
}

const CF_FILES: &str = "https://www.curseforge.com/api/v1/mods";

fn fetch_project_text(id: u64) -> Result<String> {
    let url = format!("{CFWIDGET}/{id}");
    for attempt in 0..QUEUED_RETRIES {
        sleep(POLITE);
        let mut resp = ureq::get(&url)
            .header("User-Agent", USER_AGENT)
            .call()
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .with_config()
            .limit(MAX_PROJECT_JSON)
            .read_to_string()?;
        if status == 200 && text.contains("\"files\"") {
            return Ok(text);
        }
        println!("[modpack] cfwidget is indexing project {id} ({status}), retry {attempt}");
        sleep(Duration::from_secs(5));
    }
    bail!("cfwidget never answered for project {id}")
}

/// CurseForge's own file record, `{"data":{"fileName":…,"fileLength":…}}`.
pub fn parse_file_record(text: &str) -> Result<CfFile> {
    let root: Value = serde_json::from_str(text).context("file record is not JSON")?;
    let data = &root["data"];
    Ok(CfFile {
        name: data["fileName"]
            .as_str()
            .context("file record without fileName")?
            .to_owned(),
        size: data["fileLength"].as_u64().unwrap_or(0),
    })
}

/// For files cfwidget does not list (it keeps a project's newest files only).
fn file_record(cache: &Path, id: u64, file: u64) -> Result<CfFile> {
    let path = cache.join(format!("file-{file}.json"));
    if let Ok(text) = fs::read_to_string(&path)
        && let Ok(f) = parse_file_record(&text)
    {
        return Ok(f);
    }
    sleep(POLITE);
    let url = format!("{CF_FILES}/{id}/files/{file}");
    let text = ureq::get(&url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("GET {url}"))?
        .body_mut()
        .with_config()
        .limit(MAX_PROJECT_JSON)
        .read_to_string()?;
    let f = parse_file_record(&text)?;
    fs::create_dir_all(cache)?;
    fs::write(&path, &text)?;
    Ok(f)
}

/// The cached cfwidget answer, refreshed when it does not list `file` yet; CurseForge's file record fills
/// in a file cfwidget still lacks.
fn project(cache: &Path, id: u64, file: u64) -> Result<CfProject> {
    let path = cache.join(format!("{id}.json"));
    if let Ok(text) = fs::read_to_string(&path)
        && let Ok(mut p) = parse_project(&text)
    {
        if p.files.contains_key(&file) {
            return Ok(p);
        }
        if let Ok(f) = file_record(cache, id, file) {
            p.files.insert(file, f);
            return Ok(p);
        }
    }
    let text = fetch_project_text(id)?;
    let mut p = parse_project(&text)?;
    fs::create_dir_all(cache)?;
    fs::write(&path, &text)?;
    if let std::collections::hash_map::Entry::Vacant(e) = p.files.entry(file) {
        e.insert(file_record(cache, id, file)?);
    }
    Ok(p)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Keep,
    Drop(String),
}

impl Place {
    fn kept(&self) -> bool {
        *self == Self::Keep
    }

    fn json(&self) -> Value {
        match self {
            Self::Keep => json!("kept"),
            Self::Drop(why) => json!({ "dropped": why }),
        }
    }
}

/// Where one Fabric jar goes before dependencies are checked: the policy and the jar's environment.
pub fn place(info: &ModInfo, policy: &Policy) -> (Place, Place) {
    if info.kind != ModKind::Fabric {
        let why = match &info.kind {
            ModKind::Invalid(why) => format!("unreadable: {why}"),
            other => format!("not a Fabric mod ({other:?})"),
        };
        return (Place::Drop(why.clone()), Place::Drop(why));
    }
    let entry = policy.get(&info.id);
    let client = match (info.side, entry.map(|e| e.status)) {
        (Side::Server, _) => Place::Drop("server-only (environment)".into()),
        (_, Some(PolicyStatus::Blocked | PolicyStatus::Unsupported | PolicyStatus::Server)) => {
            Place::Drop(format!("policy: {}", entry.map_or("", |e| &e.reason)))
        }
        _ => Place::Keep,
    };
    let server = match info.side {
        Side::Client => Place::Drop("client-only (environment)".into()),
        Side::Server => Place::Keep,
        // A mod on the server alone may register content or required channels the client lacks.
        Side::Both if !client.kept() && entry.is_none_or(|e| e.status != PolicyStatus::Server) => {
            Place::Drop("left out of the client, so the server would not match it".into())
        }
        Side::Both => Place::Keep,
    };
    (client, server)
}

/// Drops jars whose dependencies were dropped, as mcr's pre-check and Fabric would.
fn settle(mods: &[ModInfo], places: &mut [Place]) {
    let unavailable: HashMap<String, String> = mods
        .iter()
        .zip(places.iter())
        .filter(|(_, p)| !p.kept())
        .flat_map(|(m, _)| std::iter::once(&m.id).chain(&m.provides))
        .map(|id| (id.clone(), "dropped".to_owned()))
        .collect();
    let kept: Vec<usize> = (0..mods.len()).filter(|&i| places[i].kept()).collect();
    let refs: Vec<&ModInfo> = kept.iter().map(|&i| &mods[i]).collect();
    for (&i, verdict) in kept
        .iter()
        .zip(resolve::resolve(&refs, &unavailable, &HostEnv::default()))
    {
        if let Some(why) = verdict {
            places[i] = Place::Drop(why);
        }
    }
}

#[derive(Debug)]
pub struct FetchOptions {
    pub zip: PathBuf,
    pub game_dir: Option<PathBuf>,
    pub force: bool,
}

struct Entry {
    title: String,
    kind: String,
    path: Option<PathBuf>,
}

pub fn run(layout: &Layout, opts: &FetchOptions) -> Result<()> {
    let mut zip = zip::ZipArchive::new(
        File::open(&opts.zip).with_context(|| format!("opening {}", opts.zip.display()))?,
    )
    .context("the pack is not a zip")?;
    let manifest = {
        let mut text = String::new();
        zip.by_name("manifest.json")
            .context("the pack has no manifest.json")?
            .take(MAX_MANIFEST)
            .read_to_string(&mut text)?;
        parse_manifest(&text)?
    };
    if manifest.minecraft != GAME_VERSION {
        bail!(
            "the pack is for Minecraft {}, mcr runs {GAME_VERSION}",
            manifest.minecraft
        );
    }
    let Some(loader_version) = manifest.loader.strip_prefix("fabric-") else {
        bail!(
            "the pack needs {}; modpack-fetch handles Fabric packs",
            manifest.loader
        );
    };
    if loader_version != crate::pins::LOADER_VERSION {
        println!(
            "[modpack] the pack asks for Fabric Loader {loader_version}; mcr runs {}",
            crate::pins::LOADER_VERSION
        );
    }
    let slug = slug(&manifest.name);
    if slug.is_empty() {
        bail!("the pack name has no usable characters");
    }
    let packs = layout.packs_dir();
    let out = packs.join(&slug);
    println!(
        "[modpack] {} {} -> {}",
        manifest.name,
        manifest.version,
        out.display()
    );

    let cf_cache = packs.join("cfwidget");
    let files_dir = packs.join("files");
    let mut entries = Vec::new();
    for f in &manifest.files {
        if !f.required {
            println!(
                "[modpack] skipping optional project {} file {}",
                f.project, f.file
            );
            continue;
        }
        let p = project(&cf_cache, f.project, f.file)?;
        let cf = &p.files[&f.file];
        if !safe_name(&cf.name) {
            bail!(
                "project {} file {} has an unsafe name {:?}",
                f.project,
                f.file,
                cf.name
            );
        }
        let dest = files_dir.join(f.file.to_string()).join(&cf.name);
        let fresh = fs::metadata(&dest).is_ok_and(|m| cf.size == 0 || m.len() == cf.size);
        if opts.force || !fresh {
            if cf.size > MAX_FILE_BYTES {
                bail!("{} is {} bytes", cf.name, cf.size);
            }
            println!("[download] {}", cf.name);
            download_to(&file_url(f.file, &cf.name), &dest)?;
            let got = fs::metadata(&dest)?.len();
            if cf.size != 0 && got != cf.size {
                let _ = fs::remove_file(&dest);
                bail!("{}: {got} bytes, cfwidget says {}", cf.name, cf.size);
            }
        }
        entries.push(Entry {
            title: p.title.clone(),
            kind: p.kind.clone(),
            path: Some(dest),
        });
    }

    let policy = Policy::builtin();
    let mut report = Vec::new();
    let mut resource_packs = Vec::new();
    let mut mods = Vec::new();
    let mut mod_entry = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let path = e.path.as_ref().expect("downloaded");
        let is_jar = path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("jar"));
        match e.kind.as_str() {
            "Resource Packs" => resource_packs.push(path.clone()),
            "Shaders" => report.push(json!({
                "project": e.title, "file": file_name(path),
                "client": Place::Drop("shader pack: needs Iris, which mcr's renderer replaces".into()).json(),
                "server": Place::Drop("client content".into()).json(),
            })),
            _ if is_jar => {
                mods.push(scan::scan_jar(path));
                mod_entry.push(i);
            }
            other => report.push(json!({
                "project": e.title, "file": file_name(path),
                "client": Place::Drop(format!("unhandled project type {other:?}")).json(),
                "server": Place::Drop(format!("unhandled project type {other:?}")).json(),
            })),
        }
    }
    let (mut client, mut server): (Vec<Place>, Vec<Place>) =
        mods.iter().map(|m| place(m, &policy)).unzip();
    settle(&mods, &mut client);
    settle(&mods, &mut server);

    let mods_dir = out.join("mods");
    clear_jars(&mods_dir)?;
    // Before this layout server-only jars sat in a subfolder.
    let _ = fs::remove_dir_all(mods_dir.join("server"));
    fs::create_dir_all(&mods_dir)?;
    let mut table = Vec::new();
    for (k, m) in mods.iter().enumerate() {
        let e = &entries[mod_entry[k]];
        let src = e.path.as_ref().expect("downloaded");
        // Server-only jars sit with the rest; mcr's client sees their environment or policy and skips them.
        if client[k].kept() || server[k].kept() {
            link(src, &mods_dir.join(&m.file_name))?;
        }
        report.push(json!({
            "project": e.title, "file": m.file_name, "id": m.id, "version": m.version,
            "environment": format!("{:?}", m.side).to_lowercase(),
            "client": client[k].json(), "server": server[k].json(),
        }));
        table.push((m.id.clone(), client[k].clone(), server[k].clone()));
    }

    let overrides = out.join("overrides");
    if overrides.exists() {
        fs::remove_dir_all(&overrides)?;
    }
    let written = extract_overrides(&mut zip, &manifest.overrides, &overrides)?;
    for rp in &resource_packs {
        let dest = overrides.join("resourcepacks").join(file_name(rp));
        fs::create_dir_all(dest.parent().expect("has parent"))?;
        fs::copy(rp, &dest)?;
    }
    fs::write(
        out.join("pack.json"),
        serde_json::to_string_pretty(&json!({
            "name": manifest.name,
            "version": manifest.version,
            "minecraft": manifest.minecraft,
            "loader": manifest.loader,
            "files": report,
        }))?,
    )?;

    table.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, c, s) in &table {
        let show = |p: &Place| match p {
            Place::Keep => "kept".to_owned(),
            Place::Drop(why) => format!("dropped ({why})"),
        };
        if !c.kept() || !s.kept() {
            println!("[modpack] {id}: client {}, server {}", show(c), show(s));
        }
    }
    let kept_client = client.iter().filter(|p| p.kept()).count();
    let kept_server = server.iter().filter(|p| p.kept()).count();
    println!(
        "[modpack] {} jars: {kept_client} for the client, {kept_server} for the server; {written} override files, {} resource packs",
        mods.len(),
        resource_packs.len()
    );

    let game = opts
        .game_dir
        .clone()
        .unwrap_or_else(|| layout.pack_game_dir(&slug));
    let installed = install_overrides(&overrides, &game.join("java"), opts.force)?;
    println!(
        "[modpack] installed {installed} override files into {}",
        game.join("java").display()
    );
    println!(
        "[modpack] play: mcr --mods {} --game-dir {}",
        mods_dir.display(),
        game.display()
    );
    println!("[modpack] server: cargo xtask fabric-server --pack {slug}");
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn clear_jars(dir: &Path) -> Result<()> {
    let Ok(read) = fs::read_dir(dir) else {
        return Ok(());
    };
    for e in read.flatten() {
        let p = e.path();
        if p.is_file() && p.extension().is_some_and(|x| x == "jar") {
            fs::remove_file(&p)?;
        }
    }
    Ok(())
}

fn link(src: &Path, dest: &Path) -> Result<()> {
    if fs::hard_link(src, dest).is_err() {
        fs::copy(src, dest).with_context(|| format!("copying {}", src.display()))?;
    }
    Ok(())
}

/// Unpacks `<prefix>/…` into `dest`, refusing paths that leave it and bounding count and size.
fn extract_overrides<R: Read + io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    prefix: &str,
    dest: &Path,
) -> Result<usize> {
    let mut written = 0;
    let mut bytes = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Ok(rel) = name.strip_prefix(prefix) else {
            continue;
        };
        if rel.as_os_str().is_empty()
            || rel
                .components()
                .next()
                .is_some_and(|c| SKIPPED_OVERRIDES.iter().any(|s| c.as_os_str() == *s))
        {
            continue;
        }
        let out = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        written += 1;
        bytes += entry.size();
        if written > MAX_OVERRIDE_ENTRIES || bytes > MAX_OVERRIDE_BYTES {
            bail!("the pack's overrides are too large");
        }
        fs::create_dir_all(out.parent().expect("has parent"))?;
        let mut file = File::create(&out)?;
        io::copy(&mut (&mut entry).take(MAX_OVERRIDE_BYTES), &mut file)?;
    }
    Ok(written)
}

/// Copies files missing from `dest` (all of them with `force`), so a player's later edits survive.
pub fn install_overrides(src: &Path, dest: &Path, force: bool) -> Result<usize> {
    let mut count = 0;
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let Ok(read) = fs::read_dir(src.join(&rel)) else {
            continue;
        };
        for e in read.flatten() {
            let rel = rel.join(e.file_name());
            let ty = e.file_type()?;
            if ty.is_dir() {
                stack.push(rel);
            } else if ty.is_file() {
                let to = dest.join(&rel);
                if force || !to.exists() {
                    fs::create_dir_all(to.parent().expect("has parent"))?;
                    fs::copy(e.path(), &to)?;
                    count += 1;
                }
            }
        }
    }
    Ok(count)
}

/// Every jar a server of the pack runs: the two-sided jars and the server-only ones.
pub fn server_jars(mods_dir: &Path) -> Vec<PathBuf> {
    let policy = Policy::builtin();
    scan::scan_dir(mods_dir)
        .into_iter()
        .filter(|m| policy.runs_on_server(m))
        .map(|m| m.path)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{"author":"a","files":[{"fileID":8905047,"projectID":686911,"required":true},
        {"fileID":2,"projectID":3,"required":false}],"manifestType":"minecraftModpack","manifestVersion":1,
        "minecraft":{"modLoaders":[{"id":"fabric-0.19.5","primary":true}],"version":"26.3"},
        "name":"Echoes of the Sift Fabric","overrides":"overrides","version":"1.0.1"}"#;

    #[test]
    fn manifest_parses() {
        let m = parse_manifest(MANIFEST).unwrap();
        assert_eq!(m.name, "Echoes of the Sift Fabric");
        assert_eq!(m.loader, "fabric-0.19.5");
        assert_eq!(m.minecraft, "26.3");
        assert_eq!(
            m.files,
            [
                FileRef {
                    project: 686911,
                    file: 8905047,
                    required: true
                },
                FileRef {
                    project: 3,
                    file: 2,
                    required: false
                }
            ]
        );
        assert!(parse_manifest(r#"{"manifestType":"other"}"#).is_err());
        let many = format!(
            r#"{{"manifestType":"minecraftModpack","name":"x","minecraft":{{"version":"26.3","modLoaders":[{{"id":"fabric-1"}}]}},"files":[{}]}}"#,
            vec![r#"{"projectID":1,"fileID":1}"#; MAX_FILES + 1].join(",")
        );
        assert!(parse_manifest(&many).is_err());
    }

    #[test]
    fn slugs_and_urls() {
        assert_eq!(
            slug("Echoes of the Sift Fabric"),
            "echoes-of-the-sift-fabric"
        );
        assert_eq!(slug("  Medieval -- Immersed!! "), "medieval-immersed");
        assert_eq!(slug("../.."), "");
        assert_eq!(
            file_url(8905047, "ImmediatelyFast-Fabric-1.17.1+26.3.jar"),
            "https://edge.forgecdn.net/files/8905/47/ImmediatelyFast-Fabric-1.17.1%2B26.3.jar"
        );
        assert_eq!(
            file_url(9019496, "a b.jar"),
            "https://edge.forgecdn.net/files/9019/496/a%20b.jar"
        );
        for bad in [
            "", "../x.jar", "a/b.jar", ".hidden", "a\\b", "c:d", "x\n.jar",
        ] {
            assert!(!safe_name(bad), "{bad:?}");
        }
        assert!(safe_name("fabric-api-0.162.0+26.3.jar"));
    }

    #[test]
    fn project_answers_parse() {
        let p = parse_project(
            r#"{"title":"Sodium","type":"Mods","files":[{"id":5,"name":"s.jar","filesize":10},{"id":"x"}]}"#,
        )
        .unwrap();
        assert_eq!(p.kind, "Mods");
        assert_eq!(
            p.files[&5],
            CfFile {
                name: "s.jar".into(),
                size: 10
            }
        );
        assert_eq!(p.files.len(), 1);
        assert!(parse_project(r#"{"error":"in_queue"}"#).is_err());
        let f = parse_file_record(
            r#"{"data":{"id":8951591,"fileLength":258328,"fileName":"the-deeper-end-fabric-1.1.1.jar"}}"#,
        )
        .unwrap();
        assert_eq!(f.name, "the-deeper-end-fabric-1.1.1.jar");
        assert_eq!(f.size, 258328);
        assert!(parse_file_record(r#"{"data":{}}"#).is_err());
    }

    fn info(id: &str, side: Side, depends: &[&str]) -> ModInfo {
        let mut m = scan::scan_reader(
            Path::new("x.jar"),
            "x.jar",
            io::Cursor::new(Vec::<u8>::new()),
        );
        m.kind = ModKind::Fabric;
        m.id = id.into();
        m.file_name = format!("{id}.jar");
        m.version = "1.0.0".into();
        m.side = side;
        m.depends = depends
            .iter()
            .map(|d| mcr_modmeta::scan::Dependency {
                id: (*d).into(),
                range: vec!["*".into()],
            })
            .collect();
        m
    }

    #[test]
    fn policy_and_environment_split_the_sides() {
        let policy = Policy::builtin();
        let drop = |p: &Place| !p.kept();
        let (c, s) = place(&info("sodium", Side::Client, &[]), &policy);
        assert!(drop(&c) && drop(&s));
        let (c, s) = place(&info("lithium", Side::Both, &[]), &policy);
        assert!(drop(&c) && s.kept(), "lithium runs on the server only");
        let (c, s) = place(&info("waystones", Side::Both, &[]), &policy);
        assert!(c.kept() && s.kept());
        let (c, s) = place(&info("servercore", Side::Server, &[]), &policy);
        assert!(drop(&c) && s.kept());
        let (c, s) = place(&info("sound_physics_remastered", Side::Both, &[]), &policy);
        assert!(drop(&c) && drop(&s), "a blocked two-sided mod leaves both");
        let mut neo = info("x", Side::Both, &[]);
        neo.kind = ModKind::NeoForge;
        let (c, s) = place(&neo, &policy);
        assert!(drop(&c) && drop(&s));
    }

    #[test]
    fn dependents_of_dropped_jars_drop_too() {
        let policy = Policy::builtin();
        let mods = vec![
            info("iris", Side::Client, &[]),
            info("euphoria_patches", Side::Client, &["iris"]),
            info("jade", Side::Both, &[]),
        ];
        let (mut client, _): (Vec<Place>, Vec<Place>) =
            mods.iter().map(|m| place(m, &policy)).unzip();
        settle(&mods, &mut client);
        assert!(!client[0].kept());
        assert!(matches!(&client[1], Place::Drop(why) if why.contains("iris")));
        assert!(client[2].kept());
    }

    #[test]
    fn overrides_install_without_clobbering() {
        let root = std::env::temp_dir().join(format!("mcr-modpack-{}", std::process::id()));
        let (src, dest) = (root.join("src"), root.join("dest"));
        fs::create_dir_all(src.join("config")).unwrap();
        fs::write(src.join("config/a.json"), "pack").unwrap();
        fs::write(src.join("options.txt"), "pack").unwrap();
        fs::create_dir_all(dest.join("config")).unwrap();
        fs::write(dest.join("config/a.json"), "mine").unwrap();
        assert_eq!(install_overrides(&src, &dest, false).unwrap(), 1);
        assert_eq!(
            fs::read_to_string(dest.join("config/a.json")).unwrap(),
            "mine"
        );
        assert_eq!(install_overrides(&src, &dest, true).unwrap(), 2);
        assert_eq!(
            fs::read_to_string(dest.join("config/a.json")).unwrap(),
            "pack"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
