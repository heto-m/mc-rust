mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::harness::{Harness, SIZE, TestView, send};
use mcr_ui::{
    ModIcon, ModRow, ModTone, ModsModel, ModsScreen, NoView, Screen, ScreenAction, SharedMods,
    UiInput, crash_fallback_screen,
};

struct Memory {
    rows: Vec<ModRow>,
    heap: u32,
    running: Vec<(String, bool)>,
    opened: u32,
}

impl Memory {
    fn new(rows: Vec<ModRow>) -> Self {
        let running = rows.iter().map(|r| (r.id.clone(), r.enabled)).collect();
        Self {
            rows,
            heap: 2048,
            running,
            opened: 0,
        }
    }
}

impl ModsModel for Memory {
    fn refresh(&mut self) -> Vec<ModRow> {
        self.rows.clone()
    }

    fn set_enabled(&mut self, id: &str, on: bool) {
        if let Some(row) = self.rows.iter_mut().find(|r| r.id == id && r.toggleable) {
            row.enabled = on;
            row.badge = if on { "Active" } else { "Disabled" }.into();
            row.tone = if on { ModTone::Good } else { ModTone::Neutral };
        }
    }

    fn heap_mib(&self) -> u32 {
        self.heap
    }

    fn set_heap_mib(&mut self, mib: u32) {
        self.heap = mib;
    }

    fn heap_range(&self) -> (u32, u32) {
        (1024, 4096)
    }

    fn restart_required(&self) -> bool {
        self.rows
            .iter()
            .zip(&self.running)
            .any(|(r, (_, was))| r.enabled != *was)
    }

    fn notice(&self) -> Option<String> {
        Some("Last session: Mod `broken` failed in init".into())
    }

    fn open_folder(&mut self) {
        self.opened += 1;
    }

    fn folder_label(&self) -> String {
        "run/client/mods".into()
    }
}

fn row(id: &str, name: &str, badge: &str, tone: ModTone, reason: &str, enabled: bool) -> ModRow {
    ModRow {
        id: id.into(),
        name: name.into(),
        version: "1.0.0".into(),
        description: format!("The {name} mod does useful things in the game."),
        badge: badge.into(),
        tone,
        reason: reason.into(),
        enabled,
        toggleable: !matches!(tone, ModTone::Bad),
        icon: None,
    }
}

fn icon(id: u64, color: [u8; 4]) -> Rc<ModIcon> {
    Rc::new(ModIcon {
        id,
        width: 16,
        height: 16,
        rgba: (0..256).flat_map(|_| color).collect(),
    })
}

fn rows() -> Vec<ModRow> {
    let mut api = row(
        "fabric-api",
        "Fabric API",
        "Active",
        ModTone::Good,
        "",
        true,
    );
    api.icon = Some(icon(1, [60, 120, 220, 255]));
    let mut menu = row("modmenu", "Mod Menu", "Active", ModTone::Good, "", true);
    menu.icon = Some(icon(2, [220, 120, 60, 255]));
    vec![
        api,
        menu,
        row(
            "sodium",
            "Sodium",
            "Blocked",
            ModTone::Bad,
            "replaces the OpenGL renderer; mcr renders terrain natively",
            false,
        ),
        row(
            "neo.jar",
            "neo",
            "Not supported",
            ModTone::Bad,
            "NeoForge mods are not supported yet",
            false,
        ),
        row(
            "needy",
            "Needy",
            "Not loaded",
            ModTone::Bad,
            "missing dependency `foo`",
            true,
        ),
        row(
            "freecam",
            "Freecam",
            "Unsupported",
            ModTone::Warn,
            "camera is owned by mcr",
            false,
        ),
    ]
}

fn shared(rows: Vec<ModRow>) -> (Rc<RefCell<Memory>>, SharedMods) {
    let memory = Rc::new(RefCell::new(Memory::new(rows)));
    let model: SharedMods = memory.clone();
    (memory, model)
}

#[test]
fn mods_screen_shot_lists_all_rows_with_status() {
    let Some(mut h) = Harness::new() else { return };
    let (_, model) = shared(rows());
    let mut screen = ModsScreen::new(model);
    h.shot(
        "screen_mods.png",
        SIZE,
        &mut screen,
        &NoView,
        (300.0, 120.0),
    );
    assert_eq!(screen.rows().len(), 6);
}

