//! Vanilla's `WorldOptionsScreen`: difficulty, game rules, restrictions, game modes, allow commands, LAN and guest access.

use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, UiInput, contains};
use super::lan_screen::{LanSettings, PortCheck};
use super::list::ScrollList;
use super::menus::{WHITE, draw_dim_background};
use super::restrictions::{ChatLimit, RestrictionsScreen};
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
const RESTRICTIONS: u32 = 11;
const GUEST_COMMANDS: u32 = 12;
const FORCE_MODE: u32 = 13;
const MIN_PORT: u16 = 1024;
const VIEW_TOP: i32 = 31;
const FOOTER: i32 = 28;
const BAR_GAP: i32 = 6;
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
    pub chat_limit: ChatLimit,
    pub local: Option<LocalWorldOptions>,
}

/// The extra rows of a world hosted by this client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalWorldOptions {
    pub default_mode: u8,
    pub personal_mode: u8,
    pub allow_commands: bool,
    /// The port the world is shared on, while it is open to LAN.
    pub lan_port: Option<u16>,
    pub guest_commands: bool,
    pub force_game_mode: bool,
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
    /// Stop sharing first (alone to close LAN, with `lan` to move it to another port).
    pub close_lan: bool,
    pub guest_commands: Option<bool>,
    pub force_game_mode: Option<bool>,
}

