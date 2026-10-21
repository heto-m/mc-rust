//! Create New World's extra screens: game rules, data packs with experiments, and superflat layers.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use mcr_data::game_rules::{Category, GAME_RULES_26_3, GameRule, Kind, changed_rules};

use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, MouseButton, UiInput, contains};
use super::list::ScrollList;
use super::menus::{WHITE, draw_menu_background, tile_texture};
use super::screen::{Screen, ScreenAction};
use super::widgets::{Button, WidgetEvent, WidgetEventKind, WidgetSet};
use super::world_list::SharedWorlds;

const ROW: i32 = 24;
const TOP: i32 = 32;
const FOOTER: i32 = 36;
const LABEL: u32 = 0xFFA0_A0A0;
const ERROR: u32 = 0xFFFF_5555;
const MAX_NUMBER_CHARS: usize = 11;
/// Vanilla's Classic Flat preset.
pub const CLASSIC_FLAT: &str =
    "minecraft:bedrock,2*minecraft:dirt,minecraft:grass_block;minecraft:plains";

/// Create New World's settings kept outside its widgets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateExtras {
    pub game_rules: Vec<(String, String)>,
    pub experiments: Vec<String>,
    pub data_packs: Vec<String>,
    pub pack_dir: Option<PathBuf>,
    pub flat: Option<String>,
}

pub type SharedExtras = Rc<RefCell<CreateExtras>>;

const DONE: u32 = 1;
const CANCEL: u32 = 2;
const OPEN_FOLDER: u32 = 3;
const PRESET: u32 = 4;
const LAYERS: u32 = 5;

fn footer(widgets: &mut WidgetSet, width: i32, height: i32) {
    widgets.add(Button::new(
        DONE,
        [width / 2 - 155, height - 28, 150, 20],
        "lang:gui.done",
    ));
    widgets.add(Button::new(
        CANCEL,
        [width / 2 + 5, height - 28, 150, 20],
        "lang:gui.cancel",
    ));
}

fn toggle_button(cx: &mut RenderCx<'_, '_>, rect: [i32; 4], on: bool, hot: bool) {
    let sprite = if hot {
        "widget/button_highlighted"
    } else {
        "widget/button"
    };
    cx.sprite(sprite, rect[0], rect[1], rect[2], rect[3]);
    let label = cx.lang(if on { "options.on" } else { "options.off" });
    cx.centered_text(&label, rect[0] + rect[2] / 2, rect[1] + 6, WHITE, true);
}

/// A lang key's text, or `fallback` when the language has no entry for it.
fn lang_or(cx: &mut RenderCx<'_, '_>, key: &str, fallback: &str) -> String {
    let text = cx.lang(key);
    if text == key {
        fallback.to_owned()
    } else {
        text
    }
}

fn humanize(id: &str) -> String {
    let text = id.replace('_', " ");
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

enum RuleRow {
    Header(Category),
    Rule(&'static GameRule),
}

/// Vanilla's Edit Game Rules for a new world: rules by category, toggles and bounded number fields.
pub struct GameRulesScreen {
    extras: SharedExtras,
    rows: Vec<RuleRow>,
    values: HashMap<&'static str, String>,
    /// The running world's values; Done then sends the differences instead of storing them.
    live: Option<HashMap<&'static str, String>>,
    editing: Option<usize>,
    list: ScrollList,
    widgets: WidgetSet,
    gui: (i32, i32),
    mouse: (f32, f32),
}

impl std::fmt::Debug for GameRulesScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameRulesScreen")
            .field("changed", &self.changed())
            .finish()
    }
}

impl GameRulesScreen {
    pub fn new(extras: SharedExtras) -> Self {
        let mut rows = Vec::new();
        for category in Category::ALL {
            rows.push(RuleRow::Header(category));
            let mut rules: Vec<&'static GameRule> = GAME_RULES_26_3
                .iter()
                .filter(|r| r.category == category)
                .collect();
            rules.sort_by_key(|r| r.id);
            rows.extend(rules.into_iter().map(RuleRow::Rule));
        }
        let mut values: HashMap<&'static str, String> = GAME_RULES_26_3
            .iter()
            .map(|r| (r.id, r.default_value()))
            .collect();
        for (id, value) in &extras.borrow().game_rules {
            if let Some(rule) = mcr_data::game_rules::game_rule(id) {
                values.insert(rule.id, value.clone());
            }
        }
        Self {
            extras,
            rows,
            values,
            live: None,
            editing: None,
            list: ScrollList::new([0, TOP, 0, 0], ROW),
            widgets: WidgetSet::default(),
            gui: (0, 0),
            mouse: (0.0, 0.0),
        }
    }

