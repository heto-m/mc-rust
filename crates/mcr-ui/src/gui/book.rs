use std::ops::Range;

use mcr_protocol::types::text::{ClickEvent, Component};

use super::clicks::click_event_actions;
use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, MouseButton, UiInput, contains};
use super::multiline::MultiLineEdit;
use super::screen::{Screen, ScreenAction};
use super::widgets::{Button, WidgetEvent, WidgetEventKind, WidgetSet};
use crate::style::Cell;
use crate::text::{ClickSpan, click_at, flatten_events};

const BLACK: u32 = 0xFF00_0000;
const DARK_GRAY: u32 = 0xFF55_5555;
const TEXT_WIDTH: f32 = 114.0;
const VISIBLE_LINES: usize = 14;
const MAX_PAGES: usize = 100;
const MAX_PAGE_CHARS: usize = 1024;
const MAX_TITLE: usize = 15;
const PAGE_TURN: &str = "item.book.page_turn";

const BACK: u32 = 1;
const FORWARD: u32 = 2;
const SIGN: u32 = 3;
const DONE: u32 = 4;
const FINALIZE: u32 = 5;
const CANCEL: u32 = 6;

fn left(gui_width: i32) -> i32 {
    (gui_width - 192) / 2
}

fn draw_background(cx: &mut RenderCx<'_, '_>, gui_width: i32) {
    super::draw_dim_background(cx);
    cx.texture("book", (0, 0), (left(gui_width), 2, 192, 192));
}

fn draw_page_buttons(cx: &mut RenderCx<'_, '_>, widgets: &WidgetSet) {
    for (id, forward) in [(BACK, false), (FORWARD, true)] {
        let Some(super::widgets::Widget::Button(b)) = widgets.find(id) else {
            continue;
        };
        if !b.visible {
            continue;
        }
        let hovered = contains(b.rect, cx.mouse.0, cx.mouse.1);
        let name = format!(
            "widget/page_{}{}",
            if forward { "forward" } else { "backward" },
            if hovered { "_highlighted" } else { "" }
        );
        cx.sprite(&name, b.rect[0], b.rect[1], 23, 13);
    }
}

fn add_page_buttons(widgets: &mut WidgetSet, gui_width: i32) {
    let l = left(gui_width);
    widgets.add(Button::new(BACK, [l + 43, 2 + 157, 23, 13], "").flat());
    widgets.add(Button::new(FORWARD, [l + 116, 2 + 157, 23, 13], "").flat());
}

fn page_indicator(cx: &mut RenderCx<'_, '_>, gui_width: i32, page: usize, total: usize) {
    let text = cx.lang_fmt(
        "book.pageIndicator",
        &[&(page + 1).to_string(), &total.max(1).to_string()],
    );
    let w = cx.text_width(&text);
    cx.text(&text, left(gui_width) + 148 - w, 2 + 16, BLACK, false);
}

fn set_visible(widgets: &mut WidgetSet, id: u32, visible: bool) {
    if let Some(b) = widgets.button_mut(id) {
        b.visible = visible;
    }
}

enum Mode {
    Edit,
    Sign,
}

/// Edits a book and quill; signing asks for a title and finalizes the book.
pub struct BookEditScreen {
    author: String,
    pages: Vec<String>,
    page: usize,
    gui: (i32, i32),
    editor: MultiLineEdit,
    title: EditBox,
    mode: Mode,
    widgets: WidgetSet,
}

impl std::fmt::Debug for BookEditScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BookEditScreen")
            .field("page", &self.page)
            .finish()
    }
}

impl BookEditScreen {
    pub fn new(author: &str, mut pages: Vec<String>) -> Self {
        pages.truncate(MAX_PAGES);
        if pages.is_empty() {
            pages.push(String::new());
        }
        let mut title = EditBox::new(0, [0, 0, 114, 20]).with_max_length(MAX_TITLE);
        title.bordered = false;
        let mut screen = Self {
            author: author.to_owned(),
            pages,
            page: 0,
            gui: (0, 0),
            editor: MultiLineEdit::new(0, [0, 28, 122, 134])
                .with_limits(MAX_PAGE_CHARS, Some(VISIBLE_LINES))
                .plain(BLACK),
            title,
            mode: Mode::Edit,
            widgets: WidgetSet::default(),
        };
        screen.load_page();
        screen
    }

