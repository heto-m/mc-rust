//! `SocialInteractionsScreen`: the online players, with each one's chat hidden or shown for this session.

use std::cell::RefCell;
use std::rc::Rc;

use mcr_protocol::types::text::Component;
use mcr_ui::{
    Button, EditBox, Env, Key, RenderCx, Screen, ScreenAction, SpriteId, TooltipLines, UiInput,
    WidgetEventKind, WidgetSet, contains, draw_menu_background,
};
use rustc_hash::{FxHashMap, FxHashSet};
use uuid::Uuid;

pub type HiddenPlayers = Rc<RefCell<FxHashSet<Uuid>>>;

/// Faces the screen asks for while drawing and the game fills from its skin cache.
#[derive(Debug, Default)]
pub struct Faces {
    pub wanted: Vec<Uuid>,
    pub sprites: FxHashMap<Uuid, SpriteId>,
}

pub type SharedFaces = Rc<RefCell<Faces>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocialPlayer {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    All,
    Hidden,
    Blocked,
}

const ALL: u32 = 1;
const HIDDEN: u32 = 2;
const BLOCKED: u32 = 3;
const SEARCH: u32 = 4;
const DONE: u32 = 5;
const ROW: i32 = 36;
const ROW_WIDTH: i32 = 220;
const BG_FILL: u32 = 0xFF4A_4A4A;
const BG_FILL_REMOVED: u32 = 0xFF30_3030;
const STATUS_COLOR: u32 = 0x8CFF_FFFF;
const MAX_ROWS: usize = 1024;

pub struct SocialInteractionsScreen {
    players: Vec<SocialPlayer>,
    own: Uuid,
    server_label: Option<(String, usize)>,
    hidden: HiddenPlayers,
    faces: SharedFaces,
    toggle_key: Option<Key>,
    page: Page,
    /// Players listed on the current page, kept after they are shown again so the row stays.
    rows: Vec<Uuid>,
    scroll: i32,
    widgets: WidgetSet,
    gui: (i32, i32),
    mouse: (f32, f32),
}

impl std::fmt::Debug for SocialInteractionsScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SocialInteractionsScreen")
            .field("players", &self.players.len())
            .finish_non_exhaustive()
    }
}

impl SocialInteractionsScreen {
    pub fn new(
        mut players: Vec<SocialPlayer>,
        own: Uuid,
        server_name: String,
        hidden: HiddenPlayers,
        faces: SharedFaces,
        toggle_key: Option<Key>,
    ) -> Self {
        players.sort_by_key(|p| p.name.to_lowercase());
        players.truncate(MAX_ROWS);
        let count = players.len();
        let mut screen = Self {
            players,
            own,
            server_label: Some((server_name, count)),
            hidden,
            faces,
            toggle_key,
            page: Page::All,
            rows: Vec::new(),
            scroll: 0,
            widgets: WidgetSet::default(),
            gui: (0, 0),
            mouse: (0.0, 0.0),
        };
        screen.refresh_rows();
        screen
    }

    fn window_height(&self) -> i32 {
        (self.gui.1 - 128 - 16).max(52)
    }

    fn list_top(&self) -> i32 {
        88
    }

    fn list_bottom(&self) -> i32 {
        80 + self.window_height() - 8
    }

    fn row_left(&self) -> i32 {
        self.gui.0 / 2 - ROW_WIDTH / 2
    }

    fn filter(&self) -> String {
        self.widgets
            .edit(SEARCH)
            .map(|e| e.value().to_lowercase())
            .unwrap_or_default()
    }

    fn refresh_rows(&mut self) {
        let filter = self.filter();
        let hidden = self.hidden.clone();
        let hidden = hidden.borrow();
        self.rows = self
            .players
            .iter()
            .filter(|p| match self.page {
                Page::All => true,
                Page::Hidden => hidden.contains(&p.id),
                Page::Blocked => false,
            })
            .filter(|p| filter.is_empty() || p.name.to_lowercase().contains(&filter))
            .map(|p| p.id)
            .collect();
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        let content = self.rows.len() as i32 * ROW;
        let view = self.list_bottom() - self.list_top();
        self.scroll = self.scroll.clamp(0, (content - view).max(0));
    }

