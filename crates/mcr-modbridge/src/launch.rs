use std::path::{Path, PathBuf};

use mcr_jvm::JvmOptions;
use serde_json::Value;

use crate::BridgeError;
use crate::abi::{COMPOSE_PROPERTY, TABLE_PROPERTY};
use crate::loader::Loader;

pub const LAUNCH_MANIFEST: &str = "fabric/launch.json";
pub const BOOT_JAR: &str = "java/mcr-boot.jar";
pub const BRIDGE_JAR: &str = "java/mcr-bridge.jar";
const LAUNCH_FORMAT: u64 = 1;
pub const HOSTS_FILE: &str = "mcr-hosts";
const HOSTS: &str = "127.0.0.1 localhost\n::1 localhost\n";

/// What `cargo xtask fabric-fetch` or `neoforge-fetch` resolved: paths are absolute (joined onto the cache dir).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchManifest {
    pub loader: Loader,
    pub game_version: String,
    pub loader_version: String,
    pub main_class: String,
    pub jvm_args: Vec<String>,
    /// Game arguments the loader needs ahead of vanilla's (NeoForge's `--fml.*`).
    pub game_args: Vec<String>,
    pub class_path: Vec<PathBuf>,
    /// NeoForge's `libraryDirectory`, where FML finds the patched game and NeoForge's own jar.
    pub library_dir: Option<PathBuf>,
    pub game_jar: PathBuf,
    pub natives_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub asset_index: String,
    pub test_mods: Vec<PathBuf>,
}

impl LaunchManifest {
    /// Fabric's manifest; see [`Self::load_for`].
    pub fn load(cache_dir: &Path) -> Result<Self, BridgeError> {
        Self::load_for(cache_dir, Loader::Fabric)
    }

    pub fn load_for(cache_dir: &Path, loader: Loader) -> Result<Self, BridgeError> {
        let path = cache_dir.join(loader.manifest());
        let text = std::fs::read_to_string(&path).map_err(|e| BridgeError::Missing {
            path: path.clone(),
            hint: format!("{e}; run `{}`", loader.fetch_command()),
        })?;
        let manifest = Self::parse(&text, cache_dir)?;
        if manifest.loader != loader {
            return Err(BridgeError::Manifest(format!(
                "{} is for {}, not {loader}",
                path.display(),
                manifest.loader
            )));
        }
        Ok(manifest)
    }

    pub fn parse(text: &str, cache_dir: &Path) -> Result<Self, BridgeError> {
        let bad = |what: &str| BridgeError::Manifest(what.to_owned());
        let root: Value = serde_json::from_str(text).map_err(|e| bad(&e.to_string()))?;
        let loader = match root["loader"].as_str() {
            None => Loader::Fabric,
            Some(id) if id == Loader::Fabric.id() => Loader::Fabric,
            Some(id) if id == Loader::NeoForge.id() => Loader::NeoForge,
            Some(other) => return Err(bad(&format!("unknown loader {other}"))),
        };
        if root["format"].as_u64() != Some(LAUNCH_FORMAT) {
            return Err(bad(&format!(
                "unsupported format; rerun `{}`",
                loader.fetch_command()
            )));
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
        let library_dir = match root["library_dir"].as_str() {
            Some(dir) => Some(path(dir.to_owned())?),
            None if loader == Loader::NeoForge => return Err(bad("missing library_dir")),
            None => None,
        };
        Ok(Self {
            loader,
            game_version: string("game_version")?,
            loader_version: string("loader_version")?,
            main_class: string("main_class")?,
            jvm_args: strings("jvm_args")?,
            game_args: if root.get("game_args").is_some() {
                strings("game_args")?
            } else {
                Vec::new()
            },
            class_path: paths("class_path")?,
            library_dir,
            game_jar: path(string("game_jar")?)?,
            natives_dir: path(string("natives_dir")?)?,
            assets_dir: path(string("assets_dir")?)?,
            asset_index: string("asset_index")?,
            test_mods: paths("test_mods")?,
        })
    }
}

pub const SAVES_PROPERTY: &str = "mcr.bridge.saves";
pub const BACKUPS_PROPERTY: &str = "mcr.bridge.backups";
pub const SERVICES_KEYS_PROPERTY: &str = "mcr.bridge.servicesKeys";

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub loader: Loader,
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
    /// The JVM's AOT cache; `None` runs without one.
    pub aot: Option<AotSettings>,
    /// mcr's saves folder, which Java's world screens list; its backups go to `backups` beside it.
    pub saves_dir: Option<PathBuf>,
    /// An online account plays: Java fetches Mojang's key set to verify other players' chat.
    pub services_keys: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotSettings {
    /// Caches and recordings, one set of files per key.
    pub dir: PathBuf,
    /// Runs `<helper> --jvm-aot-create <dir> <key>` (see [`mcr_jvm::aot::CREATE_ARG`]) after a recording.
    pub helper: PathBuf,
}

impl BridgeConfig {
    /// Java resolves relative paths against `user.dir` (the game dir), so every path it gets must be absolute.
    pub fn make_absolute(&mut self) -> std::io::Result<()> {
        self.cache_dir = std::path::absolute(&self.cache_dir)?;
        self.game_dir = std::path::absolute(&self.game_dir)?;
        if let Some(aot) = &mut self.aot {
            aot.dir = std::path::absolute(&aot.dir)?;
        }
        if let Some(saves) = &mut self.saves_dir {
            *saves = std::path::absolute(&*saves)?;
        }
        for jar in &mut self.mods {
            *jar = std::path::absolute(&*jar)?;
        }
        Ok(())
    }