    /// Vanilla's `InWorldGameRulesScreen`: edits the running world's `current` values.
    pub fn in_world(current: &[(String, String)]) -> Self {
        let extras = Rc::new(RefCell::new(CreateExtras {
            game_rules: current.to_vec(),
            ..CreateExtras::default()
        }));
        let mut screen = Self::new(extras);
        screen.live = Some(screen.values.clone());
        screen
    }

    /// The rules whose value differs from the running world's, normalized for the command.
    pub fn live_changes(&self) -> Vec<(String, String)> {
        let Some(live) = &self.live else {
            return Vec::new();
        };
        let mut changes: Vec<(String, String)> = GAME_RULES_26_3
            .iter()
            .filter_map(|r| {
                let now = r.normalize(self.values.get(r.id)?)?;
                (live.get(r.id).and_then(|v| r.normalize(v)).as_ref() != Some(&now))
                    .then(|| (r.id.to_owned(), now))
            })
            .collect();
        changes.truncate(GAME_RULES_26_3.len());
        changes
    }

    /// The rules that differ from their defaults, as Done would store them.
    pub fn changed(&self) -> Vec<(&'static str, String)> {
        changed_rules(self.values.iter().map(|(k, v)| (*k, v.as_str())))
    }

    fn valid(&self) -> bool {
        GAME_RULES_26_3.iter().all(|r| {
            self.values
                .get(r.id)
                .is_some_and(|v| r.normalize(v).is_some())
        })
    }

    fn control_rect(&self, index: usize) -> [i32; 4] {
        [self.gui.0 / 2 + 5, self.list.item_top(index) + 2, 150, 20]
    }

    fn sync_done(&mut self) {
        let valid = self.valid();
        if let Some(b) = self.widgets.button_mut(DONE) {
            b.active = valid;
        }
    }

    fn bar_x(&self) -> i32 {
        self.gui.0 / 2 + 160
    }

    fn click(&mut self, x: f32, y: f32, out: &mut Vec<ScreenAction>) -> bool {
        if !contains(self.list.rect(), x, y) {
            return false;
        }
        let Some(index) = self.list.item_at(x, y) else {
            return false;
        };
        if !contains(self.control_rect(index), x, y) {
            self.editing = None;
            return true;
        }
        let RuleRow::Rule(rule) = self.rows[index] else {
            return true;
        };
        out.push(ScreenAction::ClickSound);
        match rule.kind {
            Kind::Bool(_) => {
                let value = self.values.entry(rule.id).or_default();
                *value = if value == "true" { "false" } else { "true" }.into();
                self.editing = None;
            }
            Kind::Int { .. } => self.editing = Some(index),
        }
        self.sync_done();
        true
    }

    fn edit_key(&mut self, input: &UiInput) -> bool {
        let Some(index) = self.editing else {
            return false;
        };
        let RuleRow::Rule(rule) = self.rows[index] else {
            return false;
        };
        let value = self.values.entry(rule.id).or_default();
        match *input {
            UiInput::Char(c)
                if (c.is_ascii_digit() || c == '-') && value.len() < MAX_NUMBER_CHARS =>
            {
                value.push(c);
            }
            UiInput::Key {
                key: Key::Backspace,
                ..
            } => {
                value.pop();
            }
            UiInput::Key {
                key: Key::Enter | Key::Escape | Key::Tab,
                ..
            } => self.editing = None,
            UiInput::Char(_) | UiInput::Key { .. } => {}
            _ => return false,
        }
        self.sync_done();
        true
    }
}

