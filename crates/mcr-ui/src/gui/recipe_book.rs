use mcr_protocol::play::clientbound::ui::{RecipeBookSettings, RecipeDisplay, SlotDisplay};
use mcr_protocol::play::serverbound::RecipeBookType;
use mcr_protocol::types::item::ItemStack;
use rustc_hash::{FxHashMap, FxHashSet};

use super::container::{
    ContainerLayout, FurnaceKind, GROUP_GRID, GROUP_PLAYER, MenuKind, SlotKind,
};
use super::edit_box::EditBox;
use super::env::{Clipboard, Env, GameView, RenderCx};
use super::input::{Key, MouseButton, UiInput, contains};
use super::screen::ScreenAction;
use super::tooltip::TooltipLines;
use crate::items::StackView;

pub const PANEL_WIDTH: i32 = 147;
pub const PANEL_HEIGHT: i32 = 166;
const OFFSET_X: i32 = 86;
const NARROW_WIDTH: i32 = 379;
const PER_PAGE: usize = 20;
const CYCLE_TICKS: u32 = 30;
const ANIMATION_TICKS: u32 = 15;
const MAX_REQUIREMENTS: usize = 64;
const MAX_ALTERNATIVES: usize = 256;
const MAX_RESULTS: usize = 16;
const MAX_OVERLAY: usize = 64;
const MAX_NEED_ITEMS: usize = 1 << 20;
const MAX_SEARCH: usize = 50;
const SEARCH_ID: u32 = 1;
const GHOST_BACK: u32 = 0x30FF_0000;
const GHOST_FRONT: u32 = 0x30FF_FFFF;

#[derive(Clone, Debug)]
struct Tab {
    categories: &'static [i32],
    icons: (&'static str, Option<&'static str>),
    search: bool,
}

const CRAFTING_TABS: [Tab; 5] = [
    Tab {
        categories: &[2, 0, 3, 1],
        icons: ("compass", None),
        search: true,
    },
    Tab {
        categories: &[2],
        icons: ("iron_axe", Some("golden_sword")),
        search: false,
    },
    Tab {
        categories: &[0],
        icons: ("bricks", None),
        search: false,
    },
    Tab {
        categories: &[3],
        icons: ("lava_bucket", Some("apple")),
        search: false,
    },
    Tab {
        categories: &[1],
        icons: ("redstone", None),
        search: false,
    },
];

const FURNACE_TABS: [Tab; 4] = [
    Tab {
        categories: &[4, 5, 6],
        icons: ("compass", None),
        search: true,
    },
    Tab {
        categories: &[4],
        icons: ("porkchop", None),
        search: false,
    },
    Tab {
        categories: &[5],
        icons: ("stone", None),
        search: false,
    },
    Tab {
        categories: &[6],
        icons: ("lava_bucket", Some("emerald")),
        search: false,
    },
];

const BLAST_TABS: [Tab; 3] = [
    Tab {
        categories: &[7, 8],
        icons: ("compass", None),
        search: true,
    },
    Tab {
        categories: &[7],
        icons: ("redstone_ore", None),
        search: false,
    },
    Tab {
        categories: &[8],
        icons: ("iron_shovel", Some("golden_leggings")),
        search: false,
    },
];

const SMOKER_TABS: [Tab; 2] = [
    Tab {
        categories: &[9],
        icons: ("compass", None),
        search: true,
    },
    Tab {
        categories: &[9],
        icons: ("porkchop", None),
        search: false,
    },
];

/// Where the recipe book toggle sits relative to the container's left edge and the screen height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Crafting {
        grid: (i32, i32),
        button: (i32, i32),
    },
    Furnace {
        button: (i32, i32),
    },
}

#[derive(Clone, Debug)]
struct Entry {
    id: i32,
    display: RecipeDisplay,
    results: Vec<ItemStack>,
    needs: Vec<Vec<i32>>,
    highlight: bool,
    selectable: bool,
}

#[derive(Clone, Debug)]
struct Collection {
    category: i32,
    entries: Vec<Entry>,
    names: Option<String>,
}

#[derive(Clone, Debug)]
struct PageButton {
    collection: usize,
    entries: Vec<usize>,
    craftable: bool,
    same_result: bool,
    animation: u32,
}

#[derive(Clone, Debug)]
struct OverlayButton {
    x: i32,
    y: i32,
    recipe: i32,
    craftable: bool,
    slots: Vec<(i32, i32, Vec<ItemStack>)>,
}

#[derive(Clone, Debug)]
struct Overlay {
    x: i32,
    y: i32,
    columns: usize,
    buttons: Vec<OverlayButton>,
}

#[derive(Clone, Debug)]
struct GhostSlot {
    slot: usize,
    items: Vec<ItemStack>,
    result: bool,
}

#[derive(Debug)]
pub struct RecipeBook {
    book_type: RecipeBookType,
    shape: Shape,
    tabs: &'static [Tab],
    filter_label: &'static str,
    filter_sprite: &'static str,
    open: bool,
    filtering: bool,
    view_open: bool,
    view_filtering: bool,
    narrow: bool,
    gui: (i32, i32),
    selected: usize,
    search: EditBox,
    searching: bool,
    last_search: String,
    page: usize,
    revision: Option<u64>,
    supply_signature: u64,
    collections: Vec<Collection>,
    tab_visible: Vec<bool>,
    tab_animation: Vec<u32>,
    craftable: FxHashSet<i32>,
    shown: Vec<PageButton>,
    dirty: bool,
    ticks: u32,
    overlay: Option<Overlay>,
    ghost: Vec<GhostSlot>,
    ghost_applied: u64,
    ghost_dismissed: u64,
    last_placed: Option<i32>,
    pending: Vec<ScreenAction>,
    icon_cache: FxHashMap<&'static str, Option<ItemStack>>,
}

