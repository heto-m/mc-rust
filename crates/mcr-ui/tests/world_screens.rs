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
const EDIT_BUTTON: (f32, f32) = (213.0 - 154.0 + 49.0, (H - 28 + 10) as f32);
const DELETE_BUTTON: (f32, f32) = (213.0 - 154.0 + 103.0 + 49.0, (H - 28 + 10) as f32);

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
