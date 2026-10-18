// HotSpot allows one VM per process, so each session runs as a child of this test.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use mcr_modbridge::preload::{self, PreloadPlan};
use mcr_modbridge::{
    AotSettings, Bridge, BridgeConfig, BridgeEvent, LaunchManifest, ShutdownOutcome,
};

const TEST_NAME: &str = "jvm_preload";
const CHILD_ENV: &str = "MCR_PRELOAD_CHILD";
const DEADLINE: Duration = Duration::from_secs(240);

fn game_dir() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("jvm-preload")
}

fn aot_dir() -> PathBuf {
    game_dir().join("cache/jvm-aot")
}

/// Boots to the title screen and quits; returns the bridge's info events.
fn session() -> Vec<(String, String)> {
    let cache = mcr_jvm::dev_cache_dir();
    let manifest = LaunchManifest::load(&cache).expect("run `cargo xtask fabric-fetch`");
    let mut cfg = BridgeConfig::new(cache, game_dir(), manifest.test_mods.clone());
    cfg.max_heap = "1536m".into();
    cfg.aot = Some(AotSettings {
        dir: aot_dir(),
        helper: std::env::current_exe().unwrap(),
    });
    let mut bridge = Bridge::start(cfg).expect("bridge start");
    let mut info = Vec::new();
    let started = Instant::now();
    let mut title = None;
    while title.is_none_or(|t: Instant| t.elapsed() < Duration::from_secs(3)) {
        assert!(started.elapsed() < DEADLINE, "no title screen: {info:?}");
        match bridge.next_event(Duration::from_millis(200)) {
            Some(BridgeEvent::Info { key, value }) => info.push((key, value)),
            Some(e) if e.is_title_screen() => title = title.or(Some(Instant::now())),
            Some(e @ (BridgeEvent::Crash { .. } | BridgeEvent::VmExited { .. })) => panic!("{e:?}"),
            _ => {}
        }
    }
    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );
    info
}

fn list_file() -> Option<PathBuf> {
    std::fs::read_dir(aot_dir())
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "classes"))
}

#[test]
fn jvm_preload() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set MCR_JVM_TESTS=1 to boot Fabric");
        return;
    }
    if std::env::var(CHILD_ENV).is_ok() {
        for (key, value) in session() {
            println!("info {key}={value}");
        }
        std::process::exit(0);
    }
    let _ = std::fs::remove_dir_all(game_dir());
    let run = |plan: &str| {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD_ENV, "1")
            .env(mcr_jvm::aot::ENV, "off")
            .env(preload::ENV, plan)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&child.stdout).into_owned();
        assert!(
            child.status.success(),
            "{plan} child {:?}: {stdout}\n{}",
            child.status,
            String::from_utf8_lossy(&child.stderr)
        );
        stdout
    };

    let recording = run("on");
    assert!(!recording.contains("info preload"), "{recording}");
    let list = list_file().expect("a class list after the first session");
    let segments = preload::parse_list(&std::fs::read_to_string(&list).unwrap()).unwrap();
    assert!(
        segments.len() >= 3,
        "milestones up to the title: {}",
        segments.len()
    );
    assert!(segments[0].iter().any(|n| n.starts_with("net.minecraft.")));
    assert!(segments[0].len() > 1000, "{}", segments[0].len());

    let preloading = run("on");
    assert!(!preloading.contains("preload_stopped"), "{preloading}");
    let first = preloading
        .lines()
        .find_map(|l| l.strip_prefix("info preload=segment 0: loaded "))
        .unwrap_or_else(|| panic!("no preload report: {preloading}"));
    let loaded: usize = first.split(',').next().unwrap().parse().unwrap();
    assert!(loaded > 1000, "{first}");
    let key = list.file_stem().unwrap().to_str().unwrap();
    assert!(!aot_dir().join(format!("{key}.strikes")).exists());
    let classes = preload::ClassList::new(aot_dir(), key, mcr_modbridge::PRELOAD_MARKER);
    assert!(matches!(
        classes.plan(true),
        PreloadPlan::On { list: Some(_), .. }
    ));

    let off = run("off");
    assert!(!off.contains("info preload"), "{off}");
    eprintln!("jvm-preload: {first}");
}