impl RecipeBook {
    pub fn for_menu(kind: MenuKind) -> Option<Self> {
        let (book_type, shape, tabs, label, sprite): (_, _, &'static [Tab], _, _) = match kind {
            MenuKind::PlayerInventory => (
                RecipeBookType::Crafting,
                Shape::Crafting {
                    grid: (2, 2),
                    button: (104, -22),
                },
                &CRAFTING_TABS,
                "gui.recipebook.toggleRecipes.craftable",
                "recipe_book/filter",
            ),
            MenuKind::Crafting => (
                RecipeBookType::Crafting,
                Shape::Crafting {
                    grid: (3, 3),
                    button: (5, -49),
                },
                &CRAFTING_TABS,
                "gui.recipebook.toggleRecipes.craftable",
                "recipe_book/filter",
            ),
            MenuKind::Furnace(FurnaceKind::Furnace) => (
                RecipeBookType::Furnace,
                Shape::Furnace { button: (20, -49) },
                &FURNACE_TABS,
                "gui.recipebook.toggleRecipes.smeltable",
                "recipe_book/furnace_filter",
            ),
            MenuKind::Furnace(FurnaceKind::BlastFurnace) => (
                RecipeBookType::BlastFurnace,
                Shape::Furnace { button: (20, -49) },
                &BLAST_TABS,
                "gui.recipebook.toggleRecipes.blastable",
                "recipe_book/furnace_filter",
            ),
            MenuKind::Furnace(FurnaceKind::Smoker) => (
                RecipeBookType::Smoker,
                Shape::Furnace { button: (20, -49) },
                &SMOKER_TABS,
                "gui.recipebook.toggleRecipes.smokable",
                "recipe_book/furnace_filter",
            ),
            _ => return None,
        };
        let mut search = EditBox::new(SEARCH_ID, [0, 0, 81, 14]).with_max_length(MAX_SEARCH);
        search.text_color = 0xFFFF_FFFF;
        Some(Self {
            book_type,
            shape,
            tabs,
            filter_label: label,
            filter_sprite: sprite,
            open: false,
            filtering: false,
            view_open: false,
            view_filtering: false,
            narrow: false,
            gui: (0, 0),
            selected: 0,
            search,
            searching: false,
            last_search: String::new(),
            page: 0,
            revision: None,
            supply_signature: 0,
            collections: Vec::new(),
            tab_visible: vec![true; tabs.len()],
            tab_animation: vec![0; tabs.len()],
            craftable: FxHashSet::default(),
            shown: Vec::new(),
            dirty: true,
            ticks: 0,
            overlay: None,
            ghost: Vec::new(),
            ghost_applied: 0,
            ghost_dismissed: 0,
            last_placed: None,
            pending: Vec::new(),
            icon_cache: FxHashMap::default(),
        })
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn is_narrow_open(&self) -> bool {
        self.open && self.narrow
    }

    pub fn capturing_text(&self) -> bool {
        self.open && self.searching
    }

    pub fn resize(&mut self, gui: (i32, i32)) {
        self.gui = gui;
        self.narrow = gui.0 < NARROW_WIDTH;
        self.dirty = true;
    }

    /// The container's left edge: shifted right while the book sits beside it.
    pub fn container_left(&self, image_width: i32) -> i32 {
        if self.open && !self.narrow {
            177 + (self.gui.0 - image_width - 200) / 2
        } else {
            (self.gui.0 - image_width) / 2
        }
    }

    pub fn button_rect(&self, left: i32) -> [i32; 4] {
        let (dx, dy) = match self.shape {
            Shape::Crafting { button, .. } | Shape::Furnace { button } => button,
        };
        [left + dx, self.gui.1 / 2 + dy, 20, 18]
    }

    fn origin(&self) -> (i32, i32) {
        let offset = if self.narrow { 0 } else { OFFSET_X };
        (
            (self.gui.0 - PANEL_WIDTH) / 2 - offset,
            (self.gui.1 - PANEL_HEIGHT) / 2,
        )
    }

    fn grid(&self) -> (i32, i32) {
        match self.shape {
            Shape::Crafting { grid, .. } => grid,
            Shape::Furnace { .. } => (0, 0),
        }
    }

    fn type_settings(&self, settings: &RecipeBookSettings) -> (bool, bool) {
        let s = match self.book_type {
            RecipeBookType::Crafting => settings.crafting,
            RecipeBookType::Furnace => settings.furnace,
            RecipeBookType::BlastFurnace => settings.blast_furnace,
            RecipeBookType::Smoker => settings.smoker,
        };
        (s.open, s.filtering)
    }

    fn adopt_settings(&mut self, view: &dyn GameView) {
        let (open, filtering) = self.type_settings(&view.recipe_settings());
        if open != self.view_open {
            self.view_open = open;
            self.open = open;
            self.overlay = None;
        }
        if filtering != self.view_filtering {
            self.view_filtering = filtering;
            self.filtering = filtering;
            self.dirty = true;
        }
    }

    fn settings_action(&self) -> ScreenAction {
        ScreenAction::RecipeBookSettings {
            book_type: self.book_type,
            open: self.open,
            filtering: self.filtering,
        }
    }

    /// Opens or closes the book; returns whether the container must move.
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.overlay = None;
        self.dirty = true;
        self.pending.push(self.settings_action());
    }

    pub fn take_actions(&mut self, out: &mut Vec<ScreenAction>) {
        out.append(&mut self.pending);
    }

    pub fn crafting_slot_clicked(&mut self, slot: usize, layout: &ContainerLayout) {
        if self.is_crafting_slot(slot, layout) {
            self.last_placed = None;
            self.ghost_dismissed = self.ghost_applied;
            self.ghost.clear();
        }
    }

    fn is_crafting_slot(&self, slot: usize, layout: &ContainerLayout) -> bool {
        match (self.shape, layout.slots.get(slot)) {
            (Shape::Crafting { .. }, Some(spec)) => {
                spec.kind == SlotKind::Result || spec.group == GROUP_GRID
            }
            (Shape::Furnace { .. }, _) => slot <= 2,
            _ => false,
        }
    }

    pub fn tick(&mut self, view: &dyn GameView, layout: &ContainerLayout) {
        self.ticks = self.ticks.wrapping_add(1);
        for a in &mut self.tab_animation {
            *a = a.saturating_sub(1);
        }
        for b in &mut self.shown {
            b.animation = b.animation.saturating_sub(1);
        }
        self.adopt_settings(view);
        let revision = view.recipe_book_revision();
        if self.revision != Some(revision) {
            self.revision = Some(revision);
            self.rebuild(view);
        }
        let signature = self.supply_signature(view, layout);
        if signature != self.supply_signature {
            self.supply_signature = signature;
            self.recompute_craftable(view, layout);
        }
        if let Some((rev, display)) = view.ghost_recipe()
            && rev != self.ghost_applied
        {
            self.ghost_applied = rev;
            self.ghost = if rev == self.ghost_dismissed {
                Vec::new()
            } else {
                self.build_ghost(view, layout, display)
            };
        }
    }

