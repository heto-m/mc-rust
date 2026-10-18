use std::collections::VecDeque;

use mcr_protocol::types::text::Component;

use crate::gui::TooltipLines;
use crate::style::{Cell, with_alpha};
use crate::text::{ClickSpan, click_at, flatten_events, parse_legacy};
use crate::ui::{LINE_HEIGHT, Ui, UiFrame};
use mcr_protocol::types::text::{ClickEvent, Color, NamedColor, Style as TextStyle};

const MAX_STORED_LINES: usize = 100;
const FADE_TICKS: f32 = 200.0;
const DELETED_MARKER: &str = "chat.deleted_marker";

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

/// Vanilla's chat line tags: a colored bar left of the line and a tooltip in open chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatLineTag {
    System,
    NotSecure,
    /// A signed message the server changed; `original` is what the sender signed.
    Modified {
        original: String,
    },
    Error,
}

pub const MODIFIED_ICON: &str = "icon/chat_modified";
const ICON_SIZE: i32 = 9;
const ICON_MARGIN: i32 = 4;

impl ChatLineTag {
    pub fn indicator_color(&self) -> u32 {
        match self {
            Self::System | Self::NotSecure => 0xD0_D0D0,
            Self::Modified { .. } => 0x60_6060,
            Self::Error => 0xFF_5555,
        }
    }

    pub fn has_icon(&self) -> bool {
        matches!(self, Self::Modified { .. })
    }

    pub fn tooltip(&self) -> TooltipLines {
        let key = match self {
            Self::System => "chat.tag.system",
            Self::NotSecure => "chat.tag.not_secure",
            Self::Modified { .. } => "chat.tag.modified",
            Self::Error => "chat.tag.error",
        };
        let mut lines = TooltipLines::default();
        lines.push_component(Component::translatable(key, Vec::new()));
        if let Self::Modified { original } = self {
            lines.push_component(Component::text(original.clone()).styled(TextStyle {
                color: Some(Color::Named(NamedColor::Gray)),
                ..TextStyle::default()
            }));
        }
        lines
    }
}

#[derive(Debug)]
struct ChatLine {
    cells: Vec<Cell>,
    added: u32,
    spans: Vec<ClickSpan>,
    ends: Vec<f32>,
    tag: Option<ChatLineTag>,
    entry: u64,
    /// The message's bottom line, where vanilla forces the modified icon while the message is hovered.
    end_of_entry: bool,
}

/// Recent chat messages, newest line first, wrapped to the chat width when added.
#[derive(Debug, Default)]
pub struct ChatHud {
    lines: VecDeque<ChatLine>,
    scroll: usize,
    recent: Vec<String>,
    next_entry: u64,
    signed: VecDeque<(u64, Box<[u8; 256]>, u32)>,
    pending_deletions: Vec<(Box<[u8; 256]>, u32)>,
}

