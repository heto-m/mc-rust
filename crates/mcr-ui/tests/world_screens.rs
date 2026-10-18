mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::harness::{send, typed};
use mcr_ui::{
    CreateWorldScreen, Key, MemoryServerStore, MemoryWorldStore, MouseButton, NewGameMode, NoView,
    Screen, ScreenAction, SelectWorldScreen, SharedWorlds, TitleScreen, UiInput, WorldEntry,
    WorldPreset,
};

const W: i32 = 427;
const H: i32 = 240;

fn click(x: f32, y: f32) -> Vec<UiInput> {
    vec![
        UiInput::press(x, y, MouseButton::Left),
        UiInput::release(x, y, MouseButton::Left),
    ]
}

fn store() -> Rc<RefCell<MemoryWorldStore>> {
    Rc::new(RefCell::new(MemoryWorldStore {
        entries: ["Alpha", "Beta", "Gamma"]
            .iter()
            .map(|n| WorldEntry {
                folder: n.to_lowercase(),
                name: (*n).into(),
                played: "2026-10-17 21:00".into(),
                mode_key: "gameMode.survival".into(),
                version: "26.3".into(),
                ..WorldEntry::default()
            })
            .collect(),
    }))
}

fn list(store: &Rc<RefCell<MemoryWorldStore>>) -> SelectWorldScreen {
    let shared: SharedWorlds = store.clone();
    let mut screen = SelectWorldScreen::new(shared);
    screen.init(W, H);
    screen
}

/// Middle of a row's text, away from the play icon.
fn row(index: usize) -> (f32, f32) {
    (213.0 + 40.0, (33 + 4 + index as i32 * 36 + 18) as f32)
}

fn played(out: &[ScreenAction]) -> Option<&str> {
    out.iter().find_map(|a| match a {
        ScreenAction::PlayWorld(f) => Some(f.as_str()),
        _ => None,
    })
}

fn pushed(out: Vec<ScreenAction>) -> Option<Box<dyn Screen>> {
    out.into_iter().find_map(|a| match a {
        ScreenAction::Push(s) => Some(s),
        _ => None,
    })
}

const PLAY_BUTTON: (f32, f32) = (213.0 - 154.0 + 75.0, (H - 52 + 10) as f32);
const EDIT_BUTTON: (f32, f32) = (213.0 - 150.0 + 36.0, (H - 28 + 10) as f32);
const DELETE_BUTTON: (f32, f32) = (213.0 - 150.0 + 76.0 + 36.0, (H - 28 + 10) as f32);
const RECREATE_BUTTON: (f32, f32) = (213.0 - 150.0 + 152.0 + 36.0, (H - 28 + 10) as f32);

#[test]
fn play_needs_a_selection() {
    let s = store();
    let mut screen = list(&s);
    assert!(
        played(&send(
            &mut screen,
            &NoView,
            &click(PLAY_BUTTON.0, PLAY_BUTTON.1)
        ))
        .is_none()
    );
    let (x, y) = row(1);
    assert!(played(&send(&mut screen, &NoView, &click(x, y))).is_none());
    assert_eq!(screen.selected(), Some(1));
    let out = send(&mut screen, &NoView, &click(PLAY_BUTTON.0, PLAY_BUTTON.1));
    assert_eq!(played(&out), Some("beta"));
}

#[test]
fn double_click_enter_and_icon_play() {
    let s = store();
    let mut screen = list(&s);
    let (x, y) = row(2);
    send(&mut screen, &NoView, &click(x, y));
    let double = UiInput::MousePress {
        x,
        y,
        button: MouseButton::Left,
        mods: mcr_ui::Modifiers::NONE,
        double: true,
    };
    assert_eq!(
        played(&send(&mut screen, &NoView, &[double])),
        Some("gamma")
    );
    assert_eq!(
        played(&send(&mut screen, &NoView, &[UiInput::key(Key::Enter)])),
        Some("gamma")
    );
    let icon = (213.0 - 135.0 + 10.0, row(0).1);
    assert_eq!(
        played(&send(&mut screen, &NoView, &click(icon.0, icon.1))),
        Some("alpha")
    );
}

