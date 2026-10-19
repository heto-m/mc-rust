use std::cell::RefCell;
use std::rc::Rc;

use std::ops::Range;

use mcr_protocol::types::text::{ClickEvent, Component};

use super::clicks::click_event_actions;
use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, MouseButton, UiInput, contains};
use super::screen::{Screen, ScreenAction};
use super::tooltip::TooltipLines;
use super::widgets::{Button, WidgetEvent, WidgetEventKind};
use crate::screens::{ChatClick, ChatHud, ChatOptions};

const INPUT_ID: u32 = 1;
const LEAVE_BED_ID: u32 = 2;
const LIST_LIMIT: usize = 10;
const ROW: i32 = 12;
const FILL: u32 = 0xD000_0000;
const SELECTED: u32 = 0xFFFF_FF00;
const UNSELECTED: u32 = 0xFFAA_AAAA;
const MAX_SUGGESTIONS: usize = 1000;
const MAX_SUGGESTION_CHARS: usize = 256;

/// One completion; `start..end` are character indices into the input it replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub tooltip: Option<String>,
}

/// The answer to one request: argument colours at once, suggestions and usage when they are known.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SuggestionSet {
    /// The input this answers; answers for older text are dropped.
    pub text: String,
    pub suggestions: Vec<Suggestion>,
    /// `CommandSuggestions#commandUsage`: errors or argument usage, shown when the popup is closed.
    pub usage: Vec<Component>,
    /// Character index the usage lines line up with.
    pub usage_start: usize,
    /// Colour runs as (character range, RGB); empty for plain chat.
    pub highlight: Vec<(Range<usize>, u32)>,
    pub command: bool,
    /// Only the colours: the suggestions follow in a later set, or were not asked for.
    pub pending: bool,
}

/// Backs the suggestion popup; the app adapts it to the Brigadier tree and `command_suggestions` round trips.
pub trait CommandSuggester {
    /// `suggest` false re-parses for colours only, as when a used suggestion keeps the popup.
    fn request(&mut self, text: &str, cursor: usize, suggest: bool);

    /// The result of the latest request once it has arrived; returned once.
    fn poll(&mut self) -> Option<SuggestionSet>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoSuggestions;

impl CommandSuggester for NoSuggestions {
    fn request(&mut self, _text: &str, _cursor: usize, _suggest: bool) {}

    fn poll(&mut self) -> Option<SuggestionSet> {
        None
    }
}

pub type SharedChat = Rc<RefCell<ChatHud>>;

fn apply(original: &str, s: &Suggestion) -> String {
    let chars: Vec<char> = original.chars().collect();
    let start = s.start.min(chars.len());
    let end = s.end.clamp(start, chars.len());
    let mut out: String = chars[..start].iter().collect();
    out.push_str(&s.text);
    out.extend(&chars[end..]);
    out
}

/// `getLastWordIndex`: just past the last run of whitespace.
fn last_word_start(text: &str) -> usize {
    let mut result = 0;
    for (i, c) in text.chars().enumerate() {
        if c.is_whitespace() {
            result = i + 1;
        }
    }
    result
}

#[derive(Debug)]
struct Popup {
    list: Vec<Suggestion>,
    original: String,
    current: usize,
    offset: usize,
    tab_cycles: bool,
    rect: [i32; 4],
    last_mouse: (f32, f32),
}

pub struct ChatScreen {
    chat: SharedChat,
    suggester: Box<dyn CommandSuggester>,
    options: ChatOptions,
    widgets: super::widgets::WidgetSet,
    gui: (i32, i32),
    initial: String,
    popup: Option<Popup>,
    usage: Vec<Component>,
    usage_start: usize,
    allow_suggestions: bool,
    keep_suggestions: bool,
    pending: Option<SuggestionSet>,
    mouse: (f32, f32),
    history_pos: usize,
    history_draft: String,
    popup_anchor: usize,
    in_bed: bool,
    shift: bool,
    queue_width: f32,
}

impl std::fmt::Debug for ChatScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatScreen")
            .field("popup", &self.popup)
            .finish()
    }
}

