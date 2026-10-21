//! Vanilla's `WorldOptionsScreen`: difficulty, game rules, game modes, allow commands and LAN.

use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, UiInput, contains};
use super::lan_screen::{LanSettings, PortCheck};
use super::menus::{WHITE, draw_dim_background};
use super::screen::{Screen, ScreenAction};
use super::widgets::{Button, CycleButton, WidgetEvent, WidgetEventKind, WidgetSet};

const DEFAULT_MODE: u32 = 1;
const PERSONAL_MODE: u32 = 2;
const ALLOW_COMMANDS: u32 = 3;
const DIFFICULTY: u32 = 4;
const LOCK: u32 = 5;
const GAME_RULES: u32 = 6;
const LAN: u32 = 7;
const PORT: u32 = 8;
const APPLY: u32 = 9;
const CANCEL: u32 = 10;
const MIN_PORT: u16 = 1024;
const ERROR: u32 = 0xFFFF_5555;
const HEADER: &str = "§n§l";

const MODES: [&str; 4] = [
    "lang:selectWorld.gameMode.survival",
    "lang:selectWorld.gameMode.creative",
    "lang:selectWorld.gameMode.adventure",
    "lang:selectWorld.gameMode.spectator",
];
const DIFFICULTIES: [&str; 4] = [
    "lang:options.difficulty.peaceful",
    "lang:options.difficulty.easy",
    "lang:options.difficulty.normal",
    "lang:options.difficulty.hard",
];

/// What the screen shows when it opens; modes are ids 0 survival to 3 spectator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldOptionsInit {
    pub difficulty: u8,
    pub difficulty_locked: bool,
    pub hardcore: bool,
    pub gamemaster: bool,
    pub local: Option<LocalWorldOptions>,
}

/// The extra rows of a world hosted by this client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalWorldOptions {
    pub default_mode: u8,
    pub personal_mode: u8,
    pub allow_commands: bool,
    pub lan_open: bool,
}

/// The changes Apply Changes sends; `None` fields stayed as they were.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldOptionsChange {
    pub difficulty: Option<u8>,
    pub lock_difficulty: bool,
    pub default_mode: Option<u8>,
    pub personal_mode: Option<u8>,
    pub allow_commands: Option<bool>,
    pub lan: Option<LanSettings>,
}

pub struct WorldOptionsScreen {
    widgets: WidgetSet,
    gui: (i32, i32),
    mouse: (f32, f32),
    init: WorldOptionsInit,
    wanted: Wanted,
    check: Option<PortCheck>,
    port: Option<u16>,
    port_error: Option<&'static str>,
}

#[derive(Clone, Copy, Debug)]
struct Wanted {
    difficulty: u8,
    locked: bool,
    default_mode: u8,
    personal_mode: u8,
    allow_commands: bool,
    lan: bool,
}

impl std::fmt::Debug for WorldOptionsScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldOptionsScreen")
            .field("wanted", &self.wanted)
            .finish_non_exhaustive()
    }
}

impl WorldOptionsScreen {
    pub fn new(init: WorldOptionsInit) -> Self {
        let local = init.local;
        Self {
            widgets: WidgetSet::default(),
            gui: (0, 0),
            mouse: (0.0, 0.0),
            wanted: Wanted {
                difficulty: init.difficulty.min(3),
                locked: init.difficulty_locked,
                default_mode: local.map_or(0, |l| l.default_mode.min(3)),
                personal_mode: local.map_or(0, |l| l.personal_mode.min(3)),
                allow_commands: local.is_some_and(|l| l.allow_commands),
                lan: local.is_some_and(|l| l.lan_open),
            },
            init,
            check: None,
            port: None,
            port_error: None,
        }
    }

    pub fn with_port_check(mut self, check: PortCheck) -> Self {
        self.check = Some(check);
        self
    }

    /// The changes Apply Changes would send.
    pub fn change(&self) -> WorldOptionsChange {
        let w = &self.wanted;
        let init = &self.init;
        let mut change = WorldOptionsChange {
            difficulty: (w.difficulty != init.difficulty.min(3)).then_some(w.difficulty),
            lock_difficulty: w.locked && !init.difficulty_locked,
            ..WorldOptionsChange::default()
        };
        if let Some(local) = init.local {
            change.default_mode =
                (w.default_mode != local.default_mode.min(3)).then_some(w.default_mode);
            change.personal_mode =
                (w.personal_mode != local.personal_mode.min(3)).then_some(w.personal_mode);
            change.allow_commands =
                (w.allow_commands != local.allow_commands).then_some(w.allow_commands);
            if w.lan && !local.lan_open {
                change.lan = Some(LanSettings {
                    mode: w.default_mode,
                    cheats: w.allow_commands,
                    port: self.port,
                });
            }
        }
        change
    }

