//! Opt-in: how a NeoForge session ends. HotSpot allows one VM per process, so every case runs as a child
//! of this test: a mod failing at load, a crash in game, `System.exit` and a clean quit.
//! `MCR_JVM_TESTS=1 cargo test -p mcr-modbridge --test neoforge_exit -- --nocapture`

mod gpu_common;

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use gpu_common::{DEADLINE, jvm_tests, neoforge_config};
use mcr_modbridge::{Bridge, BridgeEvent, ShutdownOutcome};

const TEST_NAME: &str = "neoforge_exit";
const CHILD_ENV: &str = "MCR_NEOFORGE_EXIT";

fn start(case: &str, extra: &[&str]) -> Bridge {
    let extra: Vec<String> = extra.iter().map(|s| (*s).to_owned()).collect();
    let mut cfg = neoforge_config(&format!("exit-{case}"), &extra);
    cfg.http_hosts.clear();
    Bridge::start(cfg).expect("bridge start")
}

/// Events until `done`; a message box report fails the test.
fn until(
    bridge: &mut Bridge,
    what: &str,
    mut done: impl FnMut(&BridgeEvent) -> bool,
) -> BridgeEvent {
    let start = Instant::now();
    loop {
        assert!(start.elapsed() < DEADLINE, "no {what}");
        let Some(event) = bridge.next_event(Duration::from_millis(200)) else {
            continue;
        };
        if let BridgeEvent::Info { key, value } = &event {
            assert_ne!(key, "message_box", "{value}");
        }
        if done(&event) {
            return event;
        }
    }
}

fn crash_reports(case: &str) -> usize {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("neoforge-exit-{case}"))
        .join("crash-reports");
    std::fs::read_dir(dir).map_or(0, |d| d.count())
}

/// A mod throwing in its constructor: FML writes a report and exits with 1 instead of opening its dialog.
fn load_failure() {
    let mut bridge = start("load", &["-Dmcr.bridge.test=throw-in-init"]);
    until(&mut bridge, "the VM exit", |e| {
        matches!(e, BridgeEvent::VmExited { .. })
    });
    assert_eq!(
        bridge.shutdown(Duration::from_secs(2)),
        ShutdownOutcome::Exited { code: 1 }
    );
    assert_eq!(crash_reports("load"), 1, "one crash report");
}

fn crash_in_game() {
    let mut bridge = start(
        "crash",
        &[
            "-Dmcr.bridge.test=crash-in-tick",
            "-Dmcr.bridge.test.ticks=20",
        ],
    );
    let event = until(&mut bridge, "a crash report", |e| {
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
}

fn exit_in_init() {
    let mut bridge = start("exit", &["-Dmcr.bridge.test=exit-in-init"]);
    until(&mut bridge, "the VM exit", |e| {
        matches!(e, BridgeEvent::VmExited { .. })
    });
    assert_eq!(
        bridge.shutdown(Duration::from_secs(2)),
        ShutdownOutcome::Exited { code: 7 }
    );
}

fn clean_quit() {
    let mut bridge = start("quit", &[]);
    until(
        &mut bridge,
        "the title screen",
        BridgeEvent::is_title_screen,
    );
    std::thread::sleep(Duration::from_secs(1));
    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    let ShutdownOutcome::Clean { elapsed } = outcome else {
        panic!("shutdown: {outcome:?}");
    };
    assert!(elapsed < Duration::from_secs(10), "{elapsed:?}");
    println!("quit in {} ms", elapsed.as_millis());
}

#[test]
fn neoforge_exit() {
    if !jvm_tests() {
        return;
    }
    let cases: [(&str, fn()); 4] = [
        ("load", load_failure),
        ("crash", crash_in_game),
        ("exit", exit_in_init),
        ("quit", clean_quit),
    ];
    if let Ok(case) = std::env::var(CHILD_ENV) {
        let (name, run) = cases.iter().find(|(n, _)| *n == case).expect("known case");
        run();
        println!("child {name} ok");
        std::process::exit(0);
    }
    for (name, _) in cases {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD_ENV, name)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&child.stdout);
        assert!(
            child.status.success() && stdout.contains(&format!("child {name} ok")),
            "{name} child {:?}: {stdout}\n{}",
            child.status,
            String::from_utf8_lossy(&child.stderr)
        );
        let quit = stdout
            .lines()
            .find(|l| l.starts_with("quit in"))
            .unwrap_or("");
        eprintln!("neoforge-exit: {name} ok {quit}");
    }
}