    pub fn pages(&self) -> &[String] {
        &self.pages
    }

    fn load_page(&mut self) {
        let text = self.pages[self.page].clone();
        self.editor.set_value(&text);
        self.editor.take_changed();
    }

    fn store_page(&mut self) {
        self.pages[self.page] = self.editor.value();
    }

    fn turn(&mut self, forward: bool, out: &mut Vec<ScreenAction>) {
        self.store_page();
        if forward {
            if self.page + 1 >= self.pages.len() && self.pages.len() < MAX_PAGES {
                self.pages.push(String::new());
            }
            if self.page + 1 < self.pages.len() {
                self.page += 1;
            } else {
                return;
            }
        } else if self.page > 0 {
            self.page -= 1;
        } else {
            return;
        }
        out.push(ScreenAction::PlaySound(PAGE_TURN));
        self.load_page();
    }

    fn finish(&mut self, title: Option<String>, out: &mut Vec<ScreenAction>) {
        self.store_page();
        let mut pages = self.pages.clone();
        if title.is_none() {
            while pages.last().is_some_and(String::is_empty) {
                pages.pop();
            }
        }
        out.push(ScreenAction::SubmitBook { pages, title });
        out.push(ScreenAction::Pop);
    }

    fn sign_title(&self) -> Option<String> {
        let title = self.title.value().trim().to_owned();
        (!title.is_empty()).then_some(title)
    }

    fn layout(&mut self) {
        let (w, h) = self.gui;
        self.widgets.clear();
        add_page_buttons(&mut self.widgets, w);
        let l = left(w);
        self.editor.rect = [(w - 114) / 2 - 8, 28, 122, 134];
        match self.mode {
            Mode::Edit => {
                self.widgets.add(Button::new(
                    SIGN,
                    [w / 2 - 98 - 2, 196, 98, 20],
                    "lang:book.signButton",
                ));
                self.widgets
                    .add(Button::new(DONE, [w / 2 + 2, 196, 98, 20], "lang:gui.done"));
            }
            Mode::Sign => {
                let mut finalize = Button::new(
                    FINALIZE,
                    [w / 2 - 100, 196, 98, 20],
                    "lang:book.finalizeButton",
                );
                finalize.active = self.sign_title().is_some();
                self.widgets.add(finalize);
                self.widgets.add(Button::new(
                    CANCEL,
                    [w / 2 + 2, 196, 98, 20],
                    "lang:gui.cancel",
                ));
            }
        }
        self.title.rect = [(w - 114) / 2 - 3, 50, 114, 20];
        let _ = (h, l);
    }

    fn update_buttons(&mut self) {
        let edit = matches!(self.mode, Mode::Edit);
        set_visible(&mut self.widgets, BACK, edit && self.page > 0);
        set_visible(&mut self.widgets, FORWARD, edit);
        if let Some(b) = self.widgets.button_mut(FINALIZE) {
            b.active = !self.title.value().trim().is_empty();
        }
    }

    fn pressed(&mut self, id: u32, out: &mut Vec<ScreenAction>) {
        out.push(ScreenAction::ClickSound);
        match id {
            BACK => self.turn(false, out),
            FORWARD => self.turn(true, out),
            SIGN => {
                self.store_page();
                self.mode = Mode::Sign;
                self.layout();
            }
            DONE => self.finish(None, out),
            FINALIZE => {
                if let Some(title) = self.sign_title() {
                    self.finish(Some(title), out);
                }
            }
            CANCEL => {
                self.mode = Mode::Edit;
                self.layout();
                self.load_page();
            }
            _ => {}
        }
    }
}

