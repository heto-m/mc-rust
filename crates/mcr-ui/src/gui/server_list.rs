use std::cell::RefCell;
use std::rc::Rc;

use mcr_protocol::types::text::Component;

use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, MouseButton, UiInput, contains};
use super::list::ScrollList;
use super::menus::{ConfirmScreen, WHITE, draw_menu_background, tile_texture};
use super::screen::{Screen, ScreenAction};
use super::widgets::{Button, WidgetEvent, WidgetEventKind, WidgetSet};

const ROW_HEIGHT: i32 = 36;
const ROW_WIDTH: i32 = 305;
const HEADER: i32 = 33;
const FOOTER: i32 = 60;
const MAX_SERVERS: usize = 1000;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerEntry {
    pub name: String,
    pub address: String,
}

/// A decoded 64x64 RGBA server icon; `id` identifies the image so screens upload it once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Favicon {
    pub id: u64,
    pub rgba: Vec<u8>,
}

pub const FAVICON_SIZE: u32 = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct OnlineStatus {
    pub motd: Component,
    pub players: String,
    pub version: String,
    pub ping_ms: Option<u32>,
    pub compatible: bool,
    pub favicon: Option<Rc<Favicon>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerStatus {
    Pinging,
    Online(Box<OnlineStatus>),
    Unreachable { message: String },
}

pub trait ServerStore {
    fn servers(&self) -> &[ServerEntry];
    fn add(&mut self, entry: ServerEntry);
    fn update(&mut self, index: usize, entry: ServerEntry);
    fn remove(&mut self, index: usize);
    fn swap(&mut self, a: usize, b: usize);

    /// Starts a ping on first use and returns the latest known state.
    fn status(&mut self, _index: usize) -> ServerStatus {
        ServerStatus::Pinging
    }

    fn refresh(&mut self) {}

    fn last_direct_address(&self) -> String {
        String::new()
    }

    fn set_last_direct_address(&mut self, _address: &str) {}
}

pub type SharedServers = Rc<RefCell<dyn ServerStore>>;

#[derive(Clone, Debug, Default)]
pub struct MemoryServerStore {
    pub entries: Vec<ServerEntry>,
    pub statuses: Vec<Option<ServerStatus>>,
    pub last_direct: String,
}

impl MemoryServerStore {
    pub fn shared(entries: Vec<ServerEntry>) -> SharedServers {
        Rc::new(RefCell::new(Self {
            entries,
            ..Self::default()
        }))
    }
}

impl ServerStore for MemoryServerStore {
    fn servers(&self) -> &[ServerEntry] {
        &self.entries
    }

    fn add(&mut self, entry: ServerEntry) {
        if self.entries.len() < MAX_SERVERS {
            self.entries.push(entry);
        }
    }

    fn update(&mut self, index: usize, entry: ServerEntry) {
        if let Some(e) = self.entries.get_mut(index) {
            *e = entry;
        }
    }

    fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
            if index < self.statuses.len() {
                self.statuses.remove(index);
            }
        }
    }

    fn swap(&mut self, a: usize, b: usize) {
        if a < self.entries.len() && b < self.entries.len() {
            self.entries.swap(a, b);
            if a.max(b) < self.statuses.len() {
                self.statuses.swap(a, b);
            }
        }
    }

    fn status(&mut self, index: usize) -> ServerStatus {
        self.statuses
            .get(index)
            .cloned()
            .flatten()
            .unwrap_or(ServerStatus::Pinging)
    }

    fn last_direct_address(&self) -> String {
        self.last_direct.clone()
    }

    fn set_last_direct_address(&mut self, address: &str) {
        self.last_direct = address.to_owned();
    }
}

const JOIN: u32 = 1;
const DIRECT: u32 = 2;
const ADD: u32 = 3;
const EDIT: u32 = 4;
const DELETE: u32 = 5;
const REFRESH: u32 = 6;
const BACK: u32 = 7;

pub struct ServerListScreen {
    servers: SharedServers,
    widgets: WidgetSet,
    gui: (i32, i32),
    selected: Option<usize>,
    list: ScrollList,
    mouse: (f32, f32),
    favicons: RefCell<std::collections::HashMap<u64, Option<crate::SpriteId>>>,
}

impl std::fmt::Debug for ServerListScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerListScreen")
            .field("selected", &self.selected)
            .finish()
    }
}

