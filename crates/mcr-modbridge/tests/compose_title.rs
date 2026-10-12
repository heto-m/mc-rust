//! Java's title screen composed into an offscreen mcr frame, Mod Menu's button found by pixel probe.

mod gpu_common;

use std::path::Path;
use std::time::Duration;

use gpu_common::compose::*;
use gpu_common::image::save_png;
use gpu_common::{config, jvm_tests};
use mcr_modbridge::{ComposeStats, ShutdownOutcome};

const TITLE: &str = "net.minecraft.client.gui.screens.TitleScreen";

#[test]
fn title_screen_shows_mod_menu_button() {
    if !jvm_tests() {
        return;
    }
    let mut cfg = config("compose-title", &[]);
    (cfg.width, cfg.height) = (1280, 720);
    let Some(mut rig) = Rig::start(cfg) else {
        return;
    };
    rig.until("the title screen", |r| r.screen == TITLE && r.loaded());
    rig.run_for(Duration::from_secs(2));
    let layout = rig
        .infos
        .get("screen_layout")
        .cloned()
        .expect("screen layout");
    let mods = widget(&layout, "Mods").unwrap_or_else(|| panic!("no Mods button in {layout}"));
    let img = rig.pixels();
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("compose-title.png");
    save_png(&out, &img);

    let stats = rig.stats();
    let (p50, p99, max) = ComposeStats::percentiles(&stats.waits_us);
    let (j50, j99, _) = ComposeStats::percentiles(&stats.java_us);
    eprintln!(
        "composed {} of {} posted ({} late); render-thread wait p50 {p50} us p99 {p99} us max {max} us; java frame p50 {j50} us p99 {j99} us",
        stats.composed, stats.posted, stats.late
    );

    let scale = (mods.h / 20).max(1);
    let (text, mean_x) = text_pixels(&img, mods);
    eprintln!(
        "Mods button {mods:?}: {text} text pixels, centre x {mean_x:.1}; image {}",
        out.display()
    );
    assert!(
        (25 * scale * scale..=250 * scale * scale).contains(&text),
        "expected the button's white icon pixels in {mods:?}, found {text}"
    );
    let centre = f64::from(mods.x) + f64::from(mods.w) / 2.0;
    assert!(
        (mean_x - centre).abs() < f64::from(mods.w) / 6.0,
        "label not centred: {mean_x:.1} vs {centre:.1}"
    );
    assert!(stats.composed > 60, "{stats:?}");

    // Mod Menu appends "(N Mods)" to the version line; vanilla's text ends before GUI x 100.
    let gui = scale;
    let suffix = Widget {
        x: 105 * gui,
        y: img.height - 10 * gui,
        w: 25 * gui,
        h: 9 * gui,
    };
    let (suffix_text, _) = text_pixels(&img, suffix);
    assert!(
        suffix_text > 10 * gui * gui,
        "no Mod Menu mod count after the version text ({suffix_text} px in {suffix:?})"
    );

    let backend = mcr_modbridge::gpu::backend().expect("backend").stats();
    assert_eq!(rig.detached, None, "the bridge detached");
    let outcome = rig.bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );
    assert_eq!(backend.errors, 0, "backend errors: {backend:?}");
}
