// The EmbeddedChannel seam spike as a regression check: a vanilla login pipeline on a bridge
// channel inside the booted client, fed the captured LoginFinished; no server needed.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mcr_modbridge::{Bridge, BridgeConfig, BridgeEvent, LaunchManifest, ShutdownOutcome};

const DEADLINE: Duration = Duration::from_secs(240);

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn field<'a>(report: &'a str, key: &str) -> &'a str {
    report
        .split_whitespace()
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("{key} missing in {report}"))
}

#[test]
fn net_seam() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set MCR_JVM_TESTS=1 to boot Fabric");
        return;
    }
    let cache = mcr_jvm::dev_cache_dir();
    for module in ["mcr-boot", "mcr-bridge"] {
        let jar = cache.join(format!("java/{module}.jar"));
        assert!(
            mcr_jvm::stale_java_jar(&repo().join("java").join(module), &jar).is_none(),
            "run `cargo xtask java-build`"
        );
    }
    let frame = cache.join("captures/26.3/login/clientbound/login_finished/1.bin");
    assert!(
        frame.is_file(),
        "{} missing: run the packet capture",
        frame.display()
    );
    let manifest = LaunchManifest::load(&cache).expect("run `cargo xtask fabric-fetch`");
    let game_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("net-seam");
    let _ = std::fs::remove_dir_all(&game_dir);
    let mut cfg = BridgeConfig::new(cache, game_dir, manifest.test_mods.clone());
    cfg.max_heap = "1536m".into();
    cfg.extra_jvm = vec![
        "-Dmcr.bridge.test=net-seam".into(),
        format!("-Dmcr.bridge.test.frame=2:{}", frame.display()),
    ];
    let mut bridge = Bridge::start(cfg).expect("bridge start");
    let started = Instant::now();
    let report = loop {
        assert!(started.elapsed() < DEADLINE, "no net_seam report");
        match bridge.next_event(Duration::from_millis(200)) {
            Some(BridgeEvent::Info { key, value }) if key == "net_seam" => break value,
            Some(BridgeEvent::Crash { report }) => panic!("java crashed: {report:?}"),
            _ => {}
        }
    };
    eprintln!("net-seam: {report}");
    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );

    assert!(!report.contains("error="), "{report}");
    assert_eq!(
        field(&report, "listener"),
        "ClientHandshakePacketListenerImpl"
    );
    assert_eq!(field(&report, "inbound_phase"), "1");
    assert_eq!(
        field(&report, "listener_after"),
        "ClientConfigurationPacketListenerImpl"
    );
    assert_eq!(field(&report, "inbound_phase_after"), "2");
    assert_eq!(field(&report, "open"), "true");
    assert_eq!(field(&report, "offthread_queued"), "true");
    assert_eq!(field(&report, "offthread_sent"), "true");
    assert_eq!(field(&report, "closed"), "true");
    let outbound: Vec<&str> = field(&report, "outbound").split(',').collect();
    assert_eq!(
        outbound.first(),
        Some(&"0:0"),
        "intention in the handshake phase"
    );
    assert!(
        outbound.contains(&"1:3"),
        "login_acknowledged tagged login: {outbound:?}"
    );
    assert!(
        outbound.contains(&"2:2"),
        "brand tagged configuration: {outbound:?}"
    );
    assert!(
        outbound.contains(&"2:0"),
        "client_information tagged configuration: {outbound:?}"
    );
    assert_eq!(
        outbound.last(),
        Some(&"2:4"),
        "the off-thread keep-alive went last"
    );
}