impl Screen for BookEditScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.layout();
        self.update_buttons();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_background(cx, self.gui.0);
        self.update_buttons();
        let l = left(self.gui.0);
        match self.mode {
            Mode::Edit => {
                self.editor.render(cx, true);
                page_indicator(cx, self.gui.0, self.page, self.pages.len());
            }
            Mode::Sign => {
                let label = cx.lang("book.editTitle");
                let w = cx.text_width(&label);
                cx.text(&label, l + 36 + (114 - w) / 2, 34, BLACK, false);
                self.render_title(cx);
                let by = cx.lang_fmt("book.byAuthor", &[&self.author]);
                let w = cx.text_width(&by);
                cx.text(&by, l + 36 + (114 - w) / 2, 60, DARK_GRAY, false);
                let warning = cx.lang("book.finalizeWarning");
                let cells = crate::text::parse_legacy(&warning);
                for (i, range) in cx
                    .frame
                    .ui()
                    .wrap(&cells, TEXT_WIDTH)
                    .into_iter()
                    .enumerate()
                {
                    cx.frame.cells(
                        &cells[range],
                        (l + 36) as f32,
                        (82 + i as i32 * 9) as f32,
                        BLACK,
                        false,
                    );
                }
            }
        }
        self.widgets.render(cx);
        draw_page_buttons(cx, &self.widgets);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        match (&self.mode, *input) {
            (
                _,
                UiInput::Key {
                    key: Key::PageUp, ..
                },
            ) if matches!(self.mode, Mode::Edit) => {
                self.turn(false, out);
                self.update_buttons();
                return true;
            }
            (
                _,
                UiInput::Key {
                    key: Key::PageDown, ..
                },
            ) if matches!(self.mode, Mode::Edit) => {
                self.turn(true, out);
                self.update_buttons();
                return true;
            }
            (Mode::Edit, UiInput::Key { key, mods, .. }) => {
                if key == Key::Escape {
                    self.finish(None, out);
                    return true;
                }
                self.editor.key(key, mods, env.clipboard);
                return true;
            }
            (Mode::Edit, UiInput::Char(c)) => {
                self.editor.char(c);
                return true;
            }
            (Mode::Sign, UiInput::Key { key, mods, .. }) => {
                match key {
                    Key::Escape => self.pressed(CANCEL, out),
                    Key::Enter => {
                        if let Some(title) = self.sign_title() {
                            self.finish(Some(title), out);
                        }
                    }
                    _ => {
                        self.title.key(key, mods, env.clipboard);
                    }
                }
                self.update_buttons();
                return true;
            }
            (Mode::Sign, UiInput::Char(c)) => {
                self.title.char(c);
                self.update_buttons();
                return true;
            }
            (
                Mode::Edit,
                UiInput::MousePress {
                    x,
                    y,
                    button: MouseButton::Left,
                    mods,
                    ..
                },
            ) if self.editor.contains(x, y) => {
                self.editor.click(x, y, env.now_ms, mods.shift);
                return true;
            }
            _ => {}
        }
        let mut events = Vec::new();
        self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            if kind == WidgetEventKind::Pressed {
                self.pressed(id, out);
            }
        }
        self.update_buttons();
        true
    }
}

impl BookEditScreen {
    fn render_title(&mut self, cx: &mut RenderCx<'_, '_>) {
        let [x, y, w, _] = self.title.rect;
        let text = self.title.value().to_owned();
        let width = cx.text_width(&text);
        let tx = x + (w - width) / 2;
        let ty = y + 6;
        cx.text(&text, tx, ty, BLACK, false);
        let prefix: String = text.chars().take(self.title.cursor()).collect();
        let cursor_x = tx + cx.text_width(&prefix);
        if (cx.now_ms / 300).is_multiple_of(2) {
            if self.title.cursor() >= self.title.len() {
                cx.text("_", cursor_x, ty, BLACK, false);
            } else {
                cx.frame
                    .fill_rect(cursor_x, ty - 1, cursor_x + 1, ty + 9, BLACK);
            }
        }
    }
}

/// One page of a written book wrapped to the text column, with what a click on it would do.
#[derive(Debug)]
pub(crate) struct PageLayout {
    cells: Vec<Cell>,
    spans: Vec<ClickSpan>,
    lines: Vec<Range<usize>>,
    ends: Vec<Vec<f32>>,
    page: usize,
    source: Component,
}

impl PageLayout {
    pub(crate) fn is_for(&self, component: &Component, page: usize) -> bool {
        self.page == page && self.source == *component
    }

