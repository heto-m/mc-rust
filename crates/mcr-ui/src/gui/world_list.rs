use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::create_options::{CreateExtras, DataPacksScreen, FlatLayersScreen, GameRulesScreen};
use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, MouseButton, UiInput, contains};
use super::list::ScrollList;
use super::menus::{ConfirmScreen, WHITE, draw_menu_background, tile_texture};
use super::screen::{Screen, ScreenAction};
use super::server_list::{FAVICON_SIZE, Favicon};
use super::widgets::{Button, CycleButton, WidgetEvent, WidgetEventKind, WidgetSet};

const ROW_HEIGHT: i32 = 36;
const ROW_WIDTH: i32 = 270;
const HEADER: i32 = 33;
const FOOTER: i32 = 60;
const MAX_ICONS: usize = 64;
const GRAY: u32 = 0xFF80_8080;
const LABEL: u32 = 0xFFA0_A0A0;
pub const MAX_WORLD_NAME: usize = 32;
const MAX_SEED: usize = 32;

/// One world as the list shows it; `played` is already formatted for display.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldEntry {
    pub folder: String,
    pub name: String,
    pub played: String,
    /// `gameMode.*` lang key.
    pub mode_key: String,
    pub hardcore: bool,
    pub cheats: bool,
    pub version: String,
    pub icon: Option<Rc<Favicon>>,
}

pub trait WorldStore {
    fn worlds(&self) -> &[WorldEntry];
    fn refresh(&mut self);
    fn rename(&mut self, folder: &str, name: &str) -> Result<(), String>;
    fn delete(&mut self, folder: &str) -> Result<(), String>;

    /// Create New World filled from a world's saved settings, or why it cannot be re-created.
    fn recreate(&self, _folder: &str) -> Result<CreateWorldSettings, String> {
        Err("Re-Create is not available".into())
    }

    /// The folder new worlds take data packs from, and the packs in it.
    fn data_packs(&self) -> (Option<std::path::PathBuf>, Vec<String>) {
        (None, Vec::new())
    }

    fn open_pack_folder(&mut self) {}

    /// Checks superflat layers (a vanilla preset string) before Create.
    fn check_flat(&self, _layers: &str) -> Result<(), String> {
        Ok(())
    }
}

pub type SharedWorlds = Rc<RefCell<dyn WorldStore>>;

#[derive(Clone, Debug, Default)]
pub struct MemoryWorldStore {
    pub entries: Vec<WorldEntry>,
}

impl WorldStore for MemoryWorldStore {
    fn worlds(&self) -> &[WorldEntry] {
        &self.entries
    }

    fn refresh(&mut self) {}