impl Screen for GameRulesScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.list
            .set_rect([0, TOP, width, (height - TOP - FOOTER).max(0)]);
        self.list.set_count(self.rows.len());
        self.widgets.clear();
        footer(&mut self.widgets, width, height);
        self.sync_done();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let title = cx.lang("editGamerule.title");
        cx.centered_text(&title, self.gui.0 / 2, 12, WHITE, true);
        let [lx, ly, lw, lh] = self.list.rect();
        tile_texture(cx, "menu_list_background", [lx, ly, lw, lh]);
        cx.frame.push_scissor(lx, ly, lx + lw, ly + lh);
        for index in self.list.visible_range() {
            let y = self.list.item_top(index);
            match self.rows[index] {
                RuleRow::Header(category) => {
                    let text = format!("§e§l{}", cx.lang(category.lang_key()));
                    cx.centered_text(&text, self.gui.0 / 2, y + 8, WHITE, true);
                }
                RuleRow::Rule(rule) => {
                    let name = lang_or(cx, &rule.lang_key(), &humanize(rule.id));
                    cx.text(&name, self.gui.0 / 2 - 155, y + 8, WHITE, true);
                    let rect = self.control_rect(index);
                    let hot = contains(rect, self.mouse.0, self.mouse.1);
                    let value = self.values.get(rule.id).cloned().unwrap_or_default();
                    match rule.kind {
                        Kind::Bool(_) => toggle_button(cx, rect, value == "true", hot),
                        Kind::Int { .. } => {
                            let [x, y0, w, h] = rect;
                            let editing = self.editing == Some(index);
                            let border = if editing { WHITE } else { 0xFFA0_A0A0 };
                            cx.frame.fill_rect(x, y0, x + w, y0 + h, border);
                            cx.frame
                                .fill_rect(x + 1, y0 + 1, x + w - 1, y0 + h - 1, 0xFF00_0000);
                            let color = if rule.normalize(&value).is_some() {
                                0xFFE0_E0E0
                            } else {
                                ERROR
                            };
                            let shown = if editing { format!("{value}_") } else { value };
                            cx.text(&shown, x + 4, y0 + 6, color, true);
                        }
                    }
                }
            }
        }
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, self.bar_x());
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if self.edit_key(input) {
            return true;
        }
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
                    self.button_events(events, out);
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
        self.button_events(events, out);
        used
    }
}

impl GameRulesScreen {
    fn button_events(&mut self, events: Vec<WidgetEvent>, out: &mut Vec<ScreenAction>) {
        for WidgetEvent { id, kind } in events {
            if kind != WidgetEventKind::Pressed {
                continue;
            }
            if id == DONE && !self.valid() {
                continue;
            }
            out.push(ScreenAction::ClickSound);
            if id == DONE && self.live.is_some() {
                let changes = self.live_changes();
                if !changes.is_empty() {
                    out.push(ScreenAction::SetGameRules(changes));
                }
            } else if id == DONE {
                self.extras.borrow_mut().game_rules = self
                    .changed()
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v))
                    .collect();
            }
            out.push(ScreenAction::Pop);
        }
    }
}

enum PackRow {
    Header(&'static str),
    Pack(String),
    Experiment(&'static str),
}

/// Data packs for a new world (from the store's folder, or a re-created world's own) and the experiments.
pub struct DataPacksScreen {
    extras: SharedExtras,
    store: Option<SharedWorlds>,
    dir: Option<PathBuf>,
    rows: Vec<PackRow>,
    packs: Vec<String>,
    experiments: Vec<String>,
    list: ScrollList,
    widgets: WidgetSet,
    gui: (i32, i32),
    mouse: (f32, f32),
}

impl std::fmt::Debug for DataPacksScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataPacksScreen")
            .field("packs", &self.packs)
            .field("experiments", &self.experiments)
            .finish_non_exhaustive()
    }
}

impl DataPacksScreen {
    pub fn new(extras: SharedExtras, store: Option<SharedWorlds>) -> Self {
        let (packs, experiments) = {
            let e = extras.borrow();
            (e.data_packs.clone(), e.experiments.clone())
        };
        let mut screen = Self {
            extras,
            store,
            dir: None,
            rows: Vec::new(),
            packs,
            experiments,
            list: ScrollList::new([0, TOP, 0, 0], ROW),
            widgets: WidgetSet::default(),
            gui: (0, 0),
            mouse: (0.0, 0.0),
        };
        screen.reload();
        screen
    }