impl ServerListScreen {
    pub fn new(servers: SharedServers) -> Self {
        Self {
            servers,
            widgets: WidgetSet::default(),
            gui: (0, 0),
            selected: None,
            list: ScrollList::new([0, HEADER, 0, 0], ROW_HEIGHT),
            mouse: (0.0, 0.0),
            favicons: RefCell::default(),
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn select(&mut self, index: Option<usize>) {
        let n = self.servers.borrow().servers().len();
        self.selected = index.filter(|&i| i < n);
        self.update_buttons();
    }

    fn count(&self) -> usize {
        self.servers.borrow().servers().len()
    }

    fn row_left(&self) -> i32 {
        self.gui.0 / 2 - ROW_WIDTH / 2
    }

    fn bar_x(&self) -> i32 {
        self.gui.0 / 2 + ROW_WIDTH / 2 + 6
    }

    fn sync_list(&mut self) {
        let count = self.count();
        self.list
            .set_rect([0, HEADER, self.gui.0, (self.gui.1 - HEADER - FOOTER).max(0)]);
        self.list.set_count(count);
    }

    fn update_buttons(&mut self) {
        let has = self.selected.is_some();
        for id in [JOIN, EDIT, DELETE] {
            if let Some(b) = self.widgets.button_mut(id) {
                b.active = has;
            }
        }
    }

    fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        let left = self.row_left() as f32;
        if x < left || x >= left + ROW_WIDTH as f32 {
            return None;
        }
        self.list.item_at(x, y)
    }

    fn join(&self, index: usize, out: &mut Vec<ScreenAction>) {
        if let Some(entry) = self.servers.borrow().servers().get(index) {
            out.push(ScreenAction::Connect(entry.address.clone()));
        }
    }

    fn move_server(&mut self, from: usize, to: usize) {
        self.servers.borrow_mut().swap(from, to);
        self.select(Some(to));
    }

    fn button_events(&mut self, events: Vec<WidgetEvent>, out: &mut Vec<ScreenAction>) {
        for WidgetEvent { id, kind } in events {
            if kind != WidgetEventKind::Pressed {
                continue;
            }
            out.push(ScreenAction::ClickSound);
            match id {
                JOIN => {
                    if let Some(i) = self.selected {
                        self.join(i, out);
                    }
                }
                DIRECT => out.push(ScreenAction::Push(Box::new(
                    super::menus::DirectConnectScreen::new(self.servers.clone()),
                ))),
                ADD => out.push(ScreenAction::Push(Box::new(EditServerScreen::new(
                    self.servers.clone(),
                    None,
                )))),
                EDIT => {
                    if let Some(i) = self.selected {
                        out.push(ScreenAction::Push(Box::new(EditServerScreen::new(
                            self.servers.clone(),
                            Some(i),
                        ))));
                    }
                }
                DELETE => {
                    if let Some(i) = self.selected {
                        let name = self
                            .servers
                            .borrow()
                            .servers()
                            .get(i)
                            .map(|s| s.name.clone())
                            .unwrap_or_default();
                        let store = self.servers.clone();
                        out.push(ScreenAction::Push(Box::new(ConfirmScreen::new(
                            "lang:selectServer.deleteQuestion",
                            &format!("lang:selectServer.deleteWarning|{name}"),
                            vec![
                                (
                                    "lang:selectServer.deleteButton".into(),
                                    Box::new(move || {
                                        store.borrow_mut().remove(i);
                                        vec![ScreenAction::Pop]
                                    }),
                                ),
                                (
                                    "lang:gui.cancel".into(),
                                    Box::new(|| vec![ScreenAction::Pop]),
                                ),
                            ],
                        ))));
                    }
                }
                REFRESH => self.servers.borrow_mut().refresh(),
                BACK => out.push(ScreenAction::Pop),
                _ => {}
            }
        }
    }

    fn draw_row(&self, cx: &mut RenderCx<'_, '_>, index: usize) {
        let left = self.row_left();
        let top = self.list.item_top(index);
        let (cx_, cy) = (left + 2, top + 2);
        let (mx, my) = (self.mouse.0, self.mouse.1);
        let hovered = contains([left, top, ROW_WIDTH, ROW_HEIGHT], mx, my)
            && contains(self.list.rect(), mx, my);
        if self.selected == Some(index) {
            cx.frame.fill_rect(
                left - 2,
                top - 2,
                left + ROW_WIDTH + 2,
                top + ROW_HEIGHT + 2 - 2,
                0xFF80_8080,
            );
            cx.frame.fill_rect(
                left - 1,
                top - 1,
                left + ROW_WIDTH + 1,
                top + ROW_HEIGHT + 1 - 2,
                0xFF00_0000,
            );
        }
        let entry = self.servers.borrow().servers()[index].clone();
        let status = self.servers.borrow_mut().status(index);
        let icon = match &status {
            ServerStatus::Online(o) => o.favicon.as_ref().and_then(|f| {
                *self.favicons.borrow_mut().entry(f.id).or_insert_with(|| {
                    cx.frame
                        .ui()
                        .sprites_mut()
                        .add_raw(FAVICON_SIZE, FAVICON_SIZE, &f.rgba)
                })
            }),
            _ => None,
        };
        match icon {
            Some(id) => cx.frame.sprite(id, cx_, cy, 32, 32),
            None => cx.frame.fill_rect(cx_, cy, cx_ + 32, cy + 32, 0xFF3A_3A3A),
        }
        cx.text(&entry.name, cx_ + 35, cy + 1, WHITE, true);
        let content_right = left + ROW_WIDTH - 2;
        let status_icon: String;
        let mut status_text = String::new();
        let mut motd: Option<Component> = None;
        match &status {
            ServerStatus::Pinging => {
                let frame = ((cx.now_ms / 100 + index as u64 * 2) & 7) as i32;
                let frame = if frame > 4 { 8 - frame } else { frame };
                status_icon = format!("server_list/pinging_{}", frame + 1);
            }
            ServerStatus::Online(online) => {
                let OnlineStatus {
                    motd: m,
                    players,
                    version,
                    ping_ms,
                    compatible,
                    ..
                } = &**online;
                motd = Some(m.clone());
                status_icon = if *compatible {
                    let bars = match ping_ms {
                        Some(p) if *p < 150 => 5,
                        Some(p) if *p < 300 => 4,
                        Some(p) if *p < 600 => 3,
                        Some(p) if *p < 1000 => 2,
                        Some(_) => 1,
                        None => 1,
                    };
                    format!("server_list/ping_{bars}")
                } else {
                    "server_list/incompatible".to_owned()
                };
                status_text = if *compatible {
                    players.clone()
                } else {
                    format!("§c{version}")
                };
            }
            ServerStatus::Unreachable { message } => {
                status_icon = "server_list/unreachable".to_owned();
                motd = Some(Component::text(message.clone()));
            }
        }
        let icon_x = content_right - 10 - 5;
        cx.sprite(&status_icon, icon_x, cy, 10, 8);
        let w = cx.text_width(&status_text);
        cx.text(&status_text, icon_x - w - 5, cy + 1, 0xFF80_8080, true);
        if let Some(motd) = motd {
            let cells = cx.frame.ui().flatten(&motd);
            let lines = cx.frame.ui().wrap(&cells, (ROW_WIDTH - 4 - 32 - 2) as f32);
            for (i, range) in lines.iter().take(2).enumerate() {
                cx.frame.cells(
                    &cells[range.clone()],
                    (cx_ + 35) as f32,
                    (cy + 12 + 9 * i as i32) as f32,
                    0xFF80_8080,
                    true,
                );
            }
        }
        if hovered {
            cx.frame.fill_rect(cx_, cy, cx_ + 32, cy + 32, 0xA0B0_B0B0);
            let (rx, ry) = (mx as i32 - cx_, my as i32 - cy);
            let in_icon = (0..32).contains(&rx) && (0..32).contains(&ry);
            let join = if in_icon && rx >= 16 {
                "server_list/join_highlighted"
            } else {
                "server_list/join"
            };
            cx.sprite(join, cx_, cy, 32, 32);
            if index > 0 {
                let name = if in_icon && rx < 16 && ry < 16 {
                    "server_list/move_up_highlighted"
                } else {
                    "server_list/move_up"
                };
                cx.sprite(name, cx_, cy, 32, 32);
            }
            if index + 1 < self.count() {
                let name = if in_icon && rx < 16 && ry >= 16 {
                    "server_list/move_down_highlighted"
                } else {
                    "server_list/move_down"
                };
                cx.sprite(name, cx_, cy, 32, 32);
            }
        }
    }
}

impl Screen for ServerListScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let top_x = (width - 308) / 2;
        let y1 = height - 52;
        let y2 = height - 28;
        let w = &mut self.widgets;
        w.add(Button::new(
            JOIN,
            [top_x, y1, 100, 20],
            "lang:selectServer.select",
        ));
        w.add(Button::new(
            DIRECT,
            [top_x + 104, y1, 100, 20],
            "lang:selectServer.direct",
        ));
        w.add(Button::new(
            ADD,
            [top_x + 208, y1, 100, 20],
            "lang:selectServer.add",
        ));
        w.add(Button::new(
            EDIT,
            [top_x, y2, 74, 20],
            "lang:selectServer.edit",
        ));
        w.add(Button::new(
            DELETE,
            [top_x + 78, y2, 74, 20],
            "lang:selectServer.delete",
        ));
        w.add(Button::new(
            REFRESH,
            [top_x + 156, y2, 74, 20],
            "lang:selectServer.refresh",
        ));
        w.add(Button::new(
            BACK,
            [top_x + 234, y2, 74, 20],
            "lang:gui.back",
        ));
        self.sync_list();
        self.update_buttons();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.mouse = cx.mouse;
        draw_menu_background(cx);
        if self.selected.is_some_and(|i| i >= self.count()) {
            self.selected = None;
            self.update_buttons();
        }
        self.sync_list();
        let [lx, ly, lw, lh] = self.list.rect();
        tile_texture(cx, "menu_list_background", [lx, ly, lw, lh]);
        cx.frame.push_scissor(lx, ly, lx + lw, ly + lh);
        for i in self.list.visible_range() {
            self.draw_row(cx, i);
        }
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, self.bar_x());
        if let Some(id) = cx.assets.texture("header_separator") {
            for x in (0..self.gui.0).step_by(32) {
                cx.frame.sprite(id, x, HEADER - 2, 32, 2);
                cx.frame.sprite(id, x, self.gui.1 - FOOTER, 32, 2);
            }
        }
        let title = cx.lang("multiplayer.title");
        cx.centered_text(&title, self.gui.0 / 2, 12, WHITE, true);
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        match *input {
            UiInput::Key {
                key: Key::Escape, ..
            } => {
                out.push(ScreenAction::Pop);
                return true;
            }
            UiInput::Key {
                key: Key::Up | Key::Down,
                ..
            } if self.count() > 0 => {
                let down = matches!(input, UiInput::Key { key: Key::Down, .. });
                let next = match (self.selected, down) {
                    (None, true) => 0,
                    (None, false) => self.count() - 1,
                    (Some(i), true) => (i + 1).min(self.count() - 1),
                    (Some(i), false) => i.saturating_sub(1),
                };
                self.select(Some(next));
                self.list.ensure_visible(next);
                return true;
            }
            UiInput::MousePress {
                x,
                y,
                button: MouseButton::Left,
                double,
                ..
            } => {
                let mut events = Vec::new();
                if self.widgets.input(env, input, &mut events) {
                    self.button_events(events, out);
                    return true;
                }
                if self.list.input(input, self.bar_x()) {
                    return true;
                }
                if let Some(index) = self.row_at(x, y) {
                    let (rx, ry) = (
                        x as i32 - (self.row_left() + 2),
                        y as i32 - (self.list.item_top(index) + 2),
                    );
                    let in_icon = (0..32).contains(&rx) && (0..32).contains(&ry);
                    if in_icon && rx >= 16 {
                        self.select(Some(index));
                        self.join(index, out);
                    } else if in_icon && index > 0 && ry < 16 {
                        self.move_server(index, index - 1);
                    } else if in_icon && index + 1 < self.count() && ry >= 16 {
                        self.move_server(index, index + 1);
                    } else {
                        let was = self.selected == Some(index);
                        self.select(Some(index));
                        if double && was {
                            self.join(index, out);
                        }
                    }
                    return true;
                }
                return false;
            }
            UiInput::MouseMove { x, y } => {
                self.mouse = (x, y);
                if self.list.input(input, self.bar_x()) {
                    return true;
                }
            }
            UiInput::MouseRelease { .. } => {
                self.list.input(input, self.bar_x());
            }
            UiInput::Scroll { .. } => {
                return self.list.input(input, self.bar_x());
            }
            UiInput::Key {
                key: Key::Enter, ..
            } => {
                let mut events = Vec::new();
                if self.widgets.input(env, input, &mut events) && !events.is_empty() {
                    self.button_events(events, out);
                    return true;
                }
                if let Some(i) = self.selected {
                    self.join(i, out);
                    return true;
                }
                return false;
            }
            _ => {}
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        self.button_events(events, out);
        used
    }
}

