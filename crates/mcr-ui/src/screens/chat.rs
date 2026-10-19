use std::collections::VecDeque;

use mcr_protocol::types::text::Component;

use crate::gui::TooltipLines;
use crate::style::{Cell, with_alpha};
use crate::text::{ClickSpan, InsertSpan, click_at, flatten_chat, parse_legacy};
use crate::ui::{Ui, UiFrame};
use mcr_protocol::types::text::{ClickEvent, Color, NamedColor, Style as TextStyle};

const MAX_STORED_LINES: usize = 100;
const FADE_TICKS: f32 = 200.0;
const DELETED_MARKER: &str = "chat.deleted_marker";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChatOptions {
    pub width: i32,
    /// `getLinesPerPage`: the chat height over the line height.
    pub visible_lines: usize,
    pub background_opacity: f32,
    pub text_opacity: f32,
    pub focused: bool,
    /// `chatScale`.
    pub scale: f32,
    /// `chatLineSpacing`.
    pub line_spacing: f32,
    /// `chatColors`: false strips `§` codes from messages.
    pub colors: bool,
    /// `autoSuggestions`: the command popup opens without Tab.
    pub auto_suggestions: bool,
}

impl Default for ChatOptions {
    fn default() -> Self {
        Self {
            width: 320,
            visible_lines: 10,
            background_opacity: 0.5,
            text_opacity: 1.0,
            focused: false,
            scale: 1.0,
            line_spacing: 0.0,
            colors: true,
            auto_suggestions: true,
        }
    }
}

impl ChatOptions {
    /// `(int)(9 * (spacing + 1))`.
    pub fn line_height(&self) -> i32 {
        (9.0 * (self.line_spacing + 1.0)) as i32
    }

    /// Lines per page for a chat `height` in GUI pixels.
    pub fn lines_for_height(height: i32, line_spacing: f32) -> usize {
        let line = ((9.0 * (line_spacing + 1.0)) as i32).max(1);
        (height / line).max(0) as usize
    }

    /// Wrap width in chat space: `floor(width / scale)`.
    fn wrap_width(&self) -> i32 {
        let scale = self.scale.max(1.0e-3);
        (self.width as f32 / scale).floor().clamp(1.0, 100_000.0) as i32
    }
}

/// The chat's transform and line metrics, as `ChatComponent#extractRenderState` sets them up: chat space
/// is GUI space divided by the scale, with the text 4 px from the left edge.
#[derive(Clone, Copy, Debug)]
struct Layout {
    scale: f32,
    bottom: i32,
    entry: i32,
    text_offset: i32,
    max_width: i32,
}

impl Layout {
    fn new(options: &ChatOptions, gui_height: i32) -> Self {
        let scale = options.scale.max(0.0);
        let spacing = f64::from(options.line_spacing);
        let safe = scale.max(1.0e-3);
        Self {
            scale,
            bottom: ((gui_height - 40) as f32 / safe).floor() as i32,
            entry: options.line_height().max(1),
            text_offset: (8.0 * (spacing + 1.0) - 4.0 * spacing).round() as i32,
            max_width: (options.width as f32 / safe).ceil() as i32,
        }
    }

    fn visible(&self) -> bool {
        self.scale > 0.0
    }

    fn fill(&self, frame: &mut UiFrame<'_>, x0: i32, y0: i32, x1: i32, y1: i32, argb: u32) {
        let s = self.scale;
        frame.fill_rect_f(
            [x0 as f32 * s, y0 as f32 * s, x1 as f32 * s, y1 as f32 * s],
            argb,
        );
    }

    fn cells(&self, frame: &mut UiFrame<'_>, cells: &[Cell], x: i32, y: i32, argb: u32) {
        let s = self.scale;
        frame.cells_scaled(cells, x as f32 * s, y as f32 * s, argb, true, s);
    }

    fn chat_point(&self, x: f32, y: f32) -> (f32, f32) {
        let s = self.scale.max(1.0e-3);
        (x / s, y / s)
    }

    fn entry_bottom(&self, row: usize) -> i32 {
        self.bottom - row as i32 * self.entry
    }
}