    fn show_page(&mut self, page: Page) {
        self.page = page;
        self.refresh_rows();
    }

    fn label_tabs(&mut self, cx: &mut RenderCx<'_, '_>) {
        let hint = format!("§o{}", cx.lang("gui.socialInteractions.search_hint"));
        if let Some(e) = self.widgets.edit_mut(SEARCH) {
            e.hint = Some(hint);
        }
        for (id, key, page) in [
            (ALL, "gui.socialInteractions.tab_all", Page::All),
            (HIDDEN, "gui.socialInteractions.tab_hidden", Page::Hidden),
            (BLOCKED, "gui.socialInteractions.tab_blocked", Page::Blocked),
        ] {
            let text = cx.lang(key);
            if let Some(b) = self.widgets.button_mut(id) {
                b.label = if page == self.page {
                    format!("§n{text}")
                } else {
                    text
                };
            }
        }
    }

    fn row_top(&self, index: usize) -> i32 {
        self.list_top() + index as i32 * ROW - self.scroll
    }

    fn player(&self, id: Uuid) -> Option<&SocialPlayer> {
        self.players.iter().find(|p| p.id == id)
    }

    /// The hide/show button of a row, absent for the local player.
    fn toggle_rect(&self, index: usize) -> Option<[i32; 4]> {
        let id = *self.rows.get(index)?;
        (id != self.own).then(|| {
            let right = self.row_left() + ROW_WIDTH;
            [right - 48, self.row_top(index) + 6, 20, 20]
        })
    }

    fn report_rect(&self, index: usize) -> [i32; 4] {
        let right = self.row_left() + ROW_WIDTH;
        [right - 24, self.row_top(index) + 6, 20, 20]
    }

    fn row_visible(&self, index: usize) -> bool {
        let top = self.row_top(index);
        top + ROW > self.list_top() && top < self.list_bottom()
    }

    fn toggle_hidden(&mut self, id: Uuid, out: &mut Vec<ScreenAction>) {
        let hide = !self.hidden.borrow().contains(&id);
        if hide {
            self.hidden.borrow_mut().insert(id);
        } else {
            self.hidden.borrow_mut().remove(&id);
        }
        out.push(ScreenAction::ClickSound);
    }
}