    fn rename(&mut self, folder: &str, name: &str) -> Result<(), String> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.folder == folder)
            .ok_or("no such world")?;
        entry.name = name.to_owned();
        Ok(())
    }

    fn delete(&mut self, folder: &str) -> Result<(), String> {
        let before = self.entries.len();
        self.entries.retain(|e| e.folder != folder);
        if self.entries.len() == before {
            return Err("no such world".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NewGameMode {
    #[default]
    Survival,
    Hardcore,
    Creative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WorldPreset {
    #[default]
    Normal,
    Flat,
    LargeBiomes,
    Amplified,
}

/// The Create New World screen's result; `difficulty` is 0 (peaceful) to 3 (hard).
#[derive(Clone, Debug, PartialEq)]
pub struct CreateWorldSettings {
    pub name: String,
    pub mode: NewGameMode,
    pub difficulty: u8,
    pub cheats: bool,
    pub preset: WorldPreset,
    pub seed: String,
    pub structures: bool,
    pub bonus_chest: bool,
    /// Game rules that differ from the default, as `(id, value)`.
    pub game_rules: Vec<(String, String)>,
    /// Experimental feature packs to enable.
    pub experiments: Vec<String>,
    /// Data packs from `pack_dir` to copy into the new world.
    pub data_packs: Vec<String>,
    pub pack_dir: Option<std::path::PathBuf>,
    /// Superflat layers: a vanilla preset string or the generator JSON; `None` for the default.
    pub flat: Option<String>,
}

impl Default for CreateWorldSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            mode: NewGameMode::Survival,
            difficulty: 2,
            cheats: false,
            preset: WorldPreset::Normal,
            seed: String::new(),
            structures: true,
            bonus_chest: false,
            game_rules: Vec::new(),
            experiments: Vec::new(),
            data_packs: Vec::new(),
            pack_dir: None,
            flat: None,
        }
    }
}

const PLAY: u32 = 1;
const CREATE: u32 = 2;
const EDIT: u32 = 3;
const DELETE: u32 = 4;
const BACK: u32 = 5;
const RECREATE: u32 = 6;

pub struct SelectWorldScreen {
    worlds: SharedWorlds,
    widgets: WidgetSet,
    gui: (i32, i32),
    selected: Option<usize>,
    list: ScrollList,
    mouse: (f32, f32),
    icons: RefCell<HashMap<u64, Option<crate::SpriteId>>>,
    releaser: RefCell<Option<crate::SpriteReleaser>>,
}

impl Drop for SelectWorldScreen {
    fn drop(&mut self) {
        if let Some(releaser) = self.releaser.get_mut() {
            releaser.release(self.icons.get_mut().drain().filter_map(|(_, id)| id));
        }
    }
}

impl std::fmt::Debug for SelectWorldScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectWorldScreen")
            .field("selected", &self.selected)
            .finish()
    }
}