    fn supply_slots<'a>(&self, layout: &'a ContainerLayout) -> impl Iterator<Item = usize> + 'a {
        let furnace = matches!(self.shape, Shape::Furnace { .. });
        layout
            .slots
            .iter()
            .enumerate()
            .filter(move |(i, s)| {
                if furnace {
                    *i == 0 || (s.group == GROUP_PLAYER && s.kind == SlotKind::Normal)
                } else {
                    s.group == GROUP_GRID || (s.group == GROUP_PLAYER && s.kind == SlotKind::Normal)
                }
            })
            .map(|(i, _)| i)
    }

    fn supply_signature(&self, view: &dyn GameView, layout: &ContainerLayout) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for i in self.supply_slots(layout) {
            let (item, count) = view
                .slot(i)
                .filter(|s| !s.is_empty())
                .map_or((-1, 0), |s| (s.item, s.count));
            for v in [i as u64, item as u64, count as u64] {
                h = (h ^ v).wrapping_mul(0x0100_0000_01b3);
            }
        }
        h
    }

    fn supply(&self, view: &dyn GameView, layout: &ContainerLayout) -> FxHashMap<i32, i32> {
        let mut supply: FxHashMap<i32, i32> = FxHashMap::default();
        for i in self.supply_slots(layout) {
            if let Some(stack) = view
                .slot(i)
                .filter(|s| !s.is_empty() && s.components.is_empty())
            {
                let entry = supply.entry(stack.item).or_insert(0);
                *entry = entry.saturating_add(stack.count);
            }
        }
        supply
    }

    fn recompute_craftable(&mut self, view: &dyn GameView, layout: &ContainerLayout) {
        let supply = self.supply(view, layout);
        self.craftable.clear();
        for entry in self.collections.iter().flat_map(|c| &c.entries) {
            if entry.selectable && can_craft(&entry.needs, &supply) {
                self.craftable.insert(entry.id);
            }
        }
        self.dirty = true;
    }

    fn can_display(&self, display: &RecipeDisplay) -> bool {
        let (gw, gh) = self.grid();
        match (self.shape, display) {
            (Shape::Crafting { .. }, RecipeDisplay::CraftingShaped { width, height, .. }) => {
                gw >= *width && gh >= *height
            }
            (Shape::Crafting { .. }, RecipeDisplay::CraftingShapeless { ingredients, .. }) => {
                usize::try_from(gw * gh).is_ok_and(|n| n >= ingredients.len())
            }
            (Shape::Furnace { .. }, RecipeDisplay::Furnace { .. }) => true,
            _ => false,
        }
    }

    fn rebuild(&mut self, view: &dyn GameView) {
        let wanted: FxHashSet<i32> = self
            .tabs
            .iter()
            .flat_map(|t| t.categories.iter().copied())
            .collect();
        let mut entries = view.recipe_entries();
        entries.retain(|e| wanted.contains(&e.contents.category));
        entries.sort_by_key(|e| e.contents.id);
        let mut groups: Vec<(i32, Option<i32>, Vec<Entry>)> = Vec::new();
        let mut group_index: FxHashMap<(i32, i32), usize> = FxHashMap::default();
        let mut tags: FxHashMap<String, Vec<i32>> = FxHashMap::default();
        let mut budget = MAX_NEED_ITEMS;
        for e in entries {
            let c = &e.contents;
            let needs = c
                .crafting_requirements
                .as_ref()
                .map(|sets| {
                    let cost = sets.len().min(MAX_REQUIREMENTS) * MAX_ALTERNATIVES;
                    if cost > budget {
                        return vec![Vec::new()];
                    }
                    budget -= cost;
                    sets.iter()
                        .take(MAX_REQUIREMENTS)
                        .map(|set| {
                            let key = format!("{set:?}");
                            tags.entry(key)
                                .or_insert_with(|| {
                                    let mut items = view.holder_items(set);
                                    items.truncate(MAX_ALTERNATIVES);
                                    items
                                })
                                .clone()
                        })
                        .collect()
                })
                .unwrap_or_else(|| vec![Vec::new()]);
            let entry = Entry {
                id: c.id,
                display: c.display.clone(),
                results: {
                    let mut r = view.resolve_display(display_result(&c.display));
                    r.truncate(MAX_RESULTS);
                    r
                },
                needs: if c.crafting_requirements.is_some() {
                    needs
                } else {
                    vec![Vec::new()]
                },
                highlight: e.highlight(),
                selectable: self.can_display(&c.display),
            };
            match c.group {
                None => groups.push((c.category, None, vec![entry])),
                Some(g) => match group_index.get(&(c.category, g)) {
                    Some(&i) => groups[i].2.push(entry),
                    None => {
                        group_index.insert((c.category, g), groups.len());
                        groups.push((c.category, Some(g), vec![entry]));
                    }
                },
            }
        }
        self.collections = groups
            .into_iter()
            .map(|(category, _, entries)| Collection {
                category,
                entries,
                names: None,
            })
            .collect();
        self.supply_signature = 0;
        self.craftable.clear();
        self.tab_visible = self
            .tabs
            .iter()
            .map(|t| {
                t.search
                    || self.collections.iter().any(|c| {
                        t.categories.contains(&c.category) && c.entries.iter().any(|e| e.selectable)
                    })
            })
            .collect();
        if !self
            .tab_visible
            .get(self.selected)
            .copied()
            .unwrap_or(false)
        {
            self.selected = 0;
        }
        self.dirty = true;
    }

    fn ensure_names(cx: &mut RenderCx<'_, '_>, collection: &mut Collection) {
        if collection.names.is_some() {
            return;
        }
        let mut names = String::new();
        for stack in collection
            .entries
            .iter()
            .flat_map(|e| e.results.iter().take(4))
        {
            if names.len() > 512 {
                break;
            }
            names.push_str(&cx.item_tooltip(stack).first_plain().to_lowercase());
            names.push(' ');
            if let Some(view) = StackView::new(stack) {
                let id = view.name();
                names.push_str(
                    &id.strip_prefix("minecraft:")
                        .unwrap_or(id)
                        .replace('_', " "),
                );
                names.push(' ');
            }
        }
        collection.names = Some(names);
    }

    fn refresh_shown(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.dirty = false;
        let query = self.search.value().trim().to_lowercase();
        let Some(tab) = self.tabs.get(self.selected) else {
            return;
        };
        let mut shown = Vec::new();
        let mut seen: Vec<i32> = Vec::new();
        for &category in tab.categories {
            if seen.contains(&category) {
                continue;
            }
            seen.push(category);
            for index in 0..self.collections.len() {
                if self.collections[index].category != category {
                    continue;
                }
                if !query.is_empty() {
                    Self::ensure_names(cx, &mut self.collections[index]);
                    if !self.collections[index]
                        .names
                        .as_deref()
                        .is_some_and(|n| n.contains(&query))
                    {
                        continue;
                    }
                }
                let collection = &self.collections[index];
                let entries: Vec<usize> = collection
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| {
                        e.selectable && (!self.filtering || self.craftable.contains(&e.id))
                    })
                    .map(|(i, _)| i)
                    .collect();
                if entries.is_empty() {
                    continue;
                }
                let craftable = entries
                    .iter()
                    .any(|&i| self.craftable.contains(&collection.entries[i].id));
                let first = &collection.entries[entries[0]];
                let same_result = entries.iter().all(|&i| {
                    collection.entries[i].results.first() == first.results.first()
                        && collection.entries[i].results.len() == first.results.len()
                });
                let highlighted = entries.iter().any(|&i| collection.entries[i].highlight);
                shown.push(PageButton {
                    collection: index,
                    entries,
                    craftable,
                    same_result,
                    animation: if highlighted { ANIMATION_TICKS } else { 0 },
                });
            }
        }
        for button in &shown {
            if button.animation == 0 {
                continue;
            }
            for &i in &button.entries {
                let entry = &mut self.collections[button.collection].entries[i];
                if entry.highlight {
                    entry.highlight = false;
                    self.pending.push(ScreenAction::RecipeSeen(entry.id));
                }
            }
        }
        self.shown = shown;
        let pages = self.shown.len().div_ceil(PER_PAGE);
        if self.page >= pages {
            self.page = 0;
        }
    }

    fn set_tab(&mut self, index: usize) {
        if index != self.selected {
            self.selected = index;
            self.page = 0;
            self.overlay = None;
            self.dirty = true;
        }
    }

    fn visible_tabs(&self) -> Vec<(usize, i32, i32)> {
        let (ox, oy) = self.origin();
        let mut out = Vec::new();
        let mut row = 0;
        for (i, visible) in self.tab_visible.iter().enumerate() {
            if *visible {
                out.push((i, ox - 30, oy + 3 + 27 * row));
                row += 1;
            }
        }
        out
    }

    fn button_pos(&self, index: usize) -> (i32, i32) {
        let (ox, oy) = self.origin();
        (
            ox + 11 + 25 * (index % 5) as i32,
            oy + 31 + 25 * (index / 5) as i32,
        )
    }

    fn page_count(&self) -> usize {
        self.shown.len().div_ceil(PER_PAGE)
    }

    fn cycle_index(&self) -> usize {
        (self.ticks / CYCLE_TICKS) as usize
    }

    fn display_stack(&self, button: &PageButton) -> Option<&ItemStack> {
        let n = button.entries.len();
        let index = self.cycle_index();
        let entry = &self.collections[button.collection].entries[button.entries[index % n]];
        if entry.results.is_empty() {
            return None;
        }
        entry.results.get((index / n) % entry.results.len())
    }

    fn current_recipe(&self, button: &PageButton) -> i32 {
        let n = button.entries.len();
        self.collections[button.collection].entries[button.entries[self.cycle_index() % n]].id
    }

    fn icon(&mut self, view: &dyn GameView, name: &'static str) -> Option<ItemStack> {
        self.icon_cache
            .entry(name)
            .or_insert_with(|| view.item_stack(name, 1))
            .clone()
    }

    pub fn render(&mut self, cx: &mut RenderCx<'_, '_>, left: i32) {
        let button = self.button_rect(left);
        let hovered = contains(button, cx.mouse.0, cx.mouse.1);
        let sprite = if hovered {
            "recipe_book/button_highlighted"
        } else {
            "recipe_book/button"
        };
        cx.sprite(sprite, button[0], button[1], button[2], button[3]);
        if !self.open {
            return;
        }
        if self.dirty {
            self.refresh_shown(cx);
        }
        let (ox, oy) = self.origin();
        cx.texture("recipe_book", (1, 1), (ox, oy, PANEL_WIDTH, PANEL_HEIGHT));
        self.search.rect = [ox + 25, oy + 13, 81, 14];
        self.search.hint = Some(cx.lang("gui.recipebook.search_hint"));
        let focused = self.searching;
        self.search.render(cx, focused);
        self.render_tabs(cx);
        self.render_filter(cx, ox, oy);
        self.render_page(cx, ox, oy);
        self.render_overlay(cx);
        self.render_tooltip(cx);
    }

    fn render_tabs(&mut self, cx: &mut RenderCx<'_, '_>) {
        for (i, x, y) in self.visible_tabs() {
            let selected = i == self.selected;
            let tab = &self.tabs[i];
            let draw_x = if selected { x - 2 } else { x };
            let mut rect = (draw_x, y, 35, 27);
            let animation = self.tab_animation.get(i).copied().unwrap_or(0);
            if animation > 0 {
                let squeeze = 1.0
                    + 0.1
                        * (animation as f32 / ANIMATION_TICKS as f32 * std::f32::consts::PI).sin();
                let center = (y + 12) as f32;
                rect.1 = (center + (y as f32 - center) * squeeze) as i32;
                rect.3 = (27.0 * squeeze) as i32;
            }
            cx.sprite(
                if selected {
                    "recipe_book/tab_selected"
                } else {
                    "recipe_book/tab"
                },
                rect.0,
                rect.1,
                rect.2,
                rect.3,
            );
            let shift = if selected { -2 } else { 0 };
            let (primary, secondary) = tab.icons;
            let view = cx.view;
            let first = self.icon(view, primary);
            match secondary {
                Some(second) => {
                    let second = self.icon(view, second);
                    if let Some(s) = first {
                        cx.fake_item(&s, x + 3 + shift, y + 5);
                    }
                    if let Some(s) = second {
                        cx.fake_item(&s, x + 14 + shift, y + 5);
                    }
                }
                None => {
                    if let Some(s) = first {
                        cx.fake_item(&s, x + 9 + shift, y + 5);
                    }
                }
            }
        }
    }

    fn render_filter(&mut self, cx: &mut RenderCx<'_, '_>, ox: i32, oy: i32) {
        let rect = [ox + 110, oy + 12, 26, 16];
        let hovered = contains(rect, cx.mouse.0, cx.mouse.1);
        let state = if self.filtering {
            "enabled"
        } else {
            "disabled"
        };
        let suffix = if hovered { "_highlighted" } else { "" };
        let name = format!("{}_{state}{suffix}", self.filter_sprite);
        cx.sprite(&name, rect[0], rect[1], rect[2], rect[3]);
    }

    fn render_page(&mut self, cx: &mut RenderCx<'_, '_>, ox: i32, oy: i32) {
        let pages = self.page_count();
        if pages > 1 {
            let text = cx.lang_fmt(
                "gui.recipebook.page",
                &[&(self.page + 1).to_string(), &pages.to_string()],
            );
            let w = cx.text_width(&text);
            cx.text(&text, ox - w / 2 + 73, oy + 141, 0xFFFF_FFFF, false);
            if self.page + 1 < pages {
                let hovered = contains([ox + 93, oy + 137, 12, 17], cx.mouse.0, cx.mouse.1);
                cx.sprite(
                    if hovered {
                        "recipe_book/page_forward_highlighted"
                    } else {
                        "recipe_book/page_forward"
                    },
                    ox + 93,
                    oy + 137,
                    12,
                    17,
                );
            }
            if self.page > 0 {
                let hovered = contains([ox + 38, oy + 137, 12, 17], cx.mouse.0, cx.mouse.1);
                cx.sprite(
                    if hovered {
                        "recipe_book/page_backward_highlighted"
                    } else {
                        "recipe_book/page_backward"
                    },
                    ox + 38,
                    oy + 137,
                    12,
                    17,
                );
            }
        }
        let start = self.page * PER_PAGE;
        for slot in 0..PER_PAGE {
            let Some(button) = self.shown.get(start + slot) else {
                break;
            };
            let (x, y) = self.button_pos(slot);
            let many = button.entries.len() > 1;
            let sprite = match (button.craftable, many) {
                (true, true) => "recipe_book/slot_many_craftable",
                (true, false) => "recipe_book/slot_craftable",
                (false, true) => "recipe_book/slot_many_uncraftable",
                (false, false) => "recipe_book/slot_uncraftable",
            };
            let (mut sx, mut sy, mut sw, mut sh) = (x, y, 25, 25);
            if button.animation > 0 {
                let squeeze = 1.0
                    + 0.1
                        * (button.animation as f32 / ANIMATION_TICKS as f32 * std::f32::consts::PI)
                            .sin();
                let (cx0, cy0) = ((x + 8) as f32, (y + 12) as f32);
                sx = (cx0 + (x as f32 - cx0) * squeeze) as i32;
                sy = (cy0 + (y as f32 - cy0) * squeeze) as i32;
                sw = (25.0 * squeeze) as i32;
                sh = sw;
            }
            cx.sprite(sprite, sx, sy, sw, sh);
            if let Some(stack) = self.display_stack(button).cloned() {
                let mut offset = 4;
                if many && button.same_result {
                    cx.fake_item(&stack, x + offset + 1, y + offset + 1);
                    offset -= 1;
                }
                cx.fake_item(&stack, x + offset, y + offset);
            }
        }
    }

    fn render_overlay(&mut self, cx: &mut RenderCx<'_, '_>) {
        let Some(overlay) = &self.overlay else {
            return;
        };
        let furnace = matches!(self.shape, Shape::Furnace { .. });
        let width = overlay.buttons.len().min(overlay.columns) as i32;
        let height = overlay.buttons.len().div_ceil(overlay.columns) as i32;
        cx.sprite(
            "recipe_book/overlay_recipe",
            overlay.x,
            overlay.y,
            width * 25 + 8,
            height * 25 + 8,
        );
        let (mx, my) = cx.mouse;
        let tick = self.cycle_index();
        for b in &overlay.buttons {
            let hovered = contains([b.x, b.y, 24, 24], mx, my);
            let base = if furnace {
                "recipe_book/furnace_overlay"
            } else {
                "recipe_book/crafting_overlay"
            };
            let name = format!(
                "{base}{}{}",
                if b.craftable { "" } else { "_disabled" },
                if hovered { "_highlighted" } else { "" }
            );
            cx.sprite(&name, b.x, b.y, 24, 24);
            for (sx, sy, items) in &b.slots {
                if items.is_empty() {
                    continue;
                }
                let stack = &items[tick % items.len()];
                cx.fake_item_sized(stack, b.x + 2 + sx - 3, b.y + 2 + sy - 3, 6);
            }
        }
    }

    fn render_tooltip(&mut self, cx: &mut RenderCx<'_, '_>) {
        let (mx, my) = cx.mouse;
        if self.overlay.is_some() {
            return;
        }
        let (ox, oy) = self.origin();
        if contains([ox + 110, oy + 12, 26, 16], mx, my) {
            let key = if self.filtering {
                self.filter_label
            } else {
                "gui.recipebook.toggleRecipes.all"
            };
            let text = cx.lang(key);
            cx.set_tooltip(TooltipLines::text(text));
            return;
        }
        let start = self.page * PER_PAGE;
        for slot in 0..PER_PAGE {
            let Some(button) = self.shown.get(start + slot) else {
                break;
            };
            let (x, y) = self.button_pos(slot);
            if !contains([x, y, 25, 25], mx, my) {
                continue;
            }
            if let Some(stack) = self.display_stack(button).cloned() {
                let mut lines = cx.item_tooltip(&stack);
                if button.entries.len() > 1 {
                    lines.push_text(cx.lang("gui.recipebook.moreRecipes"));
                }
                cx.set_tooltip(lines);
            }
            return;
        }
    }

    pub fn render_ghost(
        &mut self,
        cx: &mut RenderCx<'_, '_>,
        layout: &ContainerLayout,
        origin: (i32, i32),
        big_result: bool,
    ) {
        if !self.open {
            return;
        }
        let index = self.cycle_index();
        for ghost in &self.ghost {
            let Some(spec) = layout.slots.get(ghost.slot) else {
                continue;
            };
            let (x, y) = (origin.0 + spec.x, origin.1 + spec.y);
            let stack = &ghost.items[index % ghost.items.len()];
            if ghost.result && big_result {
                cx.frame.fill_rect(x - 4, y - 4, x + 20, y + 20, GHOST_BACK);
            } else {
                cx.frame.fill_rect(x, y, x + 16, y + 16, GHOST_BACK);
            }
            cx.fake_item(stack, x, y);
            cx.frame.fill_rect(x, y, x + 16, y + 16, GHOST_FRONT);
            if ghost.result && stack.count != 1 {
                let text = stack.count.to_string();
                let w = cx.text_width(&text);
                cx.text(&text, x + 17 - w, y + 9, 0xFFFF_FFFF, true);
            }
        }
    }

    pub fn ghost_tooltip(&self, cx: &mut RenderCx<'_, '_>, slot: usize) -> bool {
        let Some(ghost) = self.ghost.iter().find(|g| g.slot == slot) else {
            return false;
        };
        let stack = ghost.items[self.cycle_index() % ghost.items.len()].clone();
        let lines = cx.item_tooltip(&stack);
        cx.set_tooltip(lines);
        true
    }

    fn build_ghost(
        &self,
        view: &dyn GameView,
        layout: &ContainerLayout,
        display: &RecipeDisplay,
    ) -> Vec<GhostSlot> {
        let mut out = Vec::new();
        let mut add = |slot: usize, source: &SlotDisplay, result: bool| {
            let items = view.resolve_display(source);
            if !items.is_empty() {
                out.push(GhostSlot {
                    slot,
                    items,
                    result,
                });
            }
        };
        let result_slot = layout.slots.iter().position(|s| s.kind == SlotKind::Result);
        if let Some(slot) = result_slot {
            add(slot, display_result(display), true);
        }
        match (self.shape, display) {
            (
                Shape::Crafting { grid, .. },
                RecipeDisplay::CraftingShaped {
                    width,
                    height,
                    ingredients,
                    ..
                },
            ) => {
                let slots: Vec<usize> = layout
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.group == GROUP_GRID)
                    .map(|(i, _)| i)
                    .collect();
                for (ing, cell, _, _) in
                    place_grid(grid.0, grid.1, *width, *height, ingredients.len())
                {
                    if let (Some(&slot), Some(source)) = (slots.get(cell), ingredients.get(ing)) {
                        add(slot, source, false);
                    }
                }
            }
            (Shape::Crafting { .. }, RecipeDisplay::CraftingShapeless { ingredients, .. }) => {
                let slots = layout
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.group == GROUP_GRID)
                    .map(|(i, _)| i);
                for (slot, source) in slots.zip(ingredients) {
                    add(slot, source, false);
                }
            }
            (
                Shape::Furnace { .. },
                RecipeDisplay::Furnace {
                    ingredient, fuel, ..
                },
            ) => {
                add(0, ingredient, false);
                if view.slot(1).is_none_or(ItemStack::is_empty) {
                    add(1, fuel, false);
                }
            }
            _ => {}
        }
        out
    }

    fn place(&mut self, recipe: i32, max_items: bool) -> bool {
        if !self.craftable.contains(&recipe) && self.last_placed == Some(recipe) {
            return false;
        }
        self.last_placed = Some(recipe);
        self.ghost.clear();
        self.ghost_dismissed = self.ghost_applied;
        self.pending
            .push(ScreenAction::PlaceRecipe { recipe, max_items });
        if self.narrow {
            self.open = false;
            self.pending.push(self.settings_action());
        }
        true
    }

    fn open_overlay(&mut self, env: &mut Env<'_>, slot: usize, button: &PageButton) {
        let view = env.view;
        let collection = &self.collections[button.collection];
        let mut craftable: Vec<&Entry> = Vec::new();
        let mut others: Vec<&Entry> = Vec::new();
        for entry in collection.entries.iter().filter(|e| e.selectable) {
            if self.craftable.contains(&entry.id) {
                craftable.push(entry);
            } else if !self.filtering {
                others.push(entry);
            }
        }
        let total = (craftable.len() + others.len()).min(MAX_OVERLAY);
        if total == 0 {
            return;
        }
        let columns = if total <= 16 { 4 } else { 5 };
        let rows = total.div_ceil(columns) as i32;
        let (bx, by) = self.button_pos(slot % PER_PAGE);
        let (ox, oy) = self.origin();
        let (center_x, center_y) = (ox + PANEL_WIDTH / 2, oy + 13 + PANEL_HEIGHT / 2);
        let (mut x, mut y) = (bx, by);
        let right = x + total.min(columns) as i32 * 25;
        let max_left = center_x + 50;
        if right > max_left {
            x -= 25 * ((right - max_left) / 25);
        }
        let bottom = y + rows * 25;
        let max_bottom = center_y + 50;
        if bottom > max_bottom {
            y -= 25 * (((bottom - max_bottom) as f32 / 25.0).ceil() as i32);
        }
        let max_top = center_y - 100;
        if y < max_top {
            y -= 25 * (((y - max_top) as f32 / 25.0).ceil() as i32);
        }
        let furnace = matches!(self.shape, Shape::Furnace { .. });
        let buttons = craftable
            .iter()
            .map(|e| (*e, true))
            .chain(others.iter().map(|e| (*e, false)))
            .take(MAX_OVERLAY)
            .enumerate()
            .map(|(i, (entry, ok))| OverlayButton {
                x: x + 4 + 25 * (i % columns) as i32,
                y: y + 5 + 25 * (i / columns) as i32,
                recipe: entry.id,
                craftable: ok,
                slots: overlay_slots(view, &entry.display, furnace),
            })
            .collect();
        self.overlay = Some(Overlay {
            x,
            y,
            columns,
            buttons,
        });
    }

    fn page_click(
        &mut self,
        env: &mut Env<'_>,
        x: f32,
        y: f32,
        button: MouseButton,
        shift: bool,
    ) -> bool {
        let pages = self.page_count();
        let (ox, oy) = self.origin();
        if pages > 1 && button == MouseButton::Left {
            if self.page + 1 < pages && contains([ox + 93, oy + 137, 12, 17], x, y) {
                self.page += 1;
                return true;
            }
            if self.page > 0 && contains([ox + 38, oy + 137, 12, 17], x, y) {
                self.page -= 1;
                return true;
            }
        }
        let start = self.page * PER_PAGE;
        for slot in 0..PER_PAGE {
            let Some(button_state) = self.shown.get(start + slot).cloned() else {
                break;
            };
            let (bx, by) = self.button_pos(slot);
            if !contains([bx, by, 25, 25], x, y) {
                continue;
            }
            match button {
                MouseButton::Left => {
                    let recipe = self.current_recipe(&button_state);
                    if self.place(recipe, shift) {
                        self.pending.push(ScreenAction::ClickSound);
                    }
                }
                MouseButton::Right if button_state.entries.len() > 1 => {
                    self.open_overlay(env, start + slot, &button_state);
                    self.pending.push(ScreenAction::ClickSound);
                }
                _ => {}
            }
            return true;
        }
        false
    }

    pub fn input(
        &mut self,
        env: &mut Env<'_>,
        input: &UiInput,
        out: &mut Vec<ScreenAction>,
    ) -> bool {
        if !self.open || env.view.spectator() {
            return false;
        }
        let used = self.input_open(env, input);
        self.take_actions(out);
        used
    }

    fn input_open(&mut self, env: &mut Env<'_>, input: &UiInput) -> bool {
        match *input {
            UiInput::MousePress {
                x, y, button, mods, ..
            } => {
                if let Some(overlay) = self.overlay.take() {
                    if button == MouseButton::Left
                        && let Some(b) = overlay
                            .buttons
                            .iter()
                            .find(|b| contains([b.x, b.y, 24, 24], x, y))
                    {
                        let recipe = b.recipe;
                        if self.place(recipe, mods.shift) {
                            self.pending.push(ScreenAction::ClickSound);
                        }
                    }
                    return true;
                }
                if self.page_click(env, x, y, button, mods.shift) {
                    return true;
                }
                if button != MouseButton::Left {
                    return false;
                }
                let (ox, oy) = self.origin();
                let magnifier = contains([ox + 8, oy + 13, 17, 14], x, y);
                if magnifier || self.search.contains(x, y) {
                    self.searching = true;
                    self.search.focus(env.now_ms);
                    if !magnifier {
                        self.search.click(x, env.now_ms, false);
                    }
                    return true;
                }
                self.searching = false;
                if contains([ox + 110, oy + 12, 26, 16], x, y) {
                    self.filtering = !self.filtering;
                    self.page = 0;
                    self.dirty = true;
                    self.pending.push(self.settings_action());
                    self.pending.push(ScreenAction::ClickSound);
                    return true;
                }
                for (i, tx, ty) in self.visible_tabs() {
                    if contains([tx, ty, 35, 27], x, y) {
                        self.set_tab(i);
                        self.pending.push(ScreenAction::ClickSound);
                        return true;
                    }
                }
                false
            }
            UiInput::Key { key, mods, .. } => self.key(env, key, mods),
            UiInput::Char(c) if self.searching && self.search.char(c) => {
                self.search_changed();
                true
            }
            _ => false,
        }
    }

    fn key(&mut self, env: &mut Env<'_>, key: Key, mods: super::input::Modifiers) -> bool {
        if key == Key::Escape && self.narrow {
            self.open = false;
            self.pending.push(self.settings_action());
            return true;
        }
        if self.searching {
            if key == Key::Escape {
                self.searching = false;
                return true;
            }
            let clipboard: &mut dyn Clipboard = env.clipboard;
            let handled = self.search.key(key, mods, clipboard);
            self.search_changed();
            return handled || key != Key::Escape;
        }
        if key == Key::Letter('t') && !mods.ctrl {
            self.searching = true;
            self.search.focus(env.now_ms);
            return true;
        }
        false
    }

    fn search_changed(&mut self) {
        let text = self.search.value().to_lowercase();
        if text != self.last_search {
            self.last_search = text;
            self.page = 0;
            self.dirty = true;
        }
    }
}

