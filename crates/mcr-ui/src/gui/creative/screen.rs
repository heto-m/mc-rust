use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use mcr_protocol::types::component::{DataComponent, Unit, component_type_id};
use mcr_protocol::types::item::ItemStack;

use super::hotbar::{HOTBAR_SAVES, HOTBAR_SIZE, HotbarStore};
use super::tabs::{CreativeTab, TabKind, TabRow, creative_tabs};
use crate::gui::container::ContainerScreen;
use crate::gui::edit_box::EditBox;
use crate::gui::env::{Env, GameView, RenderCx, is_empty};
use crate::gui::input::{Key, Modifiers, MouseButton, UiInput, contains};
use crate::gui::screen::{ClickType, ContainerClick, Screen, ScreenAction};
use crate::gui::tooltip::TooltipLines;
use crate::items::StackView;
use mcr_assets::ItemProps;

const WIDTH: i32 = 195;
const HEIGHT: i32 = 136;
const COLS: usize = 9;
const ROWS: usize = 5;
const GRID: usize = COLS * ROWS;
const GRID_X: i32 = 9;
const GRID_Y: i32 = 18;
const HOTBAR_Y: i32 = 112;
const HOTBAR_FIRST: usize = 36;
const DESTROY: (i32, i32) = (173, 112);
const SCROLL_X: i32 = 175;
const SCROLL_Y: i32 = 18;
const SCROLL_H: i32 = 112;
const TAB_W: i32 = 26;
const TAB_H: i32 = 32;
const LABEL: u32 = 0xFF40_4040;
const OVERLAY_TOP: u32 = 0xC010_1010;
const OVERLAY_BOTTOM: u32 = 0xD010_1010;
const SEARCH_ID: u32 = 1;
const SPRITES: &str = "container/creative_inventory";
const ARMOR_ICONS: [&str; 4] = [
    "container/slot/helmet",
    "container/slot/chestplate",
    "container/slot/leggings",
    "container/slot/boots",
];

static SELECTED: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Grid(usize),
    Player(usize),
    Destroy,
}

fn max_stack(stack: &ItemStack) -> i32 {
    StackView::new(stack).map_or(64, |v| v.max_stack_size())
}

fn with_count(stack: &ItemStack, count: i32) -> ItemStack {
    let mut out = stack.clone();
    out.count = count;
    out
}

fn same_stack(a: &ItemStack, b: &ItemStack) -> bool {
    a.item == b.item && a.components == b.components
}

fn locked(stack: &ItemStack) -> bool {
    component_type_id("creative_slot_lock").is_some_and(|id| stack.components.get(id).is_some())
}

fn tab_x(tab: &CreativeTab) -> i32 {
    let col = i32::from(tab.column);
    if tab.aligned_right {
        WIDTH - 27 * (7 - col) + 1
    } else {
        27 * col
    }
}

fn tab_y(tab: &CreativeTab) -> i32 {
    match tab.row {
        TabRow::Top => -TAB_H,
        TabRow::Bottom => HEIGHT,
    }
}

fn row_count(len: usize) -> i32 {
    len.div_ceil(COLS) as i32 - ROWS as i32
}

fn row_for_scroll(scroll: f32, len: usize) -> usize {
    ((scroll * row_count(len) as f32 + 0.5) as i32).max(0) as usize
}

fn scroll_for_row(row: usize, len: usize) -> f32 {
    let rows = row_count(len);
    if rows <= 0 {
        return 0.0;
    }
    (row as f32 / rows as f32).clamp(0.0, 1.0)
}

fn lines_with(mut stack_lines: TooltipLines, extra: Vec<String>) -> TooltipLines {
    if stack_lines.is_empty() {
        return stack_lines;
    }
    for (n, text) in extra.into_iter().enumerate() {
        stack_lines.insert_text(1 + n, text);
    }
    stack_lines
}

/// Vanilla's `CreativeModeInventoryScreen`.
pub struct CreativeScreen {
    hotbars: Rc<RefCell<HotbarStore>>,
    left: i32,
    top: i32,
    selected: usize,
    items: Vec<ItemStack>,
    scroll: f32,
    scrolling: bool,
    search: EditBox,
    mouse: (f32, f32),
    ignore_text: bool,
    visible_tags: Vec<(String, Vec<i32>)>,
    search_pool: Vec<ItemStack>,
    names: Option<Vec<String>>,
    search_dirty: bool,
    features: Vec<String>,
    operator: bool,
    built: bool,
    last_slot_click: Option<Hit>,
}

impl std::fmt::Debug for CreativeScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreativeScreen")
            .field("selected", &self.selected)
            .field("items", &self.items.len())
            .finish()
    }
}

impl CreativeScreen {
    pub fn new(hotbars: Rc<RefCell<HotbarStore>>) -> Self {
        let mut search = EditBox::new(SEARCH_ID, [0, 0, 80, 9]).with_max_length(50);
        search.bordered = false;
        search.text_color = 0xFFFF_FFFF;
        search.visible = false;
        Self {
            hotbars,
            left: 0,
            top: 0,
            selected: SELECTED
                .load(Ordering::Relaxed)
                .min(creative_tabs().len() - 1),
            items: Vec::new(),
            scroll: 0.0,
            scrolling: false,
            search,
            mouse: (0.0, 0.0),
            ignore_text: false,
            visible_tags: Vec::new(),
            search_pool: Vec::new(),
            names: None,
            search_dirty: false,
            features: Vec::new(),
            operator: false,
            built: false,
            last_slot_click: None,
        }
    }