impl SelectWorldScreen {
    pub fn new(worlds: SharedWorlds) -> Self {
        worlds.borrow_mut().refresh();
        Self {
            worlds,
            widgets: WidgetSet::default(),
            gui: (0, 0),
            selected: None,
            list: ScrollList::new([0, HEADER, 0, 0], ROW_HEIGHT),
            mouse: (0.0, 0.0),
            icons: RefCell::default(),
            releaser: RefCell::default(),
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn select(&mut self, index: Option<usize>) {
        let n = self.count();
        self.selected = index.filter(|&i| i < n);
        self.update_buttons();
    }

    fn count(&self) -> usize {
        self.worlds.borrow().worlds().len()
    }

    fn entry(&self, index: usize) -> Option<WorldEntry> {
        self.worlds.borrow().worlds().get(index).cloned()
    }

    fn row_left(&self) -> i32 {
        self.gui.0 / 2 - ROW_WIDTH / 2
    }

    fn bar_x(&self) -> i32 {
        self.gui.0 / 2 + ROW_WIDTH / 2 + 6
    }

    fn sync_list(&mut self) {
        let count = self.count();
        self.list
            .set_rect([0, HEADER, self.gui.0, (self.gui.1 - HEADER - FOOTER).max(0)]);
        self.list.set_count(count);
    }

    fn update_buttons(&mut self) {
        let has = self.selected.is_some();
        for id in [PLAY, EDIT, DELETE, RECREATE] {
            if let Some(b) = self.widgets.button_mut(id) {
                b.active = has;
            }
        }
    }

    fn play(&self, index: usize, out: &mut Vec<ScreenAction>) {
        if let Some(entry) = self.entry(index) {
            out.push(ScreenAction::PlayWorld(entry.folder));
        }
    }

    fn delete_prompt(&self, entry: WorldEntry) -> ConfirmScreen {
        let store = self.worlds.clone();
        let folder = entry.folder.clone();
        ConfirmScreen::new(
            "lang:selectWorld.deleteQuestion",
            &format!("lang:selectWorld.deleteWarning|{}", entry.name),
            vec![
                (
                    "lang:selectWorld.deleteButton".into(),
                    Box::new(move || {
                        let result = store.borrow_mut().delete(&folder);
                        store.borrow_mut().refresh();
                        match result {
                            Ok(()) => vec![ScreenAction::Pop],
                            Err(message) => vec![
                                ScreenAction::Pop,
                                ScreenAction::Push(Box::new(ConfirmScreen::new(
                                    "lang:selectWorld.delete_failure",
                                    &message,
                                    vec![(
                                        "lang:gui.back".into(),
                                        Box::new(|| vec![ScreenAction::Pop]),
                                    )],
                                ))),
                            ],
                        }
                    }),
                ),
                (
                    "lang:gui.cancel".into(),
                    Box::new(|| vec![ScreenAction::Pop]),
                ),
            ],
        )
    }

    fn button_events(&mut self, events: Vec<WidgetEvent>, out: &mut Vec<ScreenAction>) {
        for WidgetEvent { id, kind } in events {
            if kind != WidgetEventKind::Pressed {
                continue;
            }
            out.push(ScreenAction::ClickSound);
            match id {
                PLAY => {
                    if let Some(i) = self.selected {
                        self.play(i, out);
                    }
                }
                CREATE => out.push(ScreenAction::Push(Box::new(
                    CreateWorldScreen::new().with_store(self.worlds.clone()),
                ))),
                RECREATE => {
                    if let Some(entry) = self.selected.and_then(|i| self.entry(i)) {
                        let result = self.worlds.borrow().recreate(&entry.folder);
                        let screen: Box<dyn Screen> = match result {
                            Ok(settings) => Box::new(
                                CreateWorldScreen::with_settings(settings)
                                    .with_store(self.worlds.clone()),
                            ),
                            Err(message) => Box::new(ConfirmScreen::new(
                                "lang:selectWorld.recreate.error.title",
                                &message,
                                vec![(
                                    "lang:gui.back".into(),
                                    Box::new(|| vec![ScreenAction::Pop]),
                                )],
                            )),
                        };
                        out.push(ScreenAction::Push(screen));
                    }
                }
                EDIT => {
                    if let Some(entry) = self.selected.and_then(|i| self.entry(i)) {
                        out.push(ScreenAction::Push(Box::new(EditWorldScreen::new(
                            self.worlds.clone(),
                            entry,
                        ))));
                    }
                }
                DELETE => {
                    if let Some(entry) = self.selected.and_then(|i| self.entry(i)) {
                        out.push(ScreenAction::Push(Box::new(self.delete_prompt(entry))));
                    }
                }
                BACK => out.push(ScreenAction::Pop),
                _ => {}
            }
        }
    }

    fn icon(&self, cx: &mut RenderCx<'_, '_>, favicon: &Favicon) -> Option<crate::SpriteId> {
        self.releaser
            .borrow_mut()
            .get_or_insert_with(|| cx.frame.ui().sprites_mut().releaser());
        let mut cache = self.icons.borrow_mut();
        if !cache.contains_key(&favicon.id) && cache.len() >= MAX_ICONS {
            for (_, id) in cache.drain() {
                if let Some(id) = id {
                    cx.frame.ui().sprites_mut().free_raw(id);
                }
            }
        }
        *cache.entry(favicon.id).or_insert_with(|| {
            cx.frame
                .ui()
                .sprites_mut()
                .add_raw(FAVICON_SIZE, FAVICON_SIZE, &favicon.rgba)
        })
    }

    fn draw_row(&self, cx: &mut RenderCx<'_, '_>, index: usize) {
        let Some(entry) = self.entry(index) else {
            return;
        };
        let left = self.row_left();
        let top = self.list.item_top(index);
        let (x, y) = (left + 2, top + 2);
        let (mx, my) = self.mouse;
        let hovered = contains([left, top, ROW_WIDTH, ROW_HEIGHT], mx, my)
            && contains(self.list.rect(), mx, my);
        if self.selected == Some(index) {
            cx.frame.fill_rect(
                left - 2,
                top - 2,
                left + ROW_WIDTH + 2,
                top + ROW_HEIGHT,
                0xFF80_8080,
            );
            cx.frame.fill_rect(
                left - 1,
                top - 1,
                left + ROW_WIDTH + 1,
                top + ROW_HEIGHT - 1,
                0xFF00_0000,
            );
        }
        match entry.icon.as_ref().and_then(|f| self.icon(cx, f)) {
            Some(id) => cx.frame.sprite(id, x, y, 32, 32),
            None => cx.frame.fill_rect(x, y, x + 32, y + 32, 0xFF3A_3A3A),
        }
        let width = (ROW_WIDTH - 32 - 6) as f32;
        let name = clip(cx, &entry.name, width);
        cx.text(&name, x + 35, y + 1, WHITE, true);
        let folder = clip(cx, &format!("{} ({})", entry.folder, entry.played), width);
        cx.text(&folder, x + 35, y + 12, GRAY, true);
        let mut info = if entry.hardcore {
            format!("§c{}§r", cx.lang("gameMode.hardcore"))
        } else {
            cx.lang(&entry.mode_key)
        };
        if entry.cheats {
            info.push_str(", ");
            info.push_str(&cx.lang("selectWorld.cheats"));
        }
        if !entry.version.is_empty() {
            info.push_str(", ");
            info.push_str(&entry.version);
        }
        let info = clip(cx, &info, width);
        cx.text(&info, x + 35, y + 22, GRAY, true);
        if hovered {
            cx.frame.fill_rect(x, y, x + 32, y + 32, 0xA0B0_B0B0);
            let in_icon = (mx as i32 - x) < 32;
            let sprite = if in_icon {
                "world_list/join_highlighted"
            } else {
                "world_list/join"
            };
            cx.sprite(sprite, x, y, 32, 32);
        }
    }
}

fn clip(cx: &mut RenderCx<'_, '_>, text: &str, max: f32) -> String {
    if cx.text_width(text) as f32 <= max {
        return text.to_owned();
    }
    let mut out = String::new();
    for c in text.chars() {
        out.push(c);
        if cx.text_width(&out) as f32 + 6.0 > max {
            out.pop();
            break;
        }
    }
    out.push_str("...");
    out
}

impl Screen for SelectWorldScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let x = width / 2 - 154;
        let (y1, y2) = (height - 52, height - 28);
        let w = &mut self.widgets;
        w.add(Button::new(
            PLAY,
            [x, y1, 150, 20],
            "lang:selectWorld.select",
        ));
        w.add(Button::new(
            CREATE,
            [x + 154, y1, 150, 20],
            "lang:selectWorld.create",
        ));
        let x2 = width / 2 - 150;
        w.add(Button::new(EDIT, [x2, y2, 72, 20], "lang:selectWorld.edit"));
        w.add(Button::new(
            DELETE,
            [x2 + 76, y2, 72, 20],
            "lang:selectWorld.delete",
        ));
        w.add(Button::new(
            RECREATE,
            [x2 + 152, y2, 72, 20],
            "lang:selectWorld.recreate",
        ));
        w.add(Button::new(BACK, [x2 + 228, y2, 72, 20], "lang:gui.back"));
        self.sync_list();
        self.update_buttons();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.mouse = cx.mouse;
        draw_menu_background(cx);
        if self.selected.is_some_and(|i| i >= self.count()) {
            self.selected = None;
            self.update_buttons();
        }
        self.sync_list();
        let [lx, ly, lw, lh] = self.list.rect();
        tile_texture(cx, "menu_list_background", [lx, ly, lw, lh]);
        cx.frame.push_scissor(lx, ly, lx + lw, ly + lh);
        for i in self.list.visible_range() {
            self.draw_row(cx, i);
        }
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, self.bar_x());
        if let Some(id) = cx.assets.texture("header_separator") {
            for x in (0..self.gui.0).step_by(32) {
                cx.frame.sprite(id, x, HEADER - 2, 32, 2);
                cx.frame.sprite(id, x, self.gui.1 - FOOTER, 32, 2);
            }
        }
        let title = cx.lang("selectWorld.title");
        cx.centered_text(&title, self.gui.0 / 2, 12, WHITE, true);
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
            UiInput::Key {
                key: key @ (Key::Up | Key::Down),
                ..
            } if self.count() > 0 => {
                let last = self.count() - 1;
                let next = match (self.selected, key == Key::Down) {
                    (None, true) => 0,
                    (None, false) => last,
                    (Some(i), true) => (i + 1).min(last),
                    (Some(i), false) => i.saturating_sub(1),
                };
                self.select(Some(next));
                self.list.ensure_visible(next);
                return true;
            }
            UiInput::Key {
                key: Key::Enter, ..
            } => {
                let mut events = Vec::new();
                if self.widgets.input(env, input, &mut events) && !events.is_empty() {
                    self.button_events(events, out);
                    return true;
                }
                if let Some(i) = self.selected {
                    self.play(i, out);
                    return true;
                }
                return false;
            }
            UiInput::MousePress {
                x,
                y,
                button: MouseButton::Left,
                double,
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
                let left = self.row_left() as f32;
                if x < left || x >= left + ROW_WIDTH as f32 {
                    return false;
                }
                if let Some(index) = self.list.item_at(x, y) {
                    let in_icon = (x - left - 2.0) < 32.0;
                    let was = self.selected == Some(index);
                    self.select(Some(index));
                    if in_icon || (double && was) {
                        self.play(index, out);
                    }
                    return true;
                }
                return false;
            }
            UiInput::MouseMove { x, y } => {
                self.mouse = (x, y);
                if self.list.input(input, self.bar_x()) {
                    return true;
                }
            }
            UiInput::MouseRelease { .. } => {
                self.list.input(input, self.bar_x());
            }
            UiInput::Scroll { .. } => return self.list.input(input, self.bar_x()),
            _ => {}
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        self.button_events(events, out);
        used
    }
}

