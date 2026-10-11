use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::checksum::Checksum;
use crate::net::{download_to, get_text};
use crate::paths::Layout;

pub const LOADER_VERSION: &str = "0.19.5";
const META_URL: &str = "https://meta.fabricmc.net/v2/versions/loader";
const LAUNCH_FORMAT: u32 = 1;

#[derive(Debug, Clone, Copy)]
pub struct TestMod {
    pub file: &'static str,
    pub url: &'static str,
    pub sha512: &'static str,
}

pub const TEST_MODS: &[TestMod] = &[
    TestMod {
        file: "fabric-api-0.162.0+26.3.jar",
        url: "https://cdn.modrinth.com/data/P7dR8mSH/versions/v2j28coa/fabric-api-0.162.0%2B26.3.jar",
        sha512: "5ab70908952f1d2346d16b122ca31327f4055db59c279a1bc3c3c581ce359bb5\
                 41cba15730f55b0727be7c7c4debc4a4412a183c228e0d4dea81c734f7b412ce",
    },
    TestMod {
        file: "modmenu-21.0.0.jar",
        url: "https://cdn.modrinth.com/data/mOgUt4GM/versions/kyy7dbrZ/modmenu-21.0.0.jar",
        sha512: "588c0c557963f4778572ee81bfafc190b332c26d5926c5fe7e3e1614dcf8e14f\
                 58e3bc80a97694521dd05907e9753a7c8575048ea769e4dbf40a3db74a4e2606",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub main_class: String,
    pub jvm_args: Vec<String>,
    pub libraries: Vec<Download>,
}

/// One file to fetch. `checksum` is `None` when the source publishes it only as a Maven sidecar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub name: String,
    pub path: String,
    pub url: String,
    pub checksum: Option<Checksum>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaLibs {
    pub class_path: Vec<Download>,
    pub natives: Vec<Download>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Host {
    pub os: &'static str,
    pub arch: &'static str,
}

impl Host {
    pub fn current() -> Self {
        let os = match std::env::consts::OS {
            "macos" => "osx",
            other => other,
        };
        Self {
            os,
            arch: std::env::consts::ARCH,
        }
    }

    fn lwjgl_arch(self) -> &'static str {
        match self.arch {
            "aarch64" => "arm64",
            "x86" => "x86",
            _ => "x64",
        }
    }

    fn lib_suffix(self) -> &'static str {
        match self.os {
            "windows" => ".dll",
            "osx" => ".dylib",
            _ => ".so",
        }
    }

    fn natives_dir_name(self) -> String {
        format!("{}-{}", self.os, self.arch)
    }
}

pub fn run(layout: &Layout, game: &str, force: bool) -> Result<()> {
    let fabric = layout.fabric_dir();
    let profile_url = format!("{META_URL}/{game}/{LOADER_VERSION}/profile/json");
    println!("[fabric] profile {game} / loader {LOADER_VERSION}");
    let profile_text = get_text(&profile_url)?;
    let profile = parse_profile(&profile_text)?;
    fs::create_dir_all(&fabric)?;
    fs::write(
        fabric.join(format!("profile-{game}-{LOADER_VERSION}.json")),
        &profile_text,
    )?;

    let version_json = layout.version_json(game);
    let version_text = fs::read_to_string(&version_json).with_context(|| {
        format!(
            "{} missing; run `cargo xtask fetch` first",
            version_json.display()
        )
    })?;
    let host = Host::current();
    let vanilla = parse_vanilla_libraries(&version_text, host)?;
    let asset_index = parse_asset_index(&version_text)?;
    let game_jar = layout.client_jar(game);
    if !game_jar.is_file() {
        bail!(
            "{} missing; run `cargo xtask fetch` first",
            game_jar.display()
        );
    }

    let libs = fabric.join("libraries");
    let mut class_path = Vec::new();
    for lib in profile.libraries.iter().chain(&vanilla.class_path) {
        let dest = libs.join(&lib.path);
        fetch_one(lib, &dest, force)?;
        class_path.push(dest);
    }

    let natives = fabric.join("natives").join(host.natives_dir_name());
    if natives.exists() {
        fs::remove_dir_all(&natives)?;
    }
    fs::create_dir_all(&natives)?;
    for lib in &vanilla.natives {
        let jar = libs.join(&lib.path);
        fetch_one(lib, &jar, force)?;
        extract_natives(&jar, &natives, host)
            .with_context(|| format!("extracting natives from {}", lib.name))?;
    }

    let mods_dir = layout.test_mods_dir();
    let mut mods = Vec::new();
    for m in TEST_MODS {
        let dest = mods_dir.join(m.file);
        let d = Download {
            name: m.file.into(),
            path: m.file.into(),
            url: m.url.into(),
            checksum: Some(Checksum::Sha512(m.sha512.into())),
        };
        fetch_one(&d, &dest, force)?;
        mods.push(dest);
    }

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
        "game_version": game,
        "loader_version": LOADER_VERSION,
        "main_class": profile.main_class,
        "jvm_args": profile.jvm_args,
        "class_path": class_path.iter().map(|p| rel(p)).collect::<Result<Vec<_>>>()?,
        "game_jar": rel(&game_jar)?,
        "natives_dir": rel(&natives)?,
        "assets_dir": rel(&layout.assets_dir())?,
        "asset_index": asset_index,
        "test_mods": mods.iter().map(|p| rel(p)).collect::<Result<Vec<_>>>()?,
    });
    let out = layout.fabric_launch_json();
    fs::write(&out, serde_json::to_string_pretty(&manifest)?)?;
    println!(
        "[fabric] {} libraries, natives in {}, launch manifest {}",
        class_path.len(),
        natives.display(),
        out.display()
    );
    Ok(())
}