impl ChatScreen {
    pub fn new(
        chat: SharedChat,
        suggester: Box<dyn CommandSuggester>,
        options: ChatOptions,
    ) -> Self {
        let history_pos = chat.borrow().recent().len();
        Self {
            chat,
            suggester,
            options: ChatOptions {
                focused: true,
                ..options
            },
            widgets: Default::default(),
            gui: (0, 0),
            initial: String::new(),
            popup: None,
            usage: Vec::new(),
            usage_start: 0,
            allow_suggestions: false,
            keep_suggestions: false,
            pending: None,
            mouse: (0.0, 0.0),
            history_pos,
            history_draft: String::new(),
            popup_anchor: 0,
            in_bed: false,
            shift: false,
            queue_width: 0.0,
        }
    }

    pub fn with_initial(mut self, text: &str) -> Self {
        self.initial = text.to_owned();
        self
    }

    /// Vanilla `InBedChatScreen`: a Leave Bed button, Esc wakes up instead of closing, and sending
    /// a message keeps the screen open.
    pub fn in_bed(mut self) -> Self {
        self.in_bed = true;
        self
    }

    pub fn is_in_bed(&self) -> bool {
        self.in_bed
    }

    /// `InBedChatScreen#onPlayerWokeUp`: typed text stays in a plain chat screen.
    fn woke_up(&mut self, out: &mut Vec<ScreenAction>) {
        self.in_bed = false;
        if self.text().is_empty() {
            out.push(ScreenAction::Pop);
        } else {
            let (w, h) = self.gui;
            self.init(w, h);
        }
    }

    pub fn text(&self) -> &str {
        self.widgets.edit(INPUT_ID).map_or("", EditBox::value)
    }

    pub fn popup_len(&self) -> usize {
        self.popup.as_ref().map_or(0, |p| p.list.len())
    }

    pub fn selected_suggestion(&self) -> Option<&Suggestion> {
        self.popup.as_ref().and_then(|p| p.list.get(p.current))
    }

    fn edit(&mut self) -> &mut EditBox {
        self.widgets.edit_mut(INPUT_ID).expect("chat input exists")
    }

    fn edited(&mut self) {
        self.allow_suggestions = true;
        self.update_info();
    }

    /// `CommandSuggestions#updateCommandInfo`.
    fn update_info(&mut self) {
        let keep = self.keep_suggestions && self.popup.is_some();
        if !self.keep_suggestions {
            self.popup = None;
            self.edit().set_suggestion(None);
        }
        self.usage.clear();
        let (text, cursor) = {
            let e = self.edit();
            e.set_colors(Vec::new());
            (e.value().to_owned(), e.cursor())
        };
        if !keep {
            self.pending = None;
        }
        self.suggester.request(&text, cursor, !keep);
    }

    fn take_results(&mut self) {
        while let Some(set) = self.suggester.poll() {
            if set.text != self.text() {
                continue;
            }
            self.edit().set_colors(set.highlight.clone());
            if set.pending {
                continue;
            }
            let command = set.command;
            self.usage = set.usage.clone();
            self.usage_start = set.usage_start;
            self.pending = Some(set);
            if command {
                self.popup = None;
                if self.allow_suggestions && self.options.auto_suggestions {
                    self.show_suggestions();
                }
            }
        }
    }

    /// `CommandSuggestions#showSuggestions`.
    fn show_suggestions(&mut self) {
        let Some(set) = self.pending.clone() else {
            return;
        };
        if set.suggestions.is_empty() {
            return;
        }
        let (text, cursor) = {
            let e = self.edit();
            (e.value().to_owned(), e.cursor())
        };
        let prefix: String = text.chars().take(cursor).collect();
        let word_start = last_word_start(&prefix);
        let word: String = prefix
            .chars()
            .skip(word_start)
            .collect::<String>()
            .to_lowercase();
        let (mut list, mut rest) = (Vec::new(), Vec::new());
        for mut s in set.suggestions.into_iter().take(MAX_SUGGESTIONS) {
            s.text = s.text.chars().take(MAX_SUGGESTION_CHARS).collect();
            if s.text.starts_with(&word) || s.text.starts_with(&format!("minecraft:{word}")) {
                list.push(s);
            } else {
                rest.push(s);
            }
        }
        list.extend(rest);
        self.popup_anchor = list[0].start;
        self.popup = Some(Popup {
            list,
            original: text,
            current: 0,
            offset: 0,
            tab_cycles: false,
            rect: [0; 4],
            last_mouse: (-1.0, -1.0),
        });
        self.select(0);
    }