const NAME: u32 = 1;
const SAVE: u32 = 2;
const CANCEL: u32 = 3;

pub struct EditWorldScreen {
    worlds: SharedWorlds,
    entry: WorldEntry,
    widgets: WidgetSet,
    gui: (i32, i32),
    error: Option<String>,
}

impl std::fmt::Debug for EditWorldScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditWorldScreen")
            .field("folder", &self.entry.folder)
            .finish()
    }
}

impl EditWorldScreen {
    pub fn new(worlds: SharedWorlds, entry: WorldEntry) -> Self {
        Self {
            worlds,
            entry,
            widgets: WidgetSet::default(),
            gui: (0, 0),
            error: None,
        }
    }

    fn name(&self) -> String {
        self.widgets
            .edit(NAME)
            .map(|e| e.value().trim().to_owned())
            .unwrap_or_default()
    }

    fn save(&mut self, out: &mut Vec<ScreenAction>) {
        let name = self.name();
        if name.is_empty() {
            return;
        }
        let result = self.worlds.borrow_mut().rename(&self.entry.folder, &name);
        self.worlds.borrow_mut().refresh();
        match result {
            Ok(()) => out.push(ScreenAction::Pop),
            Err(e) => self.error = Some(e),
        }
    }
}

impl Screen for EditWorldScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        let keep = self.widgets.edit(NAME).map(|e| e.value().to_owned());
        self.widgets.clear();
        let mut name =
            EditBox::new(NAME, [width / 2 - 100, 60, 200, 20]).with_max_length(MAX_WORLD_NAME);
        name.set_value(&keep.unwrap_or_else(|| self.entry.name.clone()));
        name.take_changed();
        self.widgets.add(name);
        let y = (height / 4 + 96).max(96);
        self.widgets.add(Button::new(
            SAVE,
            [width / 2 - 100, y, 200, 20],
            "lang:selectWorld.edit.save",
        ));
        self.widgets.add(Button::new(
            CANCEL,
            [width / 2 - 100, y + 24, 200, 20],
            "lang:gui.cancel",
        ));
        self.widgets.set_focus(Some(NAME), 0);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let title = cx.lang("selectWorld.edit.title");
        cx.centered_text(&title, self.gui.0 / 2, 15, WHITE, true);
        let label = cx.lang("selectWorld.enterName");
        cx.text(&label, self.gui.0 / 2 - 100, 47, LABEL, true);
        if let Some(error) = &self.error {
            let error = format!("§c{error}");
            cx.centered_text(&error, self.gui.0 / 2, 86, WHITE, true);
        }
        let empty = self.name().is_empty();
        if let Some(b) = self.widgets.button_mut(SAVE) {
            b.active = !empty;
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
                (SAVE, WidgetEventKind::Pressed) | (NAME, WidgetEventKind::Submitted) => {
                    out.push(ScreenAction::ClickSound);
                    self.save(out);
                }
                (CANCEL, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                _ => {}
            }
        }
        used
    }
}