    pub fn new(cache_dir: PathBuf, game_dir: PathBuf, mods: Vec<PathBuf>) -> Self {
        Self {
            loader: Loader::Fabric,
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
            aot: None,
            saves_dir: None,
            services_keys: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub options_class_path: Vec<PathBuf>,
    /// Jars Fabric loads besides the class path: the bridge and the mods.
    pub mod_jars: Vec<PathBuf>,
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

pub const MOD_FOLDERS_PROPERTY: &str = "fml.modFolders";
/// `McrBoot` sets SDL's video driver hint before FML runs, so FML's fatal error message box cannot open.
pub const SDL_OFFSCREEN_PROPERTY: &str = "mcr.boot.sdlOffscreen";

/// `${…}` placeholders of a launcher profile's JVM arguments.
fn substitute(arg: &str, manifest: &LaunchManifest) -> Result<String, BridgeError> {
    let mut out = arg.to_owned();
    if out.contains("${library_directory}") {
        let dir = manifest.library_dir.as_deref().ok_or_else(|| {
            BridgeError::Manifest("no library_dir for ${library_directory}".into())
        })?;
        out = out.replace("${library_directory}", utf8(dir)?);
    }
    let sep = if cfg!(windows) { ";" } else { ":" };
    out = out
        .replace("${classpath_separator}", sep)
        .replace("${version_name}", &manifest.game_version);
    if out.contains("${") {
        return Err(BridgeError::Manifest(format!(
            "unknown placeholder in JVM argument {arg}"
        )));
    }
    Ok(out)
}

/// `name%%path` per jar: FML merges entries that share a name into one mod file.
fn mod_folders(jars: &[PathBuf]) -> Result<String, BridgeError> {
    let entries = jars
        .iter()
        .enumerate()
        .map(|(i, jar)| {
            let path = utf8(jar)?;
            if path.contains("%%") {
                return Err(BridgeError::Manifest(format!(
                    "mod path {path} contains %%"
                )));
            }
            Ok(PathBuf::from(format!("mcr{i}%%{path}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    join(&entries)
}

/// The class path, `-D` properties and `McrBoot` arguments for one boot under `config.loader`.
pub fn plan(
    manifest: &LaunchManifest,
    config: &BridgeConfig,
    table_address: i64,
) -> Result<LaunchPlan, BridgeError> {
    if manifest.loader != config.loader {
        return Err(BridgeError::Manifest(format!(
            "the launch manifest is for {}, not {}",
            manifest.loader, config.loader
        )));
    }
    let boot_jar = config.cache_dir.join(BOOT_JAR);
    let bridge_jar = config.cache_dir.join(BRIDGE_JAR);
    // Knot loads the game from fabric.gameJarPath; on the class path its signed manifest costs AOT recording 9 s.
    let mut class_path = vec![boot_jar.clone()];
    class_path.extend(manifest.class_path.iter().cloned());

    let mut mods = vec![bridge_jar];
    mods.extend(config.mods.iter().cloned());
    let mod_jars = mods.clone();
    let natives = utf8(&manifest.natives_dir)?;
    let game_dir = &config.game_dir;
    let mut properties = match config.loader {
        Loader::Fabric => vec![
            format!("-Dfabric.gameJarPath={}", utf8(&manifest.game_jar)?),
            "-Dfabric.side=client".into(),
            format!("-Dfabric.addMods={}", join(&mods)?),
            format!("-Dfabric.systemLibraries={}", utf8(&boot_jar)?),
            "-Dfabric.noGui=true".into(),
        ],
        // --enable-native-access names boot-layer modules only, and FML keeps its game layer's controller.
        // Rust owns the sockets; netty's kqueue probe trips NeoForge's debug log appender.
        Loader::NeoForge => vec![
            format!("-D{MOD_FOLDERS_PROPERTY}={}", mod_folders(&mods)?),
            "--illegal-native-access=allow".into(),
            "-Dio.netty.transport.noNative=true".into(),
            format!("-D{SDL_OFFSCREEN_PROPERTY}=true"),
        ],
    };
    properties.extend([
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
        hosts_file_property(&game_dir.join(HOSTS_FILE))?,
        format!("-Duser.dir={}", utf8(game_dir)?),
        "-Djava.awt.headless=true".into(),
        "--add-exports=java.base/jdk.internal.misc=ALL-UNNAMED".into(),
        format!("-D{TABLE_PROPERTY}={:x}", table_address as u64),
    ]);
    if config.compose {
        properties.push(format!("-D{COMPOSE_PROPERTY}=true"));
    }
    if crate::profile::per_mod() {
        properties.push(format!("-D{}=true", crate::profile::PROFILE_PROPERTY));
    }
    if let Some(port) = config.proxy_port {
        properties.extend(proxy_properties(port));
    }
    if let Some(saves) = &config.saves_dir {
        properties.push(format!("-D{SAVES_PROPERTY}={}", utf8(saves)?));
        let backups = saves.parent().unwrap_or(saves).join("backups");
        properties.push(format!("-D{BACKUPS_PROPERTY}={}", utf8(&backups)?));
    }
    if config.services_keys {
        properties.push(format!("-D{SERVICES_KEYS_PROPERTY}=true"));
    }
    let mut jvm_args = manifest.jvm_args.iter();
    while let Some(arg) = jvm_args.next() {
        let mut arg = substitute(arg, manifest)?;
        // JNI_CreateJavaVM takes one string per option: `--add-opens x` must become `--add-opens=x`.
        if arg.starts_with("--") && !arg.contains('=') {
            let value = jvm_args
                .next()
                .ok_or_else(|| BridgeError::Manifest(format!("{arg} without a value")))?;
            arg = format!("{arg}={}", substitute(value, manifest)?);
        }
        properties.push(arg);
    }
    properties.extend(config.extra_jvm.iter().cloned());

    let mut args: Vec<String> = vec!["launch".into(), manifest.main_class.clone()];
    args.extend(manifest.game_args.iter().cloned());
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
        mod_jars,
        max_heap: config.max_heap.clone(),
        properties,
        args,
    })
}

/// Java resolves only localhost from this file, so mods reach no named host by themselves.
fn hosts_file_property(hosts_file: &Path) -> Result<String, BridgeError> {
    Ok(format!("-Djdk.net.hosts.file={}", utf8(hosts_file)?))
}

/// Java's HTTP stack goes through mcr's allow-list proxy on `port`.
fn proxy_properties(port: u16) -> Vec<String> {
    ["http", "https"]
        .into_iter()
        .flat_map(|scheme| {
            [
                format!("-D{scheme}.proxyHost=127.0.0.1"),
                format!("-D{scheme}.proxyPort={port}"),
            ]
        })
        .collect()
}

/// The client JVM's network fence for another JVM running the same mods (the singleplayer Fabric server):
/// the localhost-only hosts file written into `dir` and the bridge's allow-list proxy on `proxy_port`.
/// Like the client's, it covers name resolution and Java's HTTP stack, not raw sockets to literal addresses.
pub fn egress_properties(dir: &Path, proxy_port: u16) -> Result<Vec<String>, BridgeError> {
    let hosts = write_hosts_file(dir)?;
    let mut out = vec![hosts_file_property(&hosts)?];
    out.extend(proxy_properties(proxy_port));
    Ok(out)
}

fn write_hosts_file(dir: &Path) -> std::io::Result<PathBuf> {
    let path = dir.join(HOSTS_FILE);
    std::fs::write(&path, HOSTS)?;
    Ok(path)
}

/// Seeds `options.txt` (skips onboarding) and a localhost-only hosts file: Rust owns DNS and sockets.
pub fn prepare_game_dir(game_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(game_dir.join("mods"))?;
    write_hosts_file(game_dir)?;
    let options = game_dir.join("options.txt");
    if !options.exists() {
        std::fs::write(options, "onboardAccessibility:false\n")?;
    }
    Ok(())
}

const NEOFORGE_MANIFEST: &str = "META-INF/neoforge.mods.toml";

/// java-build leaves NeoForge's entry out when `.cache/neoforge` was missing at build time.
pub fn check_neoforge_bridge(jar: &Path) -> Result<(), BridgeError> {
    let has_manifest = std::fs::File::open(jar)
        .ok()
        .and_then(|f| zip::ZipArchive::new(std::io::BufReader::new(f)).ok())
        .is_some_and(|mut z| z.by_name(NEOFORGE_MANIFEST).is_ok());
    if has_manifest {
        return Ok(());
    }
    Err(BridgeError::Missing {
        path: jar.join(NEOFORGE_MANIFEST),
        hint: "the bridge was built without NeoForge; run `cargo xtask neoforge-fetch` and \
               `cargo xtask java-build`"
            .into(),
    })
}

pub const FML_CONFIG: &str = "config/fml.toml";
/// FML's own window would open beside mcr's, and its version check would phone out.
const FML_FORCED: [(&str, &str); 2] = [("earlyWindowControl", "false"), ("versionCheck", "false")];

/// Forces the keys in [`FML_FORCED`] in `config/fml.toml`, keeping everything else the file holds.
pub fn prepare_fml_config(game_dir: &Path) -> std::io::Result<()> {
    let path = game_dir.join(FML_CONFIG);
    let old = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let key_of = |line: &str| {
        let line = line.trim_start();
        let key = line.split(['=', ' ', '\t']).next().unwrap_or_default();
        FML_FORCED
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(k, v)| (*k, *v))
    };
    // Missing keys go first: a later `[table]` header would otherwise swallow them.
    let mut head: Vec<String> = FML_FORCED
        .iter()
        .filter(|(k, _)| !old.lines().any(|l| key_of(l).is_some_and(|(f, _)| f == *k)))
        .map(|(k, v)| format!("{k} = {v}"))
        .collect();
    head.extend(old.lines().map(|line| match key_of(line) {
        Some((k, v)) => format!("{k} = {v}"),
        None => line.to_owned(),
    }));
    let mut text = head.join("\n");
    text.push('\n');
    if text != old {
        std::fs::create_dir_all(path.parent().unwrap_or(game_dir))?;
        std::fs::write(&path, text)?;
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
            ]
            .map(PathBuf::from)
        );
        assert_eq!(
            p.mod_jars,
            ["/c/java/mcr-bridge.jar", "/m/modmenu.jar"].map(PathBuf::from)
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
        assert!(
            !p.properties
                .contains(&"-Dmcr.bridge.servicesKeys=true".to_owned())
        );
        cfg.services_keys = true;
        let p = plan(&m, &cfg, 0x1234_abcd).unwrap();
        assert!(
            p.properties
                .contains(&"-Dmcr.bridge.servicesKeys=true".to_owned())
        );
        cfg.saves_dir = Some("/g/saves".into());
        let p = plan(&m, &cfg, 0x1234_abcd).unwrap();
        assert!(
            p.properties
                .contains(&"-Dmcr.bridge.saves=/g/saves".to_owned())
        );
        assert!(
            p.properties
                .contains(&"-Dmcr.bridge.backups=/g/backups".to_owned())
        );
    }

    const NEO_MANIFEST: &str = r#"{
        "format": 1,
        "loader": "neoforge",
        "game_version": "26.3",
        "loader_version": "26.3.0.90",
        "main_class": "net.neoforged.fml.startup.Client",
        "jvm_args": ["-DlibraryDirectory=${library_directory}", "--add-opens", "java.base/java.lang.invoke=ALL-UNNAMED"],
        "game_args": ["--fml.neoForgeVersion", "26.3.0.90", "--fml.mcVersion", "26.3"],
        "class_path": ["neoforge/libraries/loader.jar"],
        "library_dir": "neoforge/libraries",
        "game_jar": "neoforge/libraries/patched.jar",
        "natives_dir": "neoforge/natives/linux-x86_64",
        "assets_dir": "assets",
        "asset_index": "34",
        "test_mods": []
    }"#;

    #[test]
    fn neoforge_manifest_and_plan() {
        let m = LaunchManifest::parse(NEO_MANIFEST, Path::new("/c")).unwrap();
        assert_eq!(m.loader, Loader::NeoForge);
        assert_eq!(m.library_dir, Some(PathBuf::from("/c/neoforge/libraries")));
        let no_libs = NEO_MANIFEST.replace(r#""library_dir": "neoforge/libraries","#, "");
        assert!(LaunchManifest::parse(&no_libs, Path::new("/c")).is_err());
        assert_eq!(
            LaunchManifest::parse(MANIFEST, Path::new("/c"))
                .unwrap()
                .loader,
            Loader::Fabric
        );

        let mut cfg = BridgeConfig::new("/c".into(), "/g/java".into(), vec!["/m/jei.jar".into()]);
        assert!(
            plan(&m, &cfg, 1).is_err(),
            "a Fabric config with a NeoForge manifest"
        );
        cfg.loader = Loader::NeoForge;
        let p = plan(&m, &cfg, 0x1234_abcd).unwrap();
        let sep = if cfg!(windows) { ";" } else { ":" };
        assert_eq!(
            p.options_class_path,
            ["/c/java/mcr-boot.jar", "/c/neoforge/libraries/loader.jar"].map(PathBuf::from)
        );
        assert_eq!(
            p.properties[..3],
            [
                format!("-Dfml.modFolders=mcr0%%/c/java/mcr-bridge.jar{sep}mcr1%%/m/jei.jar"),
                "--illegal-native-access=allow".into(),
                "-Dio.netty.transport.noNative=true".into(),
            ]
        );
        assert!(!p.properties.iter().any(|o| o.contains("fabric")));
        assert!(
            p.properties
                .contains(&"-DlibraryDirectory=/c/neoforge/libraries".to_owned())
        );
        assert!(
            p.properties
                .contains(&"-Dmcr.bridge.table=1234abcd".to_owned())
        );
        assert!(
            p.properties
                .contains(&"--add-opens=java.base/java.lang.invoke=ALL-UNNAMED".to_owned())
        );
        assert!(
            p.args.join(" ").starts_with(
                "launch net.neoforged.fml.startup.Client --fml.neoForgeVersion 26.3.0.90 \
                 --fml.mcVersion 26.3 --version 26.3 "
            ),
            "{:?}",
            p.args
        );
        let odd = NEO_MANIFEST.replace("${library_directory}", "${natives_directory}");
        let m = LaunchManifest::parse(&odd, Path::new("/c")).unwrap();
        assert!(plan(&m, &cfg, 1).is_err(), "unknown placeholder");
    }

    #[test]
    fn fml_config_forces_window_and_version_check_off() {
        let dir = std::env::temp_dir().join(format!("mcr-modbridge-fml-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        prepare_fml_config(&dir).unwrap();
        let path = dir.join(FML_CONFIG);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "earlyWindowControl = false\nversionCheck = false\n"
        );
        std::fs::write(
            &path,
            "#comment\nearlyWindowControl = true\nmaxThreads = 4\n[dependencyOverrides]\n",
        )
        .unwrap();
        prepare_fml_config(&dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "versionCheck = false\n#comment\nearlyWindowControl = false\nmaxThreads = 4\n\
             [dependencyOverrides]\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
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

    #[test]
    fn a_server_jvm_gets_the_clients_fence() {
        let dir = std::env::temp_dir().join(format!("mcr-modbridge-egress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let props = egress_properties(&dir, 4321).unwrap();
        let hosts = dir.join(HOSTS_FILE);
        assert_eq!(std::fs::read_to_string(&hosts).unwrap(), HOSTS);
        let mut client = BridgeConfig::new("/c".into(), dir.clone(), Vec::new());
        client.proxy_port = Some(4321);
        let m = LaunchManifest::parse(MANIFEST, Path::new("/c")).unwrap();
        let plan = plan(&m, &client, 0).unwrap();
        for prop in &props {
            assert!(
                plan.properties.contains(prop),
                "{prop} differs from the client's"
            );
        }
        assert_eq!(props.len(), 5);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
