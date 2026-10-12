//! Java's title screen driven by scripted input: Mod Menu's search box, paste, the "mcr" button,
//! the singleplayer toast and Quit.

mod gpu_common;

use std::sync::Arc;
use std::time::Duration;

use gpu_common::compose::*;
use gpu_common::input::*;
use gpu_common::{config, jvm_tests};
use mcr_modbridge::ShutdownOutcome;
use mcr_modbridge::input::{ClipboardHost, MemoryClipboard, NativeKind};

const TITLE: &str = "net.minecraft.client.gui.screens.TitleScreen";

#[test]
fn title_screen_takes_mcr_input() {
    if !jvm_tests() {
        return;
    }
    let mut cfg = config("input-title", &["-Dmcr.bridge.probeInput=true".into()]);
    (cfg.width, cfg.height) = (1280, 720);
    let Some(mut rig) = Rig::start(cfg) else {
        return;
    };
    rig.until("the title screen", |r| r.screen == TITLE && r.loaded());
    let mut l = Live::new(rig);
    l.run_for(Duration::from_secs(2));
    let clipboard = Arc::new(MemoryClipboard::default());
    l.input.set_clipboard(clipboard.clone());
    let java = l.input.java();
    assert!(java.screen && !java.grabbed, "{java:?}");
    assert!(java.gui_scale >= 1, "{java:?}");

    l.click(l.button("mcr"));
    l.run_for(Duration::from_millis(200));
    assert_eq!(l.input.take_native_request(), Some(NativeKind::Menu));

    l.click(l.button("Singleplayer"));
    l.run_for(Duration::from_millis(500));
    assert_eq!(l.rig.screen, TITLE, "singleplayer stays closed");

    l.click(l.button("Mods"));
    l.wait_screen("modmenu");
    let search = *l.edit_boxes().first().expect("Mod Menu has a search box");
    l.click(search);
    l.until("text input", |l| l.input.java().text_input);
    l.type_text("stone");
    l.wait_info("focus_value", "stone");

    clipboard.set("hello");
    l.chord(LEFT_CTRL, scancode('a'));
    l.chord(LEFT_CTRL, scancode('v'));
    l.wait_info("focus_value", "hello");
    l.chord(LEFT_CTRL, scancode('a'));
    l.chord(LEFT_CTRL, scancode('c'));
    l.run_for(Duration::from_millis(200));
    assert_eq!(
        clipboard.get().as_deref(),
        Some("hello"),
        "copy reaches mcr"
    );

    l.tap(ESCAPE);
    l.until("back on the title screen", |l| l.rig.screen == TITLE);
    l.run_for(Duration::from_millis(500));
    assert!(
        !l.input.java().text_input,
        "text input stops with the screen"
    );
    assert_eq!(l.input.dropped(), 0);
    assert_eq!(l.rig.detached, None);

    l.click(l.button("Quit Game"));
    l.until("java's quit request", |l| l.input.java().quit);
    let outcome = l.rig.bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(!matches!(outcome, ShutdownOutcome::Crashed), "{outcome:?}");
}
