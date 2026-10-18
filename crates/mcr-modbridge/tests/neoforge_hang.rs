//! Opt-in: under NeoForge, a render thread that blocks for good detaches within 3 s and mcr keeps rendering.
//! `MCR_JVM_TESTS=1 cargo test -p mcr-modbridge --test neoforge_hang -- --nocapture`

mod gpu_common;

use std::time::{Duration, Instant};

use gpu_common::compose::*;
use gpu_common::{jvm_tests, neoforge_config};
use mcr_modbridge::frame::{FRAME_WAIT, LinkState};

const TITLE: &str = "net.minecraft.client.gui.screens.TitleScreen";

#[test]
fn neoforge_hang_detaches() {
    if !jvm_tests() {
        return;
    }
    let extra = [
        "-Dmcr.bridge.test=hang-in-tick".to_owned(),
        "-Dmcr.bridge.test.ticks=60".to_owned(),
    ];
    let mut cfg = neoforge_config("hang", &extra);
    (cfg.width, cfg.height) = (854, 480);
    let Some(mut rig) = Rig::start(cfg) else {
        return;
    };
    rig.until("the title screen", |r| r.screen == TITLE && r.loaded());

    let mut last_composed = (rig.stats().composed, Instant::now());
    let mut slowest = Duration::ZERO;
    let start = Instant::now();
    while rig.detached.is_none() {
        assert!(start.elapsed() < Duration::from_secs(30), "never detached");
        let t = Instant::now();
        rig.frame();
        slowest = slowest.max(t.elapsed());
        let composed = rig.stats().composed;
        if composed != last_composed.0 {
            last_composed = (composed, Instant::now());
        }
    }
    let to_detach = last_composed.1.elapsed();
    eprintln!(
        "neoforge-hang: detached {:.2}s after Java's last frame ({:?}); slowest frame {:.0} ms",
        to_detach.as_secs_f32(),
        rig.detached,
        slowest.as_secs_f64() * 1000.0
    );
    assert!(to_detach <= Duration::from_secs(3), "{to_detach:?}");
    assert!(
        slowest <= FRAME_WAIT + Duration::from_millis(60),
        "mcr froze for {slowest:?}"
    );
    {
        let mut link = rig.frames.lock();
        assert_eq!(link.state(), LinkState::Detached);
        assert!(!link.gui_active(), "the native HUD returns");
        assert!(link.take_notice().is_some(), "the user is told");
    }

    let frames = rig.frame_count;
    let t = Instant::now();
    let mut slowest = Duration::ZERO;
    while t.elapsed() < Duration::from_secs(2) {
        let f = Instant::now();
        rig.frame();
        slowest = slowest.max(f.elapsed());
    }
    assert!(rig.frame_count - frames > 60);
    assert!(slowest < Duration::from_millis(100), "{slowest:?}");
    // Java is parked for good; the process ends without a clean VM shutdown.
    std::mem::forget(rig);
}
