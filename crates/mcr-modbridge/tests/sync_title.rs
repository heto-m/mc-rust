//! What mcr syncs into Java, checked on the title screen through the test mod's probe: key mappings both ways,
//! the render distance, a server pack loaded from mcr's file and quiet proxy refusals.

mod gpu_common;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpu_common::compose::*;
use gpu_common::{config, jvm_tests};
use mcr_modbridge::ShutdownOutcome;

const TITLE: &str = "net.minecraft.client.gui.screens.TitleScreen";
const PACKED: &str = "Multiplayer from mcr's pack";

fn info(rig: &mut Rig, key: &str, value: &str) {
    let what = format!("{key}={value}");
    rig.until(&what, |r| {
        r.infos.get(key).map(String::as_str) == Some(value)
    });
}

fn write_pack(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("server-pack.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("pack.mcmeta", opts).unwrap();
    zip.write_all(br#"{"pack":{"description":"mcr test","min_format":97,"max_format":97}}"#)
        .unwrap();
    zip.start_file("assets/minecraft/lang/en_us.json", opts)
        .unwrap();
    zip.write_all(format!(r#"{{"menu.multiplayer":"{PACKED}"}}"#).as_bytes())
        .unwrap();
    zip.finish().unwrap();
    path
}

#[test]
fn java_follows_mcr_settings() {
    if !jvm_tests() {
        return;
    }
    let mut cfg = config(
        "sync-title",
        &[
            "-Dmcr.testmod.syncProbe=true".into(),
            "-Dmcr.testmod.watchKeys=key.forward,key.sprint,key.jump,key.mcr_testmod.ping".into(),
            "-Dmcr.testmod.rebindWhen=key.forward=key.keyboard.up".into(),
            "-Dmcr.testmod.rebind=key.jump=key.keyboard.j".into(),
            "-Dmcr.testmod.webCheck=https://api.modrinth.com/v2/tag/game_version".into(),
        ],
    );
    (cfg.width, cfg.height) = (1280, 720);
    std::fs::create_dir_all(&cfg.game_dir).unwrap();
    std::fs::write(
        cfg.game_dir.join("options.txt"),
        "version:5023\nonboardAccessibility:false\nkey_key.sprint:key.keyboard.x\n",
    )
    .unwrap();
    let game_dir = cfg.game_dir.clone();
    let Some(mut rig) = Rig::start(cfg) else {
        return;
    };
    rig.until("the title screen", |r| r.screen == TITLE && r.loaded());

    let bindings = [
        ("key.forward", "key.keyboard.up"),
        ("key.sprint", "key.keyboard.left.control"),
        ("key.jump", "key.keyboard.space"),
    ];
    rig.frames.lock().set_key_bindings(bindings);
    info(&mut rig, "key_key.forward", "key.keyboard.up");
    info(&mut rig, "key_key.sprint", "key.keyboard.left.control");

    rig.until("a key report from Java", |r| !r.keys_changed.is_empty());
    rig.run_for(Duration::from_millis(500));
    assert_eq!(
        rig.keys_changed,
        [("key.jump".to_owned(), "key.keyboard.j".to_owned())],
        "only the rebound vanilla mapping is reported"
    );
    info(&mut rig, "key_key.mcr_testmod.ping", "key.keyboard.k");

    rig.frames.lock().set_view(6, 1.5);
    info(&mut rig, "effective_rd", "6");
    info(&mut rig, "entity_scale", "1.5");

    let pack = write_pack(&Path::new(env!("CARGO_TARGET_TMPDIR")).join("sync-title-downloads"));
    rig.frames.lock().set_server_packs(&[pack]);
    info(&mut rig, "lang_multiplayer", PACKED);
    rig.frames.lock().set_server_packs(&[]);
    info(&mut rig, "lang_multiplayer", "Multiplayer");

    info(&mut rig, "web_check", "done");
    rig.run_for(Duration::from_millis(500));
    let log = std::fs::read_to_string(game_dir.join("logs/latest.log")).expect("java's log");
    assert!(
        !log.contains("Tunnel failed"),
        "a refusal stack trace was logged"
    );
    assert!(
        !log.contains("Unable to tunnel"),
        "a refusal warning was logged"
    );
    assert_eq!(
        log.matches("mcr_testmod: a web request was refused")
            .count(),
        1,
        "one quiet line for the test mod's logger"
    );

    assert_eq!(rig.detached, None, "the bridge detached");
    let outcome = rig.bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(!matches!(outcome, ShutdownOutcome::Crashed), "{outcome:?}");
}