#[test]
fn delete_asks_then_removes() {
    let s = store();
    let mut screen = list(&s);
    let (x, y) = row(0);
    send(&mut screen, &NoView, &click(x, y));
    let mut confirm = pushed(send(
        &mut screen,
        &NoView,
        &click(DELETE_BUTTON.0, DELETE_BUTTON.1),
    ))
    .expect("confirmation");
    confirm.init(W, H);
    assert_eq!(
        s.borrow().entries.len(),
        3,
        "nothing deleted before confirming"
    );
    let cancel = send(&mut *confirm, &NoView, &[UiInput::key(Key::Escape)]);
    assert!(cancel.iter().any(|a| matches!(a, ScreenAction::Pop)));
    assert_eq!(s.borrow().entries.len(), 3);
    let out = send(&mut *confirm, &NoView, &click(58.0 + 75.0, 146.0));
    assert!(out.iter().any(|a| matches!(a, ScreenAction::Pop)));
    let left: Vec<String> = s
        .borrow()
        .entries
        .iter()
        .map(|e| e.folder.clone())
        .collect();
    assert_eq!(left, ["beta", "gamma"]);
}

#[test]
fn edit_renames() {
    let s = store();
    let mut screen = list(&s);
    let (x, y) = row(1);
    send(&mut screen, &NoView, &click(x, y));
    let mut edit = pushed(send(
        &mut screen,
        &NoView,
        &click(EDIT_BUTTON.0, EDIT_BUTTON.1),
    ))
    .expect("edit screen");
    edit.init(W, H);
    let mut keys = vec![UiInput::Key {
        key: Key::Letter('a'),
        mods: mcr_ui::Modifiers::CTRL,
        repeat: false,
    }];
    keys.extend(typed("Renamed"));
    keys.push(UiInput::key(Key::Enter));
    let out = send(&mut *edit, &NoView, &keys);
    assert!(out.iter().any(|a| matches!(a, ScreenAction::Pop)));
    assert_eq!(s.borrow().entries[1].name, "Renamed");
}

fn created(out: &[ScreenAction]) -> Option<mcr_ui::CreateWorldSettings> {
    out.iter().find_map(|a| match a {
        ScreenAction::CreateWorld(s) => Some((**s).clone()),
        _ => None,
    })
}

#[test]
fn create_world_defaults_and_choices() {
    let mut screen = CreateWorldScreen::new();
    screen.init(W, H);
    let create = (213.0 - 155.0 + 75.0, (H - 28 + 10) as f32);
    let out = send(&mut screen, &NoView, &click(create.0, create.1));
    let s = created(&out).expect("create action");
    assert_eq!(s.name, "New World");
    assert_eq!(s.mode, NewGameMode::Survival);
    assert_eq!(s.difficulty, 2);
    assert!(!s.cheats);
    assert!(s.structures);
    assert!(!s.bonus_chest);

    let left = 213.0 - 155.0 + 75.0;
    let right = 213.0 + 5.0 + 75.0;
    send(&mut screen, &NoView, &click(left, 72.0 + 10.0));
    send(&mut screen, &NoView, &click(left, 72.0 + 10.0));
    assert_eq!(screen.settings().mode, NewGameMode::Creative);
    assert!(screen.settings().cheats, "creative turns cheats on");
    send(&mut screen, &NoView, &click(right, 72.0 + 10.0));
    send(&mut screen, &NoView, &click(right, 120.0 + 10.0));
    send(&mut screen, &NoView, &click(left, 160.0 + 10.0));
    send(&mut screen, &NoView, &typed("1234"));
    let s = created(&send(&mut screen, &NoView, &click(create.0, create.1))).unwrap();
    assert_eq!(s.preset, WorldPreset::Flat);
    assert!(s.bonus_chest);
    assert_eq!(s.seed, "1234");
}

#[test]
fn hardcore_locks_difficulty_and_cheats() {
    let mut screen = CreateWorldScreen::new();
    screen.init(W, H);
    let left = 213.0 - 155.0 + 75.0;
    send(&mut screen, &NoView, &click(left, 72.0 + 10.0));
    assert_eq!(screen.settings().mode, NewGameMode::Hardcore);
    send(&mut screen, &NoView, &click(left, 120.0 + 10.0));
    let s = screen.result();
    assert_eq!(s.difficulty, 3);
    assert!(!s.cheats);
}

#[test]
fn title_singleplayer_opens_the_world_list() {
    let servers = Rc::new(RefCell::new(MemoryServerStore::default()));
    let button = (213.0, (H / 4 + 48 + 10) as f32);
    let mut plain = TitleScreen::new("26.3", servers.clone());
    plain.init(W, H);
    assert!(pushed(send(&mut plain, &NoView, &click(button.0, button.1))).is_none());
    let worlds: SharedWorlds = store();
    let mut title = TitleScreen::new("26.3", servers).with_worlds(worlds);
    title.init(W, H);
    assert!(pushed(send(&mut title, &NoView, &click(button.0, button.1))).is_some());
}

