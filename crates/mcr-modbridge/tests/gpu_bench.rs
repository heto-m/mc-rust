mod gpu_common;

use std::path::Path;

use gpu_common::*;
use mcr_modbridge::{Bridge, ShutdownOutcome};

/// Per-frame recording costs on the idle title screen and the test screen (task 4.2 of mod-render-backend).
#[test]
fn bench_gui_frames() {
    if !jvm_tests() {
        return;
    }
    let shot = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gui-bench.png");
    let extra = vec![
        "-Dmcr.gpu.selftest=bench".into(),
        format!("-Dmcr.testmod.gui={}", shot.display()),
        "-Dmcr.testmod.gui.delay=200".into(),
        "-Dmcr.testmod.gui.settle=200".into(),
    ];
    let mut cfg = config("bench", &extra);
    cfg.width = 1280;
    cfg.height = 720;
    let mut bridge = Bridge::start(cfg).expect("bridge start");
    let mut seen = Vec::new();
    while seen.len() < 2 {
        let report = info_value(&mut bridge, "gpu_bench");
        eprintln!("gpu_bench: {report}");
        if report.contains("screen=TitleScreen ") || report.contains("screen=GpuTestScreen ") {
            seen.push(report);
        }
    }
    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );
    for report in &seen {
        assert!(report.ends_with(" errors=0"), "{report}");
    }
}
