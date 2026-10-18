use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::checksum::{Checksum, sha1_hex};
use crate::fabric::{self, Download, Host};
use crate::paths::{Layout, java_bin};

pub use crate::pins::NEOFORGE_VERSION;
const INSTALLER_SHA512: &str = "318b0c9006cb23820a5beea72bfaf0fdcce5c039d2f3f63bb1fe65d99dfec168\
                                4c10d6610c95c6b7a68aa3843f4f1e50a61a2f8b6293fd05b722d1affbeef7a8";
const MAVEN: &str = "https://maven.neoforged.net/releases";
const LAUNCH_FORMAT: u32 = 1;
const MAX_LIBRARIES: usize = 512;
const MAX_ENTRY_BYTES: u64 = 64 << 20;
const SIDE: &str = "client";

/// What the installer's launcher profile and install profile say, read without running the installer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profiles {
    pub main_class: String,
    pub minecraft: String,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    /// The launch class path's NeoForge part (FML and its libraries).
    pub libraries: Vec<Download>,
    /// What the client processors need besides the launch libraries (installer tools, NeoForge's jar).
    pub install_libraries: Vec<Download>,
    pub data: BTreeMap<String, String>,
    pub processors: Vec<Processor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Processor {
    pub jar: String,
    pub classpath: Vec<String>,
    pub args: Vec<String>,
}

pub fn installer_url(version: &str) -> String {
    format!("{MAVEN}/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar")
}

pub fn run(layout: &Layout, game: &str, force: bool) -> Result<()> {
    let dir = layout.neoforge_dir();
    let libs = dir.join("libraries");
    let installer = dir.join(format!("neoforge-{NEOFORGE_VERSION}-installer.jar"));
    println!("[neoforge] installer {NEOFORGE_VERSION}");
    fabric::fetch_one(
        &Download {
            name: format!("neoforge-{NEOFORGE_VERSION}-installer"),
            path: String::new(),
            url: installer_url(NEOFORGE_VERSION),
            checksum: Some(Checksum::Sha512(INSTALLER_SHA512.into())),
        },
        &installer,
        force,
    )?;
    let mut zip = zip::ZipArchive::new(File::open(&installer)?)
        .with_context(|| format!("reading {}", installer.display()))?;
    let version_json = read_entry(&mut zip, "version.json")?;
    let install_json = read_entry(&mut zip, "install_profile.json")?;
    let profiles = parse_profiles(
        &String::from_utf8_lossy(&version_json),
        &String::from_utf8_lossy(&install_json),
    )?;
    if profiles.minecraft != game {
        bail!(
            "NeoForge {NEOFORGE_VERSION} is for Minecraft {}, not {game}",
            profiles.minecraft
        );
    }

    let version_text = fs::read_to_string(layout.version_json(game)).with_context(|| {
        format!(
            "{} missing; run `cargo xtask fetch` first",
            layout.version_json(game).display()
        )
    })?;
    let game_jar = layout.client_jar(game);
    if !game_jar.is_file() {
        bail!(
            "{} missing; run `cargo xtask fetch` first",
            game_jar.display()
        );
    }
    let host = Host::current();
    let vanilla = fabric::parse_vanilla_libraries(&version_text, host)?;
    let asset_index = fabric::parse_asset_index(&version_text)?;

    let mut fetched = HashSet::new();
    for lib in profiles
        .libraries
        .iter()
        .chain(&profiles.install_libraries)
        .chain(&vanilla.class_path)
        .chain(&vanilla.natives)
    {
        if fetched.insert(lib.path.clone()) {
            fetch_library(lib, &libs, &mut zip, force)?;
        }
    }
    let class_path: Vec<PathBuf> = merge_libraries(&profiles.libraries, &vanilla.class_path)
        .iter()
        .map(|d| libs.join(&d.path))
        .collect();

    let natives = dir.join("natives").join(host.natives_dir_name());
    if natives.exists() {
        fs::remove_dir_all(&natives)?;
    }
    fs::create_dir_all(&natives)?;
    for lib in &vanilla.natives {
        fabric::extract_natives(&libs.join(&lib.path), &natives, host)
            .with_context(|| format!("extracting natives from {}", lib.name))?;
    }

    let work = dir.join(format!("installer-{NEOFORGE_VERSION}"));
    let patched = run_processors(
        layout, &profiles, &mut zip, &installer, &game_jar, &libs, &work, force,
    )?;

    let cache = layout.cache_dir();
    let rel = |p: &Path| -> Result<String> {
        let r = p
            .strip_prefix(&cache)
            .with_context(|| format!("{} is outside the cache", p.display()))?;
        r.to_str()
            .map(|s| s.replace('\\', "/"))
            .with_context(|| format!("non UTF-8 path {}", r.display()))
    };
    let manifest = json!({
        "format": LAUNCH_FORMAT,
        "loader": "neoforge",
        "game_version": game,
        "loader_version": NEOFORGE_VERSION,
        "main_class": profiles.main_class,
        "jvm_args": profiles.jvm_args,
        "game_args": profiles.game_args,
        "class_path": class_path.iter().map(|p| rel(p)).collect::<Result<Vec<_>>>()?,
        "library_dir": rel(&libs)?,
        "game_jar": rel(&patched)?,
        "natives_dir": rel(&natives)?,
        "assets_dir": rel(&layout.assets_dir())?,
        "asset_index": asset_index,
        "test_mods": Vec::<String>::new(),
    });
    let out = layout.neoforge_launch_json();
    fs::write(&out, serde_json::to_string_pretty(&manifest)?)?;
    println!(
        "[neoforge] {} libraries, patched client {}, launch manifest {}",
        class_path.len(),
        patched.display(),
        out.display()
    );
    Ok(())
}

