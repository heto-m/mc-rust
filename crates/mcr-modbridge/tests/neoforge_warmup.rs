//! Opt-in: the AOT cache and class preloading under NeoForge. HotSpot allows one VM per process, so each
//! session runs as a child: the first records the cache and the class list, the test builds the cache as
//! mcr's helper would, and the second starts from the cache and preloads.
//! `MCR_JVM_TESTS=1 cargo test -p mcr-modbridge --test neoforge_warmup -- --nocapture`

mod gpu_common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gpu_common::{DEADLINE, jvm_tests, neoforge_config_in};
use mcr_modbridge::aot::{AotCache, AotPlan, CreateOutcome};
use mcr_modbridge::preload;
use mcr_modbridge::{AotSettings, Bridge, BridgeEvent, ShutdownOutcome};

const TEST_NAME: &str = "neoforge_warmup";
const CHILD_ENV: &str = "MCR_NEOFORGE_WARMUP_CHILD";

fn game_dir() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("neoforge-warmup")
}

fn aot_dir() -> PathBuf {
    game_dir().join("cache/jvm-aot")
}

/// Boots to the title screen and quits, printing the plans and the bridge's info events.
fn session() {
    let mut cfg = neoforge_config_in(&game_dir(), &[]);
    cfg.http_hosts.clear();
    cfg.aot = Some(AotSettings {
        dir: aot_dir(),
        // The test builds the cache itself; the helper mcr would start does nothing here.
        helper: PathBuf::from("/bin/true"),
    });
    let mut bridge = Bridge::start(cfg).expect("bridge start");
    println!("aot {:?}", bridge.aot_plan());
    let started = Instant::now();
    let mut title = None;
    while title.is_none_or(|t: Instant| t.elapsed() < Duration::from_secs(3)) {
        assert!(started.elapsed() < DEADLINE, "no title screen");
        match bridge.next_event(Duration::from_millis(200)) {
            Some(BridgeEvent::Info { key, value }) => println!("info {key}={value}"),
            Some(e) if e.is_title_screen() => title = title.or(Some(Instant::now())),
            Some(e @ (BridgeEvent::Crash { .. } | BridgeEvent::VmExited { .. })) => panic!("{e:?}"),
            _ => {}
        }
    }
    println!("title {:.2}", (title.unwrap() - started).as_secs_f64());
    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );
}

fn file_with(ext: &str) -> Option<PathBuf> {
    std::fs::read_dir(aot_dir())
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == ext))
}

fn run_session() -> String {
    let child = Command::new(std::env::current_exe().unwrap())
        .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .env_remove(mcr_modbridge::aot::ENV)
        .env_remove(preload::ENV)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&child.stdout).into_owned();
    assert!(
        child.status.success(),
        "session {:?}: {stdout}\n{}",
        child.status,
        String::from_utf8_lossy(&child.stderr)
    );
    stdout
}

#[test]
fn neoforge_warmup() {
    if !jvm_tests() {
        return;
    }
    if std::env::var(CHILD_ENV).is_ok() {
        session();
        std::process::exit(0);
    }
    let _ = std::fs::remove_dir_all(game_dir());

    let recording = run_session();
    assert!(recording.contains("aot Some(Record("), "{recording}");
    assert!(!recording.contains("info preload="), "{recording}");
    let list = file_with("classes").expect("a class list after the first session");
    let segments = preload::parse_list(&std::fs::read_to_string(&list).unwrap()).unwrap();
    assert!(
        segments.len() >= 3,
        "milestones up to the title: {}",
        segments.len()
    );
    assert!(segments[0].len() > 1000, "{}", segments[0].len());
    let key = list.file_stem().unwrap().to_str().unwrap().to_owned();
    let cache = AotCache::open(aot_dir(), &key).unwrap();
    assert!(cache.needs_create(), "a recording to build");
    let built = cache.create().unwrap();
    assert!(matches!(built, CreateOutcome::Created { .. }), "{built:?}");

    let warm = run_session();
    let used = format!("aot Some(Use({:?}))", cache.cache_path());
    assert!(warm.contains(&used), "{warm}");
    assert!(cache.cache_path().is_file(), "the JVM refused the cache");
    assert_eq!(cache.plan(), AotPlan::Use(cache.cache_path()));
    assert!(!warm.contains("preload_stopped"), "{warm}");
    let first = warm
        .lines()
        .find_map(|l| l.strip_prefix("info preload=segment 0: loaded "))
        .unwrap_or_else(|| panic!("no preload report: {warm}"));
    let loaded: usize = first.split(',').next().unwrap().parse().unwrap();
    assert!(loaded > 1000, "{first}");
    assert!(!aot_dir().join(format!("{key}.strikes")).exists());
    let title = |out: &str| {
        out.lines()
            .find_map(|l| l.strip_prefix("title "))
            .unwrap_or("?")
            .to_owned()
    };
    eprintln!(
        "neoforge-warmup: title {} s recording, {} s warm; {first}",
        title(&recording),
        title(&warm)
    );
}
