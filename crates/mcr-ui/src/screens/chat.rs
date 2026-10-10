use std::collections::VecDeque;

use mcr_protocol::types::text::Component;

use crate::style::{Cell, with_alpha};
use crate::text::{ClickSpan, click_at, flatten_events, parse_legacy};
use crate::ui::{LINE_HEIGHT, Ui, UiFrame};
use mcr_protocol::types::text::ClickEvent;

const MAX_STORED_LINES: usize = 100;
const FADE_TICKS: f32 = 200.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChatOptions {
    pub width: i32,
    pub visible_lines: usize,
    pub background_opacity: f32,
    pub text_opacity: f32,
    pub focused: bool,
}

impl Default for ChatOptions {
    fn default() -> Self {
        Self {
            width: 320,
            visible_lines: 10,
            background_opacity: 0.5,
            text_opacity: 1.0,
            focused: false,
        }
    }
}

#[derive(Debug)]
struct ChatLine {
    cells: Vec<Cell>,
    added: u32,
    spans: Vec<ClickSpan>,
    ends: Vec<f32>,
}

/// Recent chat messages, newest line first, wrapped to the chat width when added.
#[derive(Debug, Default)]
pub struct ChatHud {
    lines: VecDeque<ChatLine>,
    scroll: usize,
    recent: Vec<String>,
}

const MAX_RECENT: usize = 100;

pub fn fade_alpha(now: u32, added: u32) -> f32 {
    let t = (1.0 - now.saturating_sub(added) as f32 / FADE_TICKS) * 10.0;
    let t = t.clamp(0.0, 1.0);
    t * t
}

impl ChatHud {
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn add(&mut self, ui: &mut Ui, message: &Component, tick: u32, options: &ChatOptions) {
        let (cells, spans) = flatten_events(ui.lang(), message);
        self.add_cells(ui, &cells, &spans, tick, options);
    }

    pub fn add_text(&mut self, ui: &mut Ui, message: &str, tick: u32, options: &ChatOptions) {
        self.add_cells(ui, &parse_legacy(message), &[], tick, options);
    }

    fn add_cells(
        &mut self,
        ui: &mut Ui,
        cells: &[Cell],
        spans: &[ClickSpan],
        tick: u32,
        options: &ChatOptions,
    ) {
        for range in ui.wrap(cells, options.width as f32) {
            let spans = spans
                .iter()
                .filter(|s| s.range.start < range.end && s.range.end > range.start)
                .map(|s| ClickSpan {
                    range: s.range.start.max(range.start) - range.start
                        ..s.range.end.min(range.end) - range.start,
                    event: s.event.clone(),
                })
                .collect();
            let mut x = 0.0;
            let ends = cells[range.clone()]
                .iter()
                .map(|c| {
                    x += ui.width(std::slice::from_ref(c));
                    x
                })
                .collect();
            self.lines.push_front(ChatLine {
                cells: cells[range].to_vec(),
                added: tick,
                spans,
                ends,
            });
            if self.scroll > 0 {
                self.scroll += 1;
            }
        }
        self.lines.truncate(MAX_STORED_LINES);
        self.scroll = self.scroll.min(self.lines.len().saturating_sub(1));
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Positive scrolls toward older lines.
    pub fn scroll_by(&mut self, delta: i32, visible_lines: usize) {
        let max = self.lines.len().saturating_sub(visible_lines);
        self.scroll = (self.scroll as i64 + i64::from(delta)).clamp(0, max as i64) as usize;
    }

    pub fn reset_scroll(&mut self) {
        self.scroll = 0;
    }

    pub fn recent(&self) -> &[String] {
        &self.recent
    }

    pub fn add_recent(&mut self, line: &str) {
        if self.recent.last().is_some_and(|l| l == line) {
            return;
        }
        self.recent.push(line.to_owned());
        if self.recent.len() > MAX_RECENT {
            self.recent.remove(0);
        }
    }

    /// Click event under a GUI position of the open chat, if any.
    pub fn click_event_at(
        &self,
        x: f32,
        y: f32,
        gui_height: i32,
        options: &ChatOptions,
    ) -> Option<ClickEvent> {
        let bottom = gui_height - 40;
        if y >= bottom as f32 || x < 4.0 {
            return None;
        }
        let row = ((bottom as f32 - y) / LINE_HEIGHT as f32).floor() as usize;
        if row >= options.visible_lines {
            return None;
        }
        let line = self.lines.get(row + self.scroll)?;
        let index = line.ends.iter().position(|&end| x < 4.0 + end)?;
        click_at(&line.spans, index).cloned()
    }

    pub fn draw(&self, frame: &mut UiFrame<'_>, tick: u32, options: &ChatOptions) {
        let (_, gui_height) = frame.gui_size();
        let bottom = gui_height - 40;
        let alpha = |line: &ChatLine| {
            if options.focused {
                1.0
            } else {
                fade_alpha(tick, line.added)
            }
        };
        let visible: Vec<(usize, &ChatLine, f32)> = self
            .lines
            .iter()
            .skip(self.scroll)
            .take(options.visible_lines)
            .enumerate()
            .map(|(i, l)| (i, l, alpha(l)))
            .filter(|&(_, _, a)| a > 1.0e-5)
            .collect();
        for &(i, _, a) in visible.iter().rev() {
            let entry_bottom = bottom - i as i32 * LINE_HEIGHT;
            let color = with_alpha(0, a * options.background_opacity);
            frame.fill_rect(
                0,
                entry_bottom - LINE_HEIGHT,
                options.width + 12,
                entry_bottom,
                color,
            );
        }
        for &(i, line, a) in visible.iter().rev() {
            let text_top = bottom - i as i32 * LINE_HEIGHT - 8;
            let color = with_alpha(0xFF_FFFF, a * options.text_opacity);
            frame.cells(&line.cells, 4.0, text_top as f32, color, true);
        }
    }

    /// Scroll indicator at the right edge of the open chat.
    pub fn draw_scrollbar(&self, frame: &mut UiFrame<'_>, options: &ChatOptions) {
        let total = self.lines.len();
        if total <= options.visible_lines {
            return;
        }
        let (_, gui_height) = frame.gui_size();
        let height = options.visible_lines as i32 * LINE_HEIGHT;
        let top = gui_height - 40 - height;
        let x = options.width + 12;
        let thumb = (height * options.visible_lines as i32 / total as i32).max(4);
        let offset = (height - thumb) * self.scroll as i32 / (total - options.visible_lines) as i32;
        frame.fill_rect(x, top, x + 2, top + height, 0x6033_33AA);
        frame.fill_rect(
            x,
            top + height - thumb - offset,
            x + 2,
            top + height - offset,
            0xFFCC_8888,
        );
    }
}
