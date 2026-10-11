use std::ops::Range;

use rustc_hash::FxHashMap;

use super::edit_box::is_allowed_char;
use super::env::{Clipboard, RenderCx};
use super::input::{Key, Modifiers, contains};
use crate::style::{Cell, FontId, Style};
use crate::text::wrap;

const SELECTION: u32 = 0xB033_99FF;
const BLINK_MS: u64 = 300;
const LINE: i32 = 9;
const FALLBACK_ADVANCE: f32 = 6.0;

/// Word-wrapped multi-line text field with character and line limits.
///
/// Fonts are only reachable while rendering, so glyph advances are cached by [`Self::measure`]
/// and input between two renders wraps with the cached table (unknown glyphs count as 6 px).
#[derive(Clone, Debug)]
pub struct MultiLineEdit {
    pub id: u32,
    pub rect: [i32; 4],
    pub padding: i32,
    pub text_color: u32,
    pub shadow: bool,
    pub editable: bool,
    chars: Vec<char>,
    cursor: usize,
    highlight: usize,
    char_limit: usize,
    line_limit: Option<usize>,
    scroll: usize,
    focus_ms: u64,
    advances: FxHashMap<char, f32>,
    lines: Vec<Range<usize>>,
    changed: bool,
}

impl MultiLineEdit {
    pub fn new(id: u32, rect: [i32; 4]) -> Self {
        let mut edit = Self {
            id,
            rect,
            padding: 4,
            text_color: 0xFFE0_E0E0,
            shadow: true,
            editable: true,
            chars: Vec::new(),
            cursor: 0,
            highlight: 0,
            char_limit: usize::MAX,
            line_limit: None,
            scroll: 0,
            focus_ms: 0,
            advances: FxHashMap::default(),
            lines: Vec::new(),
            changed: false,
        };
        edit.relayout();
        edit
    }

    pub fn with_limits(mut self, chars: usize, lines: Option<usize>) -> Self {
        self.char_limit = chars;
        self.line_limit = lines;
        self.chars.truncate(chars);
        self.relayout();
        self
    }

    pub fn plain(mut self, color: u32) -> Self {
        self.text_color = color;
        self.shadow = false;
        self
    }

