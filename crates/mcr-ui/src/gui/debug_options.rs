//! `DebugOptionsScreen` (F3+F6): a searchable list of debug entries with Off, In Overlay and
//! Always buttons, and the profiles.

use super::debug_entries::{Category, DebugEntries, DebugProfile, ENTRIES, EntryStatus};
use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, MouseButton, UiInput, contains};
use super::list::ScrollList;
use super::menus::{WHITE, draw_dim_background};
use super::screen::{Screen, ScreenAction};
use super::tooltip::TooltipLines;
use super::widgets::{Button, WidgetEvent, WidgetEventKind, WidgetSet};

const SEARCH: u32 = 200;
const PROFILE_BASE: u32 = 201;
const DONE: u32 = 210;
const HEADER: i32 = 61;
const FOOTER: i32 = 33;
const ROW: i32 = 20;
const ROW_WIDTH: i32 = 350;
const BUTTON_WIDTH: i32 = 60;
const BUTTON_HEIGHT: i32 = 16;
const WARNING: u32 = 0xFFDF_5050;
const GRAY: u32 = 0xFFBA_BABA;
const OVERLAY: u32 = 0xFFFF_FF55;
const STATUSES: [(EntryStatus, &str, u32); 3] = [
    (EntryStatus::Never, "options.off", WHITE),
    (EntryStatus::InOverlay, "debug.entry.overlay", OVERLAY),
    (EntryStatus::AlwaysOn, "debug.entry.always", WARNING),
];

#[derive(Clone, Copy, Debug)]
enum Row {
    Category(Category),
    Entry(usize),
}

#[derive(Debug)]
pub struct DebugOptionsScreen {
    widgets: WidgetSet,
    gui: (i32, i32),
    mouse: (f32, f32),
    entries: DebugEntries,
    reduced: bool,
    rows: Vec<Row>,
    list: ScrollList,
}

impl DebugOptionsScreen {
    /// `reduced` is whether reduced debug info hides the world entries.
    pub fn new(entries: DebugEntries, reduced: bool) -> Self {
        let mut screen = Self {
            widgets: WidgetSet::default(),
            gui: (0, 0),
            mouse: (0.0, 0.0),
            entries,
            reduced,
            rows: Vec::new(),
            list: ScrollList::new([0, HEADER, 0, 0], ROW),
        };
        screen.search("");
        screen
    }

    pub fn entries(&self) -> &DebugEntries {
        &self.entries
    }

    fn search(&mut self, query: &str) {
        self.rows.clear();
        let mut category = None;
        for (i, entry) in ENTRIES.iter().enumerate() {
            if !entry.id.contains(query) {
                continue;
            }
            if category != Some(entry.category) {
                category = Some(entry.category);
                self.rows.push(Row::Category(entry.category));
            }
            self.rows.push(Row::Entry(i));
        }
        self.list.set_count(self.rows.len());
    }

    fn left(&self) -> i32 {
        self.gui.0 / 2 - ROW_WIDTH / 2
    }

    fn button_rect(&self, row_top: i32, slot: usize) -> [i32; 4] {
        let x = self.left() + ROW_WIDTH - 6 - 3 * BUTTON_WIDTH + slot as i32 * BUTTON_WIDTH;
        [x, row_top, BUTTON_WIDTH, BUTTON_HEIGHT]
    }

    fn bar_x(&self) -> i32 {
        self.gui.0 / 2 + ROW_WIDTH / 2 + 4
    }

    fn refresh_profiles(&mut self) {
        let current = self.entries.profile();
        for (i, profile) in DebugProfile::ALL.into_iter().enumerate() {
            if let Some(b) = self.widgets.button_mut(PROFILE_BASE + i as u32) {
                b.active = current != Some(profile);
            }
        }
    }

    fn click(&mut self, x: f32, y: f32, out: &mut Vec<ScreenAction>) -> bool {
        if !contains(self.list.rect(), x, y) {
            return false;
        }
        let Some(index) = self.list.item_at(x, y) else {
            return true;
        };
        let Row::Entry(entry) = self.rows[index] else {
            return true;
        };
        let top = self.list.item_top(index);
        for (slot, (status, _, _)) in STATUSES.into_iter().enumerate() {
            let id = ENTRIES[entry].id;
            if contains(self.button_rect(top, slot), x, y) && self.entries.status(id) != status {
                self.entries.set_status(id, status);
                self.refresh_profiles();
                out.push(ScreenAction::ClickSound);
                out.push(ScreenAction::SetDebugEntries(Box::new(
                    self.entries.clone(),
                )));
                break;
            }
        }
        true
    }
}

fn wrapped(cx: &mut RenderCx<'_, '_>, text: &str, limit: i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let next = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && cx.text_width(&next) > limit {
            lines.push(std::mem::replace(&mut line, word.to_owned()));
        } else {
            line = next;
        }
    }
    lines.push(line);
    lines
}