    fn has_changes(&self) -> bool {
        let c = self.change();
        c != WorldOptionsChange::default() && self.port_error.is_none()
    }

    /// Whether the player may use commands here: the host's allow commands, else gamemaster rights.
    fn commands_allowed(&self) -> bool {
        match self.init.local {
            Some(_) => self.wanted.allow_commands,
            None => self.init.gamemaster,
        }
    }

    fn difficulty_editable(&self) -> bool {
        !self.init.hardcore && !self.init.difficulty_locked && self.commands_allowed()
    }

    fn sync(&mut self) {
        let allowed = self.commands_allowed() && !self.init.hardcore;
        let difficulty = self.difficulty_editable();
        let apply = self.has_changes();
        let port_error = self.port_error.is_some();
        let w = &mut self.widgets;
        for id in [DEFAULT_MODE, PERSONAL_MODE] {
            if let Some(c) = w.cycle_mut(id) {
                c.active = allowed;
            }
        }
        if let Some(c) = w.cycle_mut(DIFFICULTY) {
            c.active = difficulty;
        }
        if let Some(b) = w.button_mut(LOCK) {
            b.active = difficulty && !self.wanted.locked;
        }
        if let Some(b) = w.button_mut(GAME_RULES) {
            b.active = allowed;
        }
        if let Some(c) = w.cycle_mut(LAN) {
            c.active = self.init.local.is_some_and(|l| !l.lan_open);
        }
        if let Some(e) = w.edit_mut(PORT) {
            e.text_color = if port_error { ERROR } else { 0xFFE0_E0E0 };
        }
        if let Some(b) = w.button_mut(APPLY) {
            b.active = apply;
        }
    }

    fn read_port(&mut self) {
        let text = self
            .widgets
            .edit(PORT)
            .map(|e| e.value().trim().to_owned())
            .unwrap_or_default();
        (self.port, self.port_error) = if text.is_empty() || !self.wanted.lan {
            (None, None)
        } else {
            match text.parse::<u16>().ok().filter(|p| *p >= MIN_PORT) {
                None => (None, Some("lanServer.port.invalid")),
                Some(p) if self.check.as_ref().is_some_and(|c| !c(p)) => {
                    (None, Some("lanServer.port.unavailable"))
                }
                Some(p) => (Some(p), None),
            }
        };
    }

    fn lock_rect(&self) -> [i32; 4] {
        let [x, y, w, _] = self.widgets.find(DIFFICULTY).map_or([0; 4], |d| d.rect());
        [x + w + 4, y, 20, 20]
    }
}

