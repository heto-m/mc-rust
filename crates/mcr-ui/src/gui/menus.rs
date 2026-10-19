use std::cell::RefCell;
use std::rc::Rc;

use mcr_protocol::types::text::Component;

use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, UiInput};
use super::screen::{Screen, ScreenAction};
use super::server_list::{ServerListScreen, SharedServers};
use super::tooltip::TooltipLines;
use super::widgets::{Button, Widget, WidgetEvent, WidgetEventKind, WidgetSet};
use super::world_list::{SelectWorldScreen, SharedWorlds};

pub(super) const WHITE: u32 = 0xFFFF_FFFF;
const MAX_LINES: usize = 40;
const MAX_URL: usize = 512;

/// Printable text only: no controls, `§`, bidi overrides, zero-width or other format characters.
pub fn is_plain(text: &str) -> bool {
    text.chars().all(|c| {
        !c.is_control()
            && c != '§'
            && !matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}' | '\u{00AD}')
    })
}

/// The host of an http(s) URL with an ASCII-only authority and no userinfo, else `None`.
pub fn parse_web_url(url: &str) -> Option<String> {
    if url.len() > MAX_URL || !url.is_ascii() || !is_plain(url) || url.contains(char::is_whitespace)
    {
        return None;
    }
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains(['@', '\\']) {
        return None;
    }
    let host = authority.rsplit_once(':').map_or(authority, |(h, port)| {
        if port.bytes().all(|b| b.is_ascii_digit()) {
            h
        } else {
            authority
        }
    });
    let valid = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'[' | b']' | b':'));
    valid.then(|| host.to_ascii_lowercase())
}

pub fn draw_dim_background(cx: &mut RenderCx<'_, '_>) {
    let (w, h) = cx.gui_size();
    cx.frame.fill_gradient(0, 0, w, h, 0xC010_1010, 0xD010_1010);
}

pub fn draw_menu_background(cx: &mut RenderCx<'_, '_>) {
    let (w, h) = cx.gui_size();
    cx.frame.fill_rect(0, 0, w, h, 0xFF38_3838);
    tile_texture(cx, "menu_background", [0, 0, w, h]);
}

pub fn tile_texture(cx: &mut RenderCx<'_, '_>, name: &str, rect: [i32; 4]) {
    let Some(id) = cx.assets.texture(name) else {
        return;
    };
    let [x0, y0, w, h] = rect;
    cx.frame.push_scissor(x0, y0, x0 + w, y0 + h);
    for y in (y0..y0 + h).step_by(32) {
        for x in (x0..x0 + w).step_by(32) {
            cx.frame.sprite(id, x, y, 32, 32);
        }
    }
    cx.frame.pop_scissor();
}

pub(super) fn centered_component(
    cx: &mut RenderCx<'_, '_>,
    component: &Component,
    center_x: i32,
    y: i32,
    max_width: f32,
    color: u32,
) -> i32 {
    let cells = cx.frame.ui().flatten(component);
    let lines = cx.frame.ui().wrap(&cells, max_width);
    for (i, range) in lines.iter().take(MAX_LINES).enumerate() {
        let w = cx.frame.ui().width(&cells[range.clone()]);
        cx.frame.cells(
            &cells[range.clone()],
            (center_x as f32 - w / 2.0).floor(),
            (y + i as i32 * 9) as f32,
            color,
            true,
        );
    }
    lines.len() as i32
}

pub(super) fn line_count(cx: &mut RenderCx<'_, '_>, component: &Component, max_width: f32) -> i32 {
    let cells = cx.frame.ui().flatten(component);
    cx.frame.ui().wrap(&cells, max_width).len().max(1) as i32
}

type Choice = (String, Box<dyn Fn() -> Vec<ScreenAction>>);

/// Behind a panel the whole screen is nearly covered, so another GUI underneath does not show around it.
const PANEL_BACKDROP: u32 = 0xF010_1010;

/// A title, a message and a row of buttons whose outcomes are produced by closures.
/// Why a clicked command needs the player's confirmation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandPrompt {
    ParseErrors,
    PermissionsRequired,
    /// It sends chat as the player; `suggest` offers to open chat with it instead of copying it.
    SignatureRequired {
        suggest: bool,
    },
}