const MAX_RECENT: usize = 100;
/// Vanilla waits until a message is 60 ticks old before replacing it with the deleted marker.
const DELETE_AFTER_TICKS: u32 = 60;
const MAX_PENDING_DELETIONS: usize = 64;

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
        self.add_tagged(ui, message, tick, options, None, None);
    }

    /// A message with its trust tag; a signature lets the server delete it later.
    pub fn add_tagged(
        &mut self,
        ui: &mut Ui,
        message: &Component,
        tick: u32,
        options: &ChatOptions,
        tag: Option<ChatLineTag>,
        signature: Option<[u8; 256]>,
    ) {
        let (cells, spans) = flatten_events(ui.lang(), message);
        let entry = self.add_cells(ui, &cells, &spans, tick, options, tag);
        if let Some(sig) = signature {
            self.signed.push_front((entry, Box::new(sig), tick));
            self.signed.truncate(MAX_STORED_LINES);
        }
    }

    pub fn add_text(&mut self, ui: &mut Ui, message: &str, tick: u32, options: &ChatOptions) {
        self.add_cells(ui, &parse_legacy(message), &[], tick, options, None);
    }

    /// Lines top to bottom; a tag with an icon leaves room for it like vanilla's `splitLines`.
    fn make_lines(
        &mut self,
        ui: &mut Ui,
        cells: &[Cell],
        spans: &[ClickSpan],
        tick: u32,
        options: &ChatOptions,
        tag: Option<ChatLineTag>,
    ) -> (u64, Vec<ChatLine>) {
        let entry = self.next_entry;
        self.next_entry += 1;
        let mut width = options.width;
        if tag.as_ref().is_some_and(ChatLineTag::has_icon) {
            width -= ICON_SIZE + ICON_MARGIN + 2;
        }
        let ranges = ui.wrap(cells, width.max(1) as f32);
        let count = ranges.len();
        let lines = ranges
            .into_iter()
            .enumerate()
            .map(|(i, range)| {
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
                ChatLine {
                    cells: cells[range].to_vec(),
                    added: tick,
                    spans,
                    ends,
                    tag: tag.clone(),
                    entry,
                    end_of_entry: i + 1 == count,
                }
            })
            .collect();
        (entry, lines)
    }

    fn add_cells(
        &mut self,
        ui: &mut Ui,
        cells: &[Cell],
        spans: &[ClickSpan],
        tick: u32,
        options: &ChatOptions,
        tag: Option<ChatLineTag>,
    ) -> u64 {
        let (entry, lines) = self.make_lines(ui, cells, spans, tick, options, tag);
        for line in lines {
            self.lines.push_front(line);
            if self.scroll > 0 {
                self.scroll += 1;
            }
        }
        self.lines.truncate(MAX_STORED_LINES);
        self.scroll = self.scroll.min(self.lines.len().saturating_sub(1));
        entry
    }

    /// Vanilla's `deleteMessage`: the line becomes the deleted marker once it is 60 ticks old.
    pub fn delete(&mut self, ui: &mut Ui, signature: &[u8; 256], tick: u32, options: &ChatOptions) {
        let Some(&(_, _, added)) = self.signed.iter().find(|(_, s, _)| **s == *signature) else {
            return;
        };
        if tick >= added.saturating_add(DELETE_AFTER_TICKS) {
            self.replace_with_marker(ui, signature, options);
        } else if self.pending_deletions.len() < MAX_PENDING_DELETIONS {
            self.pending_deletions.push((
                Box::new(*signature),
                added.saturating_add(DELETE_AFTER_TICKS),
            ));
        }
    }

    /// Applies deletions that were waiting for their message to age.
    pub fn process_deletions(&mut self, ui: &mut Ui, tick: u32, options: &ChatOptions) {
        if self.pending_deletions.is_empty() {
            return;
        }
        let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending_deletions)
            .into_iter()
            .partition(|(_, after)| tick >= *after);
        self.pending_deletions = waiting;
        for (signature, _) in due {
            self.replace_with_marker(ui, &signature, options);
        }
    }

    fn replace_with_marker(&mut self, ui: &mut Ui, signature: &[u8; 256], options: &ChatOptions) {
        let Some(pos) = self.signed.iter().position(|(_, s, _)| **s == *signature) else {
            return;
        };
        let Some((entry, _, added)) = self.signed.remove(pos) else {
            return;
        };
        let Some(first) = self.lines.iter().position(|l| l.entry == entry) else {
            return;
        };
        self.lines.retain(|l| l.entry != entry);
        let marker = Component::translatable(DELETED_MARKER, Vec::new()).styled(TextStyle {
            color: Some(Color::Named(NamedColor::Gray)),
            italic: Some(true),
            ..TextStyle::default()
        });
        let (cells, spans) = flatten_events(ui.lang(), &marker);
        let (_, lines) = self.make_lines(
            ui,
            &cells,
            &spans,
            added,
            options,
            Some(ChatLineTag::System),
        );
        for line in lines {
            self.lines.insert(first.min(self.lines.len()), line);
        }
        self.lines.truncate(MAX_STORED_LINES);
        self.scroll = self.scroll.min(self.lines.len().saturating_sub(1));
    }

    /// The plain text and tag of each stored line, newest first.
    pub fn line_texts(&self) -> Vec<(String, Option<ChatLineTag>)> {
        self.lines
            .iter()
            .map(|l| (l.cells.iter().map(|c| c.ch).collect(), l.tag.clone()))
            .collect()
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
            let entry_bottom = bottom - i as i32 * LINE_HEIGHT;
            let text_top = entry_bottom - 8;
            let color = with_alpha(0xFF_FFFF, a * options.text_opacity);
            frame.cells(&line.cells, 4.0, text_top as f32, color, true);
            if let Some(tag) = &line.tag {
                let bar = with_alpha(tag.indicator_color(), a * options.text_opacity);
                frame.fill_rect(0, entry_bottom - LINE_HEIGHT, 2, entry_bottom, bar);
            }
        }
    }

    /// Open chat only: draws the modified icon of the hovered message and returns the tooltip of the
    /// tag bar or icon under the mouse.
    pub fn hover(
        &self,
        frame: &mut UiFrame<'_>,
        mouse: (f32, f32),
        options: &ChatOptions,
    ) -> Option<TooltipLines> {
        let (_, gui_height) = frame.gui_size();
        let bottom = gui_height - 40;
        let (mx, my) = mouse;
        let over = |x0: i32, y0: i32, x1: i32, y1: i32| {
            mx >= x0 as f32 && mx < x1 as f32 && my >= y0 as f32 && my < y1 as f32
        };
        let rows: Vec<(i32, &ChatLine)> = self
            .lines
            .iter()
            .skip(self.scroll)
            .take(options.visible_lines)
            .enumerate()
            .map(|(i, l)| (bottom - i as i32 * LINE_HEIGHT, l))
            .collect();
        let hovered_entry = rows.iter().find_map(|&(entry_bottom, l)| {
            let width = l.ends.last().copied().unwrap_or(0.0);
            (my >= (entry_bottom - LINE_HEIGHT) as f32
                && my < entry_bottom as f32
                && mx >= 4.0
                && mx < 4.0 + width)
                .then_some(l.entry)
        });
        let icon = frame.ui().sprite_id(MODIFIED_ICON);
        let mut tooltip = None;
        for &(entry_bottom, line) in &rows {
            let Some(tag) = &line.tag else {
                continue;
            };
            if over(0, entry_bottom - LINE_HEIGHT, 2, entry_bottom) {
                tooltip = Some(tag.tooltip());
            }
            if !tag.has_icon() {
                continue;
            }
            let width = line.ends.last().copied().unwrap_or(0.0).ceil() as i32;
            let left = 4 + width + ICON_MARGIN;
            let icon_bottom = entry_bottom - 8 + LINE_HEIGHT;
            let top = icon_bottom - ICON_SIZE - 1;
            let on_icon = over(left, top, left + ICON_SIZE, icon_bottom);
            if on_icon {
                tooltip = Some(tag.tooltip());
            }
            let forced = line.end_of_entry && hovered_entry == Some(line.entry);
            if let (Some(id), true) = (icon, forced || on_icon) {
                frame.sprite(id, left, top, ICON_SIZE, ICON_SIZE);
            }
        }
        tooltip
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
