// HotSpot allows one VM per process: the boot runs in this test, and the crash and System.exit
// paths re-run the test binary as children after the parent's VM is gone.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use mcr_modbridge::{Bridge, BridgeConfig, BridgeEvent, LaunchManifest, ShutdownOutcome};

const TEST_NAME: &str = "fabric_boot";
const CHILD_ENV: &str = "MCR_BRIDGE_CHILD";
const BOOT_DEADLINE: Duration = Duration::from_secs(240);
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

fn config(tag: &str, extra: &[&str]) -> (BridgeConfig, LaunchManifest) {
    let cache = mcr_jvm::dev_cache_dir();
    check_jars(&cache);
    let manifest = LaunchManifest::load(&cache).expect("run `cargo xtask fabric-fetch`");
    let game_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("fabric-boot-{tag}"));
    let _ = std::fs::remove_dir_all(&game_dir);
    let mut cfg = BridgeConfig::new(cache, game_dir, manifest.test_mods.clone());
    cfg.username = "Bridge".into();
    cfg.uuid = "c1c4e2a8-6b4d-3c4f-9a43-1d6f3c0b2a11".into();
    cfg.max_heap = MAX_HEAP.into();
    cfg.extra_jvm = extra.iter().map(|s| (*s).to_owned()).collect();
    (cfg, manifest)
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

fn render_thread() -> Option<String> {
    std::fs::read_dir("/proc/self/task")
        .ok()?
        .flatten()
        .find(|e| {
            std::fs::read_to_string(e.path().join("comm"))
                .is_ok_and(|c| matches!(c.trim(), "Render thread" | "mcr-java"))
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
}

fn cpu_ns(tid: &str) -> u64 {
    std::fs::read_to_string(format!("/proc/self/task/{tid}/schedstat"))
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse().ok())
        .unwrap_or(0)
}

fn status_kib(key: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| {
            l.strip_prefix(key)?
                .trim()
                .strip_suffix("kB")?
                .trim()
                .parse()
                .ok()
        })
        .unwrap_or(0)
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
fn fabric_boot() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set MCR_JVM_TESTS=1 to boot Fabric");
        return;
    }
    match std::env::var(CHILD_ENV).as_deref() {
        Ok("crash") => crash_child(),
        Ok("exit") => exit_child(),
        _ => {}
    }

    let (cfg, _) = config("main", &[]);
    let started = Instant::now();
    let mut bridge = Bridge::start(cfg).expect("bridge start");
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
    // The widget and heap reports follow the screen report on the same downcall path.
    while let Some(event) = bridge.next_event(Duration::from_millis(500)) {
        seen.record(&event);
    }

    assert_eq!(
        seen.mods.get("fabric-api").map(String::as_str),
        Some("0.162.0+26.3")
    );
    assert_eq!(seen.mods.get("modmenu").map(String::as_str), Some("21.0.0"));
    assert!(seen.mods.contains_key("mcr_bridge"));
    assert!(seen.initialized, "bridge client initializer did not finish");
    let entrypoints = seen.last("client_entrypoints").unwrap_or_default();
    for id in ["mcr_bridge", "modmenu", "fabric-lifecycle-events-v1"] {
        assert!(
            entrypoints.split(',').any(|e| e == id),
            "{id} has no client entrypoint: {entrypoints}"
        );
    }
    let widgets = seen.last("screen_widgets").unwrap_or_default();
    assert!(
        widgets.split('|').any(|w| w.starts_with("Mods")),
        "Mod Menu button missing on the title screen: {widgets}"
    );
    assert_eq!(seen.last("sdl_driver"), Some("offscreen"));
    assert!(
        !seen.info.contains_key("openal_open_device"),
        "OpenAL device opened"
    );
    assert!(!seen.info.contains_key("message_box"), "{:?}", seen.info);
    let bridge_loader = seen.last("bridge_loader").unwrap_or_default();
    assert!(bridge_loader.contains("Knot"), "{bridge_loader}");
    let boot_loader = seen.last("boot_loader").unwrap_or_default();
    assert!(boot_loader.contains("AppClassLoader"), "{boot_loader}");
    assert!(
        tcp_sockets().is_empty(),
        "TCP sockets open at the title screen"
    );

    let tid = render_thread().expect("render thread");
    let sample = |d: Duration| {
        let before = cpu_ns(&tid);
        std::thread::sleep(d);
        (cpu_ns(&tid) - before) as f64 / d.as_secs_f64() / 1e6
    };
    let paced_ms = sample(Duration::from_secs(2));
    bridge.set_paused(true);
    std::thread::sleep(Duration::from_millis(300));
    let paused_ms = sample(Duration::from_secs(1));
    bridge.set_paused(false);
    assert!(
        paused_ms < 5.0,
        "render thread used {paused_ms:.2} ms/s without frames"
    );
    let tcp = tcp_sockets();
    assert!(tcp.is_empty(), "TCP sockets open after idling: {tcp:?}");
    let rss = status_kib("VmRSS:") >> 10;
    let hwm = status_kib("VmHWM:") >> 10;

    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    let ShutdownOutcome::Clean { elapsed } = outcome else {
        panic!("shutdown: {outcome:?}");
    };
    assert!(elapsed < Duration::from_secs(10), "{elapsed:?}");
    eprintln!(
        "fabric-boot: title {:.2}s, shutdown {} ms, render thread {paced_ms:.1} ms/s at 60 Hz, \
         {paused_ms:.3} ms/s paused, rss {rss} MiB (peak {hwm} MiB), heap used/committed/max {} MiB, \
         {} mods, screens {:?}",
        title_after.as_secs_f64(),
        elapsed.as_millis(),
        seen.last("heap_mib").unwrap_or("?"),
        seen.mods.len(),
        seen.screens
    );

    for (mode, expect) in [
        ("crash", "crash child ok"),
        ("exit", "exit child ok code 7"),
    ] {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD_ENV, mode)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&child.stdout);
        assert!(
            child.status.success() && stdout.contains(expect),
            "{mode} child {:?}: {stdout}\n{}",
            child.status,
            String::from_utf8_lossy(&child.stderr)
        );
    }
}

