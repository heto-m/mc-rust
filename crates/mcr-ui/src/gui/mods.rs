use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::env::{Env, RenderCx};
use super::input::{Key, MouseButton, UiInput, contains};
use super::list::ScrollList;
use super::menus::{ConfirmScreen, WHITE, draw_menu_background};
use super::screen::{Screen, ScreenAction};
use super::tooltip::TooltipLines;
use super::widgets::{Button, Slider, WidgetEvent, WidgetEventKind, WidgetSet};
use crate::SpriteId;

const ROW_HEIGHT: i32 = 36;
const ROW_WIDTH: i32 = 280;
const HEADER: i32 = 40;
const FOOTER: i32 = 56;
const ICON: i32 = 32;
const TOGGLE_W: i32 = 30;
const HEAP_STEP: u32 = 128;
const MAX_ROWS: usize = 1024;
const GRAY: u32 = 0xFFA0_A0A0;

const HEAP: u32 = 1;
const FOLDER: u32 = 2;
const DONE: u32 = 3;

/// A decoded RGBA mod icon; `id` identifies the image so the screen uploads it once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModIcon {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModTone {
    Good,
    Neutral,
    Warn,
    Bad,
}

impl ModTone {
    fn color(self) -> u32 {
        match self {
            Self::Good => 0xFF55_FF55,
            Self::Neutral => GRAY,
            Self::Warn => 0xFFFF_FF55,
            Self::Bad => 0xFFFF_5555,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub badge: String,
    pub tone: ModTone,
    /// Why the mod is blocked, excluded or flagged; empty when there is nothing to say.
    pub reason: String,
    pub enabled: bool,
    pub toggleable: bool,
    pub icon: Option<Rc<ModIcon>>,
}

/// What the Mods screen needs from the app: the scanned rows and the settings behind them.
pub trait ModsModel {
    /// Rescans and returns the rows.
    fn refresh(&mut self) -> Vec<ModRow>;
    fn set_enabled(&mut self, id: &str, on: bool);
    fn heap_mib(&self) -> u32;
    fn set_heap_mib(&mut self, mib: u32);
    fn heap_range(&self) -> (u32, u32);
    /// The enabled set or the heap differs from what the running Java started with.
    fn restart_required(&self) -> bool;
    /// A line about the last session or the mod mode state, if any.
    fn notice(&self) -> Option<String>;
    fn open_folder(&mut self);
    fn folder_label(&self) -> String;
}

pub type SharedMods = Rc<RefCell<dyn ModsModel>>;

/// Uploaded icon sprites by [`ModIcon::id`]; keep one for the whole session so reopening the screen reuses them.
pub type IconCache = Rc<RefCell<HashMap<u64, Option<SpriteId>>>>;

pub struct ModsScreen {
    model: SharedMods,
    rows: Vec<ModRow>,
    widgets: WidgetSet,
    gui: (i32, i32),
    list: ScrollList,
    mouse: (f32, f32),
    icons: IconCache,
}

impl std::fmt::Debug for ModsScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModsScreen")
            .field("rows", &self.rows.len())
            .finish()
    }
}

impl ModsScreen {
    pub fn new(model: SharedMods) -> Self {
        Self::with_cache(model, IconCache::default())
    }

    pub fn with_cache(model: SharedMods, icons: IconCache) -> Self {
        let mut rows = model.borrow_mut().refresh();
        rows.truncate(MAX_ROWS);
        Self {
            model,
            rows,
            widgets: WidgetSet::default(),
            gui: (0, 0),
            list: ScrollList::new([0, HEADER, 0, 0], ROW_HEIGHT),
            mouse: (0.0, 0.0),
            icons,
        }
    }

    pub fn rows(&self) -> &[ModRow] {
        &self.rows
    }

    fn row_left(&self) -> i32 {
        self.gui.0 / 2 - ROW_WIDTH / 2
    }

    fn bar_x(&self) -> i32 {
        self.gui.0 / 2 + ROW_WIDTH / 2 + 6
    }

    fn toggle_rect(&self, index: usize) -> [i32; 4] {
        let top = self.list.item_top(index);
        [
            self.row_left() + ROW_WIDTH - TOGGLE_W - 4,
            top + (ROW_HEIGHT - 20) / 2 - 1,
            TOGGLE_W,
            20,
        ]
    }

    fn sync_list(&mut self) {
        self.list
            .set_rect([0, HEADER, self.gui.0, (self.gui.1 - HEADER - FOOTER).max(0)]);
        self.list.set_count(self.rows.len());
    }

    fn reload(&mut self) {
        let mut rows = self.model.borrow_mut().refresh();
        rows.truncate(MAX_ROWS);
        self.rows = rows;
        self.sync_list();
    }