#[test]
fn mods_screen_shot_with_restart_banner_and_empty_list() {
    let Some(mut h) = Harness::new() else { return };
    let (memory, model) = shared(rows());
    memory.borrow_mut().set_enabled("modmenu", false);
    let mut screen = ModsScreen::new(model);
    h.shot(
        "screen_mods_restart.png",
        SIZE,
        &mut screen,
        &TestView::default(),
        (0.0, 0.0),
    );
    assert!(memory.borrow().restart_required());
    let (_, empty) = shared(Vec::new());
    h.shot(
        "screen_mods_empty.png",
        SIZE,
        &mut ModsScreen::new(empty),
        &NoView,
        (0.0, 0.0),
    );
}

fn click(x: f32, y: f32) -> Vec<UiInput> {
    vec![
        UiInput::MouseMove { x, y },
        UiInput::press(x, y, mcr_ui::MouseButton::Left),
        UiInput::release(x, y, mcr_ui::MouseButton::Left),
    ]
}

// Geometry at a 427x240 GUI: rows start at x=73, y=44; the toggle is 30 px wide at the right edge.
const FIRST_TOGGLE: (f32, f32) = (330.0, 60.0);
const ROW_STEP: f32 = 36.0;

#[test]
fn clicking_a_toggle_flips_the_mod_and_raises_the_restart_banner() {
    let (memory, model) = shared(rows());
    let mut screen = ModsScreen::new(model);
    screen.init(427, 240);
    assert!(!memory.borrow().restart_required());
    send(
        &mut screen,
        &NoView,
        &click(FIRST_TOGGLE.0, FIRST_TOGGLE.1 + ROW_STEP),
    );
    assert!(!memory.borrow().rows[1].enabled);
    assert!(memory.borrow().restart_required());
    assert!(!screen.rows()[1].enabled);
    send(
        &mut screen,
        &NoView,
        &click(FIRST_TOGGLE.0, FIRST_TOGGLE.1 + ROW_STEP),
    );
    assert!(memory.borrow().rows[1].enabled);
    assert!(!memory.borrow().restart_required());
}

#[test]
fn blocked_mods_have_no_working_toggle() {
    let (memory, model) = shared(rows());
    let mut screen = ModsScreen::new(model);
    screen.init(427, 240);
    send(
        &mut screen,
        &NoView,
        &click(FIRST_TOGGLE.0, FIRST_TOGGLE.1 + 2.0 * ROW_STEP),
    );
    assert!(!memory.borrow().rows[2].enabled);
    assert!(!memory.borrow().restart_required());
}

#[test]
fn heap_slider_snaps_to_steps_inside_the_range() {
    let (memory, model) = shared(rows());
    let mut screen = ModsScreen::new(model);
    screen.init(427, 240);
    let y = 240.0 - 56.0 + 6.0 + 10.0;
    send(&mut screen, &NoView, &click(58.0, y));
    assert_eq!(memory.borrow().heap, 1024);
    send(&mut screen, &NoView, &click(207.0, y));
    assert_eq!(memory.borrow().heap, 4096);
    send(&mut screen, &NoView, &click(133.0, y));
    let mib = memory.borrow().heap;
    assert!((1024..=4096).contains(&mib) && mib % 128 == 0, "{mib}");
}

#[test]
fn folder_button_and_done_report_through_the_model_and_actions() {
    let (memory, model) = shared(rows());
    let mut screen = ModsScreen::new(model);
    screen.init(427, 240);
    let y = 240.0 - 56.0 + 6.0 + 10.0;
    send(&mut screen, &NoView, &click(290.0, y));
    assert_eq!(memory.borrow().opened, 1);
    let out = send(&mut screen, &NoView, &click(213.0, y + 24.0));
    assert!(out.iter().any(|a| matches!(a, ScreenAction::Pop)));
    let out = send(&mut screen, &NoView, &[UiInput::key(mcr_ui::Key::Escape)]);
    assert!(matches!(out.as_slice(), [ScreenAction::Pop]));
}

#[test]
fn a_thousand_rows_stay_bounded() {
    let many = (0..5000)
        .map(|i| row(&format!("m{i}"), "Many", "Active", ModTone::Good, "", true))
        .collect();
    let (_, model) = shared(many);
    assert_eq!(ModsScreen::new(model).rows().len(), 1024);
}

#[test]
fn crash_fallback_prompt_shows_the_report_path() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = crash_fallback_screen(
        "Mod `broken` failed in init.",
        Some("run/client/java/crash-reports/crash-2026-10-12.txt"),
        Box::new(|| vec![ScreenAction::Pop]),
        Box::new(|| vec![ScreenAction::Quit]),
        Some(Box::new(Vec::new)),
    );
    h.shot(
        "screen_mods_crash.png",
        SIZE,
        &mut screen,
        &NoView,
        (0.0, 0.0),
    );
}
