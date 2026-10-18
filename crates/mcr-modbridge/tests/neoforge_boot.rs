//! Opt-in: boots NeoForge with the bridge to Java's title screen, headless and without a server.
//! `MCR_JVM_TESTS=1 cargo test -p mcr-modbridge --test neoforge_boot -- --nocapture`
//! `MCR_NEOFORGE_MODS=<dir>` adds every jar in that folder.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mcr_modbridge::{Bridge, BridgeConfig, BridgeEvent, Loader, ShutdownOutcome};

const BOOT_DEADLINE: Duration = Duration::from_secs(300);
const MAX_HEAP: &str = "1536m";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn check_jars(cache: &Path) {
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
}

fn extra_mods() -> Vec<PathBuf> {
    let Ok(dir) = std::env::var("MCR_NEOFORGE_MODS") else {
        return Vec::new();
    };
    let mut jars: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("MCR_NEOFORGE_MODS={dir}: {e}"))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jar"))
        .collect();
    jars.sort();
    jars
}

fn socket_inodes() -> Vec<String> {
    std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .flatten()
        .filter_map(|e| std::fs::read_link(e.path()).ok())
        .filter_map(|t| {
            let t = t.to_string_lossy().into_owned();
            Some(t.strip_prefix("socket:[")?.strip_suffix(']')?.to_owned())
        })
        .collect()
}

/// TCP sockets of this process other than the loopback HTTP proxy's listener.
fn tcp_sockets() -> Vec<String> {
    let ours = socket_inodes();
    let mut found = Vec::new();
    for table in ["/proc/self/net/tcp", "/proc/self/net/tcp6"] {
        let Ok(text) = std::fs::read_to_string(table) else {
            continue;
        };
        for line in text.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let proxy_listener = fields.get(1).is_some_and(|a| a.starts_with("0100007F:"))
                && fields.get(3) == Some(&"0A");
            if fields.len() > 9 && !proxy_listener && ours.iter().any(|i| i == fields[9]) {
                found.push(line.trim().to_owned());
            }
        }
    }
    found
}

#[derive(Debug, Default)]
struct Seen {
    mods: BTreeMap<String, String>,
    info: BTreeMap<String, Vec<String>>,
    screens: Vec<String>,
    initialized: bool,
}

impl Seen {
    fn record(&mut self, event: &BridgeEvent) {
        match event {
            BridgeEvent::ModLoaded { id, version } => {
                self.mods.insert(id.clone(), version.clone());
            }
            BridgeEvent::Info { key, value } => {
                self.info
                    .entry(key.clone())
                    .or_default()
                    .push(value.clone());
            }
            BridgeEvent::Screen(class) => self.screens.push(class.clone()),
            BridgeEvent::Initialized(_) => self.initialized = true,
            BridgeEvent::Crash { report } => panic!("java crashed: {report:?}"),
            BridgeEvent::VmExited { code } => panic!("java exited with {code}"),
            BridgeEvent::VmFinished { .. } => panic!("java finished before the title screen"),
            _ => {}
        }
    }

    fn last(&self, key: &str) -> Option<&str> {
        self.info.get(key)?.last().map(String::as_str)
    }
}

#[test]
fn neoforge_boot() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set MCR_JVM_TESTS=1 to boot NeoForge");
        return;
    }
    let cache = mcr_jvm::dev_cache_dir();
    check_jars(&cache);
    let game_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("neoforge-boot");
    let _ = std::fs::remove_dir_all(&game_dir);
    let mods = extra_mods();
    let mut cfg = BridgeConfig::new(cache, game_dir.clone(), mods.clone());
    cfg.loader = Loader::NeoForge;
    cfg.username = "Bridge".into();
    cfg.uuid = "c1c4e2a8-6b4d-3c4f-9a43-1d6f3c0b2a11".into();
    cfg.max_heap = MAX_HEAP.into();
    // Java's services discovery would leave a proxied upstream socket open at the title screen.
    cfg.http_hosts.clear();

    let started = Instant::now();
    let mut bridge = Bridge::start(cfg).expect("bridge start (run `cargo xtask neoforge-fetch`)");
    let mut seen = Seen::default();
    loop {
        assert!(
            started.elapsed() < BOOT_DEADLINE,
            "no title screen after {BOOT_DEADLINE:?}: {seen:#?}"
        );
        let Some(event) = bridge.next_event(Duration::from_millis(200)) else {
            continue;
        };
        seen.record(&event);
        if event.is_title_screen() {
            break;
        }
    }
    let title_after = started.elapsed();
    while let Some(event) = bridge.next_event(Duration::from_millis(500)) {
        seen.record(&event);
    }

    assert!(seen.initialized, "bridge init did not finish: {seen:#?}");
    assert_eq!(seen.last("loader"), Some("neoforge"));
    for id in ["minecraft", "neoforge", "mcr_bridge"] {
        assert!(seen.mods.contains_key(id), "{id} missing: {:?}", seen.mods);
    }
    assert_eq!(seen.last("sdl_driver"), Some("offscreen"));
    assert!(
        !seen.info.contains_key("openal_open_device"),
        "OpenAL device opened"
    );
    assert!(!seen.info.contains_key("message_box"), "{:?}", seen.info);
    let bridge_loader = seen.last("bridge_loader").unwrap_or_default();
    assert!(
        bridge_loader.contains("TransformingClassLoader"),
        "{bridge_loader}"
    );
    let boot_loader = seen.last("boot_loader").unwrap_or_default();
    assert!(boot_loader.contains("AppClassLoader"), "{boot_loader}");
    let tcp = tcp_sockets();
    assert!(
        tcp.is_empty(),
        "TCP sockets open at the title screen: {tcp:?}"
    );
    let fml = std::fs::read_to_string(game_dir.join(mcr_modbridge::FML_CONFIG)).unwrap();
    assert!(fml.contains("earlyWindowControl = false"), "{fml}");

    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    let ShutdownOutcome::Clean { elapsed } = outcome else {
        panic!("shutdown: {outcome:?}");
    };
    eprintln!(
        "neoforge-boot: title {:.2}s, shutdown {} ms, heap used/committed/max {} MiB, {} mods {:?}, screens {:?}",
        title_after.as_secs_f64(),
        elapsed.as_millis(),
        seen.last("heap_mib").unwrap_or("?"),
        seen.mods.len(),
        seen.mods.keys().collect::<Vec<_>>(),
        seen.screens
    );
    for jar in &mods {
        eprintln!("extra mod {}", jar.display());
    }
}
