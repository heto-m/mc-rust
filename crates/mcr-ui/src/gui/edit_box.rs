use super::env::{Clipboard, RenderCx};
use super::input::{Key, Modifiers};
use crate::style::FontId;

const MAX_HISTORY: usize = 100;
const SELECTION: u32 = 0xB033_99FF;
const BLINK_MS: u64 = 300;

pub fn is_allowed_char(c: char) -> bool {
    c >= ' ' && c != '\u{7f}' && c != '§'
}

#[derive(Clone, Debug)]
pub struct EditBox {
    pub id: u32,
    pub rect: [i32; 4],
    pub active: bool,
    pub visible: bool,
    pub bordered: bool,
    pub editable: bool,
    pub text_color: u32,
    pub hint: Option<String>,
    value: String,
    cursor: usize,
    highlight: usize,
    display_pos: usize,
    max_length: usize,
    history: Vec<String>,
    history_index: Option<usize>,
    draft: String,
    focus_ms: u64,
    changed: bool,
    widths: Vec<f32>,
}

impl EditBox {
    pub fn new(id: u32, rect: [i32; 4]) -> Self {
        Self {
            id,
            rect,
            active: true,
            visible: true,
            bordered: true,
            editable: true,
            text_color: 0xFFE0_E0E0,
            hint: None,
            value: String::new(),
            cursor: 0,
            highlight: 0,
            display_pos: 0,
            max_length: 32,
            history: Vec::new(),
            history_index: None,
            draft: String::new(),
            focus_ms: 0,
            changed: false,
            widths: Vec::new(),
        }
    }

