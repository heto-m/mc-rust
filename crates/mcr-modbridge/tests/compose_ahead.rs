//! Java's frames one mcr frame ahead: the title screen still composes every frame, with its GUI intact.

mod gpu_common;

use std::path::Path;
use std::time::Duration;

use gpu_common::compose::*;
use gpu_common::image::save_png;
use gpu_common::{config, jvm_tests};
use mcr_modbridge::{Pipelining, ShutdownOutcome};

const TITLE: &str = "net.minecraft.client.gui.screens.TitleScreen";
const FRAMES: u64 = 600;

#[test]
fn frames_ahead_compose_each_java_frame_once() {
    if !jvm_tests() {
        return;
    }
    let mut cfg = config("compose-ahead", &[]);
    (cfg.width, cfg.height) = (1280, 720);
    let Some(mut rig) = Rig::start(cfg) else {
        return;
    };
    rig.until("the title screen", |r| r.screen == TITLE && r.loaded());
    rig.run_for(Duration::from_secs(2));
    rig.frames.lock().set_pipelining(Pipelining::Ahead);
    rig.paced = false;
    rig.frame();
    rig.frames.lock().reset_stats();
    let java_before = rig.frames.lock().java().done;
    for _ in 0..FRAMES {
        rig.frame();
    }
    let stats = rig.stats();
    let java_done = rig.frames.lock().java().done - java_before;
    eprintln!(
        "{FRAMES} frames ahead: {} posted, {} composed, {} late, java finished {java_done}; {}",
        stats.posted,
        stats.composed,
        stats.late,
        stats.profile.line()
    );
    assert_eq!(rig.detached, None, "the bridge detached");
    assert!(stats.composed + 1 >= FRAMES, "{stats:?}");
    assert!(
        stats.composed <= java_done + 1,
        "a Java frame composed twice"
    );
    assert!(
        stats.profile.ahead + 1 >= stats.profile.frames,
        "frames were not composed ahead: {}",
        stats.profile.line()
    );

    let layout = rig
        .infos
        .get("screen_layout")
        .cloned()
        .expect("screen layout");
    let mods = widget(&layout, "Mods").unwrap_or_else(|| panic!("no Mods button in {layout}"));
    let img = rig.pixels();
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("compose-ahead.png");
    save_png(&out, &img);
    let scale = (mods.h / 20).max(1);
    let (text, _) = text_pixels(&img, mods);
    assert!(
        (25 * scale * scale..=250 * scale * scale).contains(&text),
        "the GUI composed ahead lost the Mods button's icon in {mods:?}: {text} px, {}",
        out.display()
    );

    rig.stop_ticks();
    rig.frames.lock().set_pipelining(Pipelining::Lockstep);
    rig.run_for(Duration::from_millis(500));
    let stats = rig.stats();
    assert_eq!(stats.java_ticks, stats.rust_ticks, "{stats:?}");
    let backend = mcr_modbridge::gpu::backend().expect("backend").stats();
    let outcome = rig.bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );
    assert_eq!(backend.errors, 0, "backend errors: {backend:?}");
}
