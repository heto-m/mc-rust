//! Shared harness for the GPU backend tests: one JVM per process, so each test lives in its own binary.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mcr_modbridge::{Bridge, BridgeConfig, BridgeEvent, LaunchManifest, Loader};

pub const DEADLINE: Duration = Duration::from_secs(300);

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn config(tag: &str, extra: &[String]) -> BridgeConfig {
    let cache = mcr_jvm::dev_cache_dir();
    for module in ["mcr-boot", "mcr-bridge", "mcr-testmod"] {
        let jar = cache.join(format!("java/{module}.jar"));
        if let Some(newer) = mcr_jvm::stale_java_jar(&repo().join("java").join(module), &jar) {
            panic!(
                "{} is missing or older than {}: run `cargo xtask java-build`",
                jar.display(),
                newer.display()
            );
        }
    }
    let manifest = LaunchManifest::load(&cache).expect("run `cargo xtask fabric-fetch`");
    let game_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("gpu-backend-{tag}"));
    let _ = std::fs::remove_dir_all(&game_dir);
    let mut mods = manifest.test_mods.clone();
    mods.push(cache.join("java/mcr-testmod.jar"));
    let mut cfg = BridgeConfig::new(cache, game_dir, mods);
    cfg.max_heap = "1536m".into();
    cfg.extra_jvm = extra.to_vec();
    cfg
}

/// NeoForge with the bridge alone in a fresh game dir; `MCR_NEOFORGE_MODS=<dir>` adds every jar in that folder.
pub fn neoforge_config(tag: &str, extra: &[String]) -> BridgeConfig {
    let game_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("neoforge-{tag}"));
    let _ = std::fs::remove_dir_all(&game_dir);
    neoforge_config_in(&game_dir, extra)
}

/// [`neoforge_config`] keeping what `game_dir` holds.
pub fn neoforge_config_in(game_dir: &Path, extra: &[String]) -> BridgeConfig {
    let cache = mcr_jvm::dev_cache_dir();
    for module in ["mcr-boot", "mcr-bridge"] {
        let jar = cache.join(format!("java/{module}.jar"));
        if let Some(newer) = mcr_jvm::stale_java_jar(&repo().join("java").join(module), &jar) {
            panic!(
                "{} is missing or older than {}: run `cargo xtask java-build`",
                jar.display(),
                newer.display()
            );
        }
    }
    LaunchManifest::load_for(&cache, Loader::NeoForge).expect("run `cargo xtask neoforge-fetch`");
    let mut mods: Vec<PathBuf> = match std::env::var("MCR_NEOFORGE_MODS") {
        Ok(dir) => std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("MCR_NEOFORGE_MODS={dir}: {e}"))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "jar"))
            .collect(),
        Err(_) => Vec::new(),
    };
    mods.sort();
    let mut cfg = BridgeConfig::new(cache, game_dir.to_path_buf(), mods);
    cfg.loader = Loader::NeoForge;
    cfg.max_heap = "1536m".into();
    cfg.extra_jvm = extra.to_vec();
    cfg
}

pub fn info_value(bridge: &mut Bridge, key: &str) -> String {
    let start = Instant::now();
    loop {
        assert!(start.elapsed() < DEADLINE, "no {key} report");
        match bridge.next_event(Duration::from_millis(200)) {
            Some(BridgeEvent::Info { key: k, value }) if k == key => return value,
            Some(BridgeEvent::Crash { report }) => panic!("java crashed: {report:?}"),
            Some(BridgeEvent::VmExited { code }) => panic!("java exited with {code}"),
            _ => {}
        }
    }
}

pub fn jvm_tests() -> bool {
    let on = std::env::var("MCR_JVM_TESTS").as_deref() == Ok("1");
    if !on {
        eprintln!("skipped: set MCR_JVM_TESTS=1 to boot the client");
    }
    on
}
pub mod compose;
pub mod image;
pub mod input;