    /// Selects a suggestion and shows the rest of it as ghost text.
    fn select(&mut self, index: i64) {
        let Some(p) = &mut self.popup else {
            return;
        };
        let n = p.list.len() as i64;
        p.current = index.rem_euclid(n) as usize;
        let applied = apply(&p.original, &p.list[p.current]);
        let value = self.text().to_owned();
        let ghost = applied.strip_prefix(value.as_str()).map(str::to_owned);
        self.edit().set_suggestion(ghost);
    }

    fn cycle(&mut self, direction: i64) {
        let Some(p) = &mut self.popup else {
            return;
        };
        let next = p.current as i64 + direction;
        self.select(next);
        let Some(p) = &mut self.popup else {
            return;
        };
        let max_offset = p.list.len().saturating_sub(LIST_LIMIT);
        if p.current < p.offset {
            p.offset = p.current.min(max_offset);
        } else if p.current > p.offset + LIST_LIMIT - 1 {
            p.offset = (p.current + 1).saturating_sub(LIST_LIMIT).min(max_offset);
        }
    }

    fn use_suggestion(&mut self) {
        let Some(p) = &mut self.popup else {
            return;
        };
        let s = p.list[p.current].clone();
        let current = p.current as i64;
        let new = apply(&p.original, &s);
        let end = s.start + s.text.chars().count();
        self.keep_suggestions = true;
        let e = self.edit();
        e.set_value(&new);
        e.move_cursor_to(end, false);
        e.take_changed();
        self.edited();
        self.select(current);
        self.keep_suggestions = false;
        if let Some(p) = &mut self.popup {
            p.tab_cycles = true;
        }
    }

    fn history(&mut self, direction: i32) {
        let chat = self.chat.clone();
        let chat = chat.borrow();
        let recent = chat.recent();
        let max = recent.len();
        let new = (self.history_pos as i64 + i64::from(direction)).clamp(0, max as i64) as usize;
        if new == self.history_pos {
            return;
        }
        if new == max {
            self.history_pos = max;
            let draft = self.history_draft.clone();
            self.edit().set_value(&draft);
        } else {
            if self.history_pos == max {
                self.history_draft = self.edit().value().to_owned();
            }
            self.edit().set_value(&recent[new]);
            self.edit().take_changed();
            self.edited();
            self.allow_suggestions = false;
            self.popup = None;
            self.history_pos = new;
            return;
        }
        self.edit().take_changed();
        self.edited();
    }

    fn submit(&mut self, out: &mut Vec<ScreenAction>) {
        let raw = self.edit().value().to_owned();
        let msg = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if !msg.is_empty() {
            self.chat.borrow_mut().add_recent(&msg);
            match msg.strip_prefix('/') {
                Some(command) => out.push(ScreenAction::SendCommand(command.to_owned())),
                None => out.push(ScreenAction::SendChat(msg)),
            }
        }
        self.chat.borrow_mut().reset_scroll();
        if self.in_bed {
            self.edit().set_value("");
            self.edit().take_changed();
            self.popup = None;
            self.history_pos = self.chat.borrow().recent().len();
        } else {
            out.push(ScreenAction::Pop);
        }
    }

    fn pressed_leave_bed(events: &[WidgetEvent], out: &mut Vec<ScreenAction>) -> bool {
        let pressed = events
            .iter()
            .any(|e| e.id == LEAVE_BED_ID && e.kind == WidgetEventKind::Pressed);
        if pressed {
            out.push(ScreenAction::ClickSound);
            out.push(ScreenAction::StopSleeping);
        }
        pressed
    }

    /// `ChatScreen#handleComponentClicked` for a click event in the chat.
    pub fn handle_click_event(&mut self, event: ClickEvent, out: &mut Vec<ScreenAction>) {
        match event {
            ClickEvent::SuggestCommand(text) => {
                self.edit().set_value(&text);
                self.edit().take_changed();
                self.edited();
            }
            other => click_event_actions(other, out),
        }
    }

    fn insert(&mut self, text: &str) {
        self.edit().insert_text(text);
        self.edit().take_changed();
        self.edited();
    }

    fn popup_key(&mut self, key: Key, mods: super::input::Modifiers) -> bool {
        if self.popup.is_none() {
            if key == Key::Tab {
                self.show_suggestions();
                return true;
            }
            return false;
        }
        match key {
            Key::Up => {
                self.cycle(-1);
                self.popup.as_mut().expect("popup").tab_cycles = false;
                true
            }
            Key::Down => {
                self.cycle(1);
                self.popup.as_mut().expect("popup").tab_cycles = false;
                true
            }
            Key::Tab => {
                if self.popup.as_ref().is_some_and(|p| p.tab_cycles) {
                    self.cycle(if mods.shift { -1 } else { 1 });
                }
                self.use_suggestion();
                true
            }
            Key::Escape => {
                self.popup = None;
                self.edit().set_suggestion(None);
                true
            }
            _ => false,
        }
    }
}