    pub(crate) fn new(cx: &mut RenderCx<'_, '_>, component: &Component, page: usize) -> Self {
        let (cells, spans) = flatten_events(cx.frame.ui().lang(), component);
        let lines = cx.frame.ui().wrap(&cells, TEXT_WIDTH);
        let ends = lines
            .iter()
            .map(|range| {
                let mut x = 0.0;
                cells[range.clone()]
                    .iter()
                    .map(|c| {
                        x += cx.frame.ui().width(std::slice::from_ref(c));
                        x
                    })
                    .collect()
            })
            .collect();
        Self {
            cells,
            spans,
            lines,
            ends,
            page,
            source: component.clone(),
        }
    }

    pub(crate) fn draw(&self, cx: &mut RenderCx<'_, '_>, gui_width: i32) {
        for (i, range) in self.lines.iter().take(VISIBLE_LINES).enumerate() {
            cx.frame.cells(
                &self.cells[range.clone()],
                (left(gui_width) + 36) as f32,
                (2 + 30 + i as i32 * 9) as f32,
                BLACK,
                false,
            );
        }
    }

    pub(crate) fn event_at(&self, gui_width: i32, x: f32, y: f32) -> Option<ClickEvent> {
        let rel_y = y - (2 + 30) as f32;
        let rel_x = x - (left(gui_width) + 36) as f32;
        if rel_y < 0.0 || rel_x < 0.0 {
            return None;
        }
        let row = (rel_y / 9.0) as usize;
        if row >= self.lines.len().min(VISIBLE_LINES) {
            return None;
        }
        let col = self.ends[row].iter().position(|&end| rel_x < end)?;
        click_at(&self.spans, self.lines[row].start + col).cloned()
    }
}

/// Reads a written book; clicking a link, command or page reference acts like in chat.
pub struct BookViewScreen {
    pages: Vec<Component>,
    page: usize,
    gui: (i32, i32),
    widgets: WidgetSet,
    laid: Option<PageLayout>,
}

impl std::fmt::Debug for BookViewScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BookViewScreen")
            .field("page", &self.page)
            .finish()
    }
}

impl BookViewScreen {
    pub fn new(pages: Vec<Component>) -> Self {
        Self {
            pages,
            page: 0,
            gui: (0, 0),
            widgets: WidgetSet::default(),
            laid: None,
        }
    }

    pub fn page(&self) -> usize {
        self.page
    }

    fn set_page(&mut self, page: usize, out: &mut Vec<ScreenAction>) {
        let page = page.min(self.pages.len().saturating_sub(1));
        if page != self.page {
            self.page = page;
            out.push(ScreenAction::PlaySound(PAGE_TURN));
        }
    }

    fn update_buttons(&mut self) {
        set_visible(&mut self.widgets, BACK, self.page > 0);
        set_visible(&mut self.widgets, FORWARD, self.page + 1 < self.pages.len());
    }

    fn lay_out(&mut self, cx: &mut RenderCx<'_, '_>) {
        let Some(component) = self.pages.get(self.page) else {
            self.laid = None;
            return;
        };
        if self
            .laid
            .as_ref()
            .is_some_and(|l| l.is_for(component, self.page))
        {
            return;
        }
        self.laid = Some(PageLayout::new(cx, component, self.page));
    }

    fn event_at(&self, x: f32, y: f32) -> Option<ClickEvent> {
        self.laid.as_ref()?.event_at(self.gui.0, x, y)
    }

    fn click(&mut self, event: ClickEvent, out: &mut Vec<ScreenAction>) {
        match event {
            ClickEvent::ChangePage(n) => {
                let target = usize::try_from(n.saturating_sub(1)).unwrap_or(0);
                self.set_page(target, out);
            }
            other => click_event_actions(other, out),
        }
    }
}

