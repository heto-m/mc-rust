//! Open to LAN: game mode, allow cheats and a port for the local world.

use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, UiInput};
use super::menus::{WHITE, draw_dim_background};
use super::screen::{Screen, ScreenAction};
use super::widgets::{Button, CycleButton, WidgetEvent, WidgetEventKind, WidgetSet};

const MODE: u32 = 1;
const CHEATS: u32 = 2;
const PORT: u32 = 3;
const START: u32 = 4;
const CANCEL: u32 = 5;
const MIN_PORT: u16 = 1024;
const ERROR: u32 = 0xFFFF_5555;
const LABEL: u32 = 0xFFA0_A0A0;

/// What Start LAN World asks for; `mode` is the game mode id (0 survival to 3 spectator).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LanSettings {
    pub mode: u8,
    pub cheats: bool,
    /// `None` takes any free port.
    pub port: Option<u16>,
}

/// Whether a port can be listened on; the app binds a socket, tests answer without one.
pub type PortCheck = Box<dyn Fn(u16) -> bool>;

pub struct ShareToLanScreen {
    widgets: WidgetSet,
    gui: (i32, i32),
    settings: LanSettings,
    hardcore: bool,
    check: Option<PortCheck>,
    port_error: Option<&'static str>,
}

impl std::fmt::Debug for ShareToLanScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShareToLanScreen")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl ShareToLanScreen {
    /// Starts from the host's game mode and the world's cheats; hardcore locks both.
    pub fn new(mode: u8, cheats: bool, hardcore: bool) -> Self {
        Self {
            widgets: WidgetSet::default(),
            gui: (0, 0),
            settings: LanSettings {
                mode: if hardcore { 0 } else { mode.min(3) },
                cheats: cheats && !hardcore,
                port: None,
            },
            hardcore,
            check: None,
            port_error: None,
        }
    }

    pub fn with_port_check(mut self, check: PortCheck) -> Self {
        self.check = Some(check);
        self
    }

    pub fn settings(&self) -> LanSettings {
        self.settings
    }

    fn read_port(&mut self) {
        let text = self
            .widgets
            .edit(PORT)
            .map(|e| e.value().trim().to_owned())
            .unwrap_or_default();
        (self.settings.port, self.port_error) = if text.is_empty() {
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
        let ok = self.port_error.is_none();
        if let Some(b) = self.widgets.button_mut(START) {
            b.active = ok;
        }
        if let Some(e) = self.widgets.edit_mut(PORT) {
            e.text_color = if ok { 0xFFE0_E0E0 } else { ERROR };
        }
    }
}

impl Screen for ShareToLanScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        let keep = self.widgets.edit(PORT).map(|e| e.value().to_owned());
        self.widgets.clear();
        let mut mode = CycleButton::new(
            MODE,
            [width / 2 - 155, 100, 150, 20],
            "lang:selectWorld.gameMode",
            vec![
                "lang:selectWorld.gameMode.survival".into(),
                "lang:selectWorld.gameMode.creative".into(),
                "lang:selectWorld.gameMode.adventure".into(),
                "lang:selectWorld.gameMode.spectator".into(),
            ],
            usize::from(self.settings.mode),
        );
        mode.active = !self.hardcore;
        self.widgets.add(mode);
        let mut cheats = CycleButton::new(
            CHEATS,
            [width / 2 + 5, 100, 150, 20],
            "lang:selectWorld.allowCommands",
            vec!["lang:options.on".into(), "lang:options.off".into()],
            usize::from(!self.settings.cheats),
        );
        cheats.active = !self.hardcore;
        self.widgets.add(cheats);
        let mut port = EditBox::new(PORT, [width / 2 - 75, 160, 150, 20]).with_max_length(5);
        port.set_value(&keep.unwrap_or_default());
        port.hint = Some("lang:lanServer.port".into());
        port.take_changed();
        self.widgets.add(port);
        self.widgets.add(Button::new(
            START,
            [width / 2 - 155, height - 28, 150, 20],
            "lang:lanServer.start",
        ));
        self.widgets.add(Button::new(
            CANCEL,
            [width / 2 + 5, height - 28, 150, 20],
            "lang:gui.cancel",
        ));
        self.read_port();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_dim_background(cx);
        let center = self.gui.0 / 2;
        let title = cx.lang("lanServer.title");
        cx.centered_text(&title, center, 50, WHITE, true);
        let others = cx.lang("lanServer.otherPlayers");
        cx.centered_text(&others, center, 82, WHITE, true);
        let label = cx.lang("lanServer.port");
        cx.centered_text(&label, center, 146, LABEL, true);
        let hint = cx.lang("lanServer.port");
        if let Some(e) = self.widgets.edit_mut(PORT) {
            e.hint = Some(hint);
        }
        if let Some(key) = self.port_error {
            let text = cx.lang_fmt(key, &["1024", "65535"]);
            for (i, line) in text.lines().take(3).enumerate() {
                cx.centered_text(line, center, 186 + i as i32 * 10, ERROR, true);
            }
        }
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if let UiInput::Key {
            key: Key::Escape, ..
        } = input
        {
            out.push(ScreenAction::Pop);
            return true;
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
                (MODE, WidgetEventKind::Cycled(i)) => {
                    out.push(ScreenAction::ClickSound);
                    self.settings.mode = i.min(3) as u8;
                }
                (CHEATS, WidgetEventKind::Cycled(i)) => {
                    out.push(ScreenAction::ClickSound);
                    self.settings.cheats = i == 0;
                }
                (PORT, WidgetEventKind::Edited) => self.read_port(),
                (START, WidgetEventKind::Pressed) | (PORT, WidgetEventKind::Submitted) => {
                    self.read_port();
                    if self.port_error.is_none() {
                        out.push(ScreenAction::ClickSound);
                        out.push(ScreenAction::Pop);
                        out.push(ScreenAction::StartLan(self.settings));
                    }
                }
                (CANCEL, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                _ => {}
            }
        }
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }
}