fn fetch_one(d: &Download, dest: &Path, force: bool) -> Result<()> {
    let checksum = match &d.checksum {
        Some(c) => c.clone(),
        None => sidecar_sha512(&d.url).with_context(|| format!("checksum for {}", d.name))?,
    };
    if !force && checksum.file_matches(dest)? {
        return Ok(());
    }
    println!("[fabric] downloading {}", d.name);
    download_to(&d.url, dest)?;
    checksum
        .verify_or_remove(dest)
        .with_context(|| format!("artifact {}", d.name))
}

fn sidecar_sha512(url: &str) -> Result<Checksum> {
    let text = get_text(&format!("{url}.sha512"))?;
    let hash = text.split_whitespace().next().unwrap_or_default();
    if hash.len() != 128 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("malformed sha512 sidecar for {url}");
    }
    Ok(Checksum::Sha512(hash.to_owned()))
}

pub fn parse_profile(text: &str) -> Result<Profile> {
    let root: Value = serde_json::from_str(text).context("Fabric profile is not JSON")?;
    let main_class = root["mainClass"]
        .as_str()
        .context("Fabric profile has no mainClass")?
        .to_owned();
    let jvm_args = root["arguments"]["jvm"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let libraries = root["libraries"]
        .as_array()
        .context("Fabric profile has no libraries")?
        .iter()
        .map(|lib| {
            let name = lib["name"].as_str().context("library without name")?;
            let repo = lib["url"]
                .as_str()
                .with_context(|| format!("library {name} has no repository url"))?;
            let path = maven_path(name)?;
            let checksum = lib["sha512"]
                .as_str()
                .map(|h| Checksum::Sha512(h.to_owned()))
                .or_else(|| lib["sha1"].as_str().map(|h| Checksum::Sha1(h.to_owned())));
            Ok(Download {
                name: name.to_owned(),
                url: format!("{}/{path}", repo.trim_end_matches('/')),
                path,
                checksum,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Profile {
        main_class,
        jvm_args,
        libraries,
    })
}

/// `group:artifact:version[:classifier][@ext]` to its repository-relative path.
pub fn maven_path(coord: &str) -> Result<String> {
    let (coord, ext) = coord.split_once('@').unwrap_or((coord, "jar"));
    let parts: Vec<&str> = coord.split(':').collect();
    let (group, artifact, version, classifier) = match parts[..] {
        [g, a, v] => (g, a, v, None),
        [g, a, v, c] => (g, a, v, Some(c)),
        _ => bail!("bad maven coordinate {coord}"),
    };
    let safe =
        |s: &str| !s.is_empty() && !s.contains("..") && !s.contains('/') && !s.contains('\\');
    if ![group, artifact, version, ext].into_iter().all(safe) || !classifier.is_none_or(safe) {
        bail!("bad maven coordinate {coord}");
    }
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Ok(format!(
        "{}/{artifact}/{version}/{file}",
        group.replace('.', "/")
    ))
}

fn rules_allow(rules: Option<&Value>, host: Host) -> bool {
    let Some(rules) = rules.and_then(Value::as_array) else {
        return true;
    };
    let mut allowed = false;
    for rule in rules {
        let os = &rule["os"];
        let name_ok = os["name"].as_str().is_none_or(|n| n == host.os);
        let arch_ok = os["arch"].as_str().is_none_or(|a| a == host.arch);
        if name_ok && arch_ok {
            allowed = rule["action"].as_str() == Some("allow");
        }
    }
    allowed
}

pub fn parse_vanilla_libraries(text: &str, host: Host) -> Result<VanillaLibs> {
    let root: Value = serde_json::from_str(text).context("version.json is not JSON")?;
    let mut class_path = Vec::new();
    let mut natives = Vec::new();
    let mut seen = BTreeMap::new();
    for lib in root["libraries"]
        .as_array()
        .context("version.json has no libraries")?
    {
        let name = lib["name"].as_str().context("library without name")?;
        if !rules_allow(lib.get("rules"), host) {
            continue;
        }
        let artifact = &lib["downloads"]["artifact"];
        let field = |k: &str| {
            artifact[k]
                .as_str()
                .with_context(|| format!("library {name} has no artifact {k}"))
        };
        let path = field("path")?;
        if path.contains("..") || path.starts_with('/') {
            bail!("library {name} has an unsafe path {path}");
        }
        if seen.insert(path.to_owned(), ()).is_some() {
            continue;
        }
        let d = Download {
            name: name.to_owned(),
            path: path.to_owned(),
            url: field("url")?.to_owned(),
            checksum: Some(Checksum::Sha1(field("sha1")?.to_owned())),
        };
        if name.contains(":natives-") {
            natives.push(d);
        } else {
            class_path.push(d);
        }
    }
    Ok(VanillaLibs {
        class_path,
        natives,
    })
}

fn parse_asset_index(text: &str) -> Result<String> {
    let root: Value = serde_json::from_str(text).context("version.json is not JSON")?;
    root["assetIndex"]["id"]
        .as_str()
        .map(str::to_owned)
        .context("version.json has no assetIndex.id")
}

/// The file name a natives-jar entry extracts to, or `None` when it is not a library for `host`.
pub fn native_target(entry: &str, host: Host) -> Option<&str> {
    if entry.starts_with("META-INF/") || !entry.ends_with(host.lib_suffix()) {
        return None;
    }
    let mut parts: Vec<&str> = entry.split('/').collect();
    let file = parts.pop()?;
    if let [os, arch, ..] = parts[..] {
        let os_ok = matches!(
            (os, host.os),
            ("linux", "linux") | ("macos", "osx") | ("windows", "windows")
        );
        if !os_ok || arch != host.lwjgl_arch() {
            return None;
        }
    }
    Some(file)
}

fn extract_natives(jar: &Path, out: &Path, host: Host) -> Result<()> {
    let mut zip = zip::ZipArchive::new(File::open(jar)?)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name()?.into_owned();
        let Some(file) = native_target(&name, host).map(PathBuf::from) else {
            continue;
        };
        let mut bytes = Vec::with_capacity(entry.size().min(64 << 20) as usize);
        entry.read_to_end(&mut bytes)?;
        let dest = out.join(&file);
        if dest.exists() && fs::read(&dest)? != bytes {
            bail!("two natives jars provide different {}", file.display());
        }
        fs::write(&dest, &bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = r#"{
        "id": "fabric-loader-0.19.5-26.3",
        "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
        "arguments": { "game": [], "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "] },
        "libraries": [
            { "name": "org.ow2.asm:asm:9.10.1", "url": "https://maven.fabricmc.net/",
              "sha1": "ada2", "sha512": "8db6" },
            { "name": "net.fabricmc:sponge-mixin:0.17.4+mixin.0.8.7", "url": "https://maven.fabricmc.net/",
              "sha1": "5f66" },
            { "name": "net.fabricmc:fabric-loader:0.19.5", "url": "https://maven.fabricmc.net" }
        ]
    }"#;

    const VERSION: &str = r#"{
        "assetIndex": { "id": "34" },
        "libraries": [
            { "name": "com.mojang:brigadier:1.3.11",
              "downloads": { "artifact": { "path": "com/mojang/brigadier/1.3.11/brigadier-1.3.11.jar",
                "sha1": "aa", "url": "https://libraries.minecraft.net/com/mojang/brigadier/1.3.11/brigadier-1.3.11.jar" } } },
            { "name": "ca.weblite:java-objc-bridge:1.1",
              "rules": [{ "action": "allow", "os": { "name": "osx" } }],
              "downloads": { "artifact": { "path": "ca/weblite/x.jar", "sha1": "bb", "url": "u" } } },
            { "name": "org.lwjgl:lwjgl-sdl:3.4.3:natives-linux",
              "rules": [{ "action": "allow", "os": { "name": "linux" } }],
              "downloads": { "artifact": { "path": "org/lwjgl/lwjgl-sdl/3.4.3/lwjgl-sdl-3.4.3-natives-linux.jar",
                "sha1": "cc", "url": "n" } } },
            { "name": "org.lwjgl:lwjgl-sdl:3.4.3:natives-windows",
              "rules": [{ "action": "allow", "os": { "name": "windows" } }],
              "downloads": { "artifact": { "path": "w.jar", "sha1": "dd", "url": "w" } } }
        ]
    }"#;

    const LINUX: Host = Host {
        os: "linux",
        arch: "x86_64",
    };

    #[test]
    fn profile_libraries_prefer_sha512_and_allow_missing_checksums() {
        let p = parse_profile(PROFILE).unwrap();
        assert_eq!(
            p.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(
            p.jvm_args,
            ["-DFabricMcEmu= net.minecraft.client.main.Main "]
        );
        let asm = &p.libraries[0];
        assert_eq!(asm.path, "org/ow2/asm/asm/9.10.1/asm-9.10.1.jar");
        assert_eq!(
            asm.url,
            "https://maven.fabricmc.net/org/ow2/asm/asm/9.10.1/asm-9.10.1.jar"
        );
        assert_eq!(asm.checksum, Some(Checksum::Sha512("8db6".into())));
        assert_eq!(p.libraries[1].checksum, Some(Checksum::Sha1("5f66".into())));
        let loader = &p.libraries[2];
        assert_eq!(loader.checksum, None);
        assert_eq!(
            loader.url,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.19.5/fabric-loader-0.19.5.jar"
        );
    }

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("net.fabricmc:sponge-mixin:0.17.4+mixin.0.8.7").unwrap(),
            "net/fabricmc/sponge-mixin/0.17.4+mixin.0.8.7/sponge-mixin-0.17.4+mixin.0.8.7.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.4.3:natives-linux").unwrap(),
            "org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3-natives-linux.jar"
        );
        assert_eq!(maven_path("a.b:c:1@zip").unwrap(), "a/b/c/1/c-1.zip");
        for bad in ["a:b", "a:b:c:d:e", "a:..:1", "a:b/c:1", "a::1"] {
            assert!(maven_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn vanilla_libraries_follow_os_rules_and_split_natives() {
        let libs = parse_vanilla_libraries(VERSION, LINUX).unwrap();
        let names: Vec<_> = libs.class_path.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["com.mojang:brigadier:1.3.11"]);
        let natives: Vec<_> = libs.natives.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(natives, ["org.lwjgl:lwjgl-sdl:3.4.3:natives-linux"]);
        assert_eq!(
            libs.class_path[0].checksum,
            Some(Checksum::Sha1("aa".into()))
        );
        assert_eq!(parse_asset_index(VERSION).unwrap(), "34");
    }

    #[test]
    fn natives_filter_by_os_and_arch() {
        let t = |e| native_target(e, LINUX);
        assert_eq!(t("linux/x64/org/lwjgl/sdl/libSDL3.so"), Some("libSDL3.so"));
        assert_eq!(t("linux/arm64/org/lwjgl/sdl/libSDL3.so"), None);
        assert_eq!(t("META-INF/linux/x64/org/lwjgl/sdl/libSDL3.so"), None);
        assert_eq!(t("linux/x64/org/lwjgl/sdl/libSDL3.so.sha1"), None);
        assert_eq!(t("libjtracy-jni-linux.so"), Some("libjtracy-jni-linux.so"));
        assert_eq!(t("windows/x64/org/lwjgl/lwjgl.dll"), None);
    }

    #[test]
    fn test_mod_hashes_are_sha512_hex() {
        for m in TEST_MODS {
            assert_eq!(m.sha512.len(), 128, "{}", m.file);
            assert!(m.sha512.bytes().all(|b| b.is_ascii_hexdigit()));
        }
    }
}