    fn tab(&self) -> &'static CreativeTab {
        &creative_tabs()[self.selected]
    }

    fn kind(&self) -> TabKind {
        self.tab().kind
    }

    fn displayed(&self, tab: &CreativeTab) -> bool {
        !tab.is_operator() || self.operator
    }

    fn tab_list(&self) -> impl Iterator<Item = (usize, &'static CreativeTab)> + '_ {
        creative_tabs()
            .iter()
            .enumerate()
            .filter(|(_, t)| self.displayed(t))
    }

    pub fn selected_tab(&self) -> &str {
        &self.tab().key
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn scroll_offset(&self) -> f32 {
        self.scroll
    }

    /// The saved hotbar a placeholder in the grid stands for: its row in the hotbars tab.
    fn hotbar_row(&self, hit: Hit) -> usize {
        match hit {
            Hit::Grid(i) => (i / COLS + row_for_scroll(self.scroll, self.items.len())).min(8),
            _ => 0,
        }
    }

    pub fn grid_item(&self, index: usize) -> Option<&ItemStack> {
        if self.kind() == TabKind::Inventory || index >= GRID {
            return None;
        }
        self.items
            .get(index + row_for_scroll(self.scroll, self.items.len()) * COLS)
            .filter(|s| !s.is_empty())
    }

    fn can_scroll(&self) -> bool {
        self.tab().can_scroll && self.items.len() > GRID
    }

    fn scroll_to(&mut self, value: f32) {
        self.scroll = if self.can_scroll() {
            value.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    pub fn select_tab_by_key(&mut self, key: &str) {
        if let Some(i) = creative_tabs().iter().position(|t| t.key == key) {
            self.select_tab(i);
        }
    }

    fn select_tab(&mut self, index: usize) {
        let changed = index != self.selected;
        self.selected = index;
        SELECTED.store(index, Ordering::Relaxed);
        self.visible_tags.clear();
        let searching = self.kind() == TabKind::Search;
        self.search.visible = searching;
        if changed || !searching {
            self.search.set_value("");
        }
        self.scroll = 0.0;
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let tab = self.tab();
        let enabled = &self.features;
        self.search_pool = self.build_search_pool();
        self.items = match tab.kind {
            TabKind::Category if self.displayed(tab) => tab
                .parent
                .iter()
                .filter(|e| e.enabled(enabled))
                .map(|e| e.stack.clone())
                .collect(),
            TabKind::Hotbar => self.hotbar_items(),
            TabKind::Search => {
                self.search_dirty = true;
                self.search_pool.clone()
            }
            _ => Vec::new(),
        };
        self.scroll = 0.0;
    }

    fn build_search_pool(&self) -> Vec<ItemStack> {
        let mut pool: Vec<ItemStack> = Vec::new();
        for (_, tab) in self.tab_list() {
            if tab.kind != TabKind::Category {
                continue;
            }
            for entry in tab.search.iter().filter(|e| e.enabled(&self.features)) {
                if !pool.iter().any(|s| same_stack(s, &entry.stack)) {
                    pool.push(entry.stack.clone());
                }
            }
        }
        pool
    }

    fn hotbar_items(&self) -> Vec<ItemStack> {
        let store = self.hotbars.borrow();
        let mut items = Vec::with_capacity(HOTBAR_SAVES * HOTBAR_SIZE);
        for index in 0..HOTBAR_SAVES {
            match store.get(index) {
                Some(hotbar) => items.extend(hotbar.iter().cloned()),
                None => {
                    for i in 0..HOTBAR_SIZE {
                        items.push(if i == index {
                            placeholder()
                        } else {
                            ItemStack::default()
                        });
                    }
                }
            }
        }
        items
    }

    /// Filters the search tab by `query` using `names` per pool item; `#` searches tags.
    pub fn apply_search(&mut self, view: &dyn GameView) {
        self.search_dirty = false;
        self.visible_tags.clear();
        let query = self.search.value().to_lowercase();
        if query.is_empty() {
            self.items = self.search_pool.clone();
        } else if let Some(tag_query) = query.strip_prefix('#') {
            let tags = view.item_tags_matching(tag_query);
            self.items = self
                .search_pool
                .iter()
                .filter(|s| tags.iter().any(|(_, ids)| ids.contains(&s.item)))
                .cloned()
                .collect();
            self.visible_tags = tags;
        } else if let Some(names) = &self.names {
            self.items = self
                .search_pool
                .iter()
                .zip(names)
                .filter(|(_, name)| name.contains(&query))
                .map(|(s, _)| s.clone())
                .collect();
        }
        self.scroll = 0.0;
    }

    fn compute_names(&mut self, cx: &mut RenderCx<'_, '_>) {
        let pool = std::mem::take(&mut self.search_pool);
        let names = pool
            .iter()
            .map(|stack| {
                let lines = cx.item_tooltip(stack);
                let text = lines.plain_lines(cx.frame.ui().lang()).join("\n");
                let id = StackView::new(stack).map_or(String::new(), |v| v.name().to_owned());
                format!("{text}\n{id}").to_lowercase()
            })
            .collect();
        self.search_pool = pool;
        self.names = Some(names);
    }

    fn sync(&mut self, view: &dyn GameView) -> bool {
        let operator = view.operator_tab();
        let features = view.enabled_features();
        if self.built && self.features == features && self.operator == operator {
            return false;
        }
        self.built = true;
        self.features = features.to_vec();
        self.operator = operator;
        self.names = None;
        let row = row_for_scroll(self.scroll, self.items.len());
        if !self.displayed(self.tab()) {
            self.selected = 0;
            SELECTED.store(0, Ordering::Relaxed);
            self.search.visible = false;
        }
        self.rebuild();
        self.scroll_to(scroll_for_row(row, self.items.len()));
        true
    }

    fn player_slots(&self) -> Vec<(usize, i32, i32)> {
        if self.kind() != TabKind::Inventory {
            return (0..9)
                .map(|i| (HOTBAR_FIRST + i, GRID_X + i as i32 * 18, HOTBAR_Y))
                .collect();
        }
        let mut slots = Vec::new();
        for i in 5..9 {
            let pos = (i - 5) as i32;
            slots.push((i, 54 + pos / 2 * 54, 6 + pos % 2 * 27));
        }
        slots.push((45, 35, 20));
        for i in 9..45 {
            let pos = (i - 9) as i32;
            let (col, row) = (pos % 9, pos / 9);
            let y = if i >= 36 { HOTBAR_Y } else { 54 + row * 18 };
            slots.push((i, 9 + col * 18, y));
        }
        slots
    }

    fn hit_at(&self, x: f32, y: f32) -> Option<Hit> {
        let (xm, ym) = (x - self.left as f32, y - self.top as f32);
        let on = |sx: i32, sy: i32| {
            xm >= (sx - 1) as f32
                && xm < (sx + 17) as f32
                && ym >= (sy - 1) as f32
                && ym < (sy + 17) as f32
        };
        if self.kind() == TabKind::Inventory {
            if on(DESTROY.0, DESTROY.1) {
                return Some(Hit::Destroy);
            }
        } else {
            for i in 0..GRID {
                if on(
                    GRID_X + (i % COLS) as i32 * 18,
                    GRID_Y + (i / COLS) as i32 * 18,
                ) {
                    return Some(Hit::Grid(i));
                }
            }
        }
        self.player_slots()
            .into_iter()
            .find(|&(_, sx, sy)| on(sx, sy))
            .map(|(slot, ..)| Hit::Player(slot))
    }

    fn outside(&self, x: f32, y: f32) -> bool {
        x < self.left as f32
            || y < self.top as f32
            || x >= (self.left + WIDTH) as f32
            || y >= (self.top + HEIGHT) as f32
    }

    fn tab_at(&self, x: f32, y: f32) -> Option<usize> {
        let (xm, ym) = (x - self.left as f32, y - self.top as f32);
        self.tab_list().find_map(|(i, t)| {
            let (tx, ty) = (tab_x(t) as f32, tab_y(t) as f32);
            (xm >= tx && xm <= tx + TAB_W as f32 && ym >= ty && ym <= ty + TAB_H as f32)
                .then_some(i)
        })
    }

    fn inside_scrollbar(&self, x: f32, y: f32) -> bool {
        self.kind() != TabKind::Inventory
            && self.tab().can_scroll
            && contains(
                [self.left + SCROLL_X, self.top + SCROLL_Y, 14, SCROLL_H],
                x,
                y,
            )
    }

    fn hovered_stack(&self, view: &dyn GameView) -> Option<(Hit, ItemStack)> {
        let hit = self.hit_at(self.mouse.0, self.mouse.1)?;
        let stack = match hit {
            Hit::Grid(i) => self.grid_item(i)?.clone(),
            Hit::Player(slot) => view.slot(slot).filter(|s| !s.is_empty())?.clone(),
            Hit::Destroy => return None,
        };
        Some((hit, stack))
    }

    fn drop_outside(
        &mut self,
        view: &dyn GameView,
        button: MouseButton,
        out: &mut Vec<ScreenAction>,
    ) {
        let Some(carried) = view.carried().filter(|c| !c.is_empty()) else {
            return;
        };
        let inventory = self.kind() == TabKind::Inventory;
        if button == MouseButton::Left || inventory {
            out.push(ScreenAction::CreativeDrop(carried.clone()));
            out.push(ScreenAction::CreativeSetCarried(ItemStack::default()));
        } else if button == MouseButton::Right {
            out.push(ScreenAction::CreativeDrop(with_count(carried, 1)));
            out.push(ScreenAction::CreativeSetCarried(with_count(
                carried,
                carried.count - 1,
            )));
        }
    }

    fn grid_click(
        &mut self,
        view: &dyn GameView,
        index: usize,
        button: MouseButton,
        mods: Modifiers,
        out: &mut Vec<ScreenAction>,
    ) {
        let clicked = self.grid_item(index).cloned();
        if clicked.as_ref().is_some_and(locked) {
            return;
        }
        let carried = view.carried().filter(|c| !c.is_empty()).cloned();
        let quick = mods.shift;
        if button == MouseButton::Middle {
            if let (None, Some(item)) = (&carried, &clicked) {
                out.push(ScreenAction::CreativeSetCarried(with_count(
                    item,
                    max_stack(item),
                )));
            }
            return;
        }
        if !matches!(button, MouseButton::Left | MouseButton::Right) {
            return;
        }
        let left = button == MouseButton::Left;
        match (carried, clicked) {
            (Some(mut carried), Some(clicked)) if same_stack(&carried, &clicked) => {
                let max = max_stack(&carried);
                if left {
                    carried.count = if quick {
                        max
                    } else {
                        (carried.count + 1).min(max)
                    };
                } else {
                    carried.count -= 1;
                }
                out.push(ScreenAction::CreativeSetCarried(if carried.count > 0 {
                    carried
                } else {
                    ItemStack::default()
                }));
            }
            (None, Some(clicked)) => {
                let count = if quick {
                    max_stack(&clicked)
                } else {
                    clicked.count
                };
                out.push(ScreenAction::CreativeSetCarried(with_count(
                    &clicked, count,
                )));
            }
            (Some(mut carried), _) => {
                if left {
                    out.push(ScreenAction::CreativeSetCarried(ItemStack::default()));
                } else {
                    carried.count -= 1;
                    out.push(ScreenAction::CreativeSetCarried(if carried.count > 0 {
                        carried
                    } else {
                        ItemStack::default()
                    }));
                }
            }
            (None, None) => {}
        }
    }

    fn player_click(
        &mut self,
        view: &dyn GameView,
        slot: usize,
        button: MouseButton,
        mods: Modifiers,
        double: bool,
        out: &mut Vec<ScreenAction>,
    ) {
        let click = |mode: ClickType, button: i32| {
            ScreenAction::CreativeClick(ContainerClick::new(slot as i32, button, mode))
        };
        match button {
            MouseButton::Middle => out.push(click(ClickType::Clone, 2)),
            MouseButton::Left | MouseButton::Right => {
                let index = button.index();
                let carried_empty = is_empty(view.carried());
                if double
                    && button == MouseButton::Left
                    && self.last_slot_click == Some(Hit::Player(slot))
                {
                    if carried_empty {
                        out.push(click(ClickType::Pickup, 0));
                    }
                    out.push(click(ClickType::PickupAll, 0));
                } else if mods.shift {
                    if self.kind() == TabKind::Inventory {
                        out.push(click(ClickType::QuickMove, index));
                    } else if !is_empty(view.slot(slot)) {
                        out.push(ScreenAction::CreativeSetSlot {
                            slot,
                            stack: ItemStack::default(),
                        });
                    }
                } else {
                    out.push(click(ClickType::Pickup, index));
                }
            }
            _ => {}
        }
    }

    fn press(
        &mut self,
        env: &Env<'_>,
        (x, y): (f32, f32),
        button: MouseButton,
        mods: Modifiers,
        double: bool,
        out: &mut Vec<ScreenAction>,
    ) -> bool {
        let view = env.view;
        if self.tab_at(x, y).is_some() {
            return true;
        }
        if button == MouseButton::Left && self.inside_scrollbar(x, y) {
            self.scrolling = self.can_scroll();
            return true;
        }
        if self.kind() == TabKind::Search
            && button == MouseButton::Left
            && self.search.contains(x, y)
        {
            self.search.click(x, env.now_ms, mods.shift);
            return true;
        }
        let hit = self.hit_at(x, y);
        match hit {
            Some(Hit::Grid(i)) => self.grid_click(view, i, button, mods, out),
            Some(Hit::Player(slot)) => self.player_click(view, slot, button, mods, double, out),
            Some(Hit::Destroy) => {
                if mods.shift {
                    out.push(ScreenAction::CreativeClearInventory);
                } else {
                    out.push(ScreenAction::CreativeSetCarried(ItemStack::default()));
                }
            }
            None if self.outside(x, y) => self.drop_outside(view, button, out),
            None => {}
        }
        self.last_slot_click = hit;
        true
    }

    fn key(
        &mut self,
        env: &mut Env<'_>,
        key: Key,
        mods: Modifiers,
        out: &mut Vec<ScreenAction>,
    ) -> bool {
        self.ignore_text = false;
        let searching = self.kind() == TabKind::Search;
        if key == Key::Escape {
            out.push(ScreenAction::CloseContainer);
            return true;
        }
        let hovered = self.hovered_stack(env.view);
        let keys = env.view.screen_keys();
        let number = keys.hotbar_key(key);
        if let (Some(n), Some((hit, stack))) = (number, &hovered) {
            match hit {
                Hit::Grid(_) => out.push(ScreenAction::CreativeSetSlot {
                    slot: HOTBAR_FIRST + n,
                    stack: with_count(stack, max_stack(stack)),
                }),
                Hit::Player(slot) => out.push(ScreenAction::CreativeClick(ContainerClick::new(
                    *slot as i32,
                    n as i32,
                    ClickType::Swap,
                ))),
                Hit::Destroy => {}
            }
            self.ignore_text = true;
            return true;
        }
        if !searching {
            match key {
                k if keys.chat.is_key(k) => {
                    self.ignore_text = true;
                    if let Some(i) = creative_tabs()
                        .iter()
                        .position(|t| t.kind == TabKind::Search)
                    {
                        self.select_tab(i);
                    }
                    return true;
                }
                k if keys.inventory.is_key(k) => {
                    out.push(ScreenAction::CloseContainer);
                    return true;
                }
                k if keys.drop.is_key(k) => {
                    if let Some((hit, stack)) = hovered {
                        let count = match hit {
                            Hit::Grid(_) if mods.ctrl => max_stack(&stack),
                            Hit::Grid(_) => 1,
                            _ if mods.ctrl => stack.count,
                            _ => 1,
                        };
                        out.push(ScreenAction::CreativeDrop(with_count(&stack, count)));
                        if let Hit::Player(slot) = hit {
                            out.push(ScreenAction::CreativeSetSlot {
                                slot,
                                stack: with_count(&stack, stack.count - count),
                            });
                        }
                    }
                    return true;
                }
                _ => return false,
            }
        }
        let before = self.search.value().to_owned();
        let handled = self.search.key(key, mods, env.clipboard);
        if self.search.value() != before {
            self.search_dirty = true;
        }
        handled || key != Key::Escape
    }
}

fn placeholder() -> ItemStack {
    let mut stack = ItemStack::new(
        mcr_data::v26_3()
            .item_by_name("minecraft:paper")
            .map_or(0, |id| id as i32),
        1,
    );
    stack
        .components
        .added
        .push(DataComponent::CreativeSlotLock(Unit));
    stack
}

impl Screen for CreativeScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.left = (width - WIDTH) / 2;
        self.top = (height - HEIGHT) / 2;
        self.search.rect = [self.left + 82, self.top + 6, 80, 9];
        self.search.visible = self.kind() == TabKind::Search;
    }

    fn pauses_game(&self) -> bool {
        false
    }

    fn entity_rect(&self) -> Option<[i32; 4]> {
        (self.kind() == TabKind::Inventory).then_some([self.left + 73, self.top + 6, 32, 43])
    }

    fn slot_center(&self, index: usize) -> Option<(i32, i32)> {
        let (_, x, y) = self
            .player_slots()
            .into_iter()
            .find(|&(slot, ..)| slot == index)?;
        Some((self.left + x + 8, self.top + y + 8))
    }

    fn tick(&mut self, env: &mut Env<'_>, out: &mut Vec<ScreenAction>) {
        if !env.view.creative() {
            out.push(ScreenAction::Replace(Box::new(
                ContainerScreen::player_inventory(),
            )));
            return;
        }
        self.sync(env.view);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.mouse = cx.mouse;
        let view = cx.view;
        self.sync(view);
        if self.kind() == TabKind::Search {
            if self.names.is_none() {
                self.compute_names(cx);
                self.search_dirty = true;
            }
            if self.search_dirty {
                self.apply_search(view);
            }
        }
        let (gw, gh) = cx.gui_size();
        cx.frame
            .fill_gradient(0, 0, gw, gh, OVERLAY_TOP, OVERLAY_BOTTOM);
        let selected = self.selected;
        let tabs: Vec<(usize, &'static CreativeTab)> = self.tab_list().collect();
        for &(i, tab) in &tabs {
            if i != selected {
                self.draw_tab_button(cx, tab, false);
            }
        }
        let texture = match self.kind() {
            TabKind::Search => "tab_item_search",
            TabKind::Inventory => "tab_inventory",
            _ => "tab_items",
        };
        cx.texture(
            &format!("{SPRITES}/{texture}"),
            (0, 0),
            (self.left, self.top, WIDTH, HEIGHT),
        );
        if self.tab().can_scroll {
            let sprite = if self.can_scroll() {
                "scroller"
            } else {
                "scroller_disabled"
            };
            let y = self.top + SCROLL_Y + ((SCROLL_H - 17) as f32 * self.scroll) as i32;
            cx.sprite(
                &format!("{SPRITES}/{sprite}"),
                self.left + SCROLL_X,
                y,
                12,
                15,
            );
        }
        if self.kind() == TabKind::Search {
            self.search.render(cx, true);
        }
        self.draw_tab_button(cx, self.tab(), true);
        if self.tab().show_title {
            let title = cx.lang(&self.tab().title_key);
            cx.text(&title, self.left + 8, self.top + 6, LABEL, false);
        }
        cx.frame.mark_preview_layer();
        self.draw_slots(cx);
        crate::gui::effects_list::draw_effects(cx, self.left, self.top, WIDTH);
        self.draw_carried(cx);
        self.draw_tooltips(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        match *input {
            UiInput::MousePress {
                x,
                y,
                button,
                mods,
                double,
            } => {
                self.mouse = (x, y);
                self.press(env, (x, y), button, mods, double, out)
            }
            UiInput::MouseMove { x, y } => {
                self.mouse = (x, y);
                if self.scrolling {
                    let top = (self.top + SCROLL_Y) as f32;
                    self.scroll_to((y - top - 7.5) / (SCROLL_H as f32 - 15.0));
                }
                true
            }
            UiInput::MouseRelease { x, y, button, .. } => {
                self.scrolling = false;
                if button == MouseButton::Left
                    && let Some(i) = self.tab_at(x, y)
                {
                    self.select_tab(i);
                    out.push(ScreenAction::ClickSound);
                }
                true
            }
            UiInput::Scroll { dy, .. } => {
                if self.can_scroll() {
                    let rows = row_count(self.items.len()).max(1) as f32;
                    self.scroll_to(self.scroll - dy / rows);
                }
                true
            }
            UiInput::Key { key, mods, .. } => self.key(env, key, mods, out),
            UiInput::Char(c) => {
                if self.ignore_text || self.kind() != TabKind::Search {
                    return false;
                }
                let before = self.search.value().to_owned();
                let used = self.search.char(c);
                if self.search.value() != before {
                    self.search_dirty = true;
                }
                used
            }
        }
    }
}

impl CreativeScreen {
    fn draw_tab_button(&mut self, cx: &mut RenderCx<'_, '_>, tab: &CreativeTab, selected: bool) {
        let top = tab.row == TabRow::Top;
        let x = self.left + tab_x(tab);
        let y = self.top + if top { -28 } else { HEIGHT - 4 };
        let row = if top { "top" } else { "bottom" };
        let state = if selected { "selected" } else { "unselected" };
        let name = format!("{SPRITES}/tab_{row}_{state}_{}", tab.column + 1);
        cx.sprite(&name, x, y, TAB_W, TAB_H);
        cx.stack(&tab.icon, x + 5, y + 8 + if top { 1 } else { -1 });
    }

    fn draw_slots(&mut self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        let hovered = self.hit_at(self.mouse.0, self.mouse.1);
        let highlight = |cx: &mut RenderCx<'_, '_>, name: &str, x: i32, y: i32| {
            cx.sprite(name, x - 4, y - 4, 24, 24);
        };
        let hover_pos = |s: &Self, hit: Hit| -> (i32, i32) {
            match hit {
                Hit::Grid(i) => (
                    s.left + GRID_X + (i % COLS) as i32 * 18,
                    s.top + GRID_Y + (i / COLS) as i32 * 18,
                ),
                Hit::Player(slot) => s
                    .player_slots()
                    .into_iter()
                    .find(|&(n, ..)| n == slot)
                    .map_or((0, 0), |(_, x, y)| (s.left + x, s.top + y)),
                Hit::Destroy => (s.left + DESTROY.0, s.top + DESTROY.1),
            }
        };
        if let Some(hit) = hovered {
            let (x, y) = hover_pos(self, hit);
            highlight(cx, "container/slot_highlight_back", x, y);
        }
        if self.kind() != TabKind::Inventory {
            for i in 0..GRID {
                if let Some(stack) = self.grid_item(i).cloned() {
                    let x = self.left + GRID_X + (i % COLS) as i32 * 18;
                    let y = self.top + GRID_Y + (i / COLS) as i32 * 18;
                    cx.stack(&stack, x, y);
                }
            }
        }
        for (slot, sx, sy) in self.player_slots() {
            let (x, y) = (self.left + sx, self.top + sy);
            match view.slot(slot).filter(|s| !s.is_empty()) {
                Some(stack) => cx.stack(stack, x, y),
                None => match slot {
                    5..=8 => cx.sprite(ARMOR_ICONS[slot - 5], x, y, 16, 16),
                    45 => cx.sprite("container/slot/shield", x, y, 16, 16),
                    _ => {}
                },
            }
        }
        if let Some(hit) = hovered {
            let (x, y) = hover_pos(self, hit);
            highlight(cx, "container/slot_highlight_front", x, y);
        }
    }

    fn draw_carried(&mut self, cx: &mut RenderCx<'_, '_>) {
        if let Some(carried) = cx.view.carried().filter(|c| !c.is_empty()) {
            let (mx, my) = (self.mouse.0 as i32, self.mouse.1 as i32);
            cx.stack(carried, mx - 8, my - 8);
        }
    }

    fn draw_tooltips(&mut self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        if !is_empty(view.carried()) {
            return;
        }
        let (mx, my) = self.mouse;
        for (_, tab) in self.tab_list().collect::<Vec<_>>() {
            let (x, y) = (self.left + tab_x(tab) + 3, self.top + tab_y(tab) + 3);
            if contains([x, y, 21, 27], mx, my) {
                let title = cx.lang(&tab.title_key);
                cx.set_tooltip(TooltipLines::text(title));
                return;
            }
        }
        if self.hit_at(mx, my) == Some(Hit::Destroy) {
            let text = cx.lang("inventory.binSlot");
            cx.set_tooltip(TooltipLines::text(text));
            return;
        }
        let Some((hit, stack)) = self.hovered_stack(view) else {
            return;
        };
        let lines = if locked(&stack) {
            let keys = view.screen_keys();
            let row = self.hotbar_row(hit);
            let lang = cx.frame.ui().lang();
            let (save, slot) = (keys.save_hotbar.label(lang), keys.hotbar[row].label(lang));
            let text = cx.lang_fmt("inventory.hotbarInfo", &[&save, &slot]);
            TooltipLines::text(text)
        } else {
            let lines = cx.item_tooltip(&stack);
            let grid = matches!(hit, Hit::Grid(_));
            if self.kind() == TabKind::Category && grid {
                lines
            } else {
                let mut extra = Vec::new();
                if self.kind() == TabKind::Search && grid {
                    for (tag, ids) in &self.visible_tags {
                        if ids.contains(&stack.item) {
                            extra.push(format!("§5#{tag}"));
                        }
                    }
                }
                for (_, tab) in self.tab_list() {
                    if tab.kind != TabKind::Search && tab.contains(&stack) {
                        extra.push(format!("§9{}", cx.lang(&tab.title_key)));
                    }
                }
                lines_with(lines, extra)
            }
        };
        cx.set_tooltip(lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::env::MemoryClipboard;

    struct Table {
        slots: Vec<ItemStack>,
        carried: ItemStack,
        features: Vec<String>,
    }

    impl GameView for Table {
        fn slot(&self, index: usize) -> Option<&ItemStack> {
            self.slots.get(index)
        }

        fn carried(&self) -> Option<&ItemStack> {
            Some(&self.carried)
        }

        fn creative(&self) -> bool {
            true
        }

        fn enabled_features(&self) -> &[String] {
            &self.features
        }

        fn item_tags_matching(&self, query: &str) -> Vec<(String, Vec<i32>)> {
            let log = mcr_data::v26_3().item_by_name("minecraft:oak_log").unwrap() as i32;
            if "logs".contains(query) && !query.is_empty() {
                vec![("minecraft:logs".into(), vec![log])]
            } else {
                Vec::new()
            }
        }
    }

    fn table() -> Table {
        Table {
            slots: vec![ItemStack::default(); 46],
            carried: ItemStack::default(),
            features: vec!["minecraft:vanilla".into()],
        }
    }

    fn item(name: &str, count: i32) -> ItemStack {
        let id = mcr_data::v26_3()
            .item_by_name(&format!("minecraft:{name}"))
            .unwrap();
        ItemStack::new(id as i32, count)
    }

    fn screen(view: &Table) -> CreativeScreen {
        let mut s = CreativeScreen::new(Rc::new(RefCell::new(HotbarStore::default())));
        s.init(320, 240);
        s.select_tab_by_key("building_blocks");
        s.sync(view);
        s.select_tab_by_key("building_blocks");
        s
    }

    fn grid_pos(s: &CreativeScreen, i: usize) -> (f32, f32) {
        (
            (s.left + GRID_X + (i % COLS) as i32 * 18 + 8) as f32,
            (s.top + GRID_Y + (i / COLS) as i32 * 18 + 8) as f32,
        )
    }

    fn click(
        s: &mut CreativeScreen,
        view: &Table,
        at: (f32, f32),
        button: MouseButton,
        mods: Modifiers,
    ) -> Vec<ScreenAction> {
        let mut clip = MemoryClipboard::default();
        let mut env = Env {
            view,
            clipboard: &mut clip,
            now_ms: 0,
        };
        let mut out = Vec::new();
        s.input(
            &mut env,
            &UiInput::MousePress {
                x: at.0,
                y: at.1,
                button,
                mods,
                double: false,
            },
            &mut out,
        );
        out
    }

    fn carried(actions: &[ScreenAction]) -> Option<&ItemStack> {
        actions.iter().find_map(|a| match a {
            ScreenAction::CreativeSetCarried(s) => Some(s),
            _ => None,
        })
    }

    #[test]
    fn building_blocks_start_with_oak_log_in_the_grid() {
        let view = table();
        let s = screen(&view);
        assert_eq!(
            s.grid_item(0).map(|st| st.item),
            Some(item("oak_log", 1).item)
        );
        assert!(s.item_count() > 400);
    }

    #[test]
    fn left_click_picks_up_the_shown_stack_and_shift_a_full_stack() {
        let view = table();
        let mut s = screen(&view);
        let at = grid_pos(&s, 0);
        let out = click(&mut s, &view, at, MouseButton::Left, Modifiers::NONE);
        assert_eq!(
            carried(&out).map(|c| (c.item, c.count)),
            Some((item("oak_log", 1).item, 1))
        );
        let out = click(&mut s, &view, at, MouseButton::Left, Modifiers::SHIFT);
        assert_eq!(carried(&out).map(|c| c.count), Some(64));
    }

    #[test]
    fn clicking_same_item_grows_and_right_click_shrinks_the_cursor() {
        let mut view = table();
        view.carried = item("oak_log", 5);
        let mut s = screen(&view);
        let at = grid_pos(&s, 0);
        let out = click(&mut s, &view, at, MouseButton::Left, Modifiers::NONE);
        assert_eq!(carried(&out).map(|c| c.count), Some(6));
        let out = click(&mut s, &view, at, MouseButton::Right, Modifiers::NONE);
        assert_eq!(carried(&out).map(|c| c.count), Some(4));
    }

    #[test]
    fn clicking_another_item_with_a_cursor_deletes_it() {
        let mut view = table();
        view.carried = item("stone", 3);
        let mut s = screen(&view);
        let at = grid_pos(&s, 0);
        let out = click(&mut s, &view, at, MouseButton::Left, Modifiers::NONE);
        assert_eq!(carried(&out), Some(&ItemStack::default()));
    }

    #[test]
    fn middle_click_clones_a_full_stack() {
        let view = table();
        let mut s = screen(&view);
        let at = grid_pos(&s, 3);
        let out = click(&mut s, &view, at, MouseButton::Middle, Modifiers::NONE);
        assert_eq!(carried(&out).map(|c| c.count), Some(64));
    }

    #[test]
    fn number_key_over_the_grid_fills_that_hotbar_slot() {
        let view = table();
        let mut s = screen(&view);
        s.mouse = grid_pos(&s, 1);
        let mut clip = MemoryClipboard::default();
        let mut env = Env {
            view: &view,
            clipboard: &mut clip,
            now_ms: 0,
        };
        let mut out = Vec::new();
        s.input(&mut env, &UiInput::key(Key::Digit(3)), &mut out);
        let Some(ScreenAction::CreativeSetSlot { slot, stack }) = out.first() else {
            panic!("{out:?}");
        };
        assert_eq!(*slot, 38);
        assert_eq!(stack.count, 64);
    }

    #[test]
    fn hotbar_slot_click_goes_to_the_player_menu() {
        let view = table();
        let mut s = screen(&view);
        let (slot, x, y) = s.player_slots()[4];
        let at = ((s.left + x + 8) as f32, (s.top + y + 8) as f32);
        let out = click(&mut s, &view, at, MouseButton::Left, Modifiers::NONE);
        let Some(ScreenAction::CreativeClick(c)) = out.first() else {
            panic!("{out:?}");
        };
        assert_eq!((c.slot, c.mode), (slot as i32, ClickType::Pickup));
        assert_eq!(slot, 40);
    }

    #[test]
    fn shift_click_on_a_hotbar_item_deletes_it() {
        let mut view = table();
        view.slots[36] = item("stone", 10);
        let mut s = screen(&view);
        let (_, x, y) = s.player_slots()[0];
        let at = ((s.left + x + 8) as f32, (s.top + y + 8) as f32);
        let out = click(&mut s, &view, at, MouseButton::Left, Modifiers::SHIFT);
        assert!(matches!(
            out.first(),
            Some(ScreenAction::CreativeSetSlot { slot: 36, stack }) if stack.is_empty()
        ));
    }

    #[test]
    fn clicking_outside_with_a_cursor_drops_it() {
        let mut view = table();
        view.carried = item("stone", 7);
        let mut s = screen(&view);
        let out = click(
            &mut s,
            &view,
            (2.0, 2.0),
            MouseButton::Left,
            Modifiers::NONE,
        );
        assert!(matches!(out.first(), Some(ScreenAction::CreativeDrop(d)) if d.count == 7));
        let out = click(
            &mut s,
            &view,
            (2.0, 2.0),
            MouseButton::Right,
            Modifiers::NONE,
        );
        assert!(matches!(out.first(), Some(ScreenAction::CreativeDrop(d)) if d.count == 1));
        assert_eq!(carried(&out).map(|c| c.count), Some(6));
    }

    #[test]
    fn scrolling_moves_whole_rows_and_clamps() {
        let view = table();
        let mut s = screen(&view);
        let first = s.grid_item(0).cloned();
        let mut clip = MemoryClipboard::default();
        let mut env = Env {
            view: &view,
            clipboard: &mut clip,
            now_ms: 0,
        };
        let mut out = Vec::new();
        s.input(
            &mut env,
            &UiInput::Scroll {
                x: 0.0,
                y: 0.0,
                dx: 0.0,
                dy: -1.0,
            },
            &mut out,
        );
        assert!(s.scroll_offset() > 0.0);
        assert_ne!(s.grid_item(0).cloned(), first);
        for _ in 0..1000 {
            s.input(
                &mut env,
                &UiInput::Scroll {
                    x: 0.0,
                    y: 0.0,
                    dx: 0.0,
                    dy: -1.0,
                },
                &mut out,
            );
        }
        assert_eq!(s.scroll_offset(), 1.0);
    }

    #[test]
    fn row_and_scroll_conversions_match_vanilla() {
        assert_eq!(row_count(45), 0);
        assert_eq!(row_count(46), 1);
        assert_eq!(row_for_scroll(0.5, 100), 4);
        assert_eq!(row_for_scroll(0.0, 100), 0);
        assert!((scroll_for_row(3, 100) - 3.0 / 7.0).abs() < 1e-6);
        assert_eq!(scroll_for_row(3, 10), 0.0);
    }

    #[test]
    fn tab_positions_match_vanilla() {
        let tabs = creative_tabs();
        let search = tabs.iter().find(|t| t.key == "search").unwrap();
        assert_eq!(tab_x(search), WIDTH - 27 + 1);
        assert_eq!(tab_y(search), -32);
        assert_eq!(tab_x(&tabs[1]), 27);
        assert_eq!(
            tab_y(tabs.iter().find(|t| t.key == "combat").unwrap()),
            HEIGHT
        );
    }

    #[test]
    fn name_search_filters_and_hash_searches_tags() {
        let view = table();
        let mut s = screen(&view);
        s.select_tab_by_key("search");
        assert!(s.item_count() > 1000);
        let pool = s.search_pool.clone();
        s.names = Some(
            pool.iter()
                .map(|st| StackView::new(st).map_or(String::new(), |v| v.name().to_lowercase()))
                .collect(),
        );
        s.search.set_value("oak_log");
        s.apply_search(&view);
        assert!(s.item_count() >= 1 && s.item_count() < 10);
        assert_eq!(
            s.grid_item(0).map(|x| x.item),
            Some(item("oak_log", 1).item)
        );
        s.search.set_value("#logs");
        s.apply_search(&view);
        assert_eq!(s.item_count(), 1);
        s.search.set_value("");
        s.apply_search(&view);
        assert_eq!(s.item_count(), pool.len());
    }

    #[test]
    fn operator_tab_needs_permission() {
        let view = table();
        let mut s = screen(&view);
        assert!(!s.tab_list().any(|(_, t)| t.is_operator()));
        s.select_tab_by_key("op_blocks");
        assert_eq!(s.selected_tab(), "op_blocks");
        assert_eq!(s.item_count(), 0);
    }

    #[test]
    fn inventory_tab_layout_has_armor_offhand_and_destroy_slot() {
        let view = table();
        let mut s = screen(&view);
        s.select_tab_by_key("inventory");
        let slots = s.player_slots();
        assert!(slots.contains(&(5, 54, 6)) && slots.contains(&(8, 108, 33)));
        assert!(slots.contains(&(45, 35, 20)) && slots.contains(&(36, 9, 112)));
        let at = (
            (s.left + DESTROY.0 + 8) as f32,
            (s.top + DESTROY.1 + 8) as f32,
        );
        assert_eq!(s.hit_at(at.0, at.1), Some(Hit::Destroy));
        assert!(s.entity_rect().is_some());
    }
}
