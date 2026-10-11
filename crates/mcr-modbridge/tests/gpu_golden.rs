mod gpu_common;

use std::path::Path;
use std::time::{Duration, Instant};

use gpu_common::image::*;
use gpu_common::*;
use mcr_modbridge::{Bridge, BridgeEvent, ShutdownOutcome};

const GOLDEN: &str = "golden/26.3/gui.png";
const MIN_SSIM: f64 = 0.98;

/// The test screen through the bridge matches vanilla's OpenGL rendering (`cargo xtask golden-gui`).
#[test]
fn gui_matches_the_opengl_golden() {
    if !jvm_tests() {
        return;
    }
    let golden_path = mcr_jvm::dev_cache_dir().join(GOLDEN);
    let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let shot = out_dir.join("gui-mcr.png");
    let _ = std::fs::remove_file(&shot);
    let extra = vec![format!("-Dmcr.testmod.gui={}", shot.display())];
    let mut cfg = config("gui", &extra);
    cfg.width = 1280;
    cfg.height = 720;
    let mut bridge = Bridge::start(cfg).expect("bridge start");
    let start = Instant::now();
    while !shot.exists() {
        assert!(start.elapsed() < DEADLINE, "no gui capture");
        if let Some(BridgeEvent::Crash { report }) = bridge.next_event(Duration::from_millis(200)) {
            panic!("java crashed: {report:?}");
        }
    }
    mcr_modbridge::gpu::capture_next_present();
    let presented = loop {
        assert!(start.elapsed() < DEADLINE, "no presented frame");
        if let Some(img) = mcr_modbridge::gpu::take_capture() {
            break img;
        }
        bridge.next_event(Duration::from_millis(50));
    };
    let stats = mcr_modbridge::gpu::backend().expect("backend").stats();
    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );
    eprintln!("backend stats: {stats:?}");
    assert_eq!(stats.errors, 0, "the backend reported errors");

    let screenshot = load_png(&shot);
    let presented = Rgba {
        width: presented.width,
        height: presented.height,
        pixels: presented.rgba,
    };
    save_png(&out_dir.join("gui-mcr-present.png"), &presented);
    let readback = ssim(&screenshot, &presented);
    eprintln!("screenshot vs presented image: SSIM {readback:.4}");
    assert!(
        readback > 0.999,
        "the screenshot readback differs from the presented image"
    );

    if !golden_path.exists() {
        eprintln!(
            "skipped the golden comparison: {} is missing (run `cargo xtask golden-gui`)",
            golden_path.display()
        );
        return;
    }
    let golden = load_png(&golden_path);
    let score = ssim(&golden, &screenshot);
    eprintln!("bridge vs OpenGL golden: SSIM {score:.4} (min {MIN_SSIM})");
    assert!(
        score >= MIN_SSIM,
        "SSIM {score:.4} below {MIN_SSIM}; see {}",
        shot.display()
    );
}