    fn reload(&mut self) {
        let own = {
            let e = self.extras.borrow();
            e.pack_dir.clone().filter(|_| !e.data_packs.is_empty())
        };
        let (dir, available) = match (own, &self.store) {
            (Some(dir), store) => {
                let from_store = store
                    .as_ref()
                    .map(|s| s.borrow().data_packs())
                    .filter(|(d, _)| d.as_ref() == Some(&dir));
                match from_store {
                    Some((_, list)) => (Some(dir), list),
                    None => (Some(dir), self.extras.borrow().data_packs.clone()),
                }
            }
            (None, Some(store)) => store.borrow().data_packs(),
            (None, None) => (None, Vec::new()),
        };
        self.dir = dir;
        self.packs.retain(|p| available.contains(p));
        self.rows.clear();
        self.rows.push(PackRow::Header("dataPack.title"));
        self.rows.extend(available.into_iter().map(PackRow::Pack));
        self.rows.push(PackRow::Header("selectWorld.experiments"));
        self.rows.extend(
            mcr_data::FEATURE_PACKS_26_3
                .iter()
                .map(|e| PackRow::Experiment(e)),
        );
        self.list.set_count(self.rows.len());
    }

    /// Selected packs and experiments, as Done would store them.
    pub fn selection(&self) -> (Vec<String>, Vec<String>) {
        (self.packs.clone(), self.experiments.clone())
    }

    fn control_rect(&self, index: usize) -> [i32; 4] {
        [self.gui.0 / 2 + 55, self.list.item_top(index) + 2, 100, 20]
    }

    fn bar_x(&self) -> i32 {
        self.gui.0 / 2 + 160
    }

    fn toggle(list: &mut Vec<String>, name: &str) {
        if let Some(i) = list.iter().position(|p| p == name) {
            list.remove(i);
        } else {
            list.push(name.to_owned());
        }
    }

    fn click(&mut self, x: f32, y: f32, out: &mut Vec<ScreenAction>) -> bool {
        if !contains(self.list.rect(), x, y) {
            return false;
        }
        let Some(index) = self.list.item_at(x, y) else {
            return false;
        };
        if !contains(self.control_rect(index), x, y) {
            return true;
        }
        match &self.rows[index] {
            PackRow::Pack(name) => {
                let name = name.clone();
                Self::toggle(&mut self.packs, &name);
            }
            PackRow::Experiment(id) => Self::toggle(&mut self.experiments, id),
            PackRow::Header(_) => return true,
        }
        out.push(ScreenAction::ClickSound);
        true
    }

    fn button_events(&mut self, events: Vec<WidgetEvent>, out: &mut Vec<ScreenAction>) {
        for WidgetEvent { id, kind } in events {
            if kind != WidgetEventKind::Pressed {
                continue;
            }
            out.push(ScreenAction::ClickSound);
            match id {
                OPEN_FOLDER => {
                    if let Some(store) = &self.store {
                        store.borrow_mut().open_pack_folder();
                    }
                    self.reload();
                }
                DONE => {
                    let mut e = self.extras.borrow_mut();
                    e.data_packs.clone_from(&self.packs);
                    e.pack_dir.clone_from(&self.dir);
                    e.experiments.clone_from(&self.experiments);
                    out.push(ScreenAction::Pop);
                }
                _ => out.push(ScreenAction::Pop),
            }
        }
    }
}

impl Screen for DataPacksScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.list
            .set_rect([0, TOP, width, (height - TOP - FOOTER - 24).max(0)]);
        self.list.set_count(self.rows.len());
        self.widgets.clear();
        let mut open = Button::new(
            OPEN_FOLDER,
            [width / 2 - 75, height - 52, 150, 20],
            "lang:pack.openFolder",
        );
        open.active = self.store.is_some();
        self.widgets.add(open);
        footer(&mut self.widgets, width, height);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let title = cx.lang("selectWorld.dataPacks");
        cx.centered_text(&title, self.gui.0 / 2, 12, WHITE, true);
        let [lx, ly, lw, lh] = self.list.rect();
        tile_texture(cx, "menu_list_background", [lx, ly, lw, lh]);
        cx.frame.push_scissor(lx, ly, lx + lw, ly + lh);
        let left = self.gui.0 / 2 - 155;
        for index in self.list.visible_range() {
            let y = self.list.item_top(index);
            let rect = self.control_rect(index);
            let hot = contains(rect, self.mouse.0, self.mouse.1);
            match &self.rows[index] {
                PackRow::Header(key) => {
                    let text = format!("§e§l{}", cx.lang(key));
                    cx.centered_text(&text, self.gui.0 / 2, y + 8, WHITE, true);
                }
                PackRow::Pack(name) => {
                    cx.text(name, left, y + 8, WHITE, true);
                    let on = self.packs.contains(name);
                    toggle_button(cx, rect, on, hot);
                }
                PackRow::Experiment(id) => {
                    let name = lang_or(cx, &format!("dataPack.{id}.name"), &humanize(id));
                    cx.text(&name, left, y + 8, WHITE, true);
                    let on = self.experiments.iter().any(|e| e == id);
                    toggle_button(cx, rect, on, hot);
                }
            }
        }
        if self.rows.len() == 2 + mcr_data::FEATURE_PACKS_26_3.len() {
            let info = cx.lang("pack.folderInfo");
            cx.centered_text(
                &info,
                self.gui.0 / 2,
                self.list.item_top(0) + ROW + 8,
                LABEL,
                true,
            );
        }
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, self.bar_x());
        self.widgets.render(cx);
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
                    self.button_events(events, out);
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
        self.button_events(events, out);
        used
    }
}

