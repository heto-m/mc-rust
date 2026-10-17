//! Java's world screens over mcr's saves: the list, Play and Create reaching mcr, mcr's progress, EULA and
//! failures in Java, no Java world access while mcr's world is busy, and `doWorldLoad` blocked.

mod gpu_common;

use std::path::Path;
use std::time::Duration;

use gpu_common::compose::*;
use gpu_common::input::*;
use gpu_common::{config, jvm_tests};
use mcr_modbridge::ShutdownOutcome;
use mcr_modbridge::world::{NewMode, WorldEvent, WorldRequest};
use mcr_nbt::{Compound, Compression, Value, write_disk};

const TITLE: &str = "net.minecraft.client.gui.screens.TitleScreen";
const SELECT: &str = "net.minecraft.client.gui.screens.worldselection.SelectWorldScreen";
const CREATE: &str = "net.minecraft.client.gui.screens.worldselection.CreateWorldScreen";
const PROGRESS: &str = "dev.mcr.bridge.McrWorldScreen";
const CONFIRM: &str = "net.minecraft.client.gui.screens.ConfirmScreen";
const ALERT: &str = "net.minecraft.client.gui.screens.AlertScreen";
const FOLDER: &str = "Probe World";

fn write_world(saves: &Path) {
    let mut version = Compound::new();
    version.insert("Name", "26.3");
    version.insert("Id", Value::Int(5023));
    version.insert("Series", "main");
    version.insert("Snapshot", Value::Byte(0));
    let mut data = Compound::new();
    data.insert("LevelName", FOLDER);
    data.insert("version", Value::Int(19133));
    data.insert("DataVersion", Value::Int(5023));
    data.insert("Version", Value::Compound(version));
    data.insert("LastPlayed", Value::Long(1_792_000_000_000));
    data.insert("GameType", Value::Int(1));
    data.insert("allowCommands", Value::Byte(1));
    let mut root = Compound::new();
    root.insert("Data", Value::Compound(data));
    let world = saves.join(FOLDER);
    std::fs::create_dir_all(&world).unwrap();
    std::fs::write(
        world.join("level.dat"),
        write_disk("", &root, Compression::Gzip).unwrap(),
    )
    .unwrap();
}

fn screen(l: &mut Live, class: &str) {
    l.until(class, |l| l.rig.screen == class);
    l.run_for(Duration::from_millis(300));
}

fn request(l: &mut Live) -> WorldRequest {
    l.until("a world request", |l| !l.rig.worlds.is_empty());
    l.rig.worlds.remove(0)
}

fn send(l: &mut Live, event: WorldEvent) {
    l.rig.frames.lock().world(event);
    l.settle(3);
}

fn button_starting(l: &Live, prefix: &str) -> Widget {
    let layout = l
        .rig
        .infos
        .get("screen_layout")
        .cloned()
        .unwrap_or_default();
    let label = layout
        .split('|')
        .skip(1)
        .filter_map(|p| p.splitn(5, ',').nth(4))
        .find(|label| label.starts_with(prefix))
        .unwrap_or_else(|| panic!("no {prefix}… button in {layout}"))
        .to_owned();
    l.button(&label)
}

#[test]
fn java_world_screens_go_through_mcr() {
    if !jvm_tests() {
        return;
    }
    let mut cfg = config(
        "world-screens",
        &[format!("-Dmcr.testmod.worldProbe={FOLDER}")],
    );
    (cfg.width, cfg.height) = (1280, 720);
    let saves = Path::new(env!("CARGO_TARGET_TMPDIR")).join("world-screens-saves");
    let _ = std::fs::remove_dir_all(&saves);
    write_world(&saves);
    cfg.saves_dir = Some(saves.clone());
    let game_dir = cfg.game_dir.clone();
    let Some(mut rig) = Rig::start(cfg) else {
        return;
    };
    rig.until("the title screen", |r| r.screen == TITLE && r.loaded());
    let mut l = Live::new(rig);
    l.run_for(Duration::from_secs(1));

    l.click(l.button("Singleplayer"));
    screen(&mut l, SELECT);
    l.wait_info("world_list", FOLDER);
    l.click(l.button("Play Selected World"));
    assert_eq!(request(&mut l), WorldRequest::Play(FOLDER.into()));
    screen(&mut l, PROGRESS);
    l.until("Java's world access attempt", |l| {
        l.rig.infos.contains_key("world_access")
    });
    let access = l.rig.infos["world_access"].clone();
    assert!(
        access.starts_with("refused") && access.contains("open in mcr"),
        "{access}"
    );

    send(&mut l, WorldEvent::Eula);
    screen(&mut l, CONFIRM);
    l.click(l.button("I agree"));
    assert_eq!(request(&mut l), WorldRequest::Eula(true));
    screen(&mut l, PROGRESS);
    send(
        &mut l,
        WorldEvent::Status("lang:menu.preparingSpawn|42".into()),
    );
    l.click(l.button("Cancel"));
    assert_eq!(request(&mut l), WorldRequest::Cancel);
    screen(&mut l, SELECT);
    l.click(l.button("Play Selected World"));
    l.run_for(Duration::from_millis(500));
    assert!(
        l.rig.worlds.is_empty(),
        "no new session before the last one stopped"
    );
    send(&mut l, WorldEvent::Stopped(None));

    l.wait_info("world_list", FOLDER);
    l.click(l.button("Play Selected World"));
    assert_eq!(request(&mut l), WorldRequest::Play(FOLDER.into()));
    screen(&mut l, PROGRESS);
    send(
        &mut l,
        WorldEvent::Failed("the server could not start".into()),
    );
    screen(&mut l, ALERT);
    l.click(l.button("Back"));
    screen(&mut l, SELECT);

    l.click(l.button("Create New World"));
    screen(&mut l, CREATE);
    l.click(button_starting(&l, "Game Mode"));
    l.click(l.button("Create New World"));
    match request(&mut l) {
        WorldRequest::Create(w) => {
            assert_eq!(w.name, "New World");
            assert_eq!(w.mode, NewMode::Hardcore);
            assert_eq!(w.difficulty, 3, "hardcore is hard");
        }
        other => panic!("expected a create request, got {other:?}"),
    }
    screen(&mut l, PROGRESS);
    send(
        &mut l,
        WorldEvent::Stopped(Some("The local server stopped".into())),
    );
    screen(&mut l, ALERT);
    l.click(l.button("Back"));
    screen(&mut l, SELECT);
    assert!(
        !saves.join("New World").exists(),
        "Java created the world folder"
    );
    assert!(
        !saves.join(FOLDER).join("session.lock").exists(),
        "Java opened the world"
    );

    l.tap(ESCAPE);
    screen(&mut l, TITLE);
    std::fs::write(game_dir.join("mcr-probe-load"), b"").unwrap();
    l.until("the doWorldLoad probe", |l| {
        l.rig.infos.contains_key("world_load")
    });
    assert_eq!(l.rig.infos["world_load"], "blocked");
    assert!(
        !saves.join("mcr-probe-load").join("level.dat").exists(),
        "an integrated server saved the world"
    );

    assert_eq!(l.rig.detached, None);
    let outcome = l.rig.bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(!matches!(outcome, ShutdownOutcome::Crashed), "{outcome:?}");
}