/// What a click on the open chat hits: a click event, a Shift-click insertion, or the delayed-message
/// line.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatClick {
    Event(ClickEvent),
    Insert(String),
    ExpandQueue,
}

/// A player message waiting for the `chatDelay` interval.
#[derive(Debug)]
struct Queued {
    message: Component,
    tag: Option<ChatLineTag>,
    signature: Option<[u8; 256]>,
}

const MAX_QUEUED: usize = 1024;
const QUEUE_KEY: &str = "chat.queue";
const QUEUE_TOOLTIP: &str = "chat.queue.tooltip";
const MAX_COMMAND_HISTORY: usize = 50;

/// Vanilla's chat line tags: a colored bar left of the line and a tooltip in open chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatLineTag {
    System,
    /// `GuiMessageTag.systemSinglePlayer`, which vanilla puts on every system message.
    SystemSinglePlayer,
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
            Self::System | Self::SystemSinglePlayer | Self::NotSecure => 0xD0_D0D0,
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
            Self::SystemSinglePlayer => "chat.tag.system_single_player",
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
    inserts: Vec<InsertSpan>,
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
    new_since_scroll: bool,
    recent: Vec<String>,
    commands: VecDeque<String>,
    commands_changed: bool,
    next_entry: u64,
    signed: VecDeque<(u64, Box<[u8; 256]>, u32)>,
    pending_deletions: Vec<(Box<[u8; 256]>, u32)>,
    queue: VecDeque<Queued>,
    previous_message_ms: u64,
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
        let (cells, spans, inserts) = flatten_chat(ui.lang(), message, options.colors);
        let entry = self.add_cells(ui, &cells, (&spans, &inserts), tick, options, tag);
        if let Some(sig) = signature {
            self.signed.push_front((entry, Box::new(sig), tick));
            self.signed.truncate(MAX_STORED_LINES);
        }
    }

    pub fn add_text(&mut self, ui: &mut Ui, message: &str, tick: u32, options: &ChatOptions) {
        self.add_cells(ui, &parse_legacy(message), (&[], &[]), tick, options, None);
    }

    /// Lines top to bottom; a tag with an icon leaves room for it like vanilla's `splitLines`.
    fn make_lines(
        &mut self,
        ui: &mut Ui,
        cells: &[Cell],
        (spans, inserts): (&[ClickSpan], &[InsertSpan]),
        tick: u32,
        options: &ChatOptions,
        tag: Option<ChatLineTag>,
    ) -> (u64, Vec<ChatLine>) {
        let entry = self.next_entry;
        self.next_entry += 1;
        let mut width = options.wrap_width();
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
                let inserts = inserts
                    .iter()
                    .filter(|s| s.range.start < range.end && s.range.end > range.start)
                    .map(|s| InsertSpan {
                        range: s.range.start.max(range.start) - range.start
                            ..s.range.end.min(range.end) - range.start,
                        text: s.text.clone(),
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
                    inserts,
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
        spans: (&[ClickSpan], &[InsertSpan]),
        tick: u32,
        options: &ChatOptions,
        tag: Option<ChatLineTag>,
    ) -> u64 {
        let (entry, lines) = self.make_lines(ui, cells, spans, tick, options, tag);
        for line in lines {
            self.lines.push_front(line);
            if self.scroll > 0 {
                self.new_since_scroll = true;
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
        let (cells, spans, inserts) = flatten_chat(ui.lang(), &marker, options.colors);
        let (_, lines) = self.make_lines(
            ui,
            &cells,
            (&spans, &inserts),
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

    /// `scrollChat`: positive scrolls toward older lines.
    pub fn scroll_by(&mut self, delta: i32, visible_lines: usize) {
        let max = self.lines.len().saturating_sub(visible_lines);
        self.scroll = (self.scroll as i64 + i64::from(delta)).clamp(0, max as i64) as usize;
        if self.scroll == 0 {
            self.new_since_scroll = false;
        }
    }

    pub fn reset_scroll(&mut self) {
        self.scroll = 0;
        self.new_since_scroll = false;
    }

    pub fn recent(&self) -> &[String] {
        &self.recent
    }

    /// `addRecentChat`; commands also go to the persisted command history.
    pub fn add_recent(&mut self, line: &str) {
        if self.recent.last().is_none_or(|l| l != line) {
            self.recent.push(line.to_owned());
            if self.recent.len() > MAX_RECENT {
                self.recent.remove(0);
            }
        }
        if line.starts_with('/') && self.commands.back().is_none_or(|l| l != line) {
            if self.commands.len() >= MAX_COMMAND_HISTORY {
                self.commands.pop_front();
            }
            self.commands.push_back(line.to_owned());
            self.commands_changed = true;
        }
    }

    /// Seeds the history from `command_history.txt`, as vanilla's `ChatComponent` does on creation.
    pub fn load_command_history(&mut self, text: &str) {
        self.commands = text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.chars().take(256).collect())
            .collect();
        while self.commands.len() > MAX_COMMAND_HISTORY {
            self.commands.pop_front();
        }
        self.recent = self.commands.iter().cloned().collect();
        self.commands_changed = false;
    }

    /// The command history file's new content, once after each change.
    pub fn take_command_history(&mut self) -> Option<String> {
        if !std::mem::take(&mut self.commands_changed) {
            return None;
        }
        let mut out = String::new();
        for line in &self.commands {
            out.push_str(line);
            out.push('\n');
        }
        Some(out)
    }

    /// `ChatListener#handleMessage`: with a `chatDelay` the message waits in the queue.
    #[allow(clippy::too_many_arguments)]
    pub fn add_delayed(
        &mut self,
        ui: &mut Ui,
        message: &Component,
        tick: u32,
        options: &ChatOptions,
        tag: Option<ChatLineTag>,
        signature: Option<[u8; 256]>,
        (now_ms, delay_ms): (u64, u64),
    ) {
        if delay_ms == 0 {
            self.add_tagged(ui, message, tick, options, tag, signature);
            self.previous_message_ms = now_ms;
            return;
        }
        if self.queue.len() >= MAX_QUEUED
            && let Some(old) = self.queue.pop_front()
        {
            self.add_tagged(ui, &old.message, tick, options, old.tag, old.signature);
        }
        self.queue.push_back(Queued {
            message: message.clone(),
            tag,
            signature,
        });
    }

    /// `ChatListener#tick`: one queued message per delay interval, all of them once the delay is off.
    pub fn tick_queue(
        &mut self,
        ui: &mut Ui,
        tick: u32,
        options: &ChatOptions,
        (now_ms, delay_ms): (u64, u64),
    ) {
        if delay_ms == 0 {
            while let Some(q) = self.queue.pop_front() {
                self.add_tagged(ui, &q.message, tick, options, q.tag, q.signature);
            }
            self.previous_message_ms = 0;
        } else if now_ms >= self.previous_message_ms.saturating_add(delay_ms) {
            self.accept_next(ui, tick, options, now_ms);
        }
    }

    /// Shows the oldest queued message, as clicking the pending-lines line does.
    pub fn accept_next(&mut self, ui: &mut Ui, tick: u32, options: &ChatOptions, now_ms: u64) {
        if let Some(q) = self.queue.pop_front() {
            self.add_tagged(ui, &q.message, tick, options, q.tag, q.signature);
            self.previous_message_ms = now_ms;
        }
    }

    /// Drops a queued message the server deleted; false when it was not queued.
    pub fn remove_queued(&mut self, signature: &[u8; 256]) -> bool {
        let before = self.queue.len();
        self.queue
            .retain(|q| q.signature.as_ref().is_none_or(|s| s != signature));
        self.queue.len() != before
    }

    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    fn queue_cells(&self, ui: &mut Ui) -> Vec<Cell> {
        let text = Component::translatable(
            QUEUE_KEY,
            vec![mcr_protocol::types::text::Arg::Str(
                self.queue.len().to_string(),
            )],
        );
        ui.flatten(&text)
    }

    /// Click event under a GUI position of the open chat, if any.
    pub fn click_event_at(
        &self,
        x: f32,
        y: f32,
        gui_height: i32,
        options: &ChatOptions,
    ) -> Option<ClickEvent> {
        match self.line_click(x, y, gui_height, options, false)? {
            ChatClick::Event(e) => Some(e),
            _ => None,
        }
    }

    fn line_at(
        &self,
        x: f32,
        y: f32,
        gui_height: i32,
        options: &ChatOptions,
    ) -> Option<(&ChatLine, usize)> {
        let layout = Layout::new(options, gui_height);
        if !layout.visible() {
            return None;
        }
        let (x, y) = layout.chat_point(x, y);
        if y >= layout.bottom as f32 || x < 4.0 {
            return None;
        }
        let row = ((layout.bottom as f32 - y) / layout.entry as f32).floor() as usize;
        if row >= options.visible_lines {
            return None;
        }
        let line = self.lines.get(row + self.scroll)?;
        let text_top = layout.entry_bottom(row) - layout.text_offset;
        if y < text_top as f32 || y >= (text_top + 9) as f32 {
            return None;
        }
        let index = line.ends.iter().position(|&end| x < 4.0 + end)?;
        Some((line, index))
    }

    fn line_click(
        &self,
        x: f32,
        y: f32,
        gui_height: i32,
        options: &ChatOptions,
        shift: bool,
    ) -> Option<ChatClick> {
        let (line, index) = self.line_at(x, y, gui_height, options)?;
        if shift {
            return line
                .inserts
                .iter()
                .filter(|s| s.range.contains(&index))
                .min_by_key(|s| s.range.len())
                .map(|s| ChatClick::Insert(s.text.clone()));
        }
        click_at(&line.spans, index).cloned().map(ChatClick::Event)
    }

    /// `ChatScreen#mouseClicked`: Shift-click inserts, a plain click runs the click event, and the
    /// pending-lines line releases the next delayed message.
    pub fn click_at(
        &self,
        (x, y): (f32, f32),
        gui_height: i32,
        options: &ChatOptions,
        shift: bool,
        queue_width: f32,
    ) -> Option<ChatClick> {
        if !shift && self.queue_hit(queue_width, x, y, gui_height, options) {
            return Some(ChatClick::ExpandQueue);
        }
        self.line_click(x, y, gui_height, options, shift)
    }

    /// Width of the pending-lines line in chat space, for [`ChatHud::click_at`].
    pub fn queue_width(&self, ui: &mut Ui) -> f32 {
        if self.queue.is_empty() {
            return 0.0;
        }
        let cells = self.queue_cells(ui);
        ui.width(&cells)
    }

    fn queue_hit(
        &self,
        width: f32,
        x: f32,
        y: f32,
        gui_height: i32,
        options: &ChatOptions,
    ) -> bool {
        if self.queue.is_empty() {
            return false;
        }
        let layout = Layout::new(options, gui_height);
        if !layout.visible() {
            return false;
        }
        let (x, y) = layout.chat_point(x, y);
        let top = (layout.bottom + 1) as f32;
        x >= 4.0 && x < 4.0 + width && y >= top && y < top + 9.0
    }

    pub fn draw(&self, frame: &mut UiFrame<'_>, tick: u32, options: &ChatOptions) {
        let (_, gui_height) = frame.gui_size();
        let layout = Layout::new(options, gui_height);
        if !layout.visible() {
            return;
        }
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
        let right = layout.max_width + 12;
        for &(i, _, a) in visible.iter().rev() {
            let bottom = layout.entry_bottom(i);
            let color = with_alpha(0, a * options.background_opacity);
            layout.fill(frame, 0, bottom - layout.entry, right, bottom, color);
        }
        if !self.queue.is_empty() {
            let color = with_alpha(0, options.background_opacity);
            layout.fill(
                frame,
                2,
                layout.bottom,
                layout.max_width + 8,
                layout.bottom + 9,
                color,
            );
        }
        for &(i, line, a) in visible.iter().rev() {
            let bottom = layout.entry_bottom(i);
            let text_top = bottom - layout.text_offset;
            let color = with_alpha(0xFF_FFFF, a * options.text_opacity);
            layout.cells(frame, &line.cells, 4, text_top, color);
            if let Some(tag) = &line.tag {
                let bar = with_alpha(tag.indicator_color(), a * options.text_opacity);
                layout.fill(frame, 0, bottom - layout.entry, 2, bottom, bar);
            }
        }
        if !self.queue.is_empty() {
            let cells = self.queue_cells(frame.ui());
            let color = with_alpha(0xFF_FFFF, 0.5 * options.text_opacity);
            layout.cells(frame, &cells, 4, layout.bottom + 1, color);
        }
    }

    /// Open chat only: draws the modified icon of the hovered message and returns the tooltip of the
    /// tag bar, icon or pending-lines line under the mouse.
    pub fn hover(
        &self,
        frame: &mut UiFrame<'_>,
        mouse: (f32, f32),
        options: &ChatOptions,
    ) -> Option<TooltipLines> {
        let (_, gui_height) = frame.gui_size();
        let layout = Layout::new(options, gui_height);
        if !layout.visible() {
            return None;
        }
        let width = self.queue_width(frame.ui());
        if self.queue_hit(width, mouse.0, mouse.1, gui_height, options) {
            let mut lines = TooltipLines::default();
            lines.push_component(Component::translatable(QUEUE_TOOLTIP, Vec::new()));
            return Some(lines);
        }
        let (mx, my) = layout.chat_point(mouse.0, mouse.1);
        let over = |x0: i32, y0: i32, x1: i32, y1: i32| {
            mx >= x0 as f32 && mx < x1 as f32 && my >= y0 as f32 && my < y1 as f32
        };
        let rows: Vec<(i32, &ChatLine)> = self
            .lines
            .iter()
            .skip(self.scroll)
            .take(options.visible_lines)
            .enumerate()
            .map(|(i, l)| (layout.entry_bottom(i), l))
            .collect();
        let entry = layout.entry;
        let hovered_entry = rows.iter().find_map(|&(entry_bottom, l)| {
            let width = l.ends.last().copied().unwrap_or(0.0);
            (my >= (entry_bottom - entry) as f32
                && my < entry_bottom as f32
                && mx >= 4.0
                && mx < 4.0 + width)
                .then_some(l.entry)
        });
        let icon = frame.ui().sprite_id(MODIFIED_ICON);
        let mut tooltip = None;
        let s = layout.scale;
        for &(entry_bottom, line) in &rows {
            let Some(tag) = &line.tag else {
                continue;
            };
            if over(0, entry_bottom - entry, 2, entry_bottom) {
                tooltip = Some(tag.tooltip());
            }
            if !tag.has_icon() {
                continue;
            }
            let width = line.ends.last().copied().unwrap_or(0.0).ceil() as i32;
            let left = 4 + width + ICON_MARGIN;
            let icon_bottom = entry_bottom - layout.text_offset + 9;
            let top = icon_bottom - ICON_SIZE - 1;
            let on_icon = over(left, top, left + ICON_SIZE, icon_bottom);
            if on_icon {
                tooltip = Some(tag.tooltip());
            }
            let forced = line.end_of_entry && hovered_entry == Some(line.entry);
            if let (Some(id), true) = (icon, forced || on_icon) {
                let dst = [
                    left as f32 * s,
                    top as f32 * s,
                    (left + ICON_SIZE) as f32 * s,
                    (top + ICON_SIZE) as f32 * s,
                ];
                frame.sprite_f(id, dst, 0xFFFF_FFFF);
            }
        }
        tooltip
    }

    /// `ChatComponent`'s scroll bar at the right edge of the open chat.
    pub fn draw_scrollbar(&self, frame: &mut UiFrame<'_>, options: &ChatOptions) {
        let total = self.lines.len() as i32;
        let (_, gui_height) = frame.gui_size();
        let layout = Layout::new(options, gui_height);
        let count = (total - self.scroll as i32).clamp(0, options.visible_lines as i32);
        if total == 0 || !layout.visible() || count == total {
            return;
        }
        let chat_height = count * layout.entry;
        let virtual_height = total * layout.entry;
        let top = layout.bottom - self.scroll as i32 * chat_height / total;
        let height = chat_height * chat_height / virtual_height;
        let rgb = if self.new_since_scroll {
            0xCC_3333
        } else {
            0x33_33AA
        };
        let x = layout.max_width + 8;
        layout.fill(frame, x, top - height, x + 2, top, (96 << 24) | rgb);
        layout.fill(
            frame,
            x + 1,
            top - height,
            x + 2,
            top,
            (96 << 24) | 0xCC_CCCC,
        );
    }
}
