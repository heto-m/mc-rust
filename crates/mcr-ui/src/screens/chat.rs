use std::collections::VecDeque;

use mcr_protocol::types::text::Component;

use crate::style::{Cell, with_alpha};
use crate::text::parse_legacy;
use crate::ui::{LINE_HEIGHT, Ui, UiFrame};

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
}

/// Recent chat messages, newest line first, wrapped to the chat width when added.
#[derive(Debug, Default)]
pub struct ChatHud {
    lines: VecDeque<ChatLine>,
}

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
        let cells = ui.flatten(message);
        self.add_cells(ui, &cells, tick, options);
    }

    pub fn add_text(&mut self, ui: &mut Ui, message: &str, tick: u32, options: &ChatOptions) {
        self.add_cells(ui, &parse_legacy(message), tick, options);
    }

    fn add_cells(&mut self, ui: &mut Ui, cells: &[Cell], tick: u32, options: &ChatOptions) {
        for range in ui.wrap(cells, options.width as f32) {
            self.lines.push_front(ChatLine {
                cells: cells[range].to_vec(),
                added: tick,
            });
        }
        self.lines.truncate(MAX_STORED_LINES);
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
}
