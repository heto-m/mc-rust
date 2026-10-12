// The proposal's acceptance test. The folder holds Fabric API, Mod Menu (the real jars from
// `.cache/mods-test`, copied to a temp dir), Sodium, a NeoForge jar and a mod with a missing dependency
// (built here). HotSpot allows one VM per process, so each boot runs in a child of this test binary.

#[path = "../src/manager/fixture.rs"]
mod fixture;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use fixture::Jar;
use mcr_modbridge::manager::{Manager, ModStatus};
use mcr_modbridge::{Bridge, BridgeConfig, BridgeEvent, LaunchManifest, ShutdownOutcome};
use mcr_settings::ModSettings;

const TEST_NAME: &str = "acceptance";
const CHILD_ENV: &str = "MCR_MODMANAGER_CHILD";
const DIR_ENV: &str = "MCR_MODMANAGER_DIR";
const BOOT_DEADLINE: Duration = Duration::from_secs(240);

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn real_jars() -> Option<Vec<PathBuf>> {
    let dir = mcr_jvm::dev_cache_dir().join("mods-test");
    let jars: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("fabric-api") || n.starts_with("modmenu"))
        })
        .collect();
    (jars.len() == 2).then_some(jars)
}

fn build_folder(tag: &str) -> Option<PathBuf> {
    let real = real_jars()?;
    let data = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("mod-manager-{tag}"));
    let _ = std::fs::remove_dir_all(&data);
    let mods = data.join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    for jar in real {
        std::fs::copy(&jar, mods.join(jar.file_name().unwrap())).unwrap();
    }
    Jar::fabric("sodium", "0.9.0").write(&mods, "sodium.jar");
    Jar::new()
        .file("META-INF/neoforge.mods.toml", b"modId=\"neo\"\n")
        .write(&mods, "neo.jar");
    Jar::fabric_with("needy", "1.0.0", r#""depends":{"foo":"*"}"#).write(&mods, "needy.jar");
    Some(data)
}

fn status(report: &mcr_modbridge::manager::Report, id: &str) -> ModStatus {
    report
        .entries
        .iter()
        .find(|e| e.info.id == id)
        .unwrap_or_else(|| panic!("{id} not listed"))
        .status
        .clone()
}

fn java_thread() -> bool {
    std::fs::read_dir("/proc/self/task")
        .unwrap()
        .flatten()
        .any(|e| {
            std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "mcr-java")
        })
}

#[test]
fn the_mods_screen_lists_all_five_and_the_loader_gets_two() {
    let Some(data) = build_folder("scan") else {
        eprintln!("skipped: no Fabric API and Mod Menu jars in .cache/mods-test");
        return;
    };
    let manager = Manager::new(&data, &data.join("mods"));
    let report = manager.scan(&ModSettings::default());
    assert_eq!(report.entries.len(), 5);
    assert_eq!(status(&report, "fabric-api"), ModStatus::Active);
    assert_eq!(status(&report, "modmenu"), ModStatus::Active);
    let ModStatus::Blocked(reason) = status(&report, "sodium") else {
        panic!("sodium is not blocked");
    };
    assert!(reason.contains("OpenGL"), "{reason}");
    assert_eq!(
        status(&report, "needy"),
        ModStatus::Excluded("missing dependency `foo`".into())
    );
    assert!(report.entries.iter().any(|e| {
        e.info.file_name == "neo.jar" && matches!(e.status, ModStatus::Unsupported(_))
    }));

    let jars = report.jars();
    assert_eq!(jars.len(), 2);
    if let Ok(manifest) = LaunchManifest::load(&mcr_jvm::dev_cache_dir()) {
        let mut cfg = BridgeConfig::new(mcr_jvm::dev_cache_dir(), data.join("java"), jars);
        cfg.max_heap = "1536m".into();
        let plan = mcr_modbridge::plan(&manifest, &cfg, 0).unwrap();
        let add_mods = plan
            .properties
            .iter()
            .find_map(|p| p.strip_prefix("-Dfabric.addMods="))
            .unwrap();
        let names: BTreeSet<String> = std::env::split_paths(add_mods)
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(names.contains("mcr-bridge.jar"));
        assert!(names.iter().any(|n| n.starts_with("fabric-api")));
        assert!(names.iter().any(|n| n.starts_with("modmenu")));
        for absent in ["sodium", "neo", "needy"] {
            assert!(
                !names.iter().any(|n| n.contains(absent)),
                "{absent} reached the loader"
            );
        }
    }

    let mut settings = ModSettings::default();
    settings.set_enabled("modmenu", false);
    let report = manager.scan(&settings);
    assert_eq!(report.active_ids(), ["fabric-api"]);

    settings.set_enabled("fabric-api", false);
    let report = manager.scan(&settings);
    assert!(report.jars().is_empty(), "nothing enabled: no JVM starts");
    assert!(!java_thread());
    let _ = std::fs::remove_dir_all(&data);
}