    fn heap_label(mib: u32) -> String {
        format!("JVM heap: {mib} MiB")
    }

    fn heap_from_slider(&self, value: f32) -> u32 {
        let (lo, hi) = self.model.borrow().heap_range();
        let mib = lo as f32 + value * (hi - lo) as f32;
        let snapped = ((mib / HEAP_STEP as f32).round() as u32) * HEAP_STEP;
        snapped.clamp(lo, hi)
    }

    fn slider_value(&self) -> f32 {
        let model = self.model.borrow();
        let (lo, hi) = model.heap_range();
        (model.heap_mib().saturating_sub(lo)) as f32 / (hi - lo).max(1) as f32
    }

    fn tooltip(row: &ModRow) -> TooltipLines {
        let mut lines = TooltipLines::text(format!("{} {}", row.name, row.version));
        lines.push_text(format!("{} - {}", row.id, row.badge));
        if !row.reason.is_empty() {
            lines.push_text(format!("§e{}", row.reason));
        }
        if !row.description.is_empty() {
            lines.push_text(format!("§7{}", row.description));
        }
        lines
    }

    fn icon_sprite(&self, cx: &mut RenderCx<'_, '_>, icon: &ModIcon) -> Option<SpriteId> {
        *self.icons.borrow_mut().entry(icon.id).or_insert_with(|| {
            cx.frame
                .ui()
                .sprites_mut()
                .add_raw(icon.width, icon.height, &icon.rgba)
        })
    }

    fn draw_row(&self, cx: &mut RenderCx<'_, '_>, index: usize) {
        let row = &self.rows[index];
        let left = self.row_left();
        let top = self.list.item_top(index);
        let (ix, iy) = (left + 2, top + 2);
        match row.icon.as_deref().and_then(|i| self.icon_sprite(cx, i)) {
            Some(id) => cx.frame.sprite(id, ix, iy, ICON, ICON),
            None => cx
                .frame
                .fill_rect(ix, iy, ix + ICON, iy + ICON, 0xFF3A_3A3A),
        }
        let text_x = ix + ICON + 3;
        let right = left + ROW_WIDTH - TOGGLE_W - 10;
        cx.frame.push_scissor(text_x, top, right, top + ROW_HEIGHT);
        let color = if row.enabled || !row.toggleable {
            WHITE
        } else {
            GRAY
        };
        cx.text(&row.name, text_x, top + 2, color, true);
        let name_w = cx.text_width(&row.name);
        cx.text(&row.version, text_x + name_w + 4, top + 2, GRAY, true);
        cx.text(&row.badge, text_x, top + 12, row.tone.color(), true);
        let badge_w = cx.text_width(&row.badge);
        let detail = if row.reason.is_empty() {
            &row.description
        } else {
            &row.reason
        };
        if !detail.is_empty() {
            cx.text(detail, text_x + badge_w + 6, top + 12, GRAY, true);
        }
        cx.text(&row.id, text_x, top + 23, 0xFF70_7070, false);
        cx.frame.pop_scissor();

        let rect = self.toggle_rect(index);
        let hovered = row.toggleable && contains(rect, self.mouse.0, self.mouse.1);
        let sprite = match (row.toggleable, hovered) {
            (false, _) => "widget/button_disabled",
            (true, true) => "widget/button_highlighted",
            (true, false) => "widget/button",
        };
        cx.sprite(sprite, rect[0], rect[1], rect[2], rect[3]);
        let (label, label_color) = if row.enabled && row.toggleable {
            ("On", 0xFF55_FF55)
        } else if row.toggleable {
            ("Off", GRAY)
        } else {
            ("-", 0xFF70_7070)
        };
        let w = cx.text_width(label);
        cx.text(
            label,
            rect[0] + (rect[2] - w) / 2,
            rect[1] + 6,
            label_color,
            true,
        );
    }

    fn banner(&self, cx: &mut RenderCx<'_, '_>) {
        let mut y = 24;
        let (w, _) = cx.gui_size();
        if let Some(notice) = self.model.borrow().notice() {
            cx.frame.push_scissor(0, y - 1, w, y + 9);
            let tw = cx.text_width(&notice);
            let x = if tw > w - 8 { 4 } else { (w - tw) / 2 };
            cx.text(&notice, x, y, 0xFFFF_AA00, true);
            cx.frame.pop_scissor();
            y += 10;
        }
        if self.model.borrow().restart_required() {
            cx.centered_text(
                "Restart mcr to apply these changes",
                w / 2,
                y,
                0xFFFF_5555,
                true,
            );
        }
    }