    pub fn value(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    pub fn focus(&mut self, now_ms: u64) {
        self.focus_ms = now_ms;
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        contains(self.rect, x, y)
    }

    fn text_width(&self) -> f32 {
        (self.rect[2] - 2 * self.padding) as f32
    }

    fn visible_lines(&self) -> usize {
        (((self.rect[3] - 2 * self.padding) / LINE).max(1)) as usize
    }

    fn advance(&self, c: char) -> f32 {
        self.advances.get(&c).copied().unwrap_or(FALLBACK_ADVANCE)
    }

    fn cells(&self) -> Vec<Cell> {
        self.chars
            .iter()
            .map(|&ch| Cell {
                ch,
                style: Style::default(),
            })
            .collect()
    }

    fn relayout(&mut self) {
        let cells = self.cells();
        let advances = &self.advances;
        let mut lines = wrap(&cells, self.text_width(), |c| {
            advances.get(&c.ch).copied().unwrap_or(FALLBACK_ADVANCE)
        });
        if lines.is_empty() {
            lines.push(0..0);
        }
        self.lines = lines;
    }

    fn line_of(&self, pos: usize) -> usize {
        self.lines
            .iter()
            .rposition(|l| l.start <= pos)
            .unwrap_or(0)
            .min(self.lines.len() - 1)
    }

    fn x_in_line(&self, line: usize, pos: usize) -> f32 {
        let range = &self.lines[line];
        let end = pos.clamp(range.start, range.end);
        self.chars[range.start..end]
            .iter()
            .map(|&c| self.advance(c))
            .sum()
    }

    fn pos_at(&self, line: usize, x: f32) -> usize {
        let range = &self.lines[line];
        let mut used = 0.0;
        for i in range.clone() {
            let w = self.advance(self.chars[i]);
            if x < used + w / 2.0 {
                return i;
            }
            used += w;
        }
        range.end
    }

    fn selection(&self) -> (usize, usize) {
        (
            self.cursor.min(self.highlight),
            self.cursor.max(self.highlight),
        )
    }

    pub fn selected(&self) -> String {
        let (a, b) = self.selection();
        self.chars[a..b].iter().collect()
    }

    pub fn set_value(&mut self, text: &str) {
        self.chars = text
            .chars()
            .filter(|&c| c == '\n' || is_allowed_char(c))
            .take(self.char_limit)
            .collect();
        self.cursor = self.chars.len();
        self.highlight = self.cursor;
        self.scroll = 0;
        self.relayout();
        self.changed = true;
    }

    pub fn insert_text(&mut self, text: &str) {
        if !self.editable {
            return;
        }
        let snapshot = (self.chars.clone(), self.cursor, self.highlight);
        let (a, b) = self.selection();
        let room = self.char_limit.saturating_sub(self.chars.len() - (b - a));
        let filtered: Vec<char> = text
            .chars()
            .filter(|&c| c == '\n' || is_allowed_char(c))
            .take(room)
            .collect();
        let count = filtered.len();
        self.chars.splice(a..b, filtered);
        self.cursor = a + count;
        self.highlight = self.cursor;
        self.relayout();
        if self
            .line_limit
            .is_some_and(|limit| self.lines.len() > limit)
        {
            (self.chars, self.cursor, self.highlight) = snapshot;
            self.relayout();
            return;
        }
        self.changed = true;
    }

    fn delete(&mut self, direction: i32, word: bool) {
        if !self.editable {
            return;
        }
        if self.cursor != self.highlight {
            self.insert_text("");
            return;
        }
        let target = if word {
            self.word_position(direction)
        } else {
            (self.cursor as i64 + i64::from(direction)).clamp(0, self.chars.len() as i64) as usize
        };
        if target != self.cursor {
            let (a, b) = (self.cursor.min(target), self.cursor.max(target));
            self.chars.drain(a..b);
            self.cursor = a;
            self.highlight = a;
            self.relayout();
            self.changed = true;
        }
    }

    fn word_position(&self, direction: i32) -> usize {
        let mut pos = self.cursor;
        if direction > 0 {
            while pos < self.chars.len() && !self.chars[pos].is_whitespace() {
                pos += 1;
            }
            while pos < self.chars.len() && self.chars[pos].is_whitespace() {
                pos += 1;
            }
        } else {
            while pos > 0 && self.chars[pos - 1].is_whitespace() {
                pos -= 1;
            }
            while pos > 0 && !self.chars[pos - 1].is_whitespace() {
                pos -= 1;
            }
        }
        pos
    }

    pub fn move_to(&mut self, pos: usize, select: bool) {
        self.cursor = pos.min(self.chars.len());
        if !select {
            self.highlight = self.cursor;
        }
    }

    fn move_line(&mut self, delta: i32, select: bool) {
        let line = self.line_of(self.cursor);
        let target = line as i64 + i64::from(delta);
        if target < 0 {
            return self.move_to(0, select);
        }
        if target as usize >= self.lines.len() {
            return self.move_to(self.chars.len(), select);
        }
        let x = self.x_in_line(line, self.cursor);
        let pos = self.pos_at(target as usize, x);
        self.move_to(pos, select);
    }

    pub fn key(&mut self, key: Key, mods: Modifiers, clipboard: &mut dyn Clipboard) -> bool {
        match key {
            Key::Enter => self.insert_text("\n"),
            Key::Backspace => self.delete(-1, mods.ctrl),
            Key::Delete => self.delete(1, mods.ctrl),
            Key::Left if mods.ctrl => {
                let p = self.word_position(-1);
                self.move_to(p, mods.shift);
            }
            Key::Right if mods.ctrl => {
                let p = self.word_position(1);
                self.move_to(p, mods.shift);
            }
            Key::Left => self.move_to(self.cursor.saturating_sub(1), mods.shift),
            Key::Right => self.move_to(self.cursor + 1, mods.shift),
            Key::Up => self.move_line(-1, mods.shift),
            Key::Down => self.move_line(1, mods.shift),
            Key::PageUp => self.move_line(-(self.visible_lines() as i32), mods.shift),
            Key::PageDown => self.move_line(self.visible_lines() as i32, mods.shift),
            Key::Home if mods.ctrl => self.move_to(0, mods.shift),
            Key::End if mods.ctrl => self.move_to(self.chars.len(), mods.shift),
            Key::Home => {
                let start = self.lines[self.line_of(self.cursor)].start;
                self.move_to(start, mods.shift);
            }
            Key::End => {
                let end = self.lines[self.line_of(self.cursor)].end;
                self.move_to(end, mods.shift);
            }
            Key::Letter('a') if mods.ctrl => {
                self.highlight = 0;
                self.cursor = self.chars.len();
            }
            Key::Letter('c') if mods.ctrl => clipboard.set(&self.selected()),
            Key::Letter('x') if mods.ctrl => {
                clipboard.set(&self.selected());
                self.insert_text("");
            }
            Key::Letter('v') if mods.ctrl => {
                let text = clipboard.get();
                self.insert_text(&text);
            }
            _ => return false,
        }
        true
    }

    pub fn char(&mut self, c: char) -> bool {
        if self.editable && is_allowed_char(c) {
            let before = self.chars.len();
            self.insert_text(c.encode_utf8(&mut [0; 4]));
            return self.chars.len() != before || self.highlight != self.cursor;
        }
        false
    }

    pub fn click(&mut self, x: f32, y: f32, now_ms: u64, select: bool) {
        self.focus_ms = now_ms;
        let row = ((y - (self.rect[1] + self.padding) as f32) / LINE as f32).floor();
        let line = (self.scroll as i64 + row as i64).clamp(0, self.lines.len() as i64 - 1) as usize;
        let pos = self.pos_at(line, x - (self.rect[0] + self.padding) as f32);
        self.move_to(pos, select);
    }

    /// Caches glyph advances for the current text and re-wraps; call before drawing.
    pub fn measure(&mut self, cx: &mut RenderCx<'_, '_>) {
        let fonts = cx.frame.ui().fonts_mut();
        let mut dirty = false;
        for &c in &self.chars {
            if let std::collections::hash_map::Entry::Vacant(e) = self.advances.entry(c) {
                e.insert(fonts.advance(FontId::DEFAULT, c, false));
                dirty = true;
            }
        }
        if dirty {
            self.relayout();
            if let Some(limit) = self.line_limit {
                while self.lines.len() > limit && !self.chars.is_empty() {
                    self.chars.pop();
                    self.relayout();
                }
                self.cursor = self.cursor.min(self.chars.len());
                self.highlight = self.highlight.min(self.chars.len());
            }
        }
    }

    pub fn render(&mut self, cx: &mut RenderCx<'_, '_>, focused: bool) {
        self.measure(cx);
        let cursor_line = self.line_of(self.cursor);
        let visible = self.visible_lines();
        if cursor_line < self.scroll {
            self.scroll = cursor_line;
        } else if cursor_line >= self.scroll + visible {
            self.scroll = cursor_line + 1 - visible;
        }
        self.scroll = self.scroll.min(self.lines.len().saturating_sub(visible));
        let (x0, y0) = (self.rect[0] + self.padding, self.rect[1] + self.padding);
        let color = if self.editable {
            self.text_color
        } else {
            0xFF70_7070
        };
        let (sel_a, sel_b) = self.selection();
        let cells = self.cells();
        for (row, line) in (self.scroll..self.lines.len()).take(visible).enumerate() {
            let range = self.lines[line].clone();
            let y = y0 + row as i32 * LINE;
            cx.frame.cells(
                &cells[range.clone()],
                x0 as f32,
                y as f32,
                color,
                self.shadow,
            );
            let (a, b) = (sel_a.max(range.start), sel_b.min(range.end));
            if a < b {
                let xa = x0 + self.x_in_line(line, a).round() as i32;
                let xb = x0 + self.x_in_line(line, b).round() as i32;
                cx.frame.fill_rect(xa, y - 1, xb, y + LINE - 1, SELECTION);
            }
        }
        let blink = (cx.now_ms.saturating_sub(self.focus_ms) / BLINK_MS).is_multiple_of(2);
        if focused
            && blink
            && self.editable
            && (self.scroll..self.scroll + visible).contains(&cursor_line)
        {
            let x = x0 + self.x_in_line(cursor_line, self.cursor).round() as i32;
            let y = y0 + (cursor_line - self.scroll) as i32 * LINE;
            if self.cursor == self.chars.len() && self.chars.len() < self.char_limit {
                cx.frame.text("_", x as f32, y as f32, color, self.shadow);
            } else {
                cx.frame
                    .fill_rect(x, y - 1, x + 1, y + LINE - 1, color | 0xFF00_0000);
            }
        }
    }

    pub fn scroll_by(&mut self, delta: i32) {
        let max = self.lines.len().saturating_sub(self.visible_lines());
        self.scroll = (self.scroll as i64 + i64::from(delta)).clamp(0, max as i64) as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::env::MemoryClipboard;

    fn field(width: i32, lines: Option<usize>) -> MultiLineEdit {
        MultiLineEdit::new(0, [0, 0, width, 100]).with_limits(1024, lines)
    }

    #[test]
    fn typing_and_newlines() {
        let mut e = field(122, None);
        let mut clip = MemoryClipboard::default();
        e.insert_text("ab");
        e.key(Key::Enter, Modifiers::NONE, &mut clip);
        e.insert_text("cd");
        assert_eq!(e.value(), "ab\ncd");
        assert_eq!(e.line_count(), 2);
    }

    #[test]
    fn wraps_at_width_and_navigates_lines() {
        let mut e = field(8 + 30, None);
        let mut clip = MemoryClipboard::default();
        e.insert_text("aaaaaaaaaaaa");
        assert!(e.line_count() > 1);
        e.key(Key::Up, Modifiers::NONE, &mut clip);
        assert!(e.cursor() < 12);
        e.key(Key::Down, Modifiers::NONE, &mut clip);
        assert!(e.cursor() > 6);
    }

    #[test]
    fn line_limit_rejects_overflowing_input() {
        let mut e = field(122, Some(2));
        e.insert_text("a\nb");
        e.insert_text("\nc");
        assert_eq!(e.value(), "a\nb");
    }

    #[test]
    fn char_limit_truncates_paste() {
        let mut e = MultiLineEdit::new(0, [0, 0, 122, 100]).with_limits(5, None);
        e.insert_text("abcdefgh");
        assert_eq!(e.value(), "abcde");
    }

    #[test]
    fn selection_replace_and_clipboard() {
        let mut e = field(122, None);
        let mut clip = MemoryClipboard::default();
        e.insert_text("hello world");
        e.key(Key::Letter('a'), Modifiers::CTRL, &mut clip);
        e.key(Key::Letter('c'), Modifiers::CTRL, &mut clip);
        assert_eq!(clip.0, "hello world");
        e.insert_text("x");
        assert_eq!(e.value(), "x");
    }

    #[test]
    fn ctrl_backspace_removes_a_word() {
        let mut e = field(122, None);
        let mut clip = MemoryClipboard::default();
        e.insert_text("one two");
        e.key(Key::Backspace, Modifiers::CTRL, &mut clip);
        assert_eq!(e.value(), "one ");
    }
}