fn boot_child(data: &Path, expect_loaded: &[&str], expect_absent: &[&str]) -> ! {
    let manager = Manager::new(data, &data.join("mods"));
    let report = manager.scan(&manager.load_settings());
    let jars = report.jars();
    assert!(!jars.is_empty());
    let cache = mcr_jvm::dev_cache_dir();
    let mut cfg = BridgeConfig::new(cache, data.join("java"), jars);
    cfg.max_heap = "1536m".into();
    let mut bridge = Bridge::start(cfg).expect("bridge start");
    let started = Instant::now();
    let mut loaded = BTreeSet::new();
    loop {
        assert!(
            started.elapsed() < BOOT_DEADLINE,
            "no title screen: {loaded:?}"
        );
        match bridge.next_event(Duration::from_millis(200)) {
            Some(BridgeEvent::ModLoaded { id, .. }) => {
                loaded.insert(id);
            }
            Some(event) if event.is_title_screen() => break,
            Some(BridgeEvent::Crash { report }) => panic!("java crashed: {report:?}"),
            _ => {}
        }
    }
    while let Some(event) = bridge.next_event(Duration::from_millis(300)) {
        if let BridgeEvent::ModLoaded { id, .. } = event {
            loaded.insert(id);
        }
    }
    assert!(java_thread() || loaded.contains("mcr_bridge"));
    for id in expect_loaded {
        assert!(loaded.contains(*id), "{id} not loaded: {loaded:?}");
    }
    for id in expect_absent {
        assert!(!loaded.contains(*id), "{id} was loaded: {loaded:?}");
    }
    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "shutdown: {outcome:?}"
    );
    println!("boot child ok");
    std::process::exit(0);
}

fn run_child(mode: &str, data: &Path) {
    let child = Command::new(std::env::current_exe().unwrap())
        .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, mode)
        .env(DIR_ENV, data)
        .env("MCR_JVM_TESTS", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&child.stdout);
    assert!(
        child.status.success() && stdout.contains("boot child ok"),
        "{mode} child {:?}: {stdout}\n{}",
        child.status,
        String::from_utf8_lossy(&child.stderr)
    );
}

#[test]
fn acceptance() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set MCR_JVM_TESTS=1 to boot Fabric");
        return;
    }
    if let Ok(mode) = std::env::var(CHILD_ENV) {
        let data = PathBuf::from(std::env::var(DIR_ENV).unwrap());
        match mode.as_str() {
            "both" => boot_child(
                &data,
                &["fabric-api", "modmenu", "mcr_bridge"],
                &["sodium", "needy"],
            ),
            "menu-off" => boot_child(
                &data,
                &["fabric-api", "mcr_bridge"],
                &["modmenu", "sodium", "needy"],
            ),
            other => panic!("unknown child mode {other}"),
        }
    }
    let Some(data) = build_folder("boot") else {
        eprintln!("skipped: no Fabric API and Mod Menu jars in .cache/mods-test");
        return;
    };
    for jar in ["mcr-boot", "mcr-bridge"] {
        let path = mcr_jvm::dev_cache_dir().join(format!("java/{jar}.jar"));
        if let Some(newer) = mcr_jvm::stale_java_jar(&repo().join("java").join(jar), &path) {
            panic!(
                "{} is stale vs {}: run `cargo xtask java-build`",
                path.display(),
                newer.display()
            );
        }
    }
    run_child("both", &data);

    let manager = Manager::new(&data, &data.join("mods"));
    let mut settings = manager.load_settings();
    settings.set_enabled("modmenu", false);
    settings.save(&manager.settings_path()).unwrap();
    run_child("menu-off", &data);

    settings.set_enabled("fabric-api", false);
    settings.save(&manager.settings_path()).unwrap();
    assert!(manager.scan(&manager.load_settings()).jars().is_empty());
    assert!(
        !java_thread(),
        "no mcr-java thread when every mod is disabled"
    );
    let _ = std::fs::remove_dir_all(&data);
}