fn display_result(display: &RecipeDisplay) -> &SlotDisplay {
    match display {
        RecipeDisplay::CraftingShapeless { result, .. }
        | RecipeDisplay::CraftingShaped { result, .. }
        | RecipeDisplay::Furnace { result, .. }
        | RecipeDisplay::Stonecutter { result, .. }
        | RecipeDisplay::Smithing { result, .. } => result,
    }
}

fn overlay_slots(
    view: &dyn GameView,
    display: &RecipeDisplay,
    furnace: bool,
) -> Vec<(i32, i32, Vec<ItemStack>)> {
    let mut out = Vec::new();
    let pos = |gx: i32, gy: i32| (3 + gx * 7, 3 + gy * 7);
    match display {
        RecipeDisplay::CraftingShaped {
            width,
            height,
            ingredients,
            ..
        } if !furnace => {
            for (ing, _, gx, gy) in place_grid(3, 3, *width, *height, ingredients.len()) {
                if let Some(source) = ingredients.get(ing) {
                    let mut items = view.resolve_display(source);
                    items.truncate(MAX_RESULTS);
                    if !items.is_empty() {
                        let (x, y) = pos(gx, gy);
                        out.push((x, y, items));
                    }
                }
            }
        }
        RecipeDisplay::CraftingShapeless { ingredients, .. } if !furnace => {
            for (i, source) in ingredients.iter().take(9).enumerate() {
                let items = view.resolve_display(source);
                if !items.is_empty() {
                    let (x, y) = pos(i as i32 % 3, i as i32 / 3);
                    out.push((x, y, items));
                }
            }
        }
        RecipeDisplay::Furnace { ingredient, .. } if furnace => {
            let items = view.resolve_display(ingredient);
            if !items.is_empty() {
                let (x, y) = pos(1, 1);
                out.push((x, y, items));
            }
        }
        _ => {}
    }
    out
}