pub struct ConfirmScreen {
    title: String,
    message: String,
    choices: Vec<Choice>,
    widgets: WidgetSet,
    gui: (i32, i32),
    delay: u32,
    dark: bool,
    panel: bool,
    panel_rect: [i32; 4],
    escape: Option<usize>,
}

impl std::fmt::Debug for ConfirmScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfirmScreen")
            .field("title", &self.title)
            .finish()
    }
}

impl ConfirmScreen {
    /// `title` and `message` are `§` text or `lang:key`; the last choice is taken on Escape.
    pub fn new(title: &str, message: &str, choices: Vec<Choice>) -> Self {
        let escape = choices.len().checked_sub(1);
        Self {
            title: title.to_owned(),
            message: message.to_owned(),
            choices,
            widgets: WidgetSet::default(),
            gui: (0, 0),
            delay: 0,
            dark: false,
            panel: false,
            panel_rect: [0; 4],
            escape,
        }
    }

    /// `None` unless `url` is a plain http(s) URL whose displayed text equals what will be opened.
    pub fn link(url: String) -> Option<Self> {
        let host = parse_web_url(&url)?;
        let open = url.clone();
        let copy = url.clone();
        Some(Self::new(
            "lang:chat.link.confirm",
            &format!("§e{host}§r\n{url}\n§7lang:chat.link.warning"),
            vec![
                (
                    "lang:gui.yes".into(),
                    Box::new(move || vec![ScreenAction::OpenUrl(open.clone()), ScreenAction::Pop]),
                ),
                (
                    "lang:chat.copy".into(),
                    Box::new(move || vec![ScreenAction::CopyToClipboard(copy.clone())]),
                ),
                ("lang:gui.no".into(), Box::new(|| vec![ScreenAction::Pop])),
            ],
        ))
    }

    /// `ClientPacketListener#openSendConfirmationWindow` for a clicked command that failed the check;
    /// `back` when a screen stays open under the prompt.
    pub fn command_prompt(command: &str, prompt: CommandPrompt, back: bool) -> Self {
        let run = command.to_owned();
        let (key, accept): (&str, Choice) = match prompt {
            CommandPrompt::ParseErrors | CommandPrompt::PermissionsRequired => (
                if prompt == CommandPrompt::ParseErrors {
                    "multiplayer.confirm_command.parse_errors"
                } else {
                    "multiplayer.confirm_command.permissions_required"
                },
                (
                    "lang:multiplayer.confirm_command.run_command".into(),
                    Box::new(move || {
                        vec![ScreenAction::SendCommand(run.clone()), ScreenAction::Pop]
                    }),
                ),
            ),
            CommandPrompt::SignatureRequired { suggest: true } => (
                "multiplayer.confirm_command.signature_required",
                (
                    "lang:multiplayer.confirm_command.suggest_command".into(),
                    Box::new(move || {
                        vec![ScreenAction::Pop, ScreenAction::OpenChat(format!("/{run}"))]
                    }),
                ),
            ),
            CommandPrompt::SignatureRequired { suggest: false } => (
                "multiplayer.confirm_command.signature_required",
                (
                    "lang:chat.copy".into(),
                    Box::new(move || {
                        vec![
                            ScreenAction::CopyToClipboard(format!("/{run}")),
                            ScreenAction::Pop,
                        ]
                    }),
                ),
            ),
        };
        let no = if back {
            "lang:gui.back"
        } else {
            "lang:gui.cancel"
        };
        Self::new(
            "lang:multiplayer.confirm_command.title",
            &format!("lang:{key}|§e{command}§r"),
            vec![accept, (no.into(), Box::new(|| vec![ScreenAction::Pop]))],
        )
    }

    pub fn with_delay(mut self, ticks: u32) -> Self {
        self.delay = ticks;
        self
    }

    pub fn with_death_background(mut self) -> Self {
        self.dark = true;
        self
    }

    /// An opaque panel behind the title and buttons, for screens drawn over another GUI.
    pub fn with_panel(mut self) -> Self {
        self.panel = true;
        self
    }