#[test]
fn local_pause_screen_pauses() {
    let mut stack = mcr_ui::ScreenStack::new(W, H);
    stack.push(Box::new(mcr_ui::PauseScreen::new(true, true)));
    stack.push(Box::new(CreateWorldScreen::new()));
    assert!(!stack.pauses_game());
    assert!(stack.any_pauses_game());
    let mut remote = mcr_ui::ScreenStack::new(W, H);
    remote.push(Box::new(mcr_ui::PauseScreen::new(false, false)));
    assert!(!remote.any_pauses_game());
}

struct RecreateStore(MemoryWorldStore);

impl mcr_ui::WorldStore for RecreateStore {
    fn worlds(&self) -> &[WorldEntry] {
        self.0.worlds()
    }
    fn refresh(&mut self) {}
    fn rename(&mut self, folder: &str, name: &str) -> Result<(), String> {
        self.0.rename(folder, name)
    }
    fn delete(&mut self, folder: &str) -> Result<(), String> {
        self.0.delete(folder)
    }
    fn recreate(&self, folder: &str) -> Result<mcr_ui::CreateWorldSettings, String> {
        if folder == "gamma" {
            return Err("Single biome worlds cannot be re-created by mcr".into());
        }
        Ok(mcr_ui::CreateWorldSettings {
            name: "Beta".into(),
            preset: WorldPreset::Amplified,
            seed: "42".into(),
            game_rules: vec![("keep_inventory".into(), "true".into())],
            ..mcr_ui::CreateWorldSettings::default()
        })
    }
}

#[test]
fn recreate_fills_create_new_world() {
    let entries = store().borrow().entries.clone();
    let shared: SharedWorlds = Rc::new(RefCell::new(RecreateStore(MemoryWorldStore { entries })));
    let mut screen = SelectWorldScreen::new(shared);
    screen.init(W, H);
    let (x, y) = row(1);
    send(&mut screen, &NoView, &click(x, y));
    let mut create = pushed(send(
        &mut screen,
        &NoView,
        &click(RECREATE_BUTTON.0, RECREATE_BUTTON.1),
    ))
    .expect("create screen");
    create.init(W, H);
    let button = (213.0 - 155.0 + 75.0, (H - 28 + 10) as f32);
    let s = created(&send(&mut *create, &NoView, &click(button.0, button.1))).unwrap();
    assert_eq!((s.name.as_str(), s.seed.as_str()), ("Beta", "42"));
    assert_eq!(s.preset, WorldPreset::Amplified);
    assert_eq!(
        s.game_rules,
        [("keep_inventory".to_owned(), "true".to_owned())]
    );

    let (x, y) = row(2);
    send(&mut screen, &NoView, &click(x, y));
    let mut refused = pushed(send(
        &mut screen,
        &NoView,
        &click(RECREATE_BUTTON.0, RECREATE_BUTTON.1),
    ))
    .expect("message");
    refused.init(W, H);
    assert!(
        created(&send(&mut *refused, &NoView, &[UiInput::key(Key::Enter)])).is_none(),
        "no create screen for a refused world"
    );
}

#[test]
fn game_rules_edit_toggles_and_bounded_numbers() {
    let mut create = CreateWorldScreen::new();
    create.init(W, H);
    let mut rules = pushed(send(
        &mut create,
        &NoView,
        &click(213.0 - 155.0 + 50.0, 196.0),
    ))
    .expect("game rules screen");
    let tall = 800;
    rules.init(W, tall);
    let control = |index: i32| (213.0 + 5.0 + 75.0, (32 + index * 24 + 2 + 10) as f32);
    let done = (213.0 - 155.0 + 75.0, (tall - 28 + 10) as f32);
    let (x, y) = control(1);
    send(&mut *rules, &NoView, &click(x, y));
    let (x, y) = control(13);
    send(&mut *rules, &NoView, &click(x, y));
    send(&mut *rules, &NoView, &typed("-5"));
    let out = send(&mut *rules, &NoView, &click(done.0, done.1));
    assert!(
        !out.iter().any(|a| matches!(a, ScreenAction::Pop)),
        "Done is inactive while a number is out of bounds"
    );
    let mut fix = vec![UiInput::key(Key::Backspace); 4];
    fix.extend(typed("7"));
    send(&mut *rules, &NoView, &fix);
    let out = send(&mut *rules, &NoView, &click(done.0, done.1));
    assert!(out.iter().any(|a| matches!(a, ScreenAction::Pop)));
    let s = create.result();
    let mut got = s.game_rules.clone();
    got.sort();
    assert_eq!(
        got,
        [
            ("drowning_damage".to_owned(), "false".to_owned()),
            (
                "players_nether_portal_creative_delay".to_owned(),
                "7".to_owned()
            )
        ]
    );
}