impl Screen for SocialInteractionsScreen {
    fn init(&mut self, width: i32, height: i32) {
        let search = self
            .widgets
            .edit(SEARCH)
            .map(|e| e.value().to_owned())
            .unwrap_or_default();
        self.gui = (width, height);
        self.widgets.clear();
        let left = self.row_left();
        let right = left + ROW_WIDTH;
        let w = ROW_WIDTH / 3;
        self.widgets.add(Button::new(ALL, [left, 45, w, 20], ""));
        self.widgets.add(Button::new(
            HIDDEN,
            [(left + right - w) / 2 + 1, 45, w, 20],
            "",
        ));
        self.widgets
            .add(Button::new(BLOCKED, [right - w + 1, 45, w, 20], ""));
        let margin_x = (width - 238) / 2;
        let mut edit = EditBox::new(SEARCH, [margin_x + 28, 74, 200, 15]).with_max_length(16);
        edit.bordered = false;
        edit.set_value(&search);
        self.widgets.add(edit);
        self.widgets.add(Button::new(
            DONE,
            [width / 2 - 100, height - 27, 200, 20],
            "lang:gui.done",
        ));
        self.show_page(self.page);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        self.mouse = cx.mouse;
        let (width, _) = self.gui;
        let title = cx.lang("gui.socialInteractions.title");
        cx.centered_text(&title, width / 2, 15, 0xFFFF_FFFF, true);
        let margin_x = (width - 238) / 2 + 3;
        cx.sprite(
            "social_interactions/background",
            margin_x,
            64,
            236,
            self.window_height() + 16,
        );
        cx.sprite("icon/search", margin_x + 10, 76, 12, 12);
        if let Some((server, count)) = &self.server_label {
            let key = if *count > 1 {
                "gui.socialInteractions.server_label.multiple"
            } else {
                "gui.socialInteractions.server_label.single"
            };
            let label = cx.lang_fmt(key, &[server, &count.to_string()]);
            cx.text(&label, (width - 238) / 2 + 8, 35, 0xFFFF_FFFF, false);
        }
        let (top, bottom) = (self.list_top(), self.list_bottom());
        let mut wanted = Vec::new();
        if self.rows.is_empty() {
            let key = if !self.filter().is_empty() {
                Some("gui.socialInteractions.search_empty")
            } else {
                match self.page {
                    Page::All => None,
                    Page::Hidden => Some("gui.socialInteractions.empty_hidden"),
                    Page::Blocked => Some("gui.socialInteractions.empty_blocked"),
                }
            };
            if let Some(key) = key {
                let text = cx.lang(key);
                cx.centered_text(&text, width / 2, (72 + bottom) / 2, 0xFFAA_AAAA, false);
            }
        }
        cx.frame.push_scissor(0, top, width, bottom);
        let hidden = self.hidden.borrow().clone();
        let mut tooltip = None;
        for (index, &id) in self.rows.iter().enumerate() {
            if !self.row_visible(index) {
                continue;
            }
            let Some(player) = self.player(id) else {
                continue;
            };
            let y = self.row_top(index);
            let left = self.row_left();
            let is_hidden = hidden.contains(&id);
            let status = is_hidden.then(|| cx.lang("gui.socialInteractions.status_hidden"));
            cx.frame.fill_rect(
                left,
                y,
                left + ROW_WIDTH,
                y + ROW - 4,
                if status.is_some() {
                    BG_FILL_REMOVED
                } else {
                    BG_FILL
                },
            );
            let (skin_x, skin_y) = (left + 4, y + (ROW - 4 - 24) / 2);
            wanted.push(id);
            if let Some(&face) = self.faces.borrow().sprites.get(&id) {
                cx.frame
                    .sprite_tinted(face, skin_x, skin_y, 24, 24, 0xFFFF_FFFF);
                if is_hidden {
                    cx.frame
                        .fill_rect(skin_x, skin_y, skin_x + 24, skin_y + 24, 0xBE00_0000);
                }
            }
            let text_x = skin_x + 28;
            match &status {
                Some(status) => {
                    let ty = y + (ROW - 4 - 18) / 2;
                    cx.text(&player.name, text_x, ty, 0xFFFF_FFFF, false);
                    cx.text(&format!("§o{status}"), text_x, ty + 12, STATUS_COLOR, false);
                }
                None => {
                    let ty = y + (ROW - 4 - 9) / 2;
                    cx.text(&player.name, text_x, ty, 0xFFFF_FFFF, false);
                }
            }
            if let Some(rect) = self.toggle_rect(index) {
                let hot = contains(rect, cx.mouse.0, cx.mouse.1);
                let sprite = match (is_hidden, hot) {
                    (true, true) => "social_interactions/unmute_button_highlighted",
                    (true, false) => "social_interactions/unmute_button",
                    (false, true) => "social_interactions/mute_button_highlighted",
                    (false, false) => "social_interactions/mute_button",
                };
                cx.sprite(sprite, rect[0], rect[1], rect[2], rect[3]);
                if hot {
                    let key = if is_hidden {
                        "gui.socialInteractions.tooltip.show"
                    } else {
                        "gui.socialInteractions.tooltip.hide"
                    };
                    tooltip = Some(key);
                }
                let report = self.report_rect(index);
                cx.sprite(
                    "social_interactions/report_button_disabled",
                    report[0],
                    report[1],
                    report[2],
                    report[3],
                );
                if contains(report, cx.mouse.0, cx.mouse.1) {
                    tooltip = Some("gui.socialInteractions.tooltip.report.disabled");
                }
            }
        }
        cx.frame.pop_scissor();
        self.faces.borrow_mut().wanted = wanted;
        self.label_tabs(cx);
        self.widgets.render(cx);
        if let Some(key) = tooltip {
            let mut lines = TooltipLines::default();
            lines.push_component(Component::translatable(key, Vec::new()));
            cx.set_tooltip(lines);
        }
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        let typing = self.widgets.focused() == Some(SEARCH);
        match *input {
            UiInput::Key {
                key: Key::Escape, ..
            } => {
                out.push(ScreenAction::Pop);
                return true;
            }
            UiInput::Key { key, .. } if !typing && Some(key) == self.toggle_key => {
                out.push(ScreenAction::Pop);
                return true;
            }
            UiInput::MouseMove { x, y } => self.mouse = (x, y),
            UiInput::Scroll { dy, .. } => {
                self.scroll -= (dy * ROW as f32 / 2.0) as i32;
                self.clamp_scroll();
                return true;
            }
            UiInput::MousePress { x, y, .. }
                if (self.list_top() as f32..self.list_bottom() as f32).contains(&y) =>
            {
                for index in 0..self.rows.len() {
                    if !self.row_visible(index) {
                        continue;
                    }
                    if let Some(rect) = self.toggle_rect(index)
                        && contains(rect, x, y)
                    {
                        let id = self.rows[index];
                        self.toggle_hidden(id, out);
                        return true;
                    }
                }
            }
            _ => {}
        }
        let before = self.filter();
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for e in events {
            if e.kind != WidgetEventKind::Pressed {
                continue;
            }
            out.push(ScreenAction::ClickSound);
            match e.id {
                ALL => self.show_page(Page::All),
                HIDDEN => self.show_page(Page::Hidden),
                BLOCKED => self.show_page(Page::Blocked),
                DONE => out.push(ScreenAction::Pop),
                _ => {}
            }
        }
        if self.filter() != before {
            self.refresh_rows();
        }
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }
}