const C_NAME: u32 = 1;
const C_MODE: u32 = 2;
const C_DIFFICULTY: u32 = 3;
const C_CHEATS: u32 = 4;
const C_TYPE: u32 = 5;
const C_STRUCTURES: u32 = 6;
const C_BONUS: u32 = 7;
const C_SEED: u32 = 8;
const C_CREATE: u32 = 9;
const C_CANCEL: u32 = 10;
const C_RULES: u32 = 11;
const C_PACKS: u32 = 12;
const C_CUSTOMIZE: u32 = 13;

const MODES: [NewGameMode; 3] = [
    NewGameMode::Survival,
    NewGameMode::Hardcore,
    NewGameMode::Creative,
];
const PRESETS: [WorldPreset; 4] = [
    WorldPreset::Normal,
    WorldPreset::Flat,
    WorldPreset::LargeBiomes,
    WorldPreset::Amplified,
];

fn on_off() -> Vec<String> {
    vec!["lang:options.on".into(), "lang:options.off".into()]
}

pub struct CreateWorldScreen {
    widgets: WidgetSet,
    gui: (i32, i32),
    settings: CreateWorldSettings,
    /// Game rules, packs and superflat layers, shared with the screens that edit them.
    extras: Rc<RefCell<CreateExtras>>,
    store: Option<SharedWorlds>,
    /// Cheats follow the game mode until the player sets them.
    cheats_touched: bool,
}