/// Vanilla's `PlaceRecipeHelper`: where each ingredient of a recipe lands in a larger grid, as
/// `(ingredient, grid index, column, row)`.
pub fn place_grid(
    grid_w: i32,
    grid_h: i32,
    recipe_w: i32,
    recipe_h: i32,
    count: usize,
) -> Vec<(usize, usize, i32, i32)> {
    let mut out = Vec::new();
    let mut next = 0usize;
    let mut index = 0i32;
    let mut y = 0;
    while y < grid_h {
        let center_y = (recipe_h as f32) < grid_h as f32 / 2.0;
        let start_y = (grid_h as f32 / 2.0 - recipe_h as f32 / 2.0).floor() as i32;
        if center_y && start_y > y {
            index += grid_w;
            y += 1;
        }
        for x in 0..grid_w {
            if next >= count {
                return out;
            }
            let center_x = (recipe_w as f32) < grid_w as f32 / 2.0;
            let start_x = (grid_w as f32 / 2.0 - recipe_w as f32 / 2.0).floor() as i32;
            let mut total = recipe_w;
            let mut add = x < recipe_w;
            if center_x {
                total = start_x + recipe_w;
                add = start_x <= x && x < start_x + recipe_w;
            }
            if add {
                out.push((next, usize::try_from(index).unwrap_or(0), x, y));
                next += 1;
            } else if total == x {
                index += grid_w - x;
                break;
            }
            index += 1;
        }
        y += 1;
    }
    out
}

