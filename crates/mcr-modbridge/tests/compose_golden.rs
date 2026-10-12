//! The test screen (text, items, a scissored list, a blurred backdrop) composed top-down into mcr's
//! scene matches vanilla's OpenGL rendering, both as mcr shows it and as Java's own screenshot reads it.

mod gpu_common;

use std::path::Path;
use std::time::{Duration, Instant};

use gpu_common::compose::*;
use gpu_common::image::*;
use gpu_common::{DEADLINE, config, jvm_tests};

const GOLDEN: &str = "golden/26.3/gui.png";
const MIN_SSIM: f64 = 0.98;

#[test]
fn composed_gui_matches_the_opengl_golden() {
    if !jvm_tests() {
        return;
    }
    let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let shot = out_dir.join("compose-gui-java.png");
    let _ = std::fs::remove_file(&shot);
    let mut cfg = config(
        "compose-golden",
        &[format!("-Dmcr.testmod.gui={}", shot.display())],
    );
    (cfg.width, cfg.height) = (1280, 720);
    let Some(mut rig) = Rig::start(cfg) else {
        return;
    };
    let start = Instant::now();
    while !shot.exists() {
        assert!(start.elapsed() < DEADLINE, "no gui capture");
        rig.frame();
    }
    rig.run_for(Duration::from_millis(200));
    let scene = rig.pixels();
    save_png(&out_dir.join("compose-gui-scene.png"), &scene);
    let java = load_png(&shot);
    let backend = mcr_modbridge::gpu::backend().expect("backend").stats();
    assert_eq!(rig.detached, None, "the bridge detached");
    let _ = rig.bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert_eq!(backend.errors, 0, "backend errors: {backend:?}");

    let readback = ssim(&java, &scene);
    eprintln!("java screenshot vs mcr scene: SSIM {readback:.4}");
    assert!(
        readback > 0.99,
        "Java's screenshot does not read the composed scene in its row order"
    );
    let golden_path = mcr_jvm::dev_cache_dir().join(GOLDEN);
    if !golden_path.exists() {
        eprintln!(
            "skipped the golden comparison: {} is missing (run `cargo xtask golden-gui`)",
            golden_path.display()
        );
        return;
    }
    let golden = load_png(&golden_path);
    let score = ssim(&golden, &scene);
    eprintln!("composed scene vs OpenGL golden: SSIM {score:.4} (min {MIN_SSIM})");
    assert!(score >= MIN_SSIM, "SSIM {score:.4} below {MIN_SSIM}");
}