    fn handle_toggle(&mut self, index: usize) {
        let row = &self.rows[index];
        if !row.toggleable {
            return;
        }
        let (id, on) = (row.id.clone(), !row.enabled);
        self.model.borrow_mut().set_enabled(&id, on);
        self.reload();
    }

    fn button_events(&mut self, events: Vec<WidgetEvent>, out: &mut Vec<ScreenAction>) {
        for WidgetEvent { id, kind } in events {
            match (id, kind) {
                (HEAP, WidgetEventKind::SliderChanged(value)) => {
                    let mib = self.heap_from_slider(value);
                    self.model.borrow_mut().set_heap_mib(mib);
                    if let Some(s) = self.widgets.slider_mut(HEAP) {
                        s.label = Self::heap_label(mib);
                    }
                }
                (FOLDER, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    self.model.borrow_mut().open_folder();
                }
                (DONE, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                _ => {}
            }
        }
    }
}

impl Screen for ModsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let heap = self.model.borrow().heap_mib();
        let y = height - FOOTER + 6;
        self.widgets.add(Slider::new(
            HEAP,
            [width / 2 - 155, y, 150, 20],
            Self::heap_label(heap),
            self.slider_value(),
        ));
        self.widgets.add(Button::new(
            FOLDER,
            [width / 2 + 5, y, 150, 20],
            "Open mods folder",
        ));
        self.widgets.add(Button::new(
            DONE,
            [width / 2 - 100, y + 24, 200, 20],
            "lang:gui.done",
        ));
        self.sync_list();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.mouse = cx.mouse;
        draw_menu_background(cx);
        let (w, _) = cx.gui_size();
        cx.centered_text("Mods", w / 2, 10, WHITE, true);
        self.banner(cx);
        let list = self.list.rect();
        cx.frame
            .push_scissor(list[0], list[1], list[0] + list[2], list[1] + list[3]);
        cx.frame.fill_rect(
            list[0],
            list[1],
            list[0] + list[2],
            list[1] + list[3],
            0x6000_0000,
        );
        if self.rows.is_empty() {
            let folder = self.model.borrow().folder_label();
            cx.centered_text("No mods found", w / 2, list[1] + 12, GRAY, true);
            cx.centered_text(&folder, w / 2, list[1] + 24, 0xFF70_7070, true);
        }
        for index in self.list.visible_range() {
            self.draw_row(cx, index);
        }
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, self.bar_x());
        self.widgets.render(cx);
        let (mx, my) = cx.mouse;
        if let Some(index) = self.list.item_at(mx, my)
            && let Some(row) = self.rows.get(index)
            && mx >= self.row_left() as f32
            && mx < (self.row_left() + ROW_WIDTH) as f32
            && !contains(self.toggle_rect(index), mx, my)
        {
            cx.set_tooltip(Self::tooltip(row));
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
            UiInput::MouseMove { x, y } => {
                self.mouse = (x, y);
                if self.list.input(input, self.bar_x()) {
                    return true;
                }
            }
            UiInput::MousePress {
                x,
                y,
                button: MouseButton::Left,
                ..
            } => {
                let mut events = Vec::new();
                if self.widgets.input(env, input, &mut events) {
                    self.button_events(events, out);
                    return true;
                }
                if self.list.input(input, self.bar_x()) {
                    return true;
                }
                if let Some(index) = self.list.item_at(x, y)
                    && contains(self.toggle_rect(index), x, y)
                {
                    out.push(ScreenAction::ClickSound);
                    self.handle_toggle(index);
                    return true;
                }
                return false;
            }
            UiInput::MouseRelease { .. } => {
                self.list.input(input, self.bar_x());
            }
            UiInput::Scroll { .. } if self.list.input(input, self.bar_x()) => return true,
            _ => {}
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        self.button_events(events, out);
        used
    }
}

pub type PromptAction = Box<dyn Fn() -> Vec<ScreenAction>>;

/// "Start without mods" and friends, shown before Java would boot again after a crash or hang.
pub fn crash_fallback_screen(
    detail: &str,
    report: Option<&str>,
    start_without: PromptAction,
    try_again: PromptAction,
    open_report: Option<PromptAction>,
) -> ConfirmScreen {
    let mut message = format!("Java stopped working last time. {detail}");
    if let Some(path) = report {
        message.push_str(&format!(" Crash report: {path}"));
    }
    let mut choices: Vec<(String, PromptAction)> = vec![
        ("Start without mods".into(), start_without),
        ("Try again".into(), try_again),
    ];
    if let Some(open) = open_report {
        choices.push(("Open crash report".into(), open));
    }
    choices.push(("Continue".into(), Box::new(|| vec![ScreenAction::Pop])));
    ConfirmScreen::new("Mods crashed the game", &message, choices)
}