    fn text(cx: &mut RenderCx<'_, '_>, raw: &str) -> String {
        let mut out = String::new();
        for (i, line) in raw.split('\n').enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let (prefix, rest) = match line.strip_prefix("§7") {
                Some(rest) => ("§7", rest),
                None => ("", line),
            };
            match rest.strip_prefix("lang:") {
                Some(spec) => {
                    out.push_str(prefix);
                    match spec.split_once('|') {
                        Some((key, arg)) => out.push_str(&cx.lang_fmt(key, &[arg])),
                        None => out.push_str(&cx.lang(spec)),
                    }
                }
                None => out.push_str(line),
            }
        }
        out
    }

    fn layout(&mut self, message_lines: i32) {
        let n = self.choices.len() as i32;
        let bw = ((self.gui.0 - 20 - (n - 1).max(0) * 10) / n.max(1)).clamp(40, 150);
        let total = n * bw + (n - 1).max(0) * 10;
        let y = (self.gui.1 / 2 - 50 + 20 + message_lines * 9 + 20).max(self.gui.1 / 6 + 96);
        for i in 0..self.choices.len() {
            if let Some(b) = self.widgets.button_mut(i as u32) {
                b.rect = [(self.gui.0 - total) / 2 + i as i32 * (bw + 10), y, bw, 20];
                b.active = self.delay == 0;
            }
        }
        let top = self.gui.1 / 2 - 50 - 14;
        let width = (total + 40).min(self.gui.0);
        self.panel_rect = [(self.gui.0 - width) / 2, top, width, y + 20 + 14 - top];
    }
}

impl Screen for ConfirmScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        for (i, (label, _)) in self.choices.iter().enumerate() {
            self.widgets
                .add(Button::new(i as u32, [0, 0, 150, 20], label.clone()));
        }
        self.layout(1);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        if self.dark {
            let (w, h) = cx.gui_size();
            cx.frame.fill_gradient(0, 0, w, h, 0x6050_0000, 0xA080_3030);
        } else if self.panel {
            let (w, h) = cx.gui_size();
            cx.frame.fill_rect(0, 0, w, h, PANEL_BACKDROP);
        } else {
            draw_dim_background(cx);
        }
        let title = Self::text(cx, &self.title);
        let message = Self::text(cx, &self.message);
        let center = self.gui.0 / 2;
        let cells = crate::text::parse_legacy(&message);
        let lines = cx.frame.ui().wrap(&cells, (self.gui.0 - 50) as f32);
        self.layout(lines.len() as i32);
        if self.panel {
            let [x, y, w, h] = self.panel_rect;
            cx.frame
                .fill_rect(x - 1, y - 1, x + w + 1, y + h + 1, 0xFF55_5555);
            cx.frame.fill_rect(x, y, x + w, y + h, 0xFF10_1010);
        }
        cx.centered_text(&title, center, self.gui.1 / 2 - 50, WHITE, true);
        for (i, range) in lines.iter().take(MAX_LINES).enumerate() {
            let w = cx.frame.ui().width(&cells[range.clone()]);
            cx.frame.cells(
                &cells[range.clone()],
                (center as f32 - w / 2.0).floor(),
                (self.gui.1 / 2 - 50 + 20 + i as i32 * 9) as f32,
                WHITE,
                true,
            );
        }
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if self.delay == 0
            && let UiInput::Key {
                key: Key::Escape, ..
            } = input
            && let Some(i) = self.escape
        {
            out.extend((self.choices[i].1)());
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for e in events {
            if e.kind == WidgetEventKind::Pressed {
                out.push(ScreenAction::ClickSound);
                out.extend((self.choices[e.id as usize].1)());
            }
        }
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }

    fn tick(&mut self, _env: &mut Env<'_>, _out: &mut Vec<ScreenAction>) {
        self.delay = self.delay.saturating_sub(1);
        self.layout(1);
    }
}

const PAUSE_RETURN: u32 = 1;
const PAUSE_ADVANCEMENTS: u32 = 2;
const PAUSE_STATS: u32 = 3;
const PAUSE_OPTIONS: u32 = 4;
const PAUSE_DISCONNECT: u32 = 5;
const PAUSE_LAN: u32 = 6;
const PAUSE_BUGS: u32 = 7;
const PAUSE_FEEDBACK: u32 = 8;
const PAUSE_FRIENDS: u32 = 9;
const PAUSE_REPORTING: u32 = 10;
const PAUSE_ADDITIONS: u32 = 11;
const BUGS_URL: &str = "https://aka.ms/snapshotbugs?ref=game";
const FEEDBACK_URL: &str = "https://aka.ms/javafeedback?ref=game";
const ICONS: [(u32, &str, &str); 4] = [
    (PAUSE_BUGS, "pause_menu/bug", "menu.reportBugs"),
    (
        PAUSE_FEEDBACK,
        "pause_menu/social_interactions",
        "menu.sendFeedback",
    ),
    (PAUSE_FRIENDS, "friends/friends", ""),
    (
        PAUSE_REPORTING,
        "pause_menu/player_reporting",
        "menu.playerReporting",
    ),
];

