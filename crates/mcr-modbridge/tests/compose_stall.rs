//! A mod that blocks Java's render thread for a few seconds once, like REI/JEI reloading their
//! plugins on join: mcr detaches, then composes Java again once it is prompt.

mod gpu_common;

use std::time::{Duration, Instant};

use gpu_common::compose::*;
use gpu_common::{config, jvm_tests};
use mcr_modbridge::ShutdownOutcome;
use mcr_modbridge::frame::LinkState;

const TITLE: &str = "net.minecraft.client.gui.screens.TitleScreen";

#[test]
fn a_stalled_java_reattaches() {
    if !jvm_tests() {
        return;
    }
    let extra = [
        "-Dmcr.testmod.hangAfterTicks=60".to_owned(),
        "-Dmcr.testmod.stallMillis=3000".to_owned(),
    ];
    let mut cfg = config("compose-stall", &extra);
    (cfg.width, cfg.height) = (854, 480);
    let Some(mut rig) = Rig::start(cfg) else {
        return;
    };
    rig.until("the title screen", |r| r.screen == TITLE && r.loaded());
    rig.until("the stall to detach", |r| r.detached.is_some());
    {
        let mut link = rig.frames.lock();
        assert_eq!(link.state(), LinkState::Detached);
        assert!(!link.gui_active(), "the native HUD returns");
        assert!(link.take_notice().is_some());
    }

    let detached = Instant::now();
    rig.until("Java to be composed again", |r| r.resumed);
    eprintln!(
        "re-attached {:.2}s after detaching ({:?})",
        detached.elapsed().as_secs_f32(),
        rig.detached
    );
    let composed = rig.stats().composed;
    rig.run_for(Duration::from_secs(1));
    let mut link = rig.frames.lock();
    assert_eq!(link.state(), LinkState::Running);
    assert!(link.gui_active(), "Java's GUI is shown again");
    assert!(link.take_notice().is_some(), "the user is told");
    drop(link);
    assert!(rig.stats().composed > composed + 30, "{:?}", rig.stats());
    let outcome = rig.bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );
}
