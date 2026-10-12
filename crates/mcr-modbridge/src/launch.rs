use std::path::{Path, PathBuf};

use mcr_jvm::JvmOptions;
use serde_json::Value;

use crate::BridgeError;
use crate::abi::{COMPOSE_PROPERTY, TABLE_PROPERTY};

pub const LAUNCH_MANIFEST: &str = "fabric/launch.json";
pub const BOOT_JAR: &str = "java/mcr-boot.jar";
pub const BRIDGE_JAR: &str = "java/mcr-bridge.jar";
const LAUNCH_FORMAT: u64 = 1;
pub const HOSTS_FILE: &str = "mcr-hosts";
const HOSTS: &str = "127.0.0.1 localhost\n::1 localhost\n";

/// What `cargo xtask fabric-fetch` resolved: paths are absolute (joined onto the cache dir).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchManifest {
    pub game_version: String,
    pub loader_version: String,
    pub main_class: String,
    pub jvm_args: Vec<String>,
    pub class_path: Vec<PathBuf>,
    pub game_jar: PathBuf,
    pub natives_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub asset_index: String,
    pub test_mods: Vec<PathBuf>,
}

impl LaunchManifest {
    pub fn load(cache_dir: &Path) -> Result<Self, BridgeError> {
        let path = cache_dir.join(LAUNCH_MANIFEST);
        let text = std::fs::read_to_string(&path).map_err(|e| BridgeError::Missing {
            path: path.clone(),
            hint: format!("{e}; run `cargo xtask fabric-fetch`"),
        })?;
        Self::parse(&text, cache_dir)
    }