/// A server's `pause_screen_additions` dialog (or its server links), offered as a button.
#[derive(Debug, Clone)]
pub struct PauseAddition {
    pub label: Component,
    pub dialog: mcr_nbt::Value,
}

#[derive(Debug)]
pub struct PauseScreen {
    widgets: WidgetSet,
    gui: (i32, i32),
    pauses: bool,
    local_server: bool,
    lan: bool,
    /// F3+Esc pauses with only the title.
    menu: bool,
    addition: Option<PauseAddition>,
}

impl PauseScreen {
    pub fn new(pauses_game: bool, local_server: bool) -> Self {
        Self {
            widgets: WidgetSet::default(),
            gui: (0, 0),
            pauses: pauses_game,
            local_server,
            lan: false,
            menu: true,
            addition: None,
        }
    }

    /// Offers Open to LAN beside Options (a local world not yet published).
    pub fn with_lan(mut self, offered: bool) -> Self {
        self.lan = offered;
        self
    }

    /// `PauseScreen(false)`: the game pauses but no menu is shown.
    pub fn without_menu(mut self) -> Self {
        self.menu = false;
        self
    }

    pub fn with_addition(mut self, addition: Option<PauseAddition>) -> Self {
        self.addition = addition;
        self
    }
}

impl Screen for PauseScreen {
    fn shows_music_toast(&self) -> bool {
        self.menu
    }

    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        if !self.menu {
            return;
        }
        let rows = 5 + i32::from(self.addition.is_some());
        let top = (height - (rows * 24 + 46)) / 4;
        let left = width / 2 - 102;
        let w = &mut self.widgets;
        let mut y = top + 50;
        w.add(Button::new(
            PAUSE_RETURN,
            [left, y, 204, 20],
            "lang:menu.returnToGame",
        ));
        y += 24;
        w.add(Button::new(
            PAUSE_ADVANCEMENTS,
            [left, y, 98, 20],
            "lang:gui.advancements",
        ));
        w.add(Button::new(
            PAUSE_STATS,
            [left + 106, y, 98, 20],
            "lang:gui.stats",
        ));
        y += 24;
        let icons_left = width / 2 - (ICONS.len() as i32 * 24 - 4) / 2;
        for (i, (id, _, key)) in ICONS.iter().enumerate() {
            let mut b = Button::new(*id, [icons_left + i as i32 * 24, y, 20, 20], "");
            if !key.is_empty() {
                let mut tip = TooltipLines::default();
                tip.push_component(Component::translatable(*key, Vec::new()));
                b.tooltip = Some(tip);
            }
            b.active = *id != PAUSE_FRIENDS;
            w.add(b);
        }
        y += 24;
        if self.addition.is_some() {
            let mut b = Button::new(PAUSE_ADDITIONS, [left, y, 204, 20], "");
            let mut tip = TooltipLines::default();
            tip.push_component(Component::translatable(
                "menu.custom_options.tooltip",
                Vec::new(),
            ));
            b.tooltip = Some(tip);
            w.add(b);
            y += 24;
        }
        let options_width = if self.lan { 98 } else { 204 };
        w.add(Button::new(
            PAUSE_OPTIONS,
            [left, y, options_width, 20],
            "lang:menu.options",
        ));
        if self.lan {
            w.add(Button::new(
                PAUSE_LAN,
                [left + 106, y, 98, 20],
                "lang:menu.multiplayerOptions.button",
            ));
        }
        y += 24;
        w.add(Button::new(
            PAUSE_DISCONNECT,
            [left, y, 204, 20],
            "lang:menu.disconnect",
        ));
        w.set_focus(Some(PAUSE_RETURN), 0);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        if !self.menu {
            let title = cx.lang("menu.paused");
            cx.centered_text(&title, self.gui.0 / 2, 10, WHITE, true);
            return;
        }
        draw_dim_background(cx);
        let title = cx.lang("menu.game");
        cx.centered_text(&title, self.gui.0 / 2, 40, WHITE, true);
        if self.local_server
            && let Some(b) = self.widgets.button_mut(PAUSE_DISCONNECT)
        {
            b.label = "lang:menu.returnToMenu".into();
        }
        if let Some(addition) = &self.addition {
            let label: String = crate::text::flatten(cx.frame.ui().lang(), &addition.label)
                .iter()
                .map(|c| c.ch)
                .collect();
            if let Some(b) = self.widgets.button_mut(PAUSE_ADDITIONS) {
                b.label = label;
            }
        }
        self.widgets.render(cx);
        for (id, sprite, _) in ICONS {
            if let Some(Widget::Button(b)) = self.widgets.find(id) {
                let [x, y, _, _] = b.rect;
                let sprite = sprite.to_owned();
                cx.sprite(&sprite, x + 2, y + 2, 15, 15);
            }
        }
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if let UiInput::Key {
            key: Key::Escape, ..
        } = input
        {
            out.push(ScreenAction::Pop);
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            if kind != WidgetEventKind::Pressed {
                continue;
            }
            out.push(ScreenAction::ClickSound);
            match id {
                PAUSE_RETURN => out.push(ScreenAction::Pop),
                PAUSE_ADVANCEMENTS => out.push(ScreenAction::OpenAdvancements),
                PAUSE_STATS => out.push(ScreenAction::OpenStats),
                PAUSE_OPTIONS => out.push(ScreenAction::OpenOptions),
                PAUSE_LAN => out.push(ScreenAction::OpenLan),
                PAUSE_DISCONNECT => out.push(ScreenAction::Disconnect),
                PAUSE_BUGS | PAUSE_FEEDBACK => {
                    let url = if id == PAUSE_BUGS {
                        BUGS_URL
                    } else {
                        FEEDBACK_URL
                    };
                    if let Some(confirm) = ConfirmScreen::link(url.to_owned()) {
                        out.push(ScreenAction::Push(Box::new(confirm)));
                    }
                }
                PAUSE_REPORTING => out.push(ScreenAction::OpenSocialInteractions),
                PAUSE_ADDITIONS => {
                    if let Some(addition) = &self.addition {
                        out.push(ScreenAction::ShowDialog(addition.dialog.clone()));
                    }
                }
                _ => {}
            }
        }
        used
    }

    fn pauses_game(&self) -> bool {
        self.pauses
    }
}