impl Screen for BookViewScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        add_page_buttons(&mut self.widgets, width);
        self.widgets.add(Button::new(
            DONE,
            [(width - 200) / 2, 196, 200, 20],
            "lang:gui.done",
        ));
        self.update_buttons();
        self.laid = None;
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_background(cx, self.gui.0);
        self.lay_out(cx);
        self.update_buttons();
        page_indicator(cx, self.gui.0, self.page, self.pages.len());
        if let Some(laid) = &self.laid {
            laid.draw(cx, self.gui.0);
        }
        self.widgets.render(cx);
        draw_page_buttons(cx, &self.widgets);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if let UiInput::Key { key, .. } = *input {
            match key {
                Key::Escape => out.push(ScreenAction::Pop),
                Key::PageUp | Key::Left => self.set_page(self.page.saturating_sub(1), out),
                Key::PageDown | Key::Right => self.set_page(self.page + 1, out),
                _ => {}
            }
            self.update_buttons();
            return true;
        }
        if let UiInput::MousePress { x, y, .. } = *input
            && let Some(event) = self.event_at(x, y)
        {
            self.click(event, out);
            return true;
        }
        let mut events = Vec::new();
        self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            if kind != WidgetEventKind::Pressed {
                continue;
            }
            out.push(ScreenAction::ClickSound);
            match id {
                BACK => self.set_page(self.page.saturating_sub(1), out),
                FORWARD => self.set_page(self.page + 1, out),
                DONE => out.push(ScreenAction::Pop),
                _ => {}
            }
        }
        self.update_buttons();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::env::{MemoryClipboard, NoView};

    fn send(screen: &mut impl Screen, input: UiInput) -> Vec<ScreenAction> {
        let mut clip = MemoryClipboard::default();
        let mut env = Env {
            view: &NoView,
            clipboard: &mut clip,
            now_ms: 0,
        };
        let mut out = Vec::new();
        screen.input(&mut env, &input, &mut out);
        out
    }

    #[test]
    fn page_forward_appends_and_stores_text() {
        let mut s = BookEditScreen::new("Steve", vec![]);
        s.init(320, 240);
        send(&mut s, UiInput::Char('a'));
        send(&mut s, UiInput::key(Key::PageDown));
        send(&mut s, UiInput::Char('b'));
        send(&mut s, UiInput::key(Key::PageUp));
        assert_eq!(s.page, 0);
        assert_eq!(s.pages(), ["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn done_drops_trailing_empty_pages() {
        let mut s = BookEditScreen::new("Steve", vec!["x".into(), String::new(), String::new()]);
        s.init(320, 240);
        let out = send(&mut s, UiInput::key(Key::Escape));
        assert!(matches!(
            &out[0],
            ScreenAction::SubmitBook { pages, title: None } if pages == &["x".to_owned()]
        ));
    }

    #[test]
    fn signing_needs_a_title_and_cancel_returns_to_editing() {
        let mut s = BookEditScreen::new("Steve", vec!["x".into()]);
        s.init(320, 240);
        s.pressed(SIGN, &mut Vec::new());
        assert!(send(&mut s, UiInput::key(Key::Enter)).is_empty());
        for c in "My Book".chars() {
            send(&mut s, UiInput::Char(c));
        }
        let out = send(&mut s, UiInput::key(Key::Enter));
        assert!(matches!(
            &out[0],
            ScreenAction::SubmitBook { title: Some(t), .. } if t == "My Book"
        ));
        let mut s = BookEditScreen::new("Steve", vec!["x".into()]);
        s.init(320, 240);
        s.pressed(SIGN, &mut Vec::new());
        send(&mut s, UiInput::key(Key::Escape));
        assert!(matches!(s.mode, Mode::Edit));
    }

    #[test]
    fn title_is_capped_at_fifteen_characters() {
        let mut s = BookEditScreen::new("Steve", vec![]);
        s.init(320, 240);
        s.pressed(SIGN, &mut Vec::new());
        for _ in 0..30 {
            send(&mut s, UiInput::Char('t'));
        }
        assert_eq!(s.title.len(), 15);
    }

    #[test]
    fn view_pages_clamp_and_change_page_links_jump() {
        let pages = vec![
            Component::text("a"),
            Component::text("b"),
            Component::text("c"),
        ];
        let mut s = BookViewScreen::new(pages);
        s.init(320, 240);
        send(&mut s, UiInput::key(Key::PageDown));
        send(&mut s, UiInput::key(Key::PageDown));
        send(&mut s, UiInput::key(Key::PageDown));
        assert_eq!(s.page(), 2);
        s.click(ClickEvent::ChangePage(1), &mut Vec::new());
        assert_eq!(s.page(), 0);
    }
}
