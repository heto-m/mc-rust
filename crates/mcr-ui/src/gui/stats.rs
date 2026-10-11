use std::cmp::Ordering;

use mcr_protocol::types::item::ItemStack;

use super::env::{Env, RenderCx};
use super::input::{Key, MouseButton, UiInput, contains};
use super::list::ScrollList;
use super::menus::tile_texture;
use super::screen::{Screen, ScreenAction};
use super::widgets::{Button, WidgetEvent, WidgetEventKind, WidgetSet};

const DONE: u32 = 1;
const WHITE: u32 = 0xFFFF_FFFF;
const SHADED: u32 = 0xFFBA_BABA;
const DIM: u32 = 0xFF7F_7F7F;
const TAB_HEIGHT: i32 = 24;
const FOOTER: i32 = 33;
const ROW_WIDTH: i32 = 280;
const MAX_ROWS: usize = 4096;

const MINED: i32 = 0;
const CRAFTED: i32 = 1;
const USED: i32 = 2;
const BROKEN: i32 = 3;
const PICKED_UP: i32 = 4;
const DROPPED: i32 = 5;
const KILLED: i32 = 6;
const KILLED_BY: i32 = 7;
const CUSTOM: i32 = 8;

const COLUMNS: [(i32, &str, &str); 6] = [
    (MINED, "statistics/block_mined", "mined"),
    (BROKEN, "statistics/item_broken", "broken"),
    (CRAFTED, "statistics/item_crafted", "crafted"),
    (USED, "statistics/item_used", "used"),
    (PICKED_UP, "statistics/item_picked_up", "picked_up"),
    (DROPPED, "statistics/item_dropped", "dropped"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Plain,
    Tenths,
    Distance,
    Time,
}

const TIME_STATS: [&str; 5] = [
    "play_time",
    "total_world_time",
    "time_since_death",
    "time_since_rest",
    "sneak_time",
];

fn format_of(name: &str) -> Format {
    if TIME_STATS.contains(&name) {
        Format::Time
    } else if name.ends_with("_one_cm") {
        Format::Distance
    } else if name.starts_with("damage_") {
        Format::Tenths
    } else {
        Format::Plain
    }
}

/// Java's `NumberFormat.getIntegerInstance(Locale.US)`.
fn group_digits(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if value < 0 {
        out.insert(0, '-');
    }
    out
}

fn two_decimals(value: f64) -> String {
    format!("{value:.2}")
}

fn format_stat(format: Format, value: i32) -> String {
    match format {
        Format::Plain => group_digits(i64::from(value)),
        Format::Tenths => two_decimals(f64::from(value) * 0.1),
        Format::Distance => {
            let meters = f64::from(value) / 100.0;
            let kilometers = meters / 1000.0;
            if kilometers > 0.5 {
                format!("{} km", two_decimals(kilometers))
            } else if meters > 0.5 {
                format!("{} m", two_decimals(meters))
            } else {
                format!("{value} cm")
            }
        }
        Format::Time => {
            let seconds = f64::from(value) / 20.0;
            let minutes = seconds / 60.0;
            let hours = minutes / 60.0;
            let days = hours / 24.0;
            let years = days / 365.0;
            if years > 0.5 {
                format!("{} y", two_decimals(years))
            } else if days > 0.5 {
                format!("{} d", two_decimals(days))
            } else if hours > 0.5 {
                format!("{} h", two_decimals(hours))
            } else if minutes > 0.5 {
                format!("{} min", two_decimals(minutes))
            } else {
                format!("{seconds:?} s")
            }
        }
    }
}

#[derive(Clone, Debug)]
struct GeneralRow {
    key: String,
    value: i32,
    format: Format,
}

#[derive(Clone, Debug)]
struct ItemRow {
    item: i32,
    stack: ItemStack,
    mined: Option<i32>,
    values: [i32; 5],
}

#[derive(Clone, Debug)]
struct MobRow {
    name: String,
    kills: i32,
    killed_by: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    General,
    Items,
    Mobs,
}

#[derive(Debug)]
pub struct StatsScreen {
    gui: (i32, i32),
    widgets: WidgetSet,
    page: Page,
    revision: Option<u64>,
    requested: bool,
    general: Vec<GeneralRow>,
    items: Vec<ItemRow>,
    mobs: Vec<MobRow>,
    sort_column: Option<usize>,
    sort_order: i32,
    list: ScrollList,
    mouse: (f32, f32),
}

impl Default for StatsScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsScreen {
    pub fn new() -> Self {
        Self {
            gui: (0, 0),
            widgets: WidgetSet::default(),
            page: Page::General,
            revision: None,
            requested: false,
            general: Vec::new(),
            items: Vec::new(),
            mobs: Vec::new(),
            sort_column: None,
            sort_order: 0,
            list: ScrollList::new([0, TAB_HEIGHT, 0, 0], 14),
            mouse: (0.0, 0.0),
        }
    }

    fn loading(&self) -> bool {
        self.revision.is_none_or(|r| r == 0)
    }

    fn list_height(&self) -> i32 {
        self.gui.1 - FOOTER - TAB_HEIGHT
    }

    fn row_height(page: Page) -> i32 {
        match page {
            Page::General => 14,
            Page::Items => 22,
            Page::Mobs => 36,
        }
    }

    fn layout_list(&mut self) {
        self.list = ScrollList::new(
            [0, TAB_HEIGHT, self.gui.0, self.list_height().max(0)],
            Self::row_height(self.page),
        );
        let count = match self.page {
            Page::General => self.general.len(),
            Page::Items => self.items.len() + usize::from(!self.items.is_empty()),
            Page::Mobs => self.mobs.len(),
        };
        self.list.set_count(count);
    }

    fn page_enabled(&self, page: Page) -> bool {
        match page {
            Page::General => true,
            Page::Items => !self.items.is_empty(),
            Page::Mobs => !self.mobs.is_empty(),
        }
    }

    fn rebuild(&mut self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        let data = mcr_data::v26_3();
        let mut general = Vec::new();
        if let Some(registry) = data.registry("minecraft:custom_stat") {
            for (id, name) in registry.iter().take(MAX_ROWS) {
                let short = name.strip_prefix("minecraft:").unwrap_or(name);
                general.push(GeneralRow {
                    key: format!("stat.minecraft.{short}"),
                    value: view.stat(CUSTOM, id as i32),
                    format: format_of(short),
                });
            }
        }
        let mut keyed: Vec<(String, GeneralRow)> = general
            .into_iter()
            .map(|row| (cx.lang(&row.key), row))
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        self.general = keyed.into_iter().map(|(_, row)| row).collect();

        let mut items = Vec::new();
        let items_registry = data.items();
        for (id, name) in items_registry.iter().take(MAX_ROWS) {
            if name == "minecraft:air" {
                continue;
            }
            let item = id as i32;
            let block = data.block_by_name(name).map(|b| b as i32);
            let mined = block.map(|b| view.stat(MINED, b));
            let values = [BROKEN, CRAFTED, USED, PICKED_UP, DROPPED].map(|t| view.stat(t, item));
            if values.iter().any(|&v| v > 0) || mined.is_some_and(|v| v > 0) {
                items.push(ItemRow {
                    item,
                    stack: ItemStack::new(item, 1),
                    mined,
                    values,
                });
            }
        }
        self.items = items;

        let mut mobs = Vec::new();
        for (id, name) in data.entity_types().iter().take(MAX_ROWS) {
            let (kills, killed_by) = (
                view.stat(KILLED, id as i32),
                view.stat(KILLED_BY, id as i32),
            );
            if kills > 0 || killed_by > 0 {
                let short = name.strip_prefix("minecraft:").unwrap_or(name);
                mobs.push(MobRow {
                    name: cx.lang(&format!("entity.minecraft.{short}")),
                    kills,
                    killed_by,
                });
            }
        }
        self.mobs = mobs;
        if !self.page_enabled(self.page) {
            self.page = Page::General;
        }
        self.apply_sort();
        self.layout_list();
    }

    fn column_value(row: &ItemRow, column: usize) -> Option<i32> {
        if column == 0 {
            row.mined
        } else {
            Some(row.values[column - 1])
        }
    }

    fn apply_sort(&mut self) {
        let (column, order) = (self.sort_column, self.sort_order);
        self.items.sort_by(|a, b| {
            let (ka, kb) = match column {
                None => (0, 0),
                Some(c) => (
                    Self::column_value(a, c).unwrap_or(-1),
                    Self::column_value(b, c).unwrap_or(-1),
                ),
            };
            let ord = if ka == kb {
                a.item.cmp(&b.item)
            } else {
                ka.cmp(&kb)
            };
            if order < 0 {
                ord.reverse()
            } else if order > 0 {
                ord
            } else {
                Ordering::Equal.then(a.item.cmp(&b.item))
            }
        });
    }

    fn sort_by(&mut self, column: usize) {
        if self.sort_column != Some(column) {
            self.sort_column = Some(column);
            self.sort_order = -1;
        } else if self.sort_order == -1 {
            self.sort_order = 1;
        } else {
            self.sort_column = None;
            self.sort_order = 0;
        }
        self.apply_sort();
    }

    fn tab_rects(&self) -> [[i32; 4]; 3] {
        let width = self.gui.0.min(400) - 28;
        let tab = (width / 3) / 2 * 2;
        let x0 = ((self.gui.0 - width) / 2) / 2 * 2;
        std::array::from_fn(|i| [x0 + tab * i as i32, 0, tab, TAB_HEIGHT])
    }

    fn content_left(&self) -> i32 {
        (self.gui.0 - ROW_WIDTH) / 2
    }

    fn column_x(column: usize) -> i32 {
        75 + 40 * column as i32
    }

    fn render_tabs(&mut self, cx: &mut RenderCx<'_, '_>) {
        let pages = [Page::General, Page::Items, Page::Mobs];
        let labels = ["stat.generalButton", "stat.itemsButton", "stat.mobsButton"];
        let rects = self.tab_rects();
        if let Some(id) = cx.assets.texture("header_separator") {
            let first = rects[0][0];
            let last = rects[2][0] + rects[2][2];
            for x in (0..first).step_by(32) {
                cx.frame.sprite(id, x, TAB_HEIGHT - 2, 32.min(first - x), 2);
            }
            for x in (last..self.gui.0).step_by(32) {
                cx.frame.sprite(id, x, TAB_HEIGHT - 2, 32, 2);
            }
        }
        for (i, rect) in rects.iter().enumerate() {
            let selected = self.page == pages[i];
            let enabled = !self.loading() && self.page_enabled(pages[i]);
            let hovered = enabled && contains(*rect, cx.mouse.0, cx.mouse.1);
            let sprite = match (selected, hovered) {
                (true, true) => "widget/tab_selected_highlighted",
                (true, false) => "widget/tab_selected",
                (false, true) => "widget/tab_highlighted",
                (false, false) => "widget/tab",
            };
            cx.sprite(sprite, rect[0], rect[1], rect[2], rect[3]);
            if selected {
                tile_texture(
                    cx,
                    "menu_background",
                    [rect[0] + 2, rect[1] + 2, rect[2] - 4, rect[3] - 2],
                );
            }
            let label = cx.lang(labels[i]);
            let w = cx.text_width(&label);
            let top = if selected { 0 } else { 3 };
            let color = if enabled || i == 0 {
                WHITE
            } else {
                0xFFA0_A0A0
            };
            cx.text(
                &label,
                rect[0] + (rect[2] - w) / 2,
                top + (TAB_HEIGHT - top - 8) / 2,
                color,
                true,
            );
            if selected {
                let width = w.min(rect[2] - 4);
                let left = rect[0] + (rect[2] - width) / 2;
                cx.frame.fill_rect(
                    left,
                    rect[1] + rect[3] - 2,
                    left + width,
                    rect[1] + rect[3] - 1,
                    color,
                );
            }
        }
    }

    fn render_general(&mut self, cx: &mut RenderCx<'_, '_>) {
        let left = self.content_left();
        for i in self.list.visible_range() {
            let row = &self.general[i];
            let y = self.list.item_top(i) + 3;
            let color = if i % 2 == 0 { WHITE } else { SHADED };
            let name = cx.lang(&row.key);
            cx.text(&name, left + 2, y, color, true);
            let value = format_stat(row.format, row.value);
            let w = cx.text_width(&value);
            cx.text(&value, left + ROW_WIDTH - w - 4, y, color, true);
        }
    }

    fn render_items(&mut self, cx: &mut RenderCx<'_, '_>) {
        let left = self.content_left();
        let mut hovered_stack: Option<ItemStack> = None;
        for i in self.list.visible_range() {
            let y = self.list.item_top(i);
            if i == 0 {
                for (c, (_, sprite, _)) in COLUMNS.iter().enumerate() {
                    let x = left + Self::column_x(c) - 18;
                    let over = contains([x, y + 1, 18, 18], cx.mouse.0, cx.mouse.1);
                    cx.sprite(
                        if over {
                            "container/slot"
                        } else {
                            "statistics/header"
                        },
                        x,
                        y + 1,
                        18,
                        18,
                    );
                    cx.sprite(sprite, x, y + 1, 18, 18);
                }
                if let Some(column) = self.sort_column {
                    let sprite = if self.sort_order == 1 {
                        "statistics/sort_up"
                    } else {
                        "statistics/sort_down"
                    };
                    cx.sprite(sprite, left + Self::column_x(column) - 36, y + 1, 18, 18);
                }
                continue;
            }
            let row = &self.items[i - 1];
            cx.sprite("container/slot", left, y + 1, 18, 18);
            cx.fake_item(&row.stack, left + 1, y + 2);
            if contains([left, y + 1, 18, 18], cx.mouse.0, cx.mouse.1) {
                hovered_stack = Some(row.stack.clone());
            }
            let color = if i % 2 == 0 { WHITE } else { SHADED };
            for column in 0..COLUMNS.len() {
                let text = match Self::column_value(row, column) {
                    Some(v) => group_digits(i64::from(v)),
                    None => cx.lang("stats.none"),
                };
                let w = cx.text_width(&text);
                cx.text(&text, left + Self::column_x(column) - w, y + 7, color, true);
            }
        }
        if let Some(stack) = hovered_stack {
            let lines = cx.item_tooltip(&stack);
            cx.set_tooltip(lines);
        }
    }

    fn render_mobs(&mut self, cx: &mut RenderCx<'_, '_>) {
        let left = self.content_left();
        for i in self.list.visible_range() {
            let row = &self.mobs[i];
            let y = self.list.item_top(i);
            cx.text(&row.name, left + 2, y + 1, WHITE, true);
            let kills = if row.kills == 0 {
                cx.lang_fmt("stat_type.minecraft.killed.none", &[&row.name])
            } else {
                cx.lang_fmt(
                    "stat_type.minecraft.killed",
                    &[&row.kills.to_string(), &row.name],
                )
            };
            let killed_by = if row.killed_by == 0 {
                cx.lang_fmt("stat_type.minecraft.killed_by.none", &[&row.name])
            } else {
                cx.lang_fmt(
                    "stat_type.minecraft.killed_by",
                    &[&row.name, &row.killed_by.to_string()],
                )
            };
            let on = |has: bool| if has { SHADED } else { DIM };
            cx.text(&kills, left + 12, y + 10, on(row.kills != 0), true);
            cx.text(&killed_by, left + 12, y + 19, on(row.killed_by != 0), true);
        }
    }
}

impl Screen for StatsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        self.widgets.add(Button::new(
            DONE,
            [width / 2 - 100, height - 27, 200, 20],
            "lang:gui.done",
        ));
        self.layout_list();
    }

    fn pauses_game(&self) -> bool {
        true
    }

    fn tick(&mut self, _env: &mut Env<'_>, out: &mut Vec<ScreenAction>) {
        if !self.requested {
            self.requested = true;
            out.push(ScreenAction::RequestStats);
        }
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.mouse = cx.mouse;
        let revision = cx.view.stats_revision();
        if self.revision != Some(revision) {
            self.revision = Some(revision);
            if revision != 0 {
                self.rebuild(cx);
            }
        }
        let (gw, gh) = cx.gui_size();
        tile_texture(cx, "menu_background", [0, 0, gw, gh]);
        tile_texture(
            cx,
            "menu_list_background",
            [0, TAB_HEIGHT, gw, self.list_height().max(0)],
        );
        if let Some(id) = cx.assets.texture("footer_separator") {
            for x in (0..gw).step_by(32) {
                cx.frame.sprite(id, x, gh - FOOTER, 32, 2);
            }
        }
        if self.loading() {
            let text = cx.lang("multiplayer.downloadingStats");
            cx.centered_text(&text, gw / 2, TAB_HEIGHT + 20, WHITE, true);
        } else {
            let [lx, ly, lw, lh] = self.list.rect();
            cx.frame.push_scissor(lx, ly, lx + lw, ly + lh);
            match self.page {
                Page::General => self.render_general(cx),
                Page::Items => self.render_items(cx),
                Page::Mobs => self.render_mobs(cx),
            }
            cx.frame.pop_scissor();
            self.list
                .render_scrollbar(cx, self.content_left() + ROW_WIDTH + 6);
        }
        self.render_tabs(cx);
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if let UiInput::Key {
            key: Key::Escape, ..
        } = *input
        {
            out.push(ScreenAction::Pop);
            return true;
        }
        if let UiInput::MousePress {
            x,
            y,
            button: MouseButton::Left,
            ..
        } = *input
        {
            let mut events = Vec::new();
            if self.widgets.input(env, input, &mut events) {
                for WidgetEvent { id, kind } in events {
                    if id == DONE && kind == WidgetEventKind::Pressed {
                        out.push(ScreenAction::ClickSound);
                        out.push(ScreenAction::Pop);
                    }
                }
                return true;
            }
            if !self.loading() {
                let pages = [Page::General, Page::Items, Page::Mobs];
                for (i, rect) in self.tab_rects().iter().enumerate() {
                    if contains(*rect, x, y) && self.page_enabled(pages[i]) {
                        if self.page != pages[i] {
                            self.page = pages[i];
                            self.layout_list();
                        }
                        return true;
                    }
                }
                if self.page == Page::Items
                    && let Some(0) = self.list.item_at(x, y)
                {
                    let left = self.content_left();
                    let top = self.list.item_top(0);
                    for column in 0..COLUMNS.len() {
                        if contains([left + Self::column_x(column) - 18, top + 1, 18, 18], x, y) {
                            out.push(ScreenAction::ClickSound);
                            self.sort_by(column);
                            return true;
                        }
                    }
                }
            }
        }
        if self.list.input(input, self.content_left() + ROW_WIDTH + 6) {
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            if id == DONE && kind == WidgetEventKind::Pressed {
                out.push(ScreenAction::ClickSound);
                out.push(ScreenAction::Pop);
            }
        }
        used
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_group_like_java() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(1234567), "1,234,567");
        assert_eq!(group_digits(-1000), "-1,000");
    }

    #[test]
    fn distances_and_times_pick_the_vanilla_unit() {
        assert_eq!(format_stat(Format::Distance, 40), "40 cm");
        assert_eq!(format_stat(Format::Distance, 250), "2.50 m");
        assert_eq!(format_stat(Format::Distance, 300_000), "3.00 km");
        assert_eq!(format_stat(Format::Time, 100), "5.0 s");
        assert_eq!(format_stat(Format::Time, 20 * 90), "1.50 min");
        assert_eq!(format_stat(Format::Tenths, 25), "2.50");
        assert_eq!(format_of("walk_one_cm"), Format::Distance);
        assert_eq!(format_of("play_time"), Format::Time);
        assert_eq!(format_of("damage_dealt"), Format::Tenths);
        assert_eq!(format_of("jump"), Format::Plain);
    }

    fn row(item: i32, mined: Option<i32>, crafted: i32) -> ItemRow {
        ItemRow {
            item,
            stack: ItemStack::new(item, 1),
            mined,
            values: [0, crafted, 0, 0, 0],
        }
    }

    #[test]
    fn sorting_cycles_descending_ascending_off_with_item_id_ties() {
        let mut screen = StatsScreen::new();
        screen.items = vec![row(1, Some(3), 9), row(2, None, 9), row(3, Some(8), 1)];
        screen.sort_by(0);
        let order: Vec<i32> = screen.items.iter().map(|r| r.item).collect();
        assert_eq!(order, [3, 1, 2]);
        screen.sort_by(0);
        let order: Vec<i32> = screen.items.iter().map(|r| r.item).collect();
        assert_eq!(order, [2, 1, 3]);
        screen.sort_by(0);
        assert_eq!(screen.sort_column, None);
        screen.sort_by(2);
        let order: Vec<i32> = screen.items.iter().map(|r| r.item).collect();
        assert_eq!(order, [2, 1, 3]);
    }

    #[test]
    fn hostile_values_never_panic() {
        for v in [i32::MIN, -1, 0, i32::MAX] {
            for f in [
                Format::Plain,
                Format::Tenths,
                Format::Distance,
                Format::Time,
            ] {
                let _ = format_stat(f, v);
            }
        }
    }
}