const DEATH_RESPAWN: u32 = 1;
const DEATH_TITLE: u32 = 2;

#[derive(Debug)]
pub struct DeathScreen {
    widgets: WidgetSet,
    gui: (i32, i32),
    cause: Option<Component>,
    hardcore: bool,
    ticks: u32,
}

impl DeathScreen {
    pub fn new(cause: Option<Component>, hardcore: bool) -> Self {
        Self {
            widgets: WidgetSet::default(),
            gui: (0, 0),
            cause,
            hardcore,
            ticks: 0,
        }
    }

    fn set_active(&mut self, active: bool) {
        for id in [DEATH_RESPAWN, DEATH_TITLE] {
            if let Some(b) = self.widgets.button_mut(id) {
                b.active = active;
            }
        }
    }
}

impl Screen for DeathScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        self.ticks = 0;
        let respawn = if self.hardcore {
            "lang:deathScreen.spectate"
        } else {
            "lang:deathScreen.respawn"
        };
        self.widgets.add(Button::new(
            DEATH_RESPAWN,
            [width / 2 - 100, height / 4 + 72, 200, 20],
            respawn,
        ));
        self.widgets.add(Button::new(
            DEATH_TITLE,
            [width / 2 - 100, height / 4 + 96, 200, 20],
            "lang:deathScreen.titleScreen",
        ));
        self.set_active(false);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        let (w, h) = cx.gui_size();
        cx.frame.fill_gradient(0, 0, w, h, 0x6050_0000, 0xA080_3030);
        let key = if self.hardcore {
            "deathScreen.title.hardcore"
        } else {
            "deathScreen.title"
        };
        let title = cx.lang(key);
        let cells = crate::text::parse_legacy(&title);
        let width = cx.frame.ui().width(&cells) * 2.0;
        cx.frame.cells_scaled(
            &cells,
            self.gui.0 as f32 / 2.0 - width / 2.0,
            60.0,
            WHITE,
            true,
            2.0,
        );
        if let Some(cause) = &self.cause {
            centered_component(
                cx,
                cause,
                self.gui.0 / 2,
                85,
                (self.gui.0 - 50) as f32,
                WHITE,
            );
        }
        let score = cx.view.score().to_string();
        let text = cx.lang_fmt("deathScreen.score.value", &[&format!("§e{score}§r")]);
        let label = cx.lang("deathScreen.score");
        let _ = label;
        cx.centered_text(&text, self.gui.0 / 2, 100, WHITE, true);
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            if kind != WidgetEventKind::Pressed {
                continue;
            }
            out.push(ScreenAction::ClickSound);
            match id {
                DEATH_RESPAWN => {
                    out.push(ScreenAction::Respawn);
                    if let Some(b) = self.widgets.button_mut(DEATH_RESPAWN) {
                        b.active = false;
                    }
                }
                DEATH_TITLE if self.hardcore => out.push(ScreenAction::Disconnect),
                DEATH_TITLE => {
                    let confirm = ConfirmScreen::new(
                        "lang:deathScreen.quit.confirm",
                        "",
                        vec![
                            (
                                "lang:deathScreen.titleScreen".into(),
                                Box::new(|| vec![ScreenAction::Pop, ScreenAction::Disconnect]),
                            ),
                            (
                                "lang:deathScreen.respawn".into(),
                                Box::new(|| vec![ScreenAction::Respawn, ScreenAction::Pop]),
                            ),
                        ],
                    )
                    .with_delay(20)
                    .with_death_background();
                    out.push(ScreenAction::Push(Box::new(confirm)));
                }
                _ => {}
            }
        }
        used
    }

    fn tick(&mut self, _env: &mut Env<'_>, _out: &mut Vec<ScreenAction>) {
        self.ticks += 1;
        if self.ticks == 20 {
            self.set_active(true);
        }
    }
}