fn wait_for(
    bridge: &mut Bridge,
    what: &str,
    mut done: impl FnMut(&BridgeEvent) -> bool,
) -> BridgeEvent {
    let start = Instant::now();
    loop {
        assert!(start.elapsed() < BOOT_DEADLINE, "no {what}");
        if let Some(event) = bridge.next_event(Duration::from_millis(200))
            && done(&event)
        {
            return event;
        }
    }
}

fn crash_child() -> ! {
    let (cfg, _) = config("crash", &["-Dmcr.bridge.test=throw-in-init"]);
    let mut bridge = Bridge::start(cfg).expect("bridge start");
    let event = wait_for(&mut bridge, "crash report", |e| {
        matches!(e, BridgeEvent::Crash { .. })
    });
    let BridgeEvent::Crash {
        report: Some(report),
    } = event
    else {
        panic!("crash without a report: {event:?}");
    };
    assert!(report.is_file(), "{}", report.display());
    assert!(bridge.crashed());
    std::thread::sleep(Duration::from_millis(500));
    let outcome = bridge.shutdown(Duration::from_secs(2));
    assert_eq!(outcome, ShutdownOutcome::Crashed);
    assert!(outcome.needs_process_exit());
    println!("crash child ok");
    std::process::exit(0);
}

fn exit_child() -> ! {
    let (cfg, _) = config("exit", &["-Dmcr.bridge.test=exit-in-init"]);
    let mut bridge = Bridge::start(cfg).expect("bridge start");
    wait_for(&mut bridge, "System.exit", |e| {
        matches!(e, BridgeEvent::VmExited { .. })
    });
    let outcome = bridge.shutdown(Duration::from_secs(2));
    let ShutdownOutcome::Exited { code } = outcome else {
        panic!("{outcome:?}");
    };
    println!("exit child ok code {code}");
    std::process::exit(0);
}