pub struct WorldOptionsScreen {
    widgets: WidgetSet,
    footer: WidgetSet,
    base: Vec<(u32, [i32; 4])>,
    list: ScrollList,
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
    guest_commands: bool,
    force_game_mode: bool,
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
            footer: WidgetSet::default(),
            base: Vec::new(),
            list: ScrollList::new([0; 4], 1),
            gui: (0, 0),
            mouse: (0.0, 0.0),
            wanted: Wanted {
                difficulty: init.difficulty.min(3),
                locked: init.difficulty_locked,
                default_mode: local.map_or(0, |l| l.default_mode.min(3)),
                personal_mode: local.map_or(0, |l| l.personal_mode.min(3)),
                allow_commands: local.is_some_and(|l| l.allow_commands),
                lan: local.is_some_and(|l| l.lan_port.is_some()),
                guest_commands: local.is_some_and(|l| l.guest_commands),
                force_game_mode: local.is_none_or(|l| l.force_game_mode),
            },
            init,
            check: None,
            port: None,
            port_error: None,
        }
    }

    /// Vanilla's `updateGuestCommandAccessButton`.
    fn update_guest_commands(&mut self) {
        let Some(local) = self.init.local else { return };
        self.wanted.guest_commands =
            self.wanted.lan && self.wanted.allow_commands && local.guest_commands;
        self.update_force_mode();
    }

    /// Vanilla's `updateForceGameModeButton`.
    fn update_force_mode(&mut self) {
        let Some(local) = self.init.local else { return };
        self.wanted.force_game_mode = if self.init.hardcore || !self.wanted.lan {
            true
        } else if self.wanted.guest_commands {
            false
        } else {
            local.force_game_mode
        };
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
            let open_port = local.lan_port;
            let settings = LanSettings {
                mode: w.default_mode,
                cheats: w.allow_commands,
                port: self.port,
            };
            if w.lan && open_port.is_none() {
                change.lan = Some(settings);
            } else if !w.lan && open_port.is_some() {
                change.close_lan = true;
            } else if w.lan && self.port.is_some() && self.port != open_port {
                change.close_lan = true;
                change.lan = Some(settings);
            }
            change.guest_commands =
                (w.guest_commands != local.guest_commands).then_some(w.guest_commands);
            change.force_game_mode =
                (w.force_game_mode != local.force_game_mode).then_some(w.force_game_mode);
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
        let commands_allowed = self.commands_allowed();
        let allowed = commands_allowed && !self.init.hardcore;
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
        if let Some(c) = w.cycle_mut(ALLOW_COMMANDS) {
            c.active = !self.init.hardcore;
        }
        if let Some(b) = w.button_mut(LOCK) {
            b.active = difficulty && !self.wanted.locked;
        }
        if let Some(b) = w.button_mut(GAME_RULES) {
            b.active = allowed;
        }
        if let Some(c) = w.cycle_mut(LAN) {
            c.tip = Some(
                if self.wanted.lan {
                    "menu.multiplayerOptions.network.lan.tooltip"
                } else {
                    "menu.multiplayerOptions.network.off.tooltip"
                }
                .into(),
            );
        }
        if let Some(e) = w.edit_mut(PORT) {
            e.text_color = if port_error { ERROR } else { 0xFFE0_E0E0 };
            e.active = self.wanted.lan;
            e.editable = self.wanted.lan;
        }
        let (lan, commands, guest, hardcore) = (
            self.wanted.lan,
            self.wanted.allow_commands,
            self.wanted.guest_commands,
            self.init.hardcore,
        );
        if let Some(c) = w.cycle_mut(GUEST_COMMANDS) {
            c.active = lan && commands;
            c.set_index(usize::from(!guest));
            c.tip = Some(
                if !lan {
                    "options.worldOptions.guest.command_access.disabled.scope.tooltip"
                } else if !commands {
                    "options.worldOptions.guest.command_access.disabled.commands.tooltip"
                } else {
                    "options.worldOptions.guest.command_access.tooltip"
                }
                .into(),
            );
        }
        let force = self.wanted.force_game_mode;
        if let Some(c) = w.cycle_mut(FORCE_MODE) {
            c.active = lan && !guest && !hardcore;
            c.set_index(usize::from(!force));
            c.tip = Some(
                if hardcore {
                    "options.worldOptions.guest.force_game_mode.off.hardcore.tooltip"
                } else if !lan {
                    "options.worldOptions.guest.force_game_mode.off.scope.tooltip"
                } else if guest {
                    "options.worldOptions.guest.force_game_mode.off.commands.tooltip"
                } else if force {
                    "options.worldOptions.guest.force_game_mode.on.tooltip"
                } else {
                    "options.worldOptions.guest.force_game_mode.off.tooltip"
                }
                .into(),
            );
        }
        let difficulty_tip = if self.init.hardcore {
            Some("options.worldOptions.difficulty.disabled.hardcore.tooltip")
        } else if self.init.difficulty_locked {
            Some("options.worldOptions.difficulty.disabled.locked.tooltip")
        } else if !commands_allowed {
            Some("options.worldOptions.difficulty.disabled.operator.tooltip")
        } else {
            None
        };
        if let Some(c) = w.cycle_mut(DIFFICULTY) {
            c.tip = difficulty_tip.map(String::from);
        }
        let rules_tip = if self.init.hardcore {
            Some("editGamerule.inGame.disabled.hardcore.tooltip")
        } else if !commands_allowed {
            Some("editGamerule.inGame.disabled.tooltip")
        } else {
            None
        };
        if let Some(b) = w.button_mut(GAME_RULES) {
            b.tip = rules_tip.map(String::from);
        }
        let mode_tip = if self.init.hardcore {
            "options.worldOptions.game_mode.disabled.tooltip"
        } else if !commands_allowed {
            "options.worldOptions.game_mode.disabled.operator.tooltip"
        } else {
            "options.worldOptions.game_mode.tooltip"
        };
        if let Some(c) = w.cycle_mut(DEFAULT_MODE) {
            c.tip = Some(mode_tip.into());
        }
        let personal_tip = if mode_tip.ends_with("game_mode.tooltip") {
            "options.worldOptions.personal_game_mode.tooltip"
        } else {
            mode_tip
        };
        if let Some(c) = w.cycle_mut(PERSONAL_MODE) {
            c.tip = Some(personal_tip.into());
        }
        if let Some(c) = w.cycle_mut(ALLOW_COMMANDS) {
            c.tip = Some(
                if self.init.hardcore {
                    "options.worldOptions.allow_commands.disabled.tooltip"
                } else {
                    "options.worldOptions.allow_commands.tooltip"
                }
                .into(),
            );
        }
        if let Some(b) = self.footer.button_mut(APPLY) {
            b.active = apply;
        }
    }

    fn view(&self) -> [i32; 4] {
        [0, VIEW_TOP, self.gui.0, self.gui.1 - FOOTER - VIEW_TOP]
    }

    fn bar_x(&self) -> i32 {
        self.gui.0 / 2 + 155 + BAR_GAP
    }

    fn scroll_y(&self) -> i32 {
        self.list.scroll() as i32
    }

    /// Moves the content widgets to their unscrolled row, less the scroll.
    fn place_content(&mut self) {
        let scroll = self.scroll_y();
        for &(id, [x, y, w, h]) in &self.base {
            if let Some(widget) = self.widgets.find_mut(id) {
                widget.set_rect([x, y - scroll, w, h]);
            }
        }
    }

    fn scroll_to_focus(&mut self) {
        let Some(id) = self.widgets.focused() else {
            return;
        };
        let Some(&(_, [_, y, _, h])) = self.base.iter().find(|(i, _)| *i == id) else {
            return;
        };
        let view = self.view();
        let (top, bottom) = (y - VIEW_TOP, y + h - VIEW_TOP + 8);
        let scroll = self.list.scroll();
        if (top as f32) < scroll {
            self.list.set_scroll(top as f32);
        } else if bottom as f32 > scroll + view[3] as f32 {
            self.list.set_scroll((bottom - view[3]) as f32);
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
                Some(p)
                    if self.init.local.and_then(|l| l.lan_port) != Some(p)
                        && self.check.as_ref().is_some_and(|c| !c(p)) =>
                {
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
        self.footer.clear();
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
        let local = self.init.local.is_some();
        self.widgets.add(Button::new(
            GAME_RULES,
            [left, y, 150, 20],
            "lang:editGamerule.inGame.button",
        ));
        self.widgets.add(Button::new(
            RESTRICTIONS,
            [right, y, 150, 20],
            "lang:restrictions_screen.button",
        ));
        y += 36;
        if local {
            let on_off = || vec!["lang:options.on".into(), "lang:options.off".into()];
            self.widgets.add(CycleButton::new(
                LAN,
                [left, y + 12, 150, 20],
                "lang:menu.multiplayerOptions.lan",
                on_off(),
                usize::from(!self.wanted.lan),
            ));
            let open_port = self.init.local.and_then(|l| l.lan_port);
            let initial =
                keep.unwrap_or_else(|| open_port.map(|p| p.to_string()).unwrap_or_default());
            let mut port = EditBox::new(PORT, [right, y + 12, 150, 20]).with_max_length(5);
            port.set_value(&initial);
            port.hint = Some("lang:lanServer.port".into());
            port.take_changed();
            self.widgets.add(port);
            self.widgets.add(CycleButton::new(
                GUEST_COMMANDS,
                [left, y + 36, 150, 20],
                "lang:options.worldOptions.guest.command_access",
                on_off(),
                usize::from(!self.wanted.guest_commands),
            ));
            self.widgets.add(CycleButton::new(
                FORCE_MODE,
                [right, y + 36, 150, 20],
                "lang:options.worldOptions.guest.force_game_mode",
                on_off(),
                usize::from(!self.wanted.force_game_mode),
            ));
        }
        self.footer.add(Button::new(
            APPLY,
            [left, height - 28, 150, 20],
            "lang:menu.multiplayerOptions.applyChanges",
        ));
        self.footer.add(Button::new(
            CANCEL,
            [right, height - 28, 150, 20],
            "lang:gui.cancel",
        ));
        self.base = [
            DEFAULT_MODE,
            PERSONAL_MODE,
            ALLOW_COMMANDS,
            DIFFICULTY,
            LOCK,
            GAME_RULES,
            RESTRICTIONS,
            LAN,
            PORT,
            GUEST_COMMANDS,
            FORCE_MODE,
        ]
        .into_iter()
        .filter_map(|id| Some((id, self.widgets.find(id)?.rect())))
        .collect();
        let bottom = self
            .base
            .iter()
            .map(|(_, r)| r[1] + r[3])
            .max()
            .unwrap_or(0);
        let view = self.view();
        self.list.set_rect(view);
        self.list
            .set_count(usize::try_from(bottom - VIEW_TOP - 8).unwrap_or(0));
        self.place_content();
        self.read_port();
        self.sync();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_dim_background(cx);
        let center = self.gui.0 / 2;
        let title = cx.lang("options.worldOptions.title");
        cx.centered_text(&title, center, 12, WHITE, true);
        let scroll = self.scroll_y();
        let [vx, vy, vw, vh] = self.view();
        cx.frame.push_scissor(vx, vy, vx + vw, vy + vh);
        let general = format!("{HEADER}{}", cx.lang("options.worldOptions.general.title"));
        cx.centered_text(&general, center, 32 - scroll, WHITE, true);
        if self.init.local.is_some() {
            let top = self.widgets.find(LAN).map_or(0, |w| w.rect()[1]) - 14;
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
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, self.bar_x());
        self.footer.render(cx);
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
        let bar_x = self.bar_x();
        let scrolled = self.list.input(input, bar_x);
        if scrolled {
            self.place_content();
        }
        let outside =
            matches!(*input, UiInput::MousePress { x, y, .. } if !contains(self.view(), x, y));
        let mut events = Vec::new();
        let mut used = scrolled || self.footer.input(env, input, &mut events);
        if !scrolled && !outside {
            used |= self.widgets.input(env, input, &mut events);
        }
        self.scroll_to_focus();
        self.place_content();
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
                    self.update_guest_commands();
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
                    self.update_guest_commands();
                    self.read_port();
                }
                (GUEST_COMMANDS, WidgetEventKind::Cycled(i)) => {
                    out.push(ScreenAction::ClickSound);
                    self.wanted.guest_commands = i == 0;
                    self.update_force_mode();
                }
                (FORCE_MODE, WidgetEventKind::Cycled(i)) => {
                    out.push(ScreenAction::ClickSound);
                    self.wanted.force_game_mode = i == 0;
                }
                (RESTRICTIONS, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Push(Box::new(RestrictionsScreen::new(
                        self.init.chat_limit,
                    ))));
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