#[derive(Debug)]
pub struct DisconnectedScreen {
    widgets: WidgetSet,
    gui: (i32, i32),
    title: Component,
    reason: Component,
    back_label: String,
}

impl DisconnectedScreen {
    pub fn new(reason: Component) -> Self {
        Self {
            widgets: WidgetSet::default(),
            gui: (0, 0),
            title: Component::translatable("disconnect.lost", Vec::new()),
            reason,
            back_label: "lang:gui.toMenu".into(),
        }
    }

    pub fn with_title(mut self, title: Component) -> Self {
        self.title = title;
        self
    }

    pub fn with_back_label(mut self, label: &str) -> Self {
        self.back_label = label.to_owned();
        self
    }
}

impl Screen for DisconnectedScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        self.widgets.add(Button::new(
            1,
            [width / 2 - 100, height / 2, 200, 20],
            self.back_label.clone(),
        ));
        self.widgets.set_focus(Some(1), 0);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let max_width = (self.gui.0 - 50) as f32;
        let lines = line_count(cx, &self.reason, max_width);
        let total = 29 + (lines * 9 + 20) + 24;
        let top = (self.gui.1 - total) / 2;
        let center = self.gui.0 / 2;
        centered_component(cx, &self.title, center, top + 10, max_width, WHITE);
        centered_component(cx, &self.reason, center, top + 39, max_width, WHITE);
        if let Some(b) = self.widgets.button_mut(1) {
            b.rect = [center - 100, top + 29 + lines * 9 + 20 + 2, 200, 20];
        }
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        if events.iter().any(|e| e.kind == WidgetEventKind::Pressed) {
            out.push(ScreenAction::ClickSound);
            out.push(ScreenAction::Pop);
        }
        used
    }
}

pub type StatusHandle = Rc<RefCell<String>>;

#[derive(Debug)]
pub struct ConnectingScreen {
    widgets: WidgetSet,
    gui: (i32, i32),
    status: StatusHandle,
    cancel: bool,
}

impl ConnectingScreen {
    pub fn new(status: StatusHandle) -> Self {
        Self {
            widgets: WidgetSet::default(),
            gui: (0, 0),
            status,
            cancel: true,
        }
    }

    /// No Cancel button and Escape does nothing, e.g. while a world saves.
    pub fn without_cancel(mut self) -> Self {
        self.cancel = false;
        self
    }

    pub fn with_text(text: &str) -> (Self, StatusHandle) {
        let handle: StatusHandle = Rc::new(RefCell::new(text.to_owned()));
        (Self::new(handle.clone()), handle)
    }
}

