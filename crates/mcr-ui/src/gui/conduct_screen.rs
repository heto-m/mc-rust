use std::ops::Range;

use super::env::{Env, RenderCx};
use super::input::{Key, UiInput};
use super::list::ScrollList;
use super::menus::{WHITE, draw_menu_background};
use super::screen::{Screen, ScreenAction};
use super::widgets::{Button, WidgetEventKind, WidgetSet};
use crate::style::Cell;

const REMEMBER: u32 = 1;
const ACCEPT: u32 = 2;
const DECLINE: u32 = 3;
const LINE_HEIGHT: i32 = 9;
const MAX_WIDTH: i32 = 360;

/// The server's code of conduct: scrollable text, a "remember" toggle, acknowledge or disconnect.
pub struct ConductScreen {
    text: String,
    cells: Vec<Cell>,
    lines: Vec<Range<usize>>,
    wrapped_for: i32,
    list: ScrollList,
    widgets: WidgetSet,
    remember: bool,
    gui: (i32, i32),
    hash: i32,
}

impl std::fmt::Debug for ConductScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConductScreen")
            .field("hash", &self.hash)
            .finish()
    }
}

impl ConductScreen {
    pub fn new(text: &str, hash: i32) -> Self {
        Self {
            text: text.to_owned(),
            cells: crate::text::parse_legacy(text),
            lines: Vec::new(),
            wrapped_for: -1,
            list: ScrollList::new([0; 4], LINE_HEIGHT),
            widgets: WidgetSet::default(),
            remember: false,
            gui: (0, 0),
            hash,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn remember(&self) -> bool {
        self.remember
    }

    fn panel_width(&self) -> i32 {
        (self.gui.0 - 40).clamp(80, MAX_WIDTH)
    }

    fn remember_label(&self, cx_label: &str) -> String {
        format!("[{}] {}", if self.remember { "x" } else { " " }, cx_label)
    }

    fn answer(&self, accept: bool) -> ScreenAction {
        ScreenAction::ConductAnswer {
            accept,
            remember: accept && self.remember,
            hash: self.hash,
        }
    }
}

impl Screen for ConductScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        let w = self.panel_width();
        let left = (width - w) / 2;
        self.list
            .set_rect([left, 36, w - 8, (height - 36 - 90).max(36)]);
        self.wrapped_for = -1;
        self.widgets.clear();
        let bottom = height - 28;
        let half = (w - 4) / 2;
        self.widgets.add(Button::new(
            REMEMBER,
            [left, bottom - 24, w, 20],
            "lang:multiplayer.codeOfConduct.check",
        ));
        self.widgets.add(Button::new(
            ACCEPT,
            [left, bottom, half, 20],
            "lang:gui.acknowledge",
        ));
        self.widgets.add(Button::new(
            DECLINE,
            [left + half + 4, bottom, half, 20],
            "lang:menu.disconnect",
        ));
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let title = cx.lang("multiplayer.codeOfConduct.title");
        cx.centered_text(&title, self.gui.0 / 2, 14, WHITE, true);
        let rect = self.list.rect();
        if self.wrapped_for != rect[2] {
            self.lines = cx.frame.ui().wrap(&self.cells, (rect[2] - 8) as f32);
            self.list.set_count(self.lines.len());
            self.wrapped_for = rect[2];
        }
        let check = cx.lang("multiplayer.codeOfConduct.check");
        let label = self.remember_label(&check);
        if let Some(b) = self.widgets.button_mut(REMEMBER) {
            b.label = label;
        }
        cx.frame.fill_rect(
            rect[0] - 1,
            rect[1] - 1,
            rect[2] + 18,
            rect[3] + 2,
            0xFF00_0000,
        );
        cx.frame
            .push_scissor(rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]);
        for i in self.list.visible_range() {
            let range = self.lines[i].clone();
            cx.frame.cells(
                &self.cells[range],
                (rect[0] + 4) as f32,
                self.list.item_top(i) as f32,
                WHITE,
                true,
            );
        }
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, rect[0] + rect[2] + 4);
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        let bar_x = self.list.rect()[0] + self.list.rect()[2] + 4;
        if self.list.input(input, bar_x) {
            return true;
        }
        if let UiInput::Key { key, .. } = input {
            match key {
                Key::Escape => {
                    out.extend([self.answer(false), ScreenAction::Pop]);
                    return true;
                }
                Key::PageDown => self.list.scroll_by(self.list.rect()[3] as f32),
                Key::PageUp => self.list.scroll_by(-(self.list.rect()[3] as f32)),
                Key::Down => self.list.scroll_by(LINE_HEIGHT as f32),
                Key::Up => self.list.scroll_by(-(LINE_HEIGHT as f32)),
                _ => {}
            }
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for e in events {
            if e.kind != WidgetEventKind::Pressed {
                continue;
            }
            out.push(ScreenAction::ClickSound);
            match e.id {
                REMEMBER => self.remember = !self.remember,
                ACCEPT => out.extend([self.answer(true), ScreenAction::Pop]),
                DECLINE => out.extend([self.answer(false), ScreenAction::Pop]),
                _ => {}
            }
        }
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }
}