#[test]
fn superflat_layers_only_for_superflat() {
    let mut create = CreateWorldScreen::new();
    create.init(W, H);
    let customize = (213.0 - 155.0 + 210.0 + 50.0, 196.0);
    assert!(
        pushed(send(&mut create, &NoView, &click(customize.0, customize.1))).is_none(),
        "Customize is inactive for the default type"
    );
    send(&mut create, &NoView, &click(213.0 + 5.0 + 75.0, 82.0));
    let mut layers = pushed(send(&mut create, &NoView, &click(customize.0, customize.1)))
        .expect("superflat screen");
    layers.init(W, H);
    let mut keys = vec![UiInput::Key {
        key: Key::Letter('a'),
        mods: mcr_ui::Modifiers::CTRL,
        repeat: false,
    }];
    keys.extend(typed(
        "minecraft:bedrock,3*minecraft:stone;minecraft:desert",
    ));
    keys.push(UiInput::key(Key::Enter));
    let out = send(&mut *layers, &NoView, &keys);
    assert!(out.iter().any(|a| matches!(a, ScreenAction::Pop)));
    assert_eq!(
        create.result().flat.as_deref(),
        Some("minecraft:bedrock,3*minecraft:stone;minecraft:desert")
    );
}

#[test]
fn pause_offers_open_to_lan_in_local_worlds() {
    let mut pause = mcr_ui::PauseScreen::new(true, true).with_lan(true);
    pause.init(W, H);
    let top = (H - 142) / 4;
    let out = send(
        &mut pause,
        &NoView,
        &click((213 - 102 + 106 + 49) as f32, (top + 98 + 10) as f32),
    );
    assert!(out.iter().any(|a| matches!(a, ScreenAction::OpenLan)));
    let mut remote = mcr_ui::PauseScreen::new(false, false);
    remote.init(W, H);
    let out = send(
        &mut remote,
        &NoView,
        &click((213 - 102 + 106 + 49) as f32, (top + 98 + 10) as f32),
    );
    assert!(out.iter().any(|a| matches!(a, ScreenAction::OpenOptions)));
}

#[test]
fn lan_screen_validates_the_port() {
    let mut lan =
        mcr_ui::ShareToLanScreen::new(0, false, false).with_port_check(Box::new(|p| p != 25602));
    lan.init(W, H);
    send(&mut lan, &NoView, &click(213.0 - 155.0 + 75.0, 110.0));
    send(&mut lan, &NoView, &click(213.0 + 5.0 + 75.0, 110.0));
    let start = (213.0 - 155.0 + 75.0, (H - 28 + 10) as f32);
    send(&mut lan, &NoView, &click(213.0, 170.0));
    send(&mut lan, &NoView, &typed("80"));
    let started = |out: &[ScreenAction]| {
        out.iter().find_map(|a| match a {
            ScreenAction::StartLan(s) => Some(*s),
            _ => None,
        })
    };
    assert!(started(&send(&mut lan, &NoView, &click(start.0, start.1))).is_none());
    send(&mut lan, &NoView, &click(213.0, 170.0));
    let mut retype = vec![UiInput::key(Key::Backspace); 2];
    retype.extend(typed("25602"));
    send(&mut lan, &NoView, &retype);
    assert!(
        started(&send(&mut lan, &NoView, &click(start.0, start.1))).is_none(),
        "unavailable port"
    );
    send(&mut lan, &NoView, &click(213.0, 170.0));
    send(&mut lan, &NoView, &[UiInput::key(Key::Backspace)]);
    send(&mut lan, &NoView, &typed("1x"));
    let s = started(&send(&mut lan, &NoView, &click(start.0, start.1))).expect("started");
    assert_eq!(
        s,
        mcr_ui::LanSettings {
            mode: 1,
            cheats: true,
            port: Some(25601)
        }
    );
    let hardcore = mcr_ui::ShareToLanScreen::new(1, true, true);
    assert_eq!(
        (hardcore.settings().mode, hardcore.settings().cheats),
        (0, false)
    );
}