    pub fn with_max_length(mut self, max: usize) -> Self {
        self.set_max_length(max);
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn len(&self) -> usize {
        self.value.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn max_length(&self) -> usize {
        self.max_length
    }

    pub fn set_max_length(&mut self, max: usize) {
        self.max_length = max;
        if self.len() > max {
            self.value = self.value.chars().take(max).collect();
            self.cursor = self.cursor.min(max);
            self.highlight = self.highlight.min(max);
        }
    }

    pub fn set_value(&mut self, text: &str) {
        self.value = text
            .chars()
            .filter(|&c| is_allowed_char(c))
            .take(self.max_length)
            .collect();
        self.cursor = self.len();
        self.highlight = self.cursor;
        self.changed = true;
    }

    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    pub fn selected(&self) -> String {
        let (a, b) = self.selection();
        self.value.chars().skip(a).take(b - a).collect()
    }

    pub fn selection(&self) -> (usize, usize) {
        (
            self.cursor.min(self.highlight),
            self.cursor.max(self.highlight),
        )
    }

    fn byte_at(&self, char_index: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_index)
            .map_or(self.value.len(), |(i, _)| i)
    }

    pub fn insert_text(&mut self, text: &str) {
        let (a, b) = self.selection();
        let room = self.max_length.saturating_sub(self.len() - (b - a));
        let filtered: String = text
            .chars()
            .filter(|&c| is_allowed_char(c))
            .take(room)
            .collect();
        let inserted = filtered.chars().count();
        let (ba, bb) = (self.byte_at(a), self.byte_at(b));
        self.value.replace_range(ba..bb, &filtered);
        self.cursor = a + inserted;
        self.highlight = self.cursor;
        self.changed = true;
    }

    fn delete_range(&mut self, a: usize, b: usize) {
        let (ba, bb) = (self.byte_at(a), self.byte_at(b));
        self.value.replace_range(ba..bb, "");
        self.cursor = a;
        self.highlight = a;
        self.changed = true;
    }

    pub fn delete_text(&mut self, direction: i32, word: bool) {
        if self.cursor != self.highlight {
            self.insert_text("");
            return;
        }
        let target = if word {
            self.word_position(direction, self.cursor)
        } else {
            (self.cursor as i64 + i64::from(direction)).clamp(0, self.len() as i64) as usize
        };
        if target != self.cursor {
            self.delete_range(self.cursor.min(target), self.cursor.max(target));
        }
    }

    pub fn word_position(&self, direction: i32, from: usize) -> usize {
        let chars: Vec<char> = self.value.chars().collect();
        let mut pos = from.min(chars.len());
        if direction > 0 {
            while pos < chars.len() && chars[pos] != ' ' {
                pos += 1;
            }
            while pos < chars.len() && chars[pos] == ' ' {
                pos += 1;
            }
        } else {
            while pos > 0 && chars[pos - 1] == ' ' {
                pos -= 1;
            }
            while pos > 0 && chars[pos - 1] != ' ' {
                pos -= 1;
            }
        }
        pos
    }

    pub fn move_cursor_to(&mut self, pos: usize, select: bool) {
        self.cursor = pos.min(self.len());
        if !select {
            self.highlight = self.cursor;
        }
    }

    pub fn move_cursor(&mut self, delta: i32, select: bool) {
        let pos = (self.cursor as i64 + i64::from(delta)).clamp(0, self.len() as i64) as usize;
        self.move_cursor_to(pos, select);
    }

    pub fn select_all(&mut self) {
        self.cursor = self.len();
        self.highlight = 0;
    }

    pub fn key(&mut self, key: Key, mods: Modifiers, clipboard: &mut dyn Clipboard) -> bool {
        if !self.active {
            return false;
        }
        match key {
            Key::Backspace => {
                if self.editable {
                    self.delete_text(-1, mods.ctrl);
                }
            }
            Key::Delete => {
                if self.editable {
                    self.delete_text(1, mods.ctrl);
                }
            }
            Key::Home => self.move_cursor_to(0, mods.shift),
            Key::End => self.move_cursor_to(self.len(), mods.shift),
            Key::Left if mods.ctrl => {
                let p = self.word_position(-1, self.cursor);
                self.move_cursor_to(p, mods.shift);
            }
            Key::Right if mods.ctrl => {
                let p = self.word_position(1, self.cursor);
                self.move_cursor_to(p, mods.shift);
            }
            Key::Left => self.move_cursor(-1, mods.shift),
            Key::Right => self.move_cursor(1, mods.shift),
            Key::Letter('a') if mods.ctrl => self.select_all(),
            Key::Letter('c') if mods.ctrl => clipboard.set(&self.selected()),
            Key::Letter('x') if mods.ctrl => {
                clipboard.set(&self.selected());
                if self.editable {
                    self.insert_text("");
                }
            }
            Key::Letter('v') if mods.ctrl => {
                if self.editable {
                    let text = clipboard.get();
                    self.insert_text(&text);
                }
            }
            _ => return false,
        }
        true
    }

    pub fn char(&mut self, c: char) -> bool {
        if self.active && self.editable && is_allowed_char(c) {
            self.insert_text(c.encode_utf8(&mut [0; 4]));
            true
        } else {
            false
        }
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        super::input::contains(self.rect, x, y)
    }

    fn inner_width(&self) -> f32 {
        (self.rect[2] - if self.bordered { 8 } else { 0 }) as f32
    }

    fn text_x(&self) -> i32 {
        self.rect[0] + if self.bordered { 4 } else { 0 }
    }

    fn fit_prefix(&self, from: usize, width: f32, widths: &[f32]) -> usize {
        let mut used = 0.0;
        let mut n = 0;
        for w in widths.iter().skip(from) {
            used += w;
            if used > width {
                break;
            }
            n += 1;
        }
        n
    }

    fn scroll_into_view(&mut self, widths: &[f32]) {
        self.display_pos = self.display_pos.min(self.len());
        let width = self.inner_width();
        if self.cursor < self.display_pos {
            self.display_pos = self.cursor;
        }
        loop {
            let shown = self.fit_prefix(self.display_pos, width, widths);
            if self.cursor <= self.display_pos + shown || self.display_pos >= self.len() {
                break;
            }
            self.display_pos += 1;
        }
    }

    /// Places the cursor under a click at GUI x using the glyph widths of the last render.
    pub fn click(&mut self, x: f32, now_ms: u64, select: bool) {
        let widths = if self.widths.len() == self.len() {
            std::mem::take(&mut self.widths)
        } else {
            vec![6.0; self.len()]
        };
        self.click_with(x, now_ms, select, &widths);
        self.widths = widths;
    }

    pub fn click_with(&mut self, x: f32, now_ms: u64, select: bool, widths: &[f32]) {
        self.focus(now_ms);
        self.scroll_into_view(widths);
        let target = x - self.text_x() as f32;
        let n = self.fit_prefix(self.display_pos, target.max(0.0), widths);
        self.move_cursor_to(self.display_pos + n, select);
    }

    pub fn focus(&mut self, now_ms: u64) {
        self.focus_ms = now_ms;
    }

    pub fn add_history(&mut self, line: &str) {
        if line.is_empty() || self.history.last().is_some_and(|l| l == line) {
            self.history_index = None;
            return;
        }
        self.history.push(line.to_owned());
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
        self.history_index = None;
    }

    pub fn history(&self) -> &[String] {
        &self.history
    }

    pub fn history_step(&mut self, delta: i32) {
        let count = self.history.len();
        let current = self.history_index.unwrap_or(count);
        let next = (current as i64 + i64::from(delta)).clamp(0, count as i64) as usize;
        if next == current {
            return;
        }
        if self.history_index.is_none() {
            self.draft = self.value.clone();
        }
        if next == count {
            self.history_index = None;
            let draft = std::mem::take(&mut self.draft);
            self.set_value(&draft);
        } else {
            self.history_index = Some(next);
            let line = self.history[next].clone();
            self.set_value(&line);
        }
    }

    pub fn render(&mut self, cx: &mut RenderCx<'_, '_>, focused: bool) {
        if !self.visible {
            return;
        }
        let [x, y, w, h] = self.rect;
        if self.bordered {
            let name = if focused {
                "widget/text_field_highlighted"
            } else {
                "widget/text_field"
            };
            cx.sprite(name, x, y, w, h);
        }
        let widths: Vec<f32> = self
            .value
            .chars()
            .map(|c| cx.frame.ui().fonts_mut().advance(FontId::DEFAULT, c, false))
            .collect();
        self.scroll_into_view(&widths);
        let shown = self.fit_prefix(self.display_pos, self.inner_width(), &widths);
        self.widths = widths;
        let displayed: String = self
            .value
            .chars()
            .skip(self.display_pos)
            .take(shown)
            .collect();
        let color = if self.editable {
            self.text_color
        } else {
            0xFF70_7070
        };
        let tx = self.text_x();
        let ty = if self.bordered { y + (h - 8) / 2 } else { y };
        if displayed.is_empty()
            && !focused
            && let Some(hint) = self.hint.clone()
        {
            cx.text(&hint, tx, ty, 0xFF80_8080, true);
        }
        cx.text(&displayed, tx, ty, color, true);
        let rel_cursor = self.cursor.saturating_sub(self.display_pos).min(shown);
        let prefix: String = displayed.chars().take(rel_cursor).collect();
        let cursor_x = tx + cx.text_width(&prefix);
        let (a, b) = self.selection();
        if a != b {
            let rel = |p: usize| p.saturating_sub(self.display_pos).min(shown);
            let pre = |n: usize| displayed.chars().take(n).collect::<String>();
            let x0 = tx + cx.text_width(&pre(rel(a)));
            let x1 = tx + cx.text_width(&pre(rel(b)));
            cx.frame
                .fill_rect(x0, ty - 1, x1.min(x + w), ty + 10, SELECTION);
        }
        let blink = (cx.now_ms.saturating_sub(self.focus_ms) / BLINK_MS).is_multiple_of(2);
        if focused && blink && self.editable {
            if self.cursor < self.len() || self.len() >= self.max_length {
                cx.frame
                    .fill_rect(cursor_x - 1, ty - 1, cursor_x, ty + 10, color | 0xFF00_0000);
            } else {
                cx.text("_", cursor_x, ty, color, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::env::MemoryClipboard;

    fn boxed(text: &str) -> EditBox {
        let mut b = EditBox::new(0, [0, 0, 100, 20]).with_max_length(20);
        b.set_value(text);
        b
    }

    fn clip(text: &str) -> MemoryClipboard {
        MemoryClipboard(text.to_owned())
    }

    #[test]
    fn typing_inserts_at_cursor() {
        let mut b = boxed("helo");
        b.move_cursor_to(3, false);
        b.char('l');
        assert_eq!(b.value(), "hello");
        assert_eq!(b.cursor(), 4);
    }

    #[test]
    fn backspace_and_delete() {
        let mut b = boxed("hello");
        let mut c = clip("");
        b.key(Key::Backspace, Modifiers::NONE, &mut c);
        assert_eq!(b.value(), "hell");
        b.move_cursor_to(0, false);
        b.key(Key::Delete, Modifiers::NONE, &mut c);
        assert_eq!(b.value(), "ell");
        b.key(Key::Backspace, Modifiers::NONE, &mut c);
        assert_eq!(b.value(), "ell");
    }

    #[test]
    fn ctrl_backspace_deletes_a_word() {
        let mut b = boxed("one two three");
        let mut c = clip("");
        b.key(Key::Backspace, Modifiers::CTRL, &mut c);
        assert_eq!(b.value(), "one two ");
        b.key(Key::Backspace, Modifiers::CTRL, &mut c);
        assert_eq!(b.value(), "one ");
    }

    #[test]
    fn ctrl_arrows_jump_words() {
        let mut b = boxed("one two three");
        let mut c = clip("");
        b.move_cursor_to(0, false);
        b.key(Key::Right, Modifiers::CTRL, &mut c);
        assert_eq!(b.cursor(), 4);
        b.key(Key::Right, Modifiers::CTRL, &mut c);
        assert_eq!(b.cursor(), 8);
        b.key(Key::Left, Modifiers::CTRL, &mut c);
        assert_eq!(b.cursor(), 4);
    }

    #[test]
    fn shift_selects_and_typing_replaces() {
        let mut b = boxed("hello");
        let mut c = clip("");
        b.move_cursor_to(0, false);
        b.key(Key::Right, Modifiers::SHIFT, &mut c);
        b.key(Key::Right, Modifiers::SHIFT, &mut c);
        assert_eq!(b.selected(), "he");
        b.char('X');
        assert_eq!(b.value(), "Xllo");
        assert_eq!(b.cursor(), 1);
    }

    #[test]
    fn select_all_copy_cut_paste() {
        let mut b = boxed("hello");
        let mut c = clip("");
        b.key(Key::Letter('a'), Modifiers::CTRL, &mut c);
        b.key(Key::Letter('c'), Modifiers::CTRL, &mut c);
        assert_eq!(c.0, "hello");
        b.key(Key::Letter('x'), Modifiers::CTRL, &mut c);
        assert_eq!(b.value(), "");
        c.0 = "pasted".into();
        b.key(Key::Letter('v'), Modifiers::CTRL, &mut c);
        assert_eq!(b.value(), "pasted");
        assert_eq!(b.cursor(), 6);
    }

    #[test]
    fn paste_replaces_selection() {
        let mut b = boxed("hello world");
        let mut c = clip("big");
        b.move_cursor_to(6, false);
        b.move_cursor_to(11, true);
        b.key(Key::Letter('v'), Modifiers::CTRL, &mut c);
        assert_eq!(b.value(), "hello big");
        assert_eq!(b.cursor(), 9);
    }

    #[test]
    fn max_length_truncates_paste_and_typing() {
        let mut b = EditBox::new(0, [0, 0, 100, 20]).with_max_length(5);
        b.insert_text("abcdefgh");
        assert_eq!(b.value(), "abcde");
        b.char('z');
        assert_eq!(b.value(), "abcde");
        b.move_cursor_to(2, false);
        b.move_cursor_to(4, true);
        b.insert_text("123456");
        assert_eq!(b.value(), "ab12e");
    }

    #[test]
    fn rejects_control_and_section_characters() {
        let mut b = boxed("");
        assert!(!b.char('\n'));
        assert!(!b.char('§'));
        b.insert_text("a\n§b\u{7f}");
        assert_eq!(b.value(), "ab");
    }

    #[test]
    fn unicode_cursor_is_per_char() {
        let mut b = boxed("añb");
        b.move_cursor_to(2, false);
        b.delete_text(-1, false);
        assert_eq!(b.value(), "ab");
    }

    #[test]
    fn home_end() {
        let mut b = boxed("hello");
        let mut c = clip("");
        b.key(Key::Home, Modifiers::NONE, &mut c);
        assert_eq!(b.cursor(), 0);
        b.key(Key::End, Modifiers::SHIFT, &mut c);
        assert_eq!(b.selected(), "hello");
    }

    #[test]
    fn history_walks_and_restores_draft() {
        let mut b = boxed("");
        b.add_history("one");
        b.add_history("two");
        b.set_value("draft");
        b.history_step(-1);
        assert_eq!(b.value(), "two");
        b.history_step(-1);
        assert_eq!(b.value(), "one");
        b.history_step(-1);
        assert_eq!(b.value(), "one");
        b.history_step(1);
        assert_eq!(b.value(), "two");
        b.history_step(1);
        assert_eq!(b.value(), "draft");
    }

    #[test]
    fn history_skips_consecutive_duplicates() {
        let mut b = boxed("");
        b.add_history("same");
        b.add_history("same");
        assert_eq!(b.history().len(), 1);
    }

    #[test]
    fn click_places_cursor_by_width() {
        let mut b = boxed("abcdef");
        b.click_with(4.0 + 3.0 * 6.0 + 1.0, 0, false, &[6.0; 6]);
        assert_eq!(b.cursor(), 3);
    }

    #[test]
    fn non_editable_ignores_typing() {
        let mut b = boxed("x");
        b.editable = false;
        assert!(!b.char('y'));
        assert_eq!(b.value(), "x");
    }
}