fn read_entry<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>> {
    let entry = zip
        .by_name(name)
        .with_context(|| format!("the installer has no {name}"))?;
    if entry.size() > MAX_ENTRY_BYTES {
        bail!("installer entry {name} is too large");
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.take(MAX_ENTRY_BYTES).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Libraries without a URL ship inside the installer under `maven/`.
fn fetch_library<R: Read + std::io::Seek>(
    lib: &Download,
    libs: &Path,
    zip: &mut zip::ZipArchive<R>,
    force: bool,
) -> Result<()> {
    let dest = libs.join(&lib.path);
    if !lib.url.is_empty() {
        return fabric::fetch_one(lib, &dest, force);
    }
    let checksum = lib
        .checksum
        .clone()
        .with_context(|| format!("library {} has neither URL nor checksum", lib.name))?;
    if !force && checksum.file_matches(&dest)? {
        return Ok(());
    }
    let bytes = read_entry(zip, &format!("maven/{}", lib.path))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&dest, bytes)?;
    checksum.verify_or_remove(&dest)
}

fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.split('/').any(|c| c == ".." || c.is_empty())
}

fn parse_libraries(root: &Value, what: &str) -> Result<Vec<Download>> {
    let list = root["libraries"]
        .as_array()
        .with_context(|| format!("{what} has no libraries"))?;
    if list.len() > MAX_LIBRARIES {
        bail!("{what} lists {} libraries", list.len());
    }
    list.iter()
        .map(|lib| {
            let name = lib["name"].as_str().context("library without name")?;
            let artifact = &lib["downloads"]["artifact"];
            let path = match artifact["path"].as_str() {
                Some(p) => p.to_owned(),
                None => fabric::maven_path(name)?,
            };
            if !safe_relative(&path) {
                bail!("library {name} has an unsafe path {path}");
            }
            let checksum = artifact["sha1"]
                .as_str()
                .map(|h| Checksum::Sha1(h.to_owned()));
            Ok(Download {
                name: name.to_owned(),
                url: artifact["url"].as_str().unwrap_or_default().to_owned(),
                path,
                checksum,
            })
        })
        .collect()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_profiles(version_json: &str, install_json: &str) -> Result<Profiles> {
    let version: Value = serde_json::from_str(version_json).context("version.json is not JSON")?;
    let install: Value =
        serde_json::from_str(install_json).context("install_profile.json is not JSON")?;
    let main_class = version["mainClass"]
        .as_str()
        .context("version.json has no mainClass")?
        .to_owned();
    let minecraft = install["minecraft"]
        .as_str()
        .or_else(|| version["inheritsFrom"].as_str())
        .context("the installer names no Minecraft version")?
        .to_owned();
    let libraries = parse_libraries(&version, "version.json")?;
    let launch_paths: HashSet<&str> = libraries.iter().map(|d| d.path.as_str()).collect();
    let install_libraries = parse_libraries(&install, "install_profile.json")?
        .into_iter()
        .filter(|d| !launch_paths.contains(d.path.as_str()))
        .collect();
    let mut data = BTreeMap::new();
    if let Some(map) = install["data"].as_object() {
        for (key, sides) in map {
            if let Some(v) = sides[SIDE].as_str() {
                data.insert(key.clone(), v.to_owned());
            }
        }
    }
    let processors = install["processors"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter(|p| {
                    p["sides"]
                        .as_array()
                        .is_none_or(|s| s.iter().any(|v| v.as_str() == Some(SIDE)))
                })
                .map(|p| -> Result<Processor> {
                    Ok(Processor {
                        jar: p["jar"]
                            .as_str()
                            .context("processor without jar")?
                            .to_owned(),
                        classpath: strings(&p["classpath"]),
                        args: strings(&p["args"]),
                    })
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(Profiles {
        main_class,
        minecraft,
        jvm_args: strings(&version["arguments"]["jvm"]),
        game_args: strings(&version["arguments"]["game"]),
        libraries,
        install_libraries,
        data,
        processors,
    })
}

/// `group:artifact[:classifier]`, the identity a launcher dedupes libraries by.
pub fn library_key(name: &str) -> String {
    let coord = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = coord.split(':').collect();
    match parts[..] {
        [g, a, _, c] => format!("{g}:{a}:{c}"),
        [g, a, ..] => format!("{g}:{a}"),
        _ => coord.to_owned(),
    }
}

/// NeoForge's libraries first; a vanilla library NeoForge also pins is left out, as launchers do.
pub fn merge_libraries(neoforge: &[Download], vanilla: &[Download]) -> Vec<Download> {
    let keys: HashSet<String> = neoforge.iter().map(|d| library_key(&d.name)).collect();
    neoforge
        .iter()
        .chain(
            vanilla
                .iter()
                .filter(|d| !keys.contains(&library_key(&d.name))),
        )
        .cloned()
        .collect()
}

pub struct ArgContext<'a> {
    pub data: &'a BTreeMap<String, String>,
    pub libs: &'a Path,
    pub work: &'a Path,
    pub minecraft_jar: &'a Path,
    pub installer: &'a Path,
    pub root: &'a Path,
    pub minecraft: &'a str,
}

fn coord_path(coord: &str, libs: &Path) -> Result<PathBuf> {
    Ok(libs.join(fabric::maven_path(coord)?))
}

fn path_string(p: &Path) -> Result<String> {
    p.to_str()
        .map(str::to_owned)
        .with_context(|| format!("non UTF-8 path {}", p.display()))
}

/// A data value: `[coord]` is a library, `'text'` a literal, `/path` a file the installer carries.
fn data_value(value: &str, ctx: &ArgContext) -> Result<String> {
    if let Some(coord) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
        return path_string(&coord_path(coord, ctx.libs)?);
    }
    if let Some(lit) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        return Ok(lit.to_owned());
    }
    if let Some(entry) = value.strip_prefix('/') {
        if !safe_relative(entry) {
            bail!("unsafe installer path {value}");
        }
        return path_string(&ctx.work.join(entry));
    }
    bail!("unknown installer data value {value}")
}

pub fn resolve_arg(arg: &str, ctx: &ArgContext) -> Result<String> {
    if let Some(key) = arg.strip_prefix('{').and_then(|a| a.strip_suffix('}')) {
        return match key {
            "MINECRAFT_JAR" => path_string(ctx.minecraft_jar),
            "INSTALLER" => path_string(ctx.installer),
            "ROOT" => path_string(ctx.root),
            "LIBRARY_DIR" => path_string(ctx.libs),
            "SIDE" => Ok(SIDE.to_owned()),
            "MINECRAFT_VERSION" => Ok(ctx.minecraft.to_owned()),
            _ => data_value(
                ctx.data
                    .get(key)
                    .with_context(|| format!("installer argument {{{key}}} has no value"))?,
                ctx,
            ),
        };
    }
    if arg.starts_with('[') {
        return data_value(arg, ctx);
    }
    if arg.contains('{') {
        let mut out = String::new();
        let mut rest = arg;
        while let Some(start) = rest.find('{') {
            let end = rest[start..]
                .find('}')
                .with_context(|| format!("unclosed token in {arg}"))?;
            out.push_str(&rest[..start]);
            out.push_str(&resolve_arg(&rest[start..=start + end], ctx)?);
            rest = &rest[start + end + 1..];
        }
        out.push_str(rest);
        return Ok(out);
    }
    Ok(arg.to_owned())
}

fn main_class(jar: &Path) -> Result<String> {
    let mut zip = zip::ZipArchive::new(File::open(jar)?)?;
    let manifest = read_entry(&mut zip, "META-INF/MANIFEST.MF")?;
    String::from_utf8_lossy(&manifest)
        .lines()
        .find_map(|l| l.strip_prefix("Main-Class:").map(|c| c.trim().to_owned()))
        .with_context(|| format!("{} has no Main-Class", jar.display()))
}

/// Runs the client processors (binary patching) unless their output matches this installer and client jar.
#[allow(clippy::too_many_arguments)]
fn run_processors<R: Read + std::io::Seek>(
    layout: &Layout,
    profiles: &Profiles,
    zip: &mut zip::ZipArchive<R>,
    installer: &Path,
    game_jar: &Path,
    libs: &Path,
    work: &Path,
    force: bool,
) -> Result<PathBuf> {
    let patched_coord = profiles
        .data
        .get("PATCHED")
        .context("install profile has no PATCHED output")?;
    let patched = PathBuf::from(data_value(
        patched_coord,
        &ArgContext {
            data: &profiles.data,
            libs,
            work,
            minecraft_jar: game_jar,
            installer,
            root: layout.neoforge_dir().as_path(),
            minecraft: &profiles.minecraft,
        },
    )?);
    let stamp_path = patched.with_extension("inputs");
    let stamp = format!("{INSTALLER_SHA512}\n{}\n", sha1_hex(&fs::read(game_jar)?));
    if !force && patched.is_file() && fs::read_to_string(&stamp_path).is_ok_and(|s| s == stamp) {
        return Ok(patched);
    }
    if work.exists() {
        fs::remove_dir_all(work)?;
    }
    fs::create_dir_all(work)?;
    for value in profiles.data.values() {
        if let Some(entry) = value.strip_prefix('/') {
            if !safe_relative(entry) {
                bail!("unsafe installer path {value}");
            }
            let dest = work.join(entry);
            fs::create_dir_all(dest.parent().context("installer data path")?)?;
            fs::write(&dest, read_entry(zip, entry)?)?;
        }
    }
    let java = java_bin(&layout.jdk_dir());
    if !java.is_file() {
        bail!("{} missing; run `cargo xtask jdk` first", java.display());
    }
    let root = layout.neoforge_dir();
    let ctx = ArgContext {
        data: &profiles.data,
        libs,
        work,
        minecraft_jar: game_jar,
        installer,
        root: &root,
        minecraft: &profiles.minecraft,
    };
    for p in &profiles.processors {
        let jar = coord_path(&p.jar, libs)?;
        let mut cp = vec![jar.clone()];
        for c in &p.classpath {
            let path = coord_path(c, libs)?;
            if !cp.contains(&path) {
                cp.push(path);
            }
        }
        let args = p
            .args
            .iter()
            .map(|a| resolve_arg(a, &ctx))
            .collect::<Result<Vec<_>>>()?;
        println!("[neoforge] processor {}", p.jar);
        let status = Command::new(&java)
            .arg("-cp")
            .arg(std::env::join_paths(&cp)?)
            .arg(main_class(&jar)?)
            .args(&args)
            .current_dir(work)
            .status()
            .with_context(|| format!("starting processor {}", p.jar))?;
        if !status.success() {
            bail!("processor {} failed ({status})", p.jar);
        }
    }
    if !patched.is_file() {
        bail!("the processors did not write {}", patched.display());
    }
    fs::write(&stamp_path, stamp)?;
    Ok(patched)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION: &str = r#"{
        "id": "neoforge-26.3.0.90",
        "mainClass": "net.neoforged.fml.startup.Client",
        "inheritsFrom": "26.3",
        "arguments": {
            "game": ["--fml.neoForgeVersion", "26.3.0.90", "--fml.mcVersion", "26.3"],
            "jvm": ["-DlibraryDirectory=${library_directory}", {"rules": [], "value": "x"}]
        },
        "libraries": [
            {"name": "net.neoforged.fancymodloader:loader:12.0.12",
             "downloads": {"artifact": {"sha1": "aa", "url": "https://maven.neoforged.net/releases/net/neoforged/fancymodloader/loader/12.0.12/loader-12.0.12.jar",
                                        "path": "net/neoforged/fancymodloader/loader/12.0.12/loader-12.0.12.jar"}}},
            {"name": "org.ow2.asm:asm:9.11",
             "downloads": {"artifact": {"sha1": "bb", "url": "https://maven.neoforged.net/releases/org/ow2/asm/asm/9.11/asm-9.11.jar",
                                        "path": "org/ow2/asm/asm/9.11/asm-9.11.jar"}}}
        ]
    }"#;

    const INSTALL: &str = r#"{
        "minecraft": "26.3",
        "data": {
            "BINPATCH": {"client": "/data/client.lzma", "server": "/data/server.lzma"},
            "PATCHED": {"client": "[net.neoforged:minecraft-client-patched:26.3.0.90]"},
            "MCP_VERSION": {"client": "'26.3-1'"}
        },
        "processors": [
            {"sides": ["server"], "jar": "a:b:1", "args": ["--server"]},
            {"jar": "net.neoforged.installertools:installertools:4.0.17:fatjar",
             "classpath": ["net.neoforged.installertools:installertools:4.0.17:fatjar"],
             "args": ["--input", "{MINECRAFT_JAR}", "--output", "{PATCHED}", "--apply-patches", "{BINPATCH}",
                      "--extract-libraries-to", "{ROOT}/libraries/", "--v", "{MCP_VERSION}"]}
        ],
        "libraries": [
            {"name": "org.ow2.asm:asm:9.11",
             "downloads": {"artifact": {"sha1": "bb", "url": "u", "path": "org/ow2/asm/asm/9.11/asm-9.11.jar"}}},
            {"name": "net.neoforged:neoforge:26.3.0.90:universal",
             "downloads": {"artifact": {"sha1": "cc", "url": "",
                                        "path": "net/neoforged/neoforge/26.3.0.90/neoforge-26.3.0.90-universal.jar"}}}
        ]
    }"#;

    #[test]
    fn profiles_keep_client_processors_and_split_libraries() {
        let p = parse_profiles(VERSION, INSTALL).unwrap();
        assert_eq!(p.main_class, "net.neoforged.fml.startup.Client");
        assert_eq!(p.minecraft, "26.3");
        assert_eq!(p.jvm_args, ["-DlibraryDirectory=${library_directory}"]);
        assert_eq!(p.game_args.len(), 4);
        assert_eq!(p.libraries.len(), 2);
        assert_eq!(p.install_libraries.len(), 1);
        assert!(p.install_libraries[0].url.is_empty());
        assert_eq!(p.processors.len(), 1);
        assert_eq!(p.data["BINPATCH"], "/data/client.lzma");
    }

    #[test]
    fn unsafe_library_paths_are_refused() {
        let bad = VERSION.replace(
            "org/ow2/asm/asm/9.11/asm-9.11.jar",
            "org/../../etc/asm-9.11.jar",
        );
        assert!(parse_profiles(&bad, INSTALL).is_err());
    }

    #[test]
    fn processor_arguments_resolve() {
        let p = parse_profiles(VERSION, INSTALL).unwrap();
        let ctx = ArgContext {
            data: &p.data,
            libs: Path::new("/c/libs"),
            work: Path::new("/c/work"),
            minecraft_jar: Path::new("/c/client.jar"),
            installer: Path::new("/c/installer.jar"),
            root: Path::new("/c"),
            minecraft: "26.3",
        };
        let args: Vec<String> = p.processors[0]
            .args
            .iter()
            .map(|a| resolve_arg(a, &ctx).unwrap())
            .collect();
        assert_eq!(
            args,
            [
                "--input",
                "/c/client.jar",
                "--output",
                "/c/libs/net/neoforged/minecraft-client-patched/26.3.0.90/minecraft-client-patched-26.3.0.90.jar",
                "--apply-patches",
                "/c/work/data/client.lzma",
                "--extract-libraries-to",
                "/c/libraries/",
                "--v",
                "26.3-1",
            ]
        );
        assert!(resolve_arg("{NOPE}", &ctx).is_err());
        let mut data = p.data.clone();
        data.insert("EVIL".into(), "/../x".into());
        let evil = ArgContext { data: &data, ..ctx };
        assert!(resolve_arg("{EVIL}", &evil).is_err());
    }

    #[test]
    fn merge_prefers_neoforge_libraries() {
        let d = |name: &str| Download {
            name: name.into(),
            path: name.replace(':', "/"),
            url: String::new(),
            checksum: None,
        };
        let merged = merge_libraries(
            &[d("org.ow2.asm:asm:9.11")],
            &[
                d("org.ow2.asm:asm:9.6"),
                d("org.lwjgl:lwjgl:3.4.3"),
                d("org.lwjgl:lwjgl:3.4.3:natives-linux"),
            ],
        );
        let names: Vec<&str> = merged.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "org.ow2.asm:asm:9.11",
                "org.lwjgl:lwjgl:3.4.3",
                "org.lwjgl:lwjgl:3.4.3:natives-linux"
            ]
        );
        assert_eq!(library_key("a:b:1:c@zip"), "a:b:c");
    }
}