impl Screen for ChatScreen {
    fn init(&mut self, width: i32, height: i32) {
        let keep = if self.gui == (0, 0) {
            self.initial.clone()
        } else {
            self.text().to_owned()
        };
        self.gui = (width, height);
        self.widgets.clear();
        let mut input =
            EditBox::new(INPUT_ID, [4, height - 12, width - 4, 12]).with_max_length(256);
        input.bordered = false;
        input.set_value(&keep);
        input.take_changed();
        self.widgets.add(input);
        if self.in_bed {
            self.widgets.add(Button::new(
                LEAVE_BED_ID,
                [width / 2 - 100, height - 40, 200, 20],
                "lang:multiplayer.stopSleeping",
            ));
        }
        self.widgets.set_focus(Some(INPUT_ID), 0);
        self.popup = None;
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.mouse = cx.mouse;
        self.take_results();
        let (w, h) = self.gui;
        cx.frame.fill_rect(2, h - 14, w - 2, h - 2, 0x8000_0000);
        {
            let chat = self.chat.borrow();
            self.queue_width = chat.queue_width(cx.frame.ui());
            chat.draw(cx.frame, (cx.now_ms / 50) as u32, &self.options);
            chat.draw_scrollbar(cx.frame, &self.options);
            if let Some(tooltip) = chat.hover(cx.frame, cx.mouse, &self.options) {
                cx.set_tooltip(tooltip);
            }
        }
        self.widgets.render(cx);
        self.draw_popup(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        match *input {
            UiInput::Key { key, mods, .. } => {
                self.shift = mods.shift;
                if self.popup_key(key, mods) {
                    return true;
                }
                match key {
                    Key::Escape if self.in_bed => {
                        out.push(ScreenAction::StopSleeping);
                        return true;
                    }
                    Key::Escape => {
                        self.chat.borrow_mut().reset_scroll();
                        out.push(ScreenAction::Pop);
                        return true;
                    }
                    Key::Enter => {
                        self.submit(out);
                        return true;
                    }
                    Key::PageUp => {
                        self.chat.borrow_mut().scroll_by(
                            self.options.visible_lines as i32 - 1,
                            self.options.visible_lines,
                        );
                        return true;
                    }
                    Key::PageDown => {
                        self.chat.borrow_mut().scroll_by(
                            1 - self.options.visible_lines as i32,
                            self.options.visible_lines,
                        );
                        return true;
                    }
                    Key::Up => {
                        self.history(-1);
                        return true;
                    }
                    Key::Down => {
                        self.history(1);
                        return true;
                    }
                    _ => {}
                }
                let mut events = Vec::new();
                let used = self.widgets.input(env, input, &mut events);
                if Self::pressed_leave_bed(&events, out) {
                    return true;
                }
                if !events.is_empty() {
                    self.edited();
                }
                used
            }
            UiInput::Char(_) => {
                let mut events = Vec::new();
                let used = self.widgets.input(env, input, &mut events);
                if !events.is_empty() {
                    self.edited();
                }
                used
            }
            UiInput::MousePress {
                x, y, button, mods, ..
            } => {
                self.shift = mods.shift;
                if let Some(p) = &self.popup
                    && contains(p.rect, x, y)
                {
                    let line = ((y as i32 - p.rect[1]) / ROW) as usize + p.offset;
                    if line < p.list.len() {
                        self.select(line as i64);
                        self.use_suggestion();
                    }
                    return true;
                }
                if button == MouseButton::Left {
                    let click = self.chat.borrow().click_at(
                        (x, y),
                        self.gui.1,
                        &self.options,
                        mods.shift,
                        self.queue_width,
                    );
                    match click {
                        Some(ChatClick::Event(event)) => {
                            self.handle_click_event(event, out);
                            return true;
                        }
                        Some(ChatClick::Insert(text)) => {
                            self.insert(&text);
                            return true;
                        }
                        Some(ChatClick::ExpandQueue) => {
                            out.push(ScreenAction::ChatQueueNext);
                            return true;
                        }
                        None => {}
                    }
                }
                let mut events = Vec::new();
                self.widgets.input(env, input, &mut events);
                Self::pressed_leave_bed(&events, out);
                true
            }
            UiInput::Scroll { x, y, dy, mods, .. } => {
                let dy = dy.clamp(-1.0, 1.0);
                if let Some(p) = &mut self.popup
                    && contains(p.rect, x, y)
                {
                    let max = p.list.len().saturating_sub(LIST_LIMIT) as i64;
                    p.offset = (p.offset as i64 - dy as i64).clamp(0, max) as usize;
                    return true;
                }
                let lines = if mods.shift { dy } else { dy * 7.0 };
                self.chat
                    .borrow_mut()
                    .scroll_by(lines as i32, self.options.visible_lines);
                true
            }
            UiInput::MouseMove { x, y } => {
                self.mouse = (x, y);
                false
            }
            UiInput::MouseRelease { .. } => false,
        }
    }

    fn tick(&mut self, env: &mut Env<'_>, out: &mut Vec<ScreenAction>) {
        self.take_results();
        if self.in_bed && !env.view.sleeping() {
            self.woke_up(out);
        }
    }
}

impl ChatScreen {
    fn draw_popup(&mut self, cx: &mut RenderCx<'_, '_>) {
        let (w, h) = self.gui;
        let mouse = self.mouse;
        let Some(p) = &mut self.popup else {
            let lines: Vec<_> = self
                .usage
                .iter()
                .map(|c| cx.frame.ui().flatten(c))
                .collect();
            let mut width = 0;
            for cells in &lines {
                width = width.max(cx.frame.ui().width(cells).ceil() as i32);
            }
            let prefix: String = self.text().chars().take(self.usage_start).collect();
            let x = (4 + cx.text_width(&prefix)).clamp(0, (w - width).max(0));
            for (i, cells) in lines.iter().enumerate() {
                let y = h - 27 - ROW * i as i32;
                cx.frame.fill_rect(x - 1, y, x + width + 1, y + ROW, FILL);
                cx.frame
                    .cells(cells, x as f32, (y + 2) as f32, 0xFFFF_FFFF, true);
            }
            return;
        };
        let limit = p.list.len().min(LIST_LIMIT);
        let mut max_w = 0;
        for s in &p.list {
            max_w = max_w.max(cx.text_width(&s.text));
        }
        let prefix: String = p.original.chars().take(self.popup_anchor).collect();
        let anchor_x = 4 + cx.text_width(&prefix);
        let x = anchor_x.clamp(0, (4 + w - 4 - max_w).max(0));
        let y = h - 12 - 3 - limit as i32 * ROW;
        p.rect = [x, y, max_w + 1, limit as i32 * ROW];
        let (has_prev, has_next) = (p.offset > 0, p.list.len() > p.offset + limit);
        if has_prev || has_next {
            cx.frame.fill_rect(x, y - 1, x + max_w + 1, y, FILL);
            cx.frame
                .fill_rect(x, y + p.rect[3], x + max_w + 1, y + p.rect[3] + 1, FILL);
            for dx in (0..=max_w).step_by(2) {
                if has_prev {
                    cx.frame
                        .fill_rect(x + dx, y - 1, x + dx + 1, y, 0xFFFF_FFFF);
                }
                if has_next {
                    cx.frame.fill_rect(
                        x + dx,
                        y + p.rect[3],
                        x + dx + 1,
                        y + p.rect[3] + 1,
                        0xFFFF_FFFF,
                    );
                }
            }
        }
        let moved = p.last_mouse != mouse;
        p.last_mouse = mouse;
        let mut hovered = false;
        for i in 0..limit {
            let row_y = y + ROW * i as i32;
            cx.frame
                .fill_rect(x, row_y, x + max_w + 1, row_y + ROW, FILL);
            if mouse.0 > x as f32
                && mouse.0 < (x + max_w + 1) as f32
                && mouse.1 > row_y as f32
                && mouse.1 < (row_y + ROW) as f32
            {
                if moved {
                    p.current = i + p.offset;
                }
                hovered = true;
            }
            let color = if i + p.offset == p.current {
                SELECTED
            } else {
                UNSELECTED
            };
            let text = p.list[i + p.offset].text.clone();
            cx.text(&text, x + 1, row_y + 2, color, true);
        }
        if hovered && let Some(t) = p.list[p.current].tooltip.clone() {
            cx.set_tooltip(TooltipLines::text(t));
        }
    }
}