impl std::fmt::Debug for CreateWorldScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateWorldScreen")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl Default for CreateWorldScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl CreateWorldScreen {
    pub fn new() -> Self {
        Self {
            widgets: WidgetSet::default(),
            gui: (0, 0),
            settings: CreateWorldSettings::default(),
            extras: Rc::default(),
            store: None,
            cheats_touched: false,
        }
    }

    /// Filled in, as Re-Create does; cheats keep the given value.
    pub fn with_settings(mut settings: CreateWorldSettings) -> Self {
        let extras = CreateExtras {
            game_rules: std::mem::take(&mut settings.game_rules),
            experiments: std::mem::take(&mut settings.experiments),
            data_packs: std::mem::take(&mut settings.data_packs),
            pack_dir: settings.pack_dir.take(),
            flat: settings.flat.take(),
        };
        Self {
            settings,
            extras: Rc::new(RefCell::new(extras)),
            cheats_touched: true,
            ..Self::new()
        }
    }

    /// The store offering data packs and checking superflat layers.
    pub fn with_store(mut self, store: SharedWorlds) -> Self {
        self.store = Some(store);
        self
    }

    pub fn settings(&self) -> &CreateWorldSettings {
        &self.settings
    }

    fn read_edits(&mut self) {
        if let Some(e) = self.widgets.edit(C_NAME) {
            self.settings.name = e.value().to_owned();
        }
        if let Some(e) = self.widgets.edit(C_SEED) {
            self.settings.seed = e.value().to_owned();
        }
    }

    fn apply(&mut self, id: u32, index: usize) {
        let s = &mut self.settings;
        match id {
            C_MODE => {
                s.mode = MODES[index.min(MODES.len() - 1)];
                if !self.cheats_touched {
                    s.cheats = s.mode == NewGameMode::Creative;
                }
            }
            C_DIFFICULTY => s.difficulty = index.min(3) as u8,
            C_CHEATS => {
                s.cheats = index == 0;
                self.cheats_touched = true;
            }
            C_TYPE => {
                s.preset = PRESETS[index.min(PRESETS.len() - 1)];
                if let Some(b) = self.widgets.button_mut(C_CUSTOMIZE) {
                    b.active = s.preset == WorldPreset::Flat;
                }
            }
            C_STRUCTURES => s.structures = index == 0,
            C_BONUS => s.bonus_chest = index == 0,
            _ => {}
        }
        self.sync_widgets();
    }

    fn sync_widgets(&mut self) {
        let hardcore = self.settings.mode == NewGameMode::Hardcore;
        let difficulty = if hardcore {
            3
        } else {
            usize::from(self.settings.difficulty)
        };
        let cheats = self.settings.cheats && !hardcore;
        if let Some(c) = self.widgets.cycle_mut(C_DIFFICULTY) {
            c.active = !hardcore;
            c.cycle(difficulty as i32 - c.index() as i32);
        }
        if let Some(c) = self.widgets.cycle_mut(C_CHEATS) {
            c.active = !hardcore;
            let want = usize::from(!cheats);
            c.cycle(want as i32 - c.index() as i32);
        }
    }

    /// The settings as they will be used: hardcore forces hard and no cheats.
    pub fn result(&self) -> CreateWorldSettings {
        let mut s = self.settings.clone();
        s.name = s.name.trim().to_owned();
        if s.name.is_empty() {
            s.name = "New World".into();
        }
        s.seed = s.seed.trim().to_owned();
        if s.mode == NewGameMode::Hardcore {
            s.difficulty = 3;
            s.cheats = false;
        }
        let extras = self.extras.borrow();
        s.game_rules.clone_from(&extras.game_rules);
        s.experiments.clone_from(&extras.experiments);
        s.data_packs.clone_from(&extras.data_packs);
        s.pack_dir = (!s.data_packs.is_empty())
            .then(|| extras.pack_dir.clone())
            .flatten();
        s.flat = extras
            .flat
            .clone()
            .filter(|_| s.preset == WorldPreset::Flat);
        s
    }
}