impl Screen for WorldOptionsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        let keep = self.widgets.edit(PORT).map(|e| e.value().to_owned());
        self.widgets.clear();
        let left = width / 2 - 155;
        let right = width / 2 + 5;
        let mut y = 46;
        if self.init.local.is_some() {
            for (id, key, value) in [
                (
                    DEFAULT_MODE,
                    "lang:options.worldOptions.game_mode",
                    self.wanted.default_mode,
                ),
                (
                    PERSONAL_MODE,
                    "lang:options.worldOptions.personal_game_mode",
                    self.wanted.personal_mode,
                ),
            ] {
                self.widgets.add(CycleButton::new(
                    id,
                    [left, y, 310, 20],
                    key,
                    MODES.map(String::from).to_vec(),
                    usize::from(value),
                ));
                y += 24;
            }
            self.widgets.add(CycleButton::new(
                ALLOW_COMMANDS,
                [left, y, 150, 20],
                "lang:selectWorld.allowCommands",
                vec!["lang:options.on".into(), "lang:options.off".into()],
                usize::from(!self.wanted.allow_commands),
            ));
        }
        self.widgets.add(CycleButton::new(
            DIFFICULTY,
            if self.init.local.is_some() {
                [right, y, 126, 20]
            } else {
                [left, y, 126, 20]
            },
            "lang:options.difficulty",
            DIFFICULTIES.map(String::from).to_vec(),
            usize::from(self.wanted.difficulty),
        ));
        let lock = self.lock_rect();
        self.widgets.add(Button::new(LOCK, lock, "").flat());
        y += 24;
        self.widgets.add(Button::new(
            GAME_RULES,
            [left, y, 150, 20],
            "lang:editGamerule.inGame.button",
        ));
        y += 36;
        if self.init.local.is_some() {
            self.widgets.add(CycleButton::new(
                LAN,
                [left, y + 12, 150, 20],
                "lang:menu.multiplayerOptions.lan",
                vec!["lang:options.on".into(), "lang:options.off".into()],
                usize::from(!self.wanted.lan),
            ));
            let mut port = EditBox::new(PORT, [right, y + 12, 150, 20]).with_max_length(5);
            port.set_value(&keep.unwrap_or_default());
            port.hint = Some("lang:lanServer.port".into());
            port.take_changed();
            self.widgets.add(port);
        }
        self.widgets.add(Button::new(
            APPLY,
            [left, height - 28, 150, 20],
            "lang:menu.multiplayerOptions.applyChanges",
        ));
        self.widgets.add(Button::new(
            CANCEL,
            [right, height - 28, 150, 20],
            "lang:gui.cancel",
        ));
        self.read_port();
        self.sync();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_dim_background(cx);
        let center = self.gui.0 / 2;
        let title = cx.lang("options.worldOptions.title");
        cx.centered_text(&title, center, 12, WHITE, true);
        let general = format!("{HEADER}{}", cx.lang("options.worldOptions.general.title"));
        cx.centered_text(&general, center, 32, WHITE, true);
        if self.init.local.is_some() {
            let top = self.widgets.find(LAN).map_or(0, |w| w.rect()[1]) - 12;
            let multiplayer = format!(
                "{HEADER}{}",
                cx.lang("options.worldOptions.multiplayer.title")
            );
            cx.centered_text(&multiplayer, center, top, WHITE, true);
        }
        let hint = cx.lang("lanServer.port");
        if let Some(e) = self.widgets.edit_mut(PORT) {
            e.hint = Some(hint);
        }
        if let Some(key) = self.port_error {
            let text = cx.lang_fmt(key, &["1024", "65535"]);
            let y = self.widgets.find(PORT).map_or(0, |w| w.rect()[1]) + 24;
            for (i, line) in text.lines().take(2).enumerate() {
                cx.centered_text(line, center, y + i as i32 * 10, ERROR, true);
            }
        }
        self.widgets.render(cx);
        let rect = self.lock_rect();
        let active = self.widgets.find(LOCK).is_some_and(|w| w.active());
        let hot = active && contains(rect, self.mouse.0, self.mouse.1);
        let base = if self.wanted.locked {
            "widget/locked_button"
        } else {
            "widget/unlocked_button"
        };
        let sprite = match (active, hot) {
            (false, _) => format!("{base}_disabled"),
            (true, true) => format!("{base}_highlighted"),
            (true, false) => base.to_owned(),
        };
        cx.sprite(&sprite, rect[0], rect[1], rect[2], rect[3]);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if let UiInput::Key {
            key: Key::Escape, ..
        } = input
        {
            out.push(ScreenAction::Pop);
            return true;
        }
        if let UiInput::MouseMove { x, y } = *input {
            self.mouse = (x, y);
        }
        if let UiInput::Char(c) = input
            && self.widgets.focused() == Some(PORT)
            && !c.is_ascii_digit()
        {
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            match (id, kind) {
                (DEFAULT_MODE, WidgetEventKind::Cycled(i)) => {
                    out.push(ScreenAction::ClickSound);
                    self.wanted.default_mode = i.min(3) as u8;
                }
                (PERSONAL_MODE, WidgetEventKind::Cycled(i)) => {
                    out.push(ScreenAction::ClickSound);
                    self.wanted.personal_mode = i.min(3) as u8;
                }
                (ALLOW_COMMANDS, WidgetEventKind::Cycled(i)) => {
                    out.push(ScreenAction::ClickSound);
                    self.wanted.allow_commands = i == 0;
                    if !self.wanted.allow_commands {
                        if let Some(local) = self.init.local {
                            self.wanted.default_mode = local.default_mode.min(3);
                            self.wanted.personal_mode = local.personal_mode.min(3);
                        }
                        self.init(self.gui.0, self.gui.1);
                    }
                }
                (DIFFICULTY, WidgetEventKind::Cycled(i)) => {
                    out.push(ScreenAction::ClickSound);
                    self.wanted.difficulty = i.min(3) as u8;
                }
                (LOCK, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    self.wanted.locked = true;
                }
                (GAME_RULES, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::OpenGameRules);
                }
                (LAN, WidgetEventKind::Cycled(i)) => {
                    out.push(ScreenAction::ClickSound);
                    self.wanted.lan = i == 0;
                    self.read_port();
                }
                (PORT, WidgetEventKind::Edited) => self.read_port(),
                (APPLY, WidgetEventKind::Pressed) => {
                    self.read_port();
                    if self.has_changes() {
                        out.push(ScreenAction::ClickSound);
                        out.push(ScreenAction::Pop);
                        out.push(ScreenAction::ApplyWorldOptions(self.change()));
                    }
                }
                (CANCEL, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                _ => {}
            }
        }
        self.sync();
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }
}
