//! `GameModeSwitcherScreen` (F3+F4): F4 cycles the modes, releasing F3 switches to the selected one.

use std::cell::Cell;
use std::rc::Rc;

use mcr_protocol::types::game::GameType;
use mcr_protocol::types::item::ItemStack;
use mcr_ui::{Env, Key, MouseButton, RenderCx, Screen, ScreenAction, UiInput, contains};

const MODES: [(GameType, &str, &str); 4] = [
    (GameType::Creative, "gameMode.creative", "grass_block"),
    (GameType::Survival, "gameMode.survival", "iron_sword"),
    (
        GameType::Adventure,
        "gameMode.adventure",
        "buried_treasure_map",
    ),
    (GameType::Spectator, "gameMode.spectator", "ender_eye"),
];
const SLOT: i32 = 26;
const SLOT_PADDED: i32 = 31;

/// Where the chosen mode goes once the screen closes.
pub type Choice = Rc<Cell<Option<GameType>>>;

#[derive(Debug)]
pub struct GameModeSwitcher {
    current: GameType,
    selected: usize,
    next_key: Option<Key>,
    next_label: String,
    modifier: Option<Key>,
    choice: Choice,
    gui: (i32, i32),
    first_mouse: Option<(f32, f32)>,
}

fn index_of(mode: GameType) -> usize {
    MODES.iter().position(|m| m.0 == mode).unwrap_or(0)
}

impl GameModeSwitcher {
    /// Starts on the previous mode, or creative/survival when there is none.
    pub fn new(
        current: GameType,
        previous: Option<GameType>,
        (next_key, next_label): (Option<Key>, String),
        modifier: Option<Key>,
        choice: Choice,
    ) -> Self {
        let start = previous.unwrap_or(if current == GameType::Creative {
            GameType::Survival
        } else {
            GameType::Creative
        });
        Self {
            current,
            selected: index_of(start),
            next_key,
            next_label,
            modifier,
            choice,
            gui: (0, 0),
            first_mouse: None,
        }
    }

    fn slot_rect(&self, i: usize) -> [i32; 4] {
        let all = MODES.len() as i32 * SLOT_PADDED - 5;
        [
            self.gui.0 / 2 - all / 2 + i as i32 * SLOT_PADDED,
            self.gui.1 / 2 - SLOT_PADDED,
            SLOT,
            SLOT,
        ]
    }

    fn switch(&self, out: &mut Vec<ScreenAction>) {
        let mode = MODES[self.selected].0;
        if mode != self.current {
            self.choice.set(Some(mode));
        }
        out.push(ScreenAction::Pop);
    }
}

impl Screen for GameModeSwitcher {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        let (w, h) = self.gui;
        cx.texture(
            "container/gamemode_switcher",
            (0, 0),
            (w / 2 - 62, h / 2 - SLOT_PADDED - 27, 125, 75),
        );
        let first = *self.first_mouse.get_or_insert(cx.mouse);
        let moved = first != cx.mouse;
        for i in 0..MODES.len() {
            let rect = self.slot_rect(i);
            if moved && contains(rect, cx.mouse.0, cx.mouse.1) {
                self.selected = i;
            }
        }
        let name = cx.lang(MODES[self.selected].1);
        cx.centered_text(&name, w / 2, h / 2 - SLOT_PADDED - 20, 0xFFFF_FFFF, true);
        let key = format!("§b{}§r", self.next_label);
        let select = cx.lang_fmt("debug.gamemodes.select_next", &[&key]);
        cx.centered_text(&select, w / 2, h / 2 + 5, 0xFFFF_FFFF, true);
        for (i, (_, _, icon)) in MODES.iter().enumerate() {
            let [x, y, sw, sh] = self.slot_rect(i);
            cx.sprite("gamemode_switcher/slot", x, y, sw, sh);
            if i == self.selected {
                cx.sprite("gamemode_switcher/selection", x, y, sw, sh);
            }
            if let Some(item) = mcr_data::v26_3().item_by_name(&format!("minecraft:{icon}"))
                && let Ok(item) = i32::try_from(item)
            {
                cx.fake_item(&ItemStack::new(item, 1), x + 5, y + 5);
            }
        }
    }

    fn input(&mut self, _env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        match *input {
            UiInput::Key { key, .. } if Some(key) == self.next_key => {
                self.first_mouse = None;
                self.selected = (self.selected + 1) % MODES.len();
            }
            UiInput::Key {
                key: Key::Escape, ..
            } => out.push(ScreenAction::Pop),
            UiInput::MousePress {
                x,
                y,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(i) = (0..MODES.len()).find(|&i| contains(self.slot_rect(i), x, y)) {
                    self.selected = i;
                }
            }
            _ => {}
        }
        true
    }

    fn key_released(&mut self, key: Key, out: &mut Vec<ScreenAction>) -> bool {
        if Some(key) != self.modifier {
            return false;
        }
        self.switch(out);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_on_the_previous_mode_and_switches_on_release() {
        let choice = Choice::default();
        let mut s = GameModeSwitcher::new(
            GameType::Survival,
            None,
            (Some(Key::Function(4)), "F4".into()),
            Some(Key::Function(3)),
            choice.clone(),
        );
        assert_eq!(MODES[s.selected].0, GameType::Creative);
        s.selected = (s.selected + 1) % MODES.len();
        let mut out = Vec::new();
        assert!(!s.key_released(Key::Letter('a'), &mut out));
        assert!(s.key_released(Key::Function(3), &mut out));
        assert_eq!(choice.get(), None, "survival is already the mode");
        s.selected = 3;
        s.key_released(Key::Function(3), &mut out);
        assert_eq!(choice.get(), Some(GameType::Spectator));
    }
}