const NAME: u32 = 1;
const ADDRESS: u32 = 2;
const DONE: u32 = 3;
const CANCEL: u32 = 4;

pub struct EditServerScreen {
    servers: SharedServers,
    index: Option<usize>,
    widgets: WidgetSet,
    gui: (i32, i32),
}

impl std::fmt::Debug for EditServerScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditServerScreen")
            .field("index", &self.index)
            .finish()
    }
}

impl EditServerScreen {
    pub fn new(servers: SharedServers, index: Option<usize>) -> Self {
        Self {
            servers,
            index,
            widgets: WidgetSet::default(),
            gui: (0, 0),
        }
    }

    fn update_buttons(&mut self) {
        let empty = self
            .widgets
            .edit(ADDRESS)
            .is_none_or(|e| e.value().trim().is_empty());
        if let Some(b) = self.widgets.button_mut(DONE) {
            b.active = !empty;
        }
    }

    fn done(&mut self, default_name: &str, out: &mut Vec<ScreenAction>) {
        let name = self
            .widgets
            .edit(NAME)
            .map(|e| e.value().to_owned())
            .unwrap_or_default();
        let address = self
            .widgets
            .edit(ADDRESS)
            .map(|e| e.value().trim().to_owned())
            .unwrap_or_default();
        if address.is_empty() {
            return;
        }
        let entry = ServerEntry {
            name: if name.is_empty() {
                default_name.to_owned()
            } else {
                name
            },
            address,
        };
        match self.index {
            Some(i) => self.servers.borrow_mut().update(i, entry),
            None => self.servers.borrow_mut().add(entry),
        }
        out.push(ScreenAction::Pop);
    }
}