    pub fn parse(text: &str, cache_dir: &Path) -> Result<Self, BridgeError> {
        let bad = |what: &str| BridgeError::Manifest(what.to_owned());
        let root: Value = serde_json::from_str(text).map_err(|e| bad(&e.to_string()))?;
        if root["format"].as_u64() != Some(LAUNCH_FORMAT) {
            return Err(bad("unsupported format; rerun `cargo xtask fabric-fetch`"));
        }
        let string = |key: &str| {
            root[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| bad(&format!("missing {key}")))
        };
        let strings = |key: &str| -> Result<Vec<String>, BridgeError> {
            root[key]
                .as_array()
                .ok_or_else(|| bad(&format!("missing {key}")))?
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<_>>()
                .ok_or_else(|| bad(&format!("{key} holds a non-string")))
        };
        let path = |rel: String| -> Result<PathBuf, BridgeError> {
            let rel = PathBuf::from(rel);
            if rel.is_absolute()
                || rel
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(bad(&format!("path {} leaves the cache", rel.display())));
            }
            Ok(cache_dir.join(rel))
        };
        let paths = |key: &str| -> Result<Vec<PathBuf>, BridgeError> {
            strings(key)?.into_iter().map(path).collect()
        };
        Ok(Self {
            game_version: string("game_version")?,
            loader_version: string("loader_version")?,
            main_class: string("main_class")?,
            jvm_args: strings("jvm_args")?,
            class_path: paths("class_path")?,
            game_jar: path(string("game_jar")?)?,
            natives_dir: path(string("natives_dir")?)?,
            assets_dir: path(string("assets_dir")?)?,
            asset_index: string("asset_index")?,
            test_mods: paths("test_mods")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// The repository `.cache` (jars, natives, assets).
    pub cache_dir: PathBuf,
    /// Java's own game directory; its `mods/` stays empty.
    pub game_dir: PathBuf,
    pub mods: Vec<PathBuf>,
    pub username: String,
    pub uuid: String,
    pub width: u32,
    pub height: u32,
    pub max_heap: String,
    /// Extra `-D` options, e.g. test hooks.
    pub extra_jvm: Vec<String>,
    /// The render thread drives Java's frames and composes its GUI ([`crate::Bridge::frames`]);
    /// needs [`crate::gpu::install`] first.
    pub compose: bool,
    /// Hosts Java's HTTP may reach through the loopback proxy (`*.example.com` for subdomains).
    pub http_hosts: Vec<String>,
    /// The proxy's port; [`crate::Bridge::start`] fills it.
    pub proxy_port: Option<u16>,
}

impl BridgeConfig {
    /// Java resolves relative paths against `user.dir` (the game dir), so every path it gets must be absolute.
    pub fn make_absolute(&mut self) -> std::io::Result<()> {
        self.cache_dir = std::path::absolute(&self.cache_dir)?;
        self.game_dir = std::path::absolute(&self.game_dir)?;
        for jar in &mut self.mods {
            *jar = std::path::absolute(&*jar)?;
        }
        Ok(())
    }

    pub fn new(cache_dir: PathBuf, game_dir: PathBuf, mods: Vec<PathBuf>) -> Self {
        Self {
            cache_dir,
            game_dir,
            mods,
            username: "Player".into(),
            uuid: "00000000-0000-0000-0000-000000000000".into(),
            width: 854,
            height: 480,
            max_heap: "2G".into(),
            extra_jvm: Vec::new(),
            compose: false,
            http_hosts: crate::proxy::MOJANG_HOSTS
                .iter()
                .map(|h| (*h).to_owned())
                .collect(),
            proxy_port: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub options_class_path: Vec<PathBuf>,
    pub max_heap: String,
    pub properties: Vec<String>,
    pub args: Vec<String>,
}

impl LaunchPlan {
    pub fn jvm_options(&self) -> JvmOptions {
        JvmOptions {
            class_path: self.options_class_path.clone(),
            max_heap: self.max_heap.clone(),
            extra: self.properties.clone(),
        }
    }
}

fn join(paths: &[PathBuf]) -> Result<String, BridgeError> {
    let joined = std::env::join_paths(paths).map_err(|e| BridgeError::Manifest(e.to_string()))?;
    joined
        .into_string()
        .map_err(|p| BridgeError::Manifest(format!("non UTF-8 path {}", p.to_string_lossy())))
}

fn utf8(p: &Path) -> Result<&str, BridgeError> {
    p.to_str()
        .ok_or_else(|| BridgeError::Manifest(format!("non UTF-8 path {}", p.display())))
}

/// The class path, `-D` properties and `McrBoot` arguments for one Fabric boot.
pub fn plan(
    manifest: &LaunchManifest,
    config: &BridgeConfig,
    table_address: i64,
) -> Result<LaunchPlan, BridgeError> {
    let boot_jar = config.cache_dir.join(BOOT_JAR);
    let bridge_jar = config.cache_dir.join(BRIDGE_JAR);
    let mut class_path = vec![boot_jar.clone()];
    class_path.extend(manifest.class_path.iter().cloned());
    class_path.push(manifest.game_jar.clone());

    let mut mods = vec![bridge_jar];
    mods.extend(config.mods.iter().cloned());
    let natives = utf8(&manifest.natives_dir)?;
    let game_dir = &config.game_dir;
    let mut properties = vec![
        format!("-Dfabric.gameJarPath={}", utf8(&manifest.game_jar)?),
        "-Dfabric.side=client".into(),
        format!("-Dfabric.addMods={}", join(&mods)?),
        format!("-Dfabric.systemLibraries={}", utf8(&boot_jar)?),
        "-Dfabric.noGui=true".into(),
        format!("-Dorg.lwjgl.librarypath={natives}"),
        format!("-Djava.library.path={natives}"),
        format!(
            "-Djna.tmpdir={}",
            utf8(&game_dir.join("natives").join("jna"))?
        ),
        format!(
            "-Dio.netty.native.workdir={}",
            utf8(&game_dir.join("natives").join("netty"))?
        ),
        format!("-Djdk.net.hosts.file={}", utf8(&game_dir.join(HOSTS_FILE))?),
        format!("-Duser.dir={}", utf8(game_dir)?),
        "-Djava.awt.headless=true".into(),
        "--add-exports=java.base/jdk.internal.misc=ALL-UNNAMED".into(),
        format!("-D{TABLE_PROPERTY}={:x}", table_address as u64),
    ];
    if config.compose {
        properties.push(format!("-D{COMPOSE_PROPERTY}=true"));
    }
    if let Some(port) = config.proxy_port {
        for scheme in ["http", "https"] {
            properties.push(format!("-D{scheme}.proxyHost=127.0.0.1"));
            properties.push(format!("-D{scheme}.proxyPort={port}"));
        }
    }
    properties.extend(manifest.jvm_args.iter().cloned());
    properties.extend(config.extra_jvm.iter().cloned());

    let mut args: Vec<String> = vec!["launch".into(), manifest.main_class.clone()];
    let pairs = [
        ("--version", manifest.game_version.clone()),
        ("--versionType", "release".into()),
        ("--accessToken", "0".into()),
        ("--username", config.username.clone()),
        ("--uuid", config.uuid.clone()),
        ("--gameDir", utf8(game_dir)?.to_owned()),
        ("--assetsDir", utf8(&manifest.assets_dir)?.to_owned()),
        ("--assetIndex", manifest.asset_index.clone()),
        ("--width", config.width.to_string()),
        ("--height", config.height.to_string()),
    ];
    for (k, v) in pairs {
        args.push(k.into());
        args.push(v);
    }
    args.push("--offlineDeveloperMode".into());

    Ok(LaunchPlan {
        options_class_path: class_path,
        max_heap: config.max_heap.clone(),
        properties,
        args,
    })
}

/// Seeds `options.txt` (skips onboarding) and a localhost-only hosts file: Rust owns DNS and sockets.
pub fn prepare_game_dir(game_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(game_dir.join("mods"))?;
    std::fs::write(game_dir.join(HOSTS_FILE), HOSTS)?;
    let options = game_dir.join("options.txt");
    if !options.exists() {
        std::fs::write(options, "onboardAccessibility:false\n")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
        "format": 1,
        "game_version": "26.3",
        "loader_version": "0.19.5",
        "main_class": "net.fabricmc.loader.impl.launch.knot.KnotClient",
        "jvm_args": ["-DFabricMcEmu= net.minecraft.client.main.Main "],
        "class_path": ["fabric/libraries/a.jar", "fabric/libraries/b.jar"],
        "game_jar": "versions/26.3/client.jar",
        "natives_dir": "fabric/natives/linux-x86_64",
        "assets_dir": "assets",
        "asset_index": "34",
        "test_mods": ["mods-test/fabric-api.jar"]
    }"#;

    #[test]
    fn manifest_paths_join_the_cache_and_stay_inside() {
        let m = LaunchManifest::parse(MANIFEST, Path::new("/c")).unwrap();
        assert_eq!(m.game_jar, PathBuf::from("/c/versions/26.3/client.jar"));
        assert_eq!(m.test_mods, [PathBuf::from("/c/mods-test/fabric-api.jar")]);
        let escape = MANIFEST.replace("\"assets\"", "\"../etc\"");
        assert!(LaunchManifest::parse(&escape, Path::new("/c")).is_err());
        let old = MANIFEST.replace("\"format\": 1", "\"format\": 0");
        assert!(LaunchManifest::parse(&old, Path::new("/c")).is_err());
    }

    #[test]
    fn command_line() {
        let m = LaunchManifest::parse(MANIFEST, Path::new("/c")).unwrap();
        let mut cfg =
            BridgeConfig::new("/c".into(), "/g/java".into(), vec!["/m/modmenu.jar".into()]);
        cfg.username = "Bot".into();
        cfg.uuid = "u-u-i-d".into();
        cfg.max_heap = "1536m".into();
        let p = plan(&m, &cfg, 0x1234_abcd).unwrap();
        let sep = if cfg!(windows) { ";" } else { ":" };
        assert_eq!(
            p.options_class_path,
            [
                "/c/java/mcr-boot.jar",
                "/c/fabric/libraries/a.jar",
                "/c/fabric/libraries/b.jar",
                "/c/versions/26.3/client.jar"
            ]
            .map(PathBuf::from)
        );
        assert_eq!(
            p.properties,
            [
                "-Dfabric.gameJarPath=/c/versions/26.3/client.jar".to_owned(),
                "-Dfabric.side=client".into(),
                format!("-Dfabric.addMods=/c/java/mcr-bridge.jar{sep}/m/modmenu.jar"),
                "-Dfabric.systemLibraries=/c/java/mcr-boot.jar".into(),
                "-Dfabric.noGui=true".into(),
                "-Dorg.lwjgl.librarypath=/c/fabric/natives/linux-x86_64".into(),
                "-Djava.library.path=/c/fabric/natives/linux-x86_64".into(),
                "-Djna.tmpdir=/g/java/natives/jna".into(),
                "-Dio.netty.native.workdir=/g/java/natives/netty".into(),
                "-Djdk.net.hosts.file=/g/java/mcr-hosts".into(),
                "-Duser.dir=/g/java".into(),
                "-Djava.awt.headless=true".into(),
                "--add-exports=java.base/jdk.internal.misc=ALL-UNNAMED".into(),
                "-Dmcr.bridge.table=1234abcd".into(),
                "-DFabricMcEmu= net.minecraft.client.main.Main ".into(),
            ]
        );
        assert_eq!(
            p.args.join(" "),
            "launch net.fabricmc.loader.impl.launch.knot.KnotClient --version 26.3 \
             --versionType release --accessToken 0 --username Bot --uuid u-u-i-d \
             --gameDir /g/java --assetsDir /c/assets --assetIndex 34 --width 854 --height 480 \
             --offlineDeveloperMode"
        );
        let opts = p.jvm_options().to_strings().unwrap();
        assert!(opts.contains(&"-Xmx1536m".to_owned()));
        cfg.compose = true;
        let p = plan(&m, &cfg, 0x1234_abcd).unwrap();
        assert!(p.properties.contains(&"-Dmcr.compose=true".to_owned()));
        cfg.proxy_port = Some(4321);
        let p = plan(&m, &cfg, 0x1234_abcd).unwrap();
        for prop in [
            "-Dhttp.proxyHost=127.0.0.1",
            "-Dhttp.proxyPort=4321",
            "-Dhttps.proxyHost=127.0.0.1",
            "-Dhttps.proxyPort=4321",
        ] {
            assert!(p.properties.contains(&prop.to_owned()), "{prop}");
        }
    }

    #[test]
    fn relative_paths_become_absolute_for_java() {
        let mut cfg = BridgeConfig::new(".cache".into(), "run/java".into(), vec!["m/a.jar".into()]);
        cfg.make_absolute().unwrap();
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(cfg.cache_dir, cwd.join(".cache"));
        assert_eq!(cfg.game_dir, cwd.join("run/java"));
        assert_eq!(cfg.mods, [cwd.join("m/a.jar")]);
    }

    #[test]
    fn game_dir_seed_keeps_existing_options() {
        let dir = std::env::temp_dir().join(format!("mcr-modbridge-gd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        prepare_game_dir(&dir).unwrap();
        let options = dir.join("options.txt");
        assert_eq!(
            std::fs::read_to_string(&options).unwrap(),
            "onboardAccessibility:false\n"
        );
        std::fs::write(&options, "fov:1.0\n").unwrap();
        prepare_game_dir(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(&options).unwrap(), "fov:1.0\n");
        assert!(dir.join("mods").is_dir());
        assert!(
            std::fs::read_to_string(dir.join(HOSTS_FILE))
                .unwrap()
                .contains("localhost")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