impl Screen for ConnectingScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        if self.cancel {
            self.widgets.add(Button::new(
                1,
                [width / 2 - 100, height / 4 + 120 + 12, 200, 20],
                "lang:gui.cancel",
            ));
        }
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let status = self.status.borrow().clone();
        let text = match status.strip_prefix("lang:").map(|s| s.split_once('|')) {
            Some(Some((key, arg))) => cx.lang_fmt(key, &[arg]),
            Some(None) => cx.lang(&status[5..]),
            None => status,
        };
        cx.centered_text(&text, self.gui.0 / 2, self.gui.1 / 2 - 50, WHITE, true);
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if !self.cancel {
            return true;
        }
        if let UiInput::Key {
            key: Key::Escape, ..
        } = input
        {
            out.push(ScreenAction::CancelConnect);
            out.push(ScreenAction::Pop);
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        if events.iter().any(|e| e.kind == WidgetEventKind::Pressed) {
            out.push(ScreenAction::ClickSound);
            out.push(ScreenAction::CancelConnect);
            out.push(ScreenAction::Pop);
        }
        used
    }
}

const TITLE_SINGLE: u32 = 1;
const TITLE_MULTI: u32 = 2;
const TITLE_OPTIONS: u32 = 3;
const TITLE_QUIT: u32 = 4;
const TITLE_MODS: u32 = 5;
const TITLE_LAUNCHER: u32 = 6;

pub struct TitleScreen {
    widgets: WidgetSet,
    gui: (i32, i32),
    version: String,
    servers: SharedServers,
    worlds: Option<SharedWorlds>,
}

impl std::fmt::Debug for TitleScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TitleScreen").finish()
    }
}

impl TitleScreen {
    pub fn new(version: &str, servers: SharedServers) -> Self {
        Self {
            widgets: WidgetSet::default(),
            gui: (0, 0),
            version: version.to_owned(),
            servers,
            worlds: None,
        }
    }

    /// Enables Singleplayer, which opens the world list over `worlds`.
    pub fn with_worlds(mut self, worlds: SharedWorlds) -> Self {
        self.worlds = Some(worlds);
        self
    }
}

impl Screen for TitleScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let y = height / 4 + 48;
        let x = width / 2 - 100;
        let single = Button::new(TITLE_SINGLE, [x, y, 200, 20], "lang:menu.singleplayer");
        self.widgets.add(if self.worlds.is_some() {
            single
        } else {
            single.disabled()
        });
        self.widgets.add(Button::new(
            TITLE_MULTI,
            [x, y + 24, 200, 20],
            "lang:menu.multiplayer",
        ));
        self.widgets
            .add(Button::new(TITLE_MODS, [x, y + 48, 98, 20], "Mods"));
        self.widgets.add(Button::new(
            TITLE_LAUNCHER,
            [width / 2 + 2, y + 48, 98, 20],
            "Launcher",
        ));
        self.widgets.add(Button::new(
            TITLE_OPTIONS,
            [x, y + 84, 98, 20],
            "lang:menu.options",
        ));
        self.widgets.add(Button::new(
            TITLE_QUIT,
            [width / 2 + 2, y + 84, 98, 20],
            "lang:menu.quit",
        ));
        self.widgets.set_focus(Some(TITLE_MULTI), 0);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        let (w, h) = cx.gui_size();
        cx.frame.fill_gradient(0, 0, w, h, 0xFF16_2433, 0xFF2B_4A3C);
        let logo_x = self.gui.0 / 2 - 128;
        if let Some(id) = cx.assets.texture("title/minecraft") {
            cx.frame.sprite(id, logo_x, 30, 256, 64);
        }
        if let Some(id) = cx.assets.texture("title/edition") {
            cx.frame.sprite(id, logo_x + 88, 30 + 37, 128, 16);
        }
        self.widgets.render(cx);
        let version = self.version.clone();
        cx.text(&version, 2, h - 10, WHITE, true);
        let copyright = cx.lang("title.credits");
        let cw = cx.text_width(&copyright);
        cx.text(&copyright, w - cw - 2, h - 10, WHITE, true);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            if kind != WidgetEventKind::Pressed {
                continue;
            }
            out.push(ScreenAction::ClickSound);
            match id {
                TITLE_SINGLE => {
                    if let Some(worlds) = &self.worlds {
                        out.push(ScreenAction::Push(Box::new(SelectWorldScreen::new(
                            worlds.clone(),
                        ))));
                    }
                }
                TITLE_MULTI => out.push(ScreenAction::Push(Box::new(ServerListScreen::new(
                    self.servers.clone(),
                )))),
                TITLE_MODS => out.push(ScreenAction::OpenMods),
                TITLE_LAUNCHER => out.push(ScreenAction::Launcher),
                TITLE_OPTIONS => out.push(ScreenAction::OpenOptions),
                TITLE_QUIT => out.push(ScreenAction::Quit),
                _ => {}
            }
        }
        used
    }
}