impl Screen for DebugOptionsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        let keep = self.widgets.edit(SEARCH).map(|e| e.value().to_owned());
        self.widgets.clear();
        let mut search = EditBox::new(SEARCH, [width / 2 + 40, 6, ROW_WIDTH / 3, 20]);
        search.set_value(&keep.unwrap_or_default());
        search.take_changed();
        self.widgets.add(search);
        let start = width / 2 - 154;
        for (i, profile) in DebugProfile::ALL.into_iter().enumerate() {
            self.widgets.add(Button::new(
                PROFILE_BASE + i as u32,
                [start + i as i32 * 124, height - 27, 120, 20],
                format!("lang:{}", profile.lang_key()),
            ));
        }
        self.widgets.add(Button::new(
            DONE,
            [start + 248, height - 27, 60, 20],
            "lang:gui.done",
        ));
        self.list
            .set_rect([0, HEADER, width, (height - HEADER - FOOTER).max(0)]);
        let query = self.widgets.edit(SEARCH).map(|e| e.value().to_owned());
        self.search(&query.unwrap_or_default());
        self.refresh_profiles();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_dim_background(cx);
        let center = self.gui.0 / 2;
        let title = cx.lang("debug.options.title");
        let title_width = cx.text_width(&title);
        let group = ROW_WIDTH / 3 + 8 + title_width + 8 + ROW_WIDTH / 3;
        let start = center - group / 2;
        cx.text(&title, start + ROW_WIDTH / 3 + 8, 12, WHITE, true);
        if let Some(e) = self.widgets.edit_mut(SEARCH) {
            e.rect = [start + group - ROW_WIDTH / 3, 6, ROW_WIDTH / 3, 20];
        }
        let hint = cx.lang("debug.options.search");
        if let Some(e) = self.widgets.edit_mut(SEARCH) {
            e.hint = Some(hint);
        }
        let warning = cx.lang("debug.options.warning");
        let mut y = 32;
        for line in wrapped(cx, &warning, ROW_WIDTH) {
            cx.centered_text(&line, center, y, WARNING, true);
            y += 10;
        }
        let [lx, ly, lw, lh] = self.list.rect();
        cx.frame.push_scissor(lx, ly, lx + lw, ly + lh);
        let left = self.left();
        let mut tooltip = None;
        for index in self.list.visible_range() {
            let top = self.list.item_top(index);
            match self.rows[index] {
                Row::Category(category) => {
                    let label = format!("§l§n{}", cx.lang(category.lang_key()));
                    cx.centered_text(&label, center, top + 5, WHITE, true);
                }
                Row::Entry(entry) => {
                    let def = &ENTRIES[entry];
                    let allowed = def.reduced_ok || !self.reduced;
                    let (name, color) = if allowed {
                        (def.id.to_owned(), WHITE)
                    } else {
                        (format!("§o{}", def.id), 0xFF80_8080)
                    };
                    cx.text(&name, left, top + 5, color, true);
                    let current = self.entries.status(def.id);
                    for (slot, (status, key, on_color)) in STATUSES.into_iter().enumerate() {
                        let rect = self.button_rect(top, slot);
                        let selected = status == current;
                        let hot = !selected && contains(rect, self.mouse.0, self.mouse.1);
                        let sprite = match (selected, hot) {
                            (true, _) => "widget/button_disabled",
                            (false, true) => "widget/button_highlighted",
                            (false, false) => "widget/button",
                        };
                        cx.sprite(sprite, rect[0], rect[1], rect[2], rect[3]);
                        let label = cx.lang(key);
                        let color = if selected { on_color } else { GRAY };
                        cx.centered_text(&label, rect[0] + rect[2] / 2, rect[1] + 4, color, true);
                    }
                    let buttons_x = self.button_rect(top, 0)[0];
                    let over_name = contains(
                        [left, top, buttons_x - left, ROW],
                        self.mouse.0,
                        self.mouse.1,
                    ) && contains(self.list.rect(), self.mouse.0, self.mouse.1);
                    if !allowed && over_name {
                        tooltip = Some(cx.lang("debug.options.notAllowed.tooltip"));
                    }
                }
            }
        }
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, self.bar_x());
        self.widgets.render(cx);
        if let Some(text) = tooltip {
            cx.set_tooltip(TooltipLines::text(text));
        }
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        match *input {
            UiInput::Key {
                key: Key::Escape, ..
            } => {
                out.push(ScreenAction::Pop);
                return true;
            }
            UiInput::MouseMove { x, y } => self.mouse = (x, y),
            UiInput::MousePress {
                x,
                y,
                button: MouseButton::Left,
                ..
            } => {
                let mut events = Vec::new();
                if self.widgets.input(env, input, &mut events) {
                    self.events(events, out);
                    return true;
                }
                if self.list.input(input, self.bar_x()) {
                    return true;
                }
                return self.click(x, y, out);
            }
            _ => {}
        }
        if self.list.input(input, self.bar_x()) {
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        self.events(events, out);
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }

    fn is_debug_options(&self) -> bool {
        true
    }
}

impl DebugOptionsScreen {
    fn events(&mut self, events: Vec<WidgetEvent>, out: &mut Vec<ScreenAction>) {
        for WidgetEvent { id, kind } in events {
            match (id, kind) {
                (SEARCH, WidgetEventKind::Edited) => {
                    let query = self
                        .widgets
                        .edit(SEARCH)
                        .map(|e| e.value().to_owned())
                        .unwrap_or_default();
                    self.search(&query);
                }
                (DONE, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                (id, WidgetEventKind::Pressed) if (PROFILE_BASE..DONE).contains(&id) => {
                    if let Some(profile) = DebugProfile::ALL.get((id - PROFILE_BASE) as usize) {
                        self.entries.load_profile(*profile);
                        self.refresh_profiles();
                        out.push(ScreenAction::ClickSound);
                        out.push(ScreenAction::SetDebugEntries(Box::new(
                            self.entries.clone(),
                        )));
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_filters_and_groups_by_category() {
        let mut screen = DebugOptionsScreen::new(DebugEntries::default(), false);
        let headers = |s: &DebugOptionsScreen| {
            s.rows
                .iter()
                .filter(|r| matches!(r, Row::Category(_)))
                .count()
        };
        assert_eq!(screen.rows.len(), ENTRIES.len() + 2);
        assert_eq!(headers(&screen), 2);
        screen.search("hitbox");
        assert_eq!(screen.rows.len(), 2);
        screen.search("zzz");
        assert!(screen.rows.is_empty());
    }
}
