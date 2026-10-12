//! Opt-in measurement: what composing Java's GUI costs mcr per frame, GPU work included.
//! `MCR_JVM_TESTS=1 MCR_COMPOSE_COST=1 cargo test -p mcr-modbridge --test compose_cost -- --nocapture`

mod gpu_common;

use std::time::{Duration, Instant};

use gpu_common::compose::*;
use gpu_common::{config, jvm_tests};
use mcr_modbridge::ComposeStats;

const TITLE: &str = "net.minecraft.client.gui.screens.TitleScreen";
const FRAMES: usize = 1200;

#[test]
fn compose_cost_at_1080p() {
    if !jvm_tests() || std::env::var("MCR_COMPOSE_COST").as_deref() != Ok("1") {
        eprintln!("skipped: set MCR_COMPOSE_COST=1 too");
        return;
    }
    let mut cfg = config("compose-cost", &[]);
    (cfg.width, cfg.height) = (1920, 1080);
    let Some(mut rig) = Rig::start(cfg) else {
        return;
    };
    rig.until("the title screen", |r| r.screen == TITLE && r.loaded());
    rig.run_for(Duration::from_secs(2));
    rig.paced = false;
    rig.frames.lock().reset_stats();
    let mut frame_us = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        let start = Instant::now();
        rig.frame();
        frame_us.push(start.elapsed().as_micros().min(u128::from(u32::MAX)) as u32);
    }
    let stats = rig.stats();
    let (f50, f99, fmax) = ComposeStats::percentiles(&frame_us);
    let (g50, g99, gmax) = ComposeStats::percentiles(&stats.gui_us);
    let (j50, j99, _) = ComposeStats::percentiles(&stats.java_us);
    eprintln!(
        "1920x1080 title, {FRAMES} unpaced frames ({} composed): mcr frame incl. GPU p50 {f50} us p99 {f99} us max {fmax} us; gui compose CPU p50 {g50} us p99 {g99} us max {gmax} us; java frame p50 {j50} us p99 {j99} us",
        stats.composed
    );
    assert_eq!(rig.detached, None, "the bridge detached");
    let _ = rig.bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
}