const DIRECT_EDIT: u32 = 1;
const DIRECT_JOIN: u32 = 2;
const DIRECT_CANCEL: u32 = 3;

pub struct DirectConnectScreen {
    widgets: WidgetSet,
    gui: (i32, i32),
    servers: SharedServers,
}

impl std::fmt::Debug for DirectConnectScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectConnectScreen").finish()
    }
}

impl DirectConnectScreen {
    pub fn new(servers: SharedServers) -> Self {
        Self {
            widgets: WidgetSet::default(),
            gui: (0, 0),
            servers,
        }
    }

    fn join(&mut self, out: &mut Vec<ScreenAction>) {
        let Some(edit) = self.widgets.edit(DIRECT_EDIT) else {
            return;
        };
        let address = edit.value().trim().to_owned();
        if address.is_empty() {
            return;
        }
        self.servers.borrow_mut().set_last_direct_address(&address);
        out.push(ScreenAction::Connect(address));
    }

    fn update_buttons(&mut self) {
        let empty = self
            .widgets
            .edit(DIRECT_EDIT)
            .is_none_or(|e| e.value().trim().is_empty());
        if let Some(b) = self.widgets.button_mut(DIRECT_JOIN) {
            b.active = !empty;
        }
    }
}

impl Screen for DirectConnectScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        let keep = self
            .widgets
            .edit(DIRECT_EDIT)
            .map(|e| e.value().to_owned())
            .unwrap_or_else(|| self.servers.borrow().last_direct_address());
        self.widgets.clear();
        let mut edit =
            EditBox::new(DIRECT_EDIT, [width / 2 - 100, 116, 200, 20]).with_max_length(128);
        edit.set_value(&keep);
        edit.take_changed();
        self.widgets.add(edit);
        self.widgets.add(Button::new(
            DIRECT_JOIN,
            [width / 2 - 100, height / 4 + 96 + 12, 200, 20],
            "lang:selectServer.select",
        ));
        self.widgets.add(Button::new(
            DIRECT_CANCEL,
            [width / 2 - 100, height / 4 + 120 + 12, 200, 20],
            "lang:gui.cancel",
        ));
        self.widgets.set_focus(Some(DIRECT_EDIT), 0);
        self.update_buttons();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let title = cx.lang("selectServer.direct");
        cx.centered_text(&title, self.gui.0 / 2, 20, WHITE, true);
        let label = cx.lang("addServer.enterIp");
        cx.text(&label, self.gui.0 / 2 - 100, 100, 0xFFA0_A0A0, true);
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
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            match (id, kind) {
                (DIRECT_JOIN, WidgetEventKind::Pressed)
                | (DIRECT_EDIT, WidgetEventKind::Submitted) => {
                    self.join(out);
                }
                (DIRECT_CANCEL, WidgetEventKind::Pressed) => out.push(ScreenAction::Pop),
                _ => {}
            }
        }
        self.update_buttons();
        used
    }
}

#[cfg(test)]
mod panel_tests {
    use super::*;

    #[test]
    fn panel_spans_title_and_buttons() {
        for (w, h) in [(320, 240), (427, 240), (640, 360)] {
            let mut s = ConfirmScreen::new(
                "mcr",
                "",
                vec![
                    ("a".into(), Box::new(Vec::new)),
                    ("b".into(), Box::new(Vec::new)),
                    ("c".into(), Box::new(Vec::new)),
                ],
            )
            .with_panel();
            s.init(w, h);
            s.layout(0);
            let [px, py, pw, ph] = s.panel_rect;
            assert!(py < h / 2 - 50, "title sits inside the panel");
            for i in 0..3 {
                let [bx, by, bw, bh] = s.widgets.button_mut(i).unwrap().rect;
                assert!(bx >= px && bx + bw <= px + pw, "{w}x{h} button {i} x");
                assert!(by >= py && by + bh <= py + ph, "{w}x{h} button {i} y");
            }
        }
    }
}