/// `ChatListener#guessChatUUID`: the name between the first `<` and `>` of a system message.
pub fn guessed_sender(plain: &str) -> Option<&str> {
    let start = plain.find('<')? + 1;
    let len = plain[start..].find('>')?;
    Some(&plain[start..start + len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_the_bracketed_name() {
        assert_eq!(guessed_sender("<Alex> hi"), Some("Alex"));
        assert_eq!(guessed_sender("[Server] <Bob> x"), Some("Bob"));
        assert_eq!(guessed_sender("no name"), None);
        assert_eq!(guessed_sender("<unterminated"), None);
    }

    fn players() -> Vec<SocialPlayer> {
        ["zed", "Alex", "bob"]
            .iter()
            .enumerate()
            .map(|(i, n)| SocialPlayer {
                id: Uuid::from_u128(i as u128 + 1),
                name: (*n).to_owned(),
            })
            .collect()
    }

    #[test]
    fn pages_list_everyone_or_the_hidden() {
        let hidden = HiddenPlayers::default();
        let mut s = SocialInteractionsScreen::new(
            players(),
            Uuid::from_u128(1),
            "srv".into(),
            hidden.clone(),
            SharedFaces::default(),
            None,
        );
        s.init(400, 300);
        let names: Vec<_> = s
            .rows
            .iter()
            .map(|id| s.player(*id).unwrap().name.clone())
            .collect();
        assert_eq!(names, ["Alex", "bob", "zed"]);
        hidden.borrow_mut().insert(Uuid::from_u128(3));
        s.show_page(Page::Hidden);
        assert_eq!(s.rows, vec![Uuid::from_u128(3)]);
        s.show_page(Page::Blocked);
        assert!(s.rows.is_empty());
    }
}