impl Screen for CreateWorldScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.read_edits();
        self.widgets.clear();
        let left = width / 2 - 155;
        let right = width / 2 + 5;
        let s = self.settings.clone();
        let mut name = EditBox::new(C_NAME, [left, 44, 310, 20]).with_max_length(MAX_WORLD_NAME);
        name.set_value(if s.name.is_empty() && !self.cheats_touched {
            ""
        } else {
            &s.name
        });
        name.hint = Some("lang:selectWorld.newWorld".into());
        name.take_changed();
        self.widgets.add(name);
        let mode = MODES.iter().position(|m| *m == s.mode).unwrap_or(0);
        self.widgets.add(CycleButton::new(
            C_MODE,
            [left, 72, 150, 20],
            "lang:selectWorld.gameMode",
            vec![
                "lang:selectWorld.gameMode.survival".into(),
                "lang:selectWorld.gameMode.hardcore".into(),
                "lang:selectWorld.gameMode.creative".into(),
            ],
            mode,
        ));
        self.widgets.add(CycleButton::new(
            C_DIFFICULTY,
            [left, 96, 150, 20],
            "lang:options.difficulty",
            vec![
                "lang:options.difficulty.peaceful".into(),
                "lang:options.difficulty.easy".into(),
                "lang:options.difficulty.normal".into(),
                "lang:options.difficulty.hard".into(),
            ],
            usize::from(s.difficulty),
        ));
        self.widgets.add(CycleButton::new(
            C_CHEATS,
            [left, 120, 150, 20],
            "lang:selectWorld.allowCommands",
            on_off(),
            usize::from(!s.cheats),
        ));
        let preset = PRESETS.iter().position(|p| *p == s.preset).unwrap_or(0);
        self.widgets.add(CycleButton::new(
            C_TYPE,
            [right, 72, 150, 20],
            "lang:selectWorld.mapType",
            vec![
                "lang:generator.minecraft.normal".into(),
                "lang:generator.minecraft.flat".into(),
                "lang:generator.minecraft.large_biomes".into(),
                "lang:generator.minecraft.amplified".into(),
            ],
            preset,
        ));
        self.widgets.add(CycleButton::new(
            C_STRUCTURES,
            [right, 96, 150, 20],
            "lang:selectWorld.mapFeatures",
            on_off(),
            usize::from(!s.structures),
        ));
        self.widgets.add(CycleButton::new(
            C_BONUS,
            [right, 120, 150, 20],
            "lang:selectWorld.bonusItems",
            on_off(),
            usize::from(!s.bonus_chest),
        ));
        let mut seed = EditBox::new(C_SEED, [left, 160, 310, 20]).with_max_length(MAX_SEED);
        seed.set_value(&s.seed);
        seed.hint = Some("lang:selectWorld.seedInfo".into());
        seed.take_changed();
        self.widgets.add(seed);
        self.widgets.add(Button::new(
            C_RULES,
            [left, 186, 100, 20],
            "lang:selectWorld.gameRules",
        ));
        self.widgets.add(Button::new(
            C_PACKS,
            [left + 105, 186, 100, 20],
            "lang:selectWorld.dataPacks",
        ));
        let mut customize = Button::new(
            C_CUSTOMIZE,
            [left + 210, 186, 100, 20],
            "lang:selectWorld.customizeType",
        );
        customize.active = s.preset == WorldPreset::Flat;
        self.widgets.add(customize);
        let y = (height - 28).max(212);
        self.widgets.add(Button::new(
            C_CREATE,
            [left, y, 150, 20],
            "lang:selectWorld.create",
        ));
        self.widgets.add(Button::new(
            C_CANCEL,
            [right, y, 150, 20],
            "lang:gui.cancel",
        ));
        self.sync_widgets();
        self.widgets.set_focus(Some(C_NAME), 0);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let center = self.gui.0 / 2;
        let title = cx.lang("selectWorld.create");
        cx.centered_text(&title, center, 15, WHITE, true);
        let name = cx.lang("selectWorld.enterName");
        cx.text(&name, center - 155, 32, LABEL, true);
        let seed = cx.lang("selectWorld.enterSeed");
        cx.text(&seed, center - 155, 148, LABEL, true);
        for (id, key) in [
            (C_NAME, "selectWorld.newWorld"),
            (C_SEED, "selectWorld.seedInfo"),
        ] {
            let hint = cx.lang(key);
            if let Some(e) = self.widgets.edit_mut(id) {
                e.hint = Some(hint);
            }
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
        self.read_edits();
        for WidgetEvent { id, kind } in events {
            match kind {
                WidgetEventKind::Cycled(index) => {
                    out.push(ScreenAction::ClickSound);
                    self.apply(id, index);
                }
                WidgetEventKind::Pressed if id == C_CREATE => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::CreateWorld(Box::new(self.result())));
                }
                WidgetEventKind::Submitted if id == C_NAME || id == C_SEED => {
                    out.push(ScreenAction::CreateWorld(Box::new(self.result())));
                }
                WidgetEventKind::Pressed if id == C_CANCEL => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                WidgetEventKind::Pressed if id == C_RULES => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Push(Box::new(GameRulesScreen::new(
                        self.extras.clone(),
                    ))));
                }
                WidgetEventKind::Pressed if id == C_PACKS => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Push(Box::new(DataPacksScreen::new(
                        self.extras.clone(),
                        self.store.clone(),
                    ))));
                }
                WidgetEventKind::Pressed if id == C_CUSTOMIZE => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Push(Box::new(FlatLayersScreen::new(
                        self.extras.clone(),
                        self.store.clone(),
                    ))));
                }
                _ => {}
            }
        }
        used
    }
}