/// Whether every requirement can take its own unit from the supply, by augmenting paths.
pub fn can_craft(needs: &[Vec<i32>], supply: &FxHashMap<i32, i32>) -> bool {
    if needs.is_empty() || needs.len() > MAX_REQUIREMENTS {
        return false;
    }
    let options: Vec<Vec<i32>> = needs
        .iter()
        .map(|set| {
            let mut items: Vec<i32> = set
                .iter()
                .copied()
                .filter(|i| supply.get(i).is_some_and(|&c| c > 0))
                .collect();
            items.sort_unstable();
            items.dedup();
            items
        })
        .collect();
    if options.iter().any(Vec::is_empty) {
        return false;
    }
    let mut holders: FxHashMap<i32, Vec<usize>> = FxHashMap::default();
    for need in 0..options.len() {
        let mut visited = FxHashSet::default();
        if !assign(need, &options, supply, &mut holders, &mut visited) {
            return false;
        }
    }
    true
}

fn assign(
    need: usize,
    options: &[Vec<i32>],
    supply: &FxHashMap<i32, i32>,
    holders: &mut FxHashMap<i32, Vec<usize>>,
    visited: &mut FxHashSet<i32>,
) -> bool {
    for &item in &options[need] {
        if !visited.insert(item) {
            continue;
        }
        let capacity = supply.get(&item).copied().unwrap_or(0).max(0) as usize;
        let current = holders.entry(item).or_default();
        if current.len() < capacity {
            current.push(need);
            return true;
        }
        let occupants = current.clone();
        for other in occupants {
            if assign(other, options, supply, holders, visited) {
                let list = holders.entry(item).or_default();
                if let Some(pos) = list.iter().position(|&n| n == other) {
                    list[pos] = need;
                }
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use mcr_protocol::play::clientbound::ui::{RecipeBookEntry, RecipeDisplayEntry};
    use mcr_protocol::types::component::HolderSet;

    use super::*;

    struct Mock {
        entries: Vec<RecipeBookEntry>,
        slots: Vec<ItemStack>,
        ghost: Option<RecipeDisplay>,
    }

    impl GameView for Mock {
        fn slot(&self, index: usize) -> Option<&ItemStack> {
            self.slots.get(index)
        }

        fn recipe_book_revision(&self) -> u64 {
            1
        }

        fn recipe_entries(&self) -> Vec<&RecipeBookEntry> {
            self.entries.iter().collect()
        }

        fn resolve_display(&self, display: &SlotDisplay) -> Vec<ItemStack> {
            match display {
                SlotDisplay::Item(id) => vec![ItemStack::new(*id, 1)],
                _ => Vec::new(),
            }
        }

        fn holder_items(&self, set: &HolderSet) -> Vec<i32> {
            match set {
                HolderSet::Ids(ids) => ids.clone(),
                HolderSet::Tag(_) => Vec::new(),
            }
        }

        fn ghost_recipe(&self) -> Option<(u64, &RecipeDisplay)> {
            self.ghost.as_ref().map(|d| (1, d))
        }
    }

    fn entry(
        id: i32,
        category: i32,
        display: RecipeDisplay,
        needs: Vec<Vec<i32>>,
    ) -> RecipeBookEntry {
        RecipeBookEntry {
            contents: RecipeDisplayEntry {
                id,
                display,
                group: None,
                category,
                crafting_requirements: Some(needs.into_iter().map(HolderSet::Ids).collect()),
            },
            flags: 0,
        }
    }

    fn shapeless(ingredient: i32, result: i32) -> RecipeDisplay {
        RecipeDisplay::CraftingShapeless {
            ingredients: vec![SlotDisplay::Item(ingredient)],
            result: SlotDisplay::Item(result),
            crafting_station: SlotDisplay::Empty,
        }
    }

    fn mock(ghost: Option<RecipeDisplay>) -> Mock {
        let big = RecipeDisplay::CraftingShaped {
            width: 3,
            height: 3,
            ingredients: vec![SlotDisplay::Item(1); 9],
            result: SlotDisplay::Item(9),
            crafting_station: SlotDisplay::Empty,
        };
        let mut slots = vec![ItemStack::default(); 46];
        slots[36] = ItemStack::new(1, 4);
        Mock {
            entries: vec![
                entry(10, 3, shapeless(1, 2), vec![vec![1]]),
                entry(11, 3, shapeless(5, 6), vec![vec![5]]),
                entry(12, 0, big, vec![vec![1]; 9]),
            ],
            slots,
            ghost,
        }
    }

    #[test]
    fn only_displayable_recipes_with_ingredients_are_craftable() {
        let view = mock(None);
        let layout = ContainerLayout::new(MenuKind::PlayerInventory);
        let mut book = RecipeBook::for_menu(MenuKind::PlayerInventory).unwrap();
        book.tick(&view, &layout);
        assert!(book.craftable.contains(&10));
        assert!(!book.craftable.contains(&11));
        assert!(!book.craftable.contains(&12));
        let selectable: Vec<i32> = book
            .collections
            .iter()
            .flat_map(|c| &c.entries)
            .filter(|e| e.selectable)
            .map(|e| e.id)
            .collect();
        assert_eq!(selectable, vec![10, 11]);
        assert_eq!(book.tab_visible, vec![true, false, false, true, false]);
    }

    #[test]
    fn ghost_recipes_fill_the_grid_and_result() {
        let view = mock(Some(shapeless(1, 2)));
        let layout = ContainerLayout::new(MenuKind::PlayerInventory);
        let mut book = RecipeBook::for_menu(MenuKind::PlayerInventory).unwrap();
        book.tick(&view, &layout);
        let slots: Vec<(usize, bool)> = book.ghost.iter().map(|g| (g.slot, g.result)).collect();
        assert_eq!(slots, vec![(0, true), (1, false)]);
        book.crafting_slot_clicked(1, &layout);
        assert!(book.ghost.is_empty());
    }

    #[test]
    fn repeating_an_unaffordable_placement_is_not_resent() {
        let mut book = RecipeBook::for_menu(MenuKind::Crafting).unwrap();
        assert!(book.place(11, false));
        assert!(!book.place(11, true));
        assert_eq!(book.pending.len(), 1);
    }

    #[test]
    fn only_crafting_and_furnace_menus_have_a_book() {
        assert!(RecipeBook::for_menu(MenuKind::Hopper).is_none());
        assert!(RecipeBook::for_menu(MenuKind::Furnace(FurnaceKind::Smoker)).is_some());
    }

    fn supply(items: &[(i32, i32)]) -> FxHashMap<i32, i32> {
        items.iter().copied().collect()
    }

    #[test]
    fn exact_shape_fills_the_grid_in_order() {
        let cells: Vec<usize> = place_grid(3, 3, 3, 3, 9).iter().map(|p| p.1).collect();
        assert_eq!(cells, (0..9).collect::<Vec<_>>());
    }

    #[test]
    fn small_recipes_centre_in_a_bigger_grid() {
        let cells: Vec<(usize, i32, i32)> = place_grid(3, 3, 1, 2, 2)
            .iter()
            .map(|p| (p.1, p.2, p.3))
            .collect();
        assert_eq!(cells, vec![(1, 1, 0), (4, 1, 1)]);
        let corner: Vec<usize> = place_grid(3, 3, 2, 2, 4).iter().map(|p| p.1).collect();
        assert_eq!(corner, vec![0, 1, 3, 4]);
    }

    #[test]
    fn matching_shares_one_item_between_alternatives() {
        let needs = vec![vec![1, 2], vec![1]];
        assert!(can_craft(&needs, &supply(&[(1, 1), (2, 1)])));
        assert!(!can_craft(&needs, &supply(&[(1, 1)])));
        assert!(can_craft(&needs, &supply(&[(1, 2)])));
    }

    #[test]
    fn requirements_need_distinct_units() {
        let needs = vec![vec![5]; 4];
        assert!(can_craft(&needs, &supply(&[(5, 4)])));
        assert!(!can_craft(&needs, &supply(&[(5, 3)])));
        assert!(!can_craft(&[], &supply(&[(5, 3)])));
        assert!(!can_craft(&vec![vec![5]; 65], &supply(&[(5, 100)])));
    }
}