/// Superflat layers as vanilla's preset string, checked by the store.
pub struct FlatLayersScreen {
    extras: SharedExtras,
    store: Option<SharedWorlds>,
    widgets: WidgetSet,
    gui: (i32, i32),
    error: Option<String>,
}

impl std::fmt::Debug for FlatLayersScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlatLayersScreen")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl FlatLayersScreen {
    pub fn new(extras: SharedExtras, store: Option<SharedWorlds>) -> Self {
        Self {
            extras,
            store,
            widgets: WidgetSet::default(),
            gui: (0, 0),
            error: None,
        }
    }

    fn text(&self) -> String {
        self.widgets
            .edit(LAYERS)
            .map(|e| e.value().trim().to_owned())
            .unwrap_or_default()
    }

    fn check(&mut self) {
        let text = self.text();
        self.error = match &self.store {
            Some(store) => store.borrow().check_flat(&text).err(),
            None => None,
        };
        if let Some(b) = self.widgets.button_mut(DONE) {
            b.active = self.error.is_none();
        }
    }
}

impl Screen for FlatLayersScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        let keep = self.widgets.edit(LAYERS).map(|e| e.value().to_owned());
        self.widgets.clear();
        let mut edit = EditBox::new(LAYERS, [width / 2 - 155, 60, 310, 20]).with_max_length(2048);
        let current = keep.unwrap_or_else(|| {
            self.extras
                .borrow()
                .flat
                .clone()
                .unwrap_or_else(|| CLASSIC_FLAT.to_owned())
        });
        edit.set_value(&current);
        edit.take_changed();
        self.widgets.add(edit);
        self.widgets.add(Button::new(
            PRESET,
            [width / 2 - 75, 108, 150, 20],
            "lang:flat_world_preset.minecraft.classic_flat",
        ));
        footer(&mut self.widgets, width, height);
        self.widgets.set_focus(Some(LAYERS), 0);
        self.check();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let title = cx.lang("createWorld.customize.flat.title");
        cx.centered_text(&title, self.gui.0 / 2, 15, WHITE, true);
        let info = cx.lang("createWorld.customize.presets.share");
        cx.text(&info, self.gui.0 / 2 - 155, 46, LABEL, true);
        if let Some(error) = &self.error {
            cx.centered_text(error, self.gui.0 / 2, 88, ERROR, true);
        }
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if let UiInput::Key {
            key: Key::Escape, ..
        } = input
        {
            out.push(ScreenAction::Pop);
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            match (id, kind) {
                (LAYERS, WidgetEventKind::Edited) => self.check(),
                (PRESET, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    if let Some(e) = self.widgets.edit_mut(LAYERS) {
                        e.set_value(CLASSIC_FLAT);
                    }
                    self.check();
                }
                (DONE, WidgetEventKind::Pressed) | (LAYERS, WidgetEventKind::Submitted) => {
                    self.check();
                    if self.error.is_none() {
                        out.push(ScreenAction::ClickSound);
                        let text = self.text();
                        self.extras.borrow_mut().flat = (text != CLASSIC_FLAT).then_some(text);
                        out.push(ScreenAction::Pop);
                    }
                }
                (CANCEL, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                _ => {}
            }
        }
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }
}