impl Screen for EditServerScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        let keep = (
            self.widgets.edit(NAME).map(|e| e.value().to_owned()),
            self.widgets.edit(ADDRESS).map(|e| e.value().to_owned()),
        );
        self.widgets.clear();
        let current = self
            .index
            .and_then(|i| self.servers.borrow().servers().get(i).cloned())
            .unwrap_or_default();
        let mut name = EditBox::new(NAME, [width / 2 - 100, 66, 200, 20])
            .with_hint("lang:selectServer.defaultName");
        name.set_value(&keep.0.unwrap_or(current.name));
        name.take_changed();
        let mut address =
            EditBox::new(ADDRESS, [width / 2 - 100, 106, 200, 20]).with_max_length(128);
        address.set_value(&keep.1.unwrap_or(current.address));
        address.take_changed();
        self.widgets.add(name);
        self.widgets.add(address);
        self.widgets.add(Button::new(
            DONE,
            [width / 2 - 100, height / 4 + 96 + 18, 200, 20],
            "lang:gui.done",
        ));
        self.widgets.add(Button::new(
            CANCEL,
            [width / 2 - 100, height / 4 + 120 + 18, 200, 20],
            "lang:gui.cancel",
        ));
        self.widgets.set_focus(Some(NAME), 0);
        self.update_buttons();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let key = if self.index.is_some() {
            "manageServer.edit.title"
        } else {
            "manageServer.add.title"
        };
        let title = cx.lang(key);
        cx.centered_text(&title, self.gui.0 / 2, 17, WHITE, true);
        let name_label = cx.lang("addServer.enterName");
        cx.text(&name_label, self.gui.0 / 2 - 100, 53, 0xFFA0_A0A0, true);
        let ip_label = cx.lang("addServer.enterIp");
        cx.text(&ip_label, self.gui.0 / 2 - 100, 94, 0xFFA0_A0A0, true);
        let hint = cx.lang("selectServer.defaultName");
        if let Some(e) = self.widgets.edit_mut(NAME) {
            e.hint = Some(hint);
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
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            match (id, kind) {
                (DONE, WidgetEventKind::Pressed) | (ADDRESS, WidgetEventKind::Submitted) => {
                    self.done("Minecraft Server", out);
                }
                (CANCEL, WidgetEventKind::Pressed) => out.push(ScreenAction::Pop),
                _ => {}
            }
        }
        self.update_buttons();
        used
    }
}
