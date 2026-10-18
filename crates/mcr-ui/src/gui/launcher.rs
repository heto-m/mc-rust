//! The launcher view shown before the title screen: profiles, accounts and Play.

use std::cell::RefCell;
use std::rc::Rc;

use mcr_protocol::types::item::ItemStack;

use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, MouseButton, UiInput, contains};
use super::list::ScrollList;
use super::menus::{ConfirmScreen, WHITE, draw_menu_background};
use super::screen::{Screen, ScreenAction};
use super::widgets::{Button, Slider, WidgetEvent, WidgetEventKind, WidgetSet};

const GRAY: u32 = 0xFFA0_A0A0;
const DARK: u32 = 0xFF70_7070;
const RED: u32 = 0xFFFF_5555;
const GREEN: u32 = 0xFF55_FF55;
const YELLOW: u32 = 0xFFFF_FF55;
const MAX_ROWS: usize = 4096;

/// A profile as the launcher lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherProfileRow {
    pub id: String,
    pub name: String,
    /// An item id from the launcher's icon set.
    pub icon: String,
    /// "Latest release (26.3)" or the version id.
    pub version: String,
    pub playable: bool,
    /// "not supported yet", or empty.
    pub note: String,
    pub last_played: String,
    pub mods: bool,
    pub default: bool,
}

/// The fields the profile editor changes; paths and resolution as typed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProfileDraft {
    /// `None` for a new profile.
    pub id: Option<String>,
    pub name: String,
    pub icon: String,
    pub version: String,
    pub game_dir: String,
    pub mods: String,
    pub heap_mib: u32,
    pub resolution: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRow {
    pub id: String,
    pub label: String,
    pub snapshot: bool,
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRow {
    pub key: String,
    pub name: String,
    pub microsoft: bool,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInView {
    Idle,
    Starting,
    Code { user_code: String, url: String },
    Failed(String),
    Done(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchView {
    Idle,
    Working(String),
    Failed(String),
}

/// What the launcher screens need from the app.
pub trait LauncherModel {
    fn profiles(&self) -> Vec<LauncherProfileRow>;
    fn selected(&self) -> String;
    fn select(&mut self, id: &str);
    fn can_add(&self) -> bool;
    /// The editor's starting values: a new profile for `None`.
    fn draft(&self, id: Option<&str>) -> ProfileDraft;
    /// Saves an edited or new profile, or says why it cannot be saved.
    fn save_profile(&mut self, draft: ProfileDraft) -> Result<(), String>;
    fn duplicate(&mut self, id: &str);
    fn delete(&mut self, id: &str);
    fn icons(&self) -> Vec<String>;
    /// The first row is the latest release; unsupported rows come greyed.
    fn versions(&self) -> Vec<VersionRow>;
    fn version_label(&self, version: &str) -> String;
    fn show_snapshots(&self) -> bool;
    fn set_show_snapshots(&mut self, on: bool);
    fn heap_range(&self) -> (u32, u32);
    fn default_game_dir(&self) -> String;
    fn accounts(&self) -> Vec<AccountRow>;
    fn set_active_account(&mut self, key: &str);
    /// Why `name` cannot be an offline account, if it cannot.
    fn offline_problem(&self, name: &str) -> Option<String>;
    fn add_offline(&mut self, name: &str);
    fn remove_account(&mut self, key: &str);
    /// Why Microsoft sign-in is unavailable, if it is.
    fn microsoft_problem(&self) -> Option<String>;
    fn start_sign_in(&mut self);
    fn sign_in(&mut self) -> SignInView;
    fn cancel_sign_in(&mut self);
    fn play(&mut self);
    fn launch(&mut self) -> LaunchView;
    /// A line for the bottom of the launcher (version list state, newer unsupported release).
    fn notice(&self) -> Option<String>;
    /// Runs background polling once per tick.
    fn tick(&mut self);
}

pub type SharedLauncher = Rc<RefCell<dyn LauncherModel>>;

fn icon_stack(icon: &str) -> Option<ItemStack> {
    let item = mcr_data::v26_3().item_by_name(&format!("minecraft:{icon}"))?;
    Some(ItemStack::new(i32::try_from(item).ok()?, 1))
}

fn draw_icon(cx: &mut RenderCx<'_, '_>, icon: &str, x: i32, y: i32, size: i32) {
    if let Some(stack) = icon_stack(icon) {
        cx.fake_item_sized(&stack, x, y, size);
    }
}

fn escape(input: &UiInput) -> bool {
    matches!(
        input,
        UiInput::Key {
            key: Key::Escape,
            ..
        }
    )
}

const L_NEW: u32 = 1;
const L_EDIT: u32 = 2;
const L_COPY: u32 = 3;
const L_DELETE: u32 = 4;
const L_ACCOUNT: u32 = 5;
const L_PLAY: u32 = 6;
const L_QUIT: u32 = 7;
const ROW_H: i32 = 36;
const LIST_W: i32 = 300;
const LIST_TOP: i32 = 50;
const FOOTER: i32 = 70;

/// The first screen: pick a profile and an account, then Play.
pub struct LauncherScreen {
    model: SharedLauncher,
    rows: Vec<LauncherProfileRow>,
    widgets: WidgetSet,
    gui: (i32, i32),
    list: ScrollList,
    mouse: (f32, f32),
}

impl std::fmt::Debug for LauncherScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LauncherScreen")
            .field("rows", &self.rows.len())
            .finish()
    }
}

impl LauncherScreen {
    pub fn new(model: SharedLauncher) -> Self {
        Self {
            model,
            rows: Vec::new(),
            widgets: WidgetSet::default(),
            gui: (0, 0),
            list: ScrollList::new([0, LIST_TOP, 0, 0], ROW_H),
            mouse: (0.0, 0.0),
        }
    }

    pub fn rows(&self) -> &[LauncherProfileRow] {
        &self.rows
    }

    fn left(&self) -> i32 {
        self.gui.0 / 2 - LIST_W / 2
    }

    fn refresh(&mut self) {
        let mut rows = self.model.borrow().profiles();
        rows.truncate(MAX_ROWS);
        self.rows = rows;
        self.list.set_rect([
            0,
            LIST_TOP,
            self.gui.0,
            (self.gui.1 - LIST_TOP - FOOTER).max(ROW_H),
        ]);
        self.list.set_count(self.rows.len());
        self.sync_buttons();
    }

    fn selected_row(&self) -> Option<&LauncherProfileRow> {
        let id = self.model.borrow().selected();
        self.rows.iter().find(|r| r.id == id)
    }

    fn sync_buttons(&mut self) {
        let selected = self.selected_row().cloned();
        let launching = matches!(self.model.borrow_mut().launch(), LaunchView::Working(_));
        let can_add = self.model.borrow().can_add();
        let account = self
            .model
            .borrow()
            .accounts()
            .into_iter()
            .find(|a| a.active)
            .map_or_else(|| "Accounts".to_owned(), |a| a.name);
        let set = |w: &mut WidgetSet, id: u32, on: bool| {
            if let Some(b) = w.button_mut(id) {
                b.active = on && !launching;
            }
        };
        set(&mut self.widgets, L_NEW, can_add);
        set(&mut self.widgets, L_EDIT, selected.is_some());
        set(&mut self.widgets, L_COPY, selected.is_some() && can_add);
        set(
            &mut self.widgets,
            L_DELETE,
            selected.as_ref().is_some_and(|r| !r.default),
        );
        set(&mut self.widgets, L_ACCOUNT, true);
        set(
            &mut self.widgets,
            L_PLAY,
            selected.as_ref().is_some_and(|r| r.playable),
        );
        if let Some(b) = self.widgets.button_mut(L_ACCOUNT) {
            b.label = account;
        }
    }

    fn draw_row(&self, cx: &mut RenderCx<'_, '_>, index: usize, selected: &str) {
        let row = &self.rows[index];
        let left = self.left();
        let top = self.list.item_top(index);
        if row.id == selected {
            cx.frame
                .fill_rect(left, top, left + LIST_W, top + ROW_H, 0xFF80_8080);
            cx.frame.fill_rect(
                left + 1,
                top + 1,
                left + LIST_W - 1,
                top + ROW_H - 1,
                0xFF00_0000,
            );
        }
        draw_icon(cx, &row.icon, left + 2, top + 2, 32);
        let tx = left + 38;
        cx.frame
            .push_scissor(tx, top, left + LIST_W - 2, top + ROW_H);
        let name_color = if row.playable { WHITE } else { GRAY };
        cx.text(&row.name, tx, top + 3, name_color, true);
        let mut detail = row.version.clone();
        if row.mods {
            detail.push_str(" - mods");
        }
        cx.text(&detail, tx, top + 14, GRAY, true);
        if row.note.is_empty() {
            cx.text(&row.last_played, tx, top + 25, DARK, false);
        } else {
            cx.text(&row.note, tx, top + 25, RED, true);
        }
        cx.frame.pop_scissor();
    }

    fn events(&mut self, events: Vec<WidgetEvent>, out: &mut Vec<ScreenAction>) {
        for WidgetEvent { id, kind } in events {
            if kind != WidgetEventKind::Pressed {
                continue;
            }
            out.push(ScreenAction::ClickSound);
            let selected = self.model.borrow().selected();
            match id {
                L_NEW => out.push(ScreenAction::Push(Box::new(ProfileEditScreen::new(
                    self.model.clone(),
                    None,
                )))),
                L_EDIT => out.push(ScreenAction::Push(Box::new(ProfileEditScreen::new(
                    self.model.clone(),
                    Some(&selected),
                )))),
                L_COPY => {
                    self.model.borrow_mut().duplicate(&selected);
                    self.refresh();
                }
                L_DELETE => {
                    if let Some(row) = self.selected_row() {
                        out.push(ScreenAction::Push(Box::new(delete_confirmation(
                            self.model.clone(),
                            row,
                        ))));
                    }
                }
                L_ACCOUNT => out.push(ScreenAction::Push(Box::new(AccountsScreen::new(
                    self.model.clone(),
                )))),
                L_PLAY => {
                    self.model.borrow_mut().play();
                    self.sync_buttons();
                }
                L_QUIT => out.push(ScreenAction::Quit),
                _ => {}
            }
        }
    }
}

fn delete_confirmation(model: SharedLauncher, row: &LauncherProfileRow) -> ConfirmScreen {
    let id = row.id.clone();
    ConfirmScreen::new(
        "Delete profile?",
        &format!(
            "'{}' will be removed from the launcher. Its game folder and worlds are kept.",
            row.name
        ),
        vec![
            (
                "Delete".into(),
                Box::new(move || {
                    model.borrow_mut().delete(&id);
                    vec![ScreenAction::Pop]
                }),
            ),
            (
                "lang:gui.cancel".into(),
                Box::new(|| vec![ScreenAction::Pop]),
            ),
        ],
    )
}

impl Screen for LauncherScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let left = width / 2 - LIST_W / 2;
        let y = height - FOOTER + 6;
        for (i, (id, label)) in [
            (L_NEW, "New"),
            (L_EDIT, "Edit"),
            (L_COPY, "Copy"),
            (L_DELETE, "Delete"),
        ]
        .into_iter()
        .enumerate()
        {
            self.widgets
                .add(Button::new(id, [left + i as i32 * 76, y, 72, 20], label));
        }
        let y = y + 26;
        self.widgets
            .add(Button::new(L_ACCOUNT, [left, y + 2, 76, 20], "Accounts"));
        self.widgets
            .add(Button::new(L_PLAY, [left + 80, y, 140, 24], "Play"));
        self.widgets.add(Button::new(
            L_QUIT,
            [left + 224, y + 2, 76, 20],
            "lang:menu.quit",
        ));
        self.widgets.set_focus(Some(L_PLAY), 0);
        self.refresh();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.mouse = cx.mouse;
        let (w, h) = cx.gui_size();
        cx.frame.fill_gradient(0, 0, w, h, 0xFF16_2433, 0xFF2B_4A3C);
        if let Some(id) = cx.assets.texture("title/minecraft") {
            cx.frame.sprite(id, w / 2 - 80, 4, 160, 40);
        }
        cx.centered_text("mc-rust launcher", w / 2, 38, YELLOW, true);
        let list = self.list.rect();
        let left = self.left();
        cx.frame.fill_rect(
            left - 2,
            list[1],
            left + LIST_W + 2,
            list[1] + list[3],
            0x8000_0000,
        );
        cx.frame.push_scissor(0, list[1], w, list[1] + list[3]);
        let selected = self.model.borrow().selected();
        for index in self.list.visible_range() {
            self.draw_row(cx, index, &selected);
        }
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, left + LIST_W + 4);
        self.widgets.render(cx);
        let launch = self.model.borrow_mut().launch();
        let status = match launch {
            LaunchView::Working(text) => Some((text, YELLOW)),
            LaunchView::Failed(text) => Some((text, RED)),
            LaunchView::Idle => self.model.borrow().notice().map(|n| (n, GRAY)),
        };
        if let Some((text, color)) = status {
            cx.frame.push_scissor(0, h - 11, w, h);
            let tw = cx.text_width(&text);
            cx.text(&text, ((w - tw) / 2).max(2), h - 10, color, true);
            cx.frame.pop_scissor();
        }
    }

    fn tick(&mut self, _env: &mut Env<'_>, _out: &mut Vec<ScreenAction>) {
        self.model.borrow_mut().tick();
        let rows = self.model.borrow().profiles();
        if rows != self.rows {
            self.refresh();
        } else {
            self.sync_buttons();
        }
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        let bar_x = self.left() + LIST_W + 4;
        match *input {
            UiInput::MouseMove { x, y } => {
                self.mouse = (x, y);
                if self.list.input(input, bar_x) {
                    return true;
                }
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
                    self.events(events, out);
                    return true;
                }
                if self.list.input(input, bar_x) {
                    return true;
                }
                let left = self.left() as f32;
                if let Some(index) = self.list.item_at(x, y)
                    && (left..left + LIST_W as f32).contains(&x)
                    && let Some(row) = self.rows.get(index)
                {
                    let id = row.id.clone();
                    let playable = row.playable;
                    self.model.borrow_mut().select(&id);
                    self.sync_buttons();
                    if double && playable {
                        self.model.borrow_mut().play();
                    }
                    return true;
                }
                return false;
            }
            UiInput::MouseRelease { .. } => {
                self.list.input(input, bar_x);
            }
            UiInput::Scroll { .. } if self.list.input(input, bar_x) => return true,
            _ => {}
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        self.events(events, out);
        used
    }
}

const E_NAME: u32 = 1;
const E_ICON: u32 = 2;
const E_VERSION: u32 = 3;
const E_RESOLUTION: u32 = 4;
const E_GAME_DIR: u32 = 5;
const E_MODS: u32 = 6;
const E_HEAP: u32 = 7;
const E_SAVE: u32 = 8;
const E_CANCEL: u32 = 9;
const HEAP_STEP: u32 = 128;

/// Create or edit one profile.
pub struct ProfileEditScreen {
    model: SharedLauncher,
    draft: ProfileDraft,
    version: Rc<RefCell<String>>,
    widgets: WidgetSet,
    gui: (i32, i32),
    error: Option<String>,
    icons: Vec<String>,
}

impl std::fmt::Debug for ProfileEditScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfileEditScreen")
            .field("draft", &self.draft)
            .finish()
    }
}

impl ProfileEditScreen {
    pub fn new(model: SharedLauncher, id: Option<&str>) -> Self {
        let draft = model.borrow().draft(id);
        let icons = model.borrow().icons();
        Self {
            version: Rc::new(RefCell::new(draft.version.clone())),
            model,
            draft,
            widgets: WidgetSet::default(),
            gui: (0, 0),
            error: None,
            icons,
        }
    }

    fn heap_label(mib: u32) -> String {
        format!("JVM heap (mods only): {mib} MiB")
    }

    fn heap_value(&self) -> f32 {
        let (lo, hi) = self.model.borrow().heap_range();
        self.draft.heap_mib.saturating_sub(lo) as f32 / hi.saturating_sub(lo).max(1) as f32
    }

    fn heap_from(&self, value: f32) -> u32 {
        let (lo, hi) = self.model.borrow().heap_range();
        let mib = lo as f32 + value * hi.saturating_sub(lo) as f32;
        ((mib / HEAP_STEP as f32).round() as u32 * HEAP_STEP).clamp(lo, hi)
    }

    fn collect(&mut self) {
        let text = |w: &WidgetSet, id| w.edit(id).map(|e| e.value().to_owned()).unwrap_or_default();
        self.draft.name = text(&self.widgets, E_NAME);
        self.draft.resolution = text(&self.widgets, E_RESOLUTION);
        self.draft.game_dir = text(&self.widgets, E_GAME_DIR);
        self.draft.mods = text(&self.widgets, E_MODS);
        self.draft.version = self.version.borrow().clone();
    }

    fn cycle_icon(&mut self, delta: i32) {
        if self.icons.is_empty() {
            return;
        }
        let n = self.icons.len() as i32;
        let at = self
            .icons
            .iter()
            .position(|i| *i == self.draft.icon)
            .map_or(0, |i| i as i32);
        self.draft.icon = self.icons[(at + delta).rem_euclid(n) as usize].clone();
    }

    fn save(&mut self, out: &mut Vec<ScreenAction>) {
        self.collect();
        let result = self.model.borrow_mut().save_profile(self.draft.clone());
        match result {
            Ok(()) => out.push(ScreenAction::Pop),
            Err(e) => self.error = Some(e),
        }
    }

    fn events(&mut self, events: Vec<WidgetEvent>, out: &mut Vec<ScreenAction>) {
        for WidgetEvent { id, kind } in events {
            match (id, kind) {
                (E_ICON, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    self.cycle_icon(1);
                }
                (E_VERSION, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    self.collect();
                    out.push(ScreenAction::Push(Box::new(VersionSelectScreen::new(
                        self.model.clone(),
                        self.version.clone(),
                    ))));
                }
                (E_HEAP, WidgetEventKind::SliderChanged(v)) => {
                    self.draft.heap_mib = self.heap_from(v);
                    if let Some(s) = self.widgets.slider_mut(E_HEAP) {
                        s.label = Self::heap_label(self.draft.heap_mib);
                    }
                }
                (E_SAVE, WidgetEventKind::Pressed) | (E_NAME, WidgetEventKind::Submitted) => {
                    out.push(ScreenAction::ClickSound);
                    self.save(out);
                }
                (E_CANCEL, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                _ => {}
            }
        }
    }
}

impl Screen for ProfileEditScreen {
    fn init(&mut self, width: i32, height: i32) {
        if !self.widgets.is_empty() {
            self.collect();
        }
        self.gui = (width, height);
        self.widgets.clear();
        let (l, r) = (width / 2 - 154, width / 2 + 4);
        let edit = |id, x, y, value: &str, max: usize| {
            let mut e = EditBox::new(id, [x, y, 150, 20]).with_max_length(max);
            e.set_value(value);
            e.take_changed();
            e
        };
        self.widgets
            .add(edit(E_NAME, l, 40, &self.draft.name.clone(), 64));
        self.widgets.add(Button::new(E_ICON, [r, 40, 150, 20], ""));
        self.widgets
            .add(Button::new(E_VERSION, [l, 76, 150, 20], ""));
        self.widgets.add(
            edit(E_RESOLUTION, r, 76, &self.draft.resolution.clone(), 11).with_hint("default"),
        );
        self.widgets
            .add(edit(E_GAME_DIR, l, 112, &self.draft.game_dir.clone(), 1024).with_hint("default"));
        self.widgets
            .add(edit(E_MODS, r, 112, &self.draft.mods.clone(), 1024).with_hint("none (vanilla)"));
        self.widgets.add(Slider::new(
            E_HEAP,
            [l, 146, 304, 20],
            Self::heap_label(self.draft.heap_mib),
            self.heap_value(),
        ));
        let y = height - 28;
        self.widgets
            .add(Button::new(E_SAVE, [l, y, 150, 20], "Save"));
        self.widgets
            .add(Button::new(E_CANCEL, [r, y, 150, 20], "lang:gui.cancel"));
        self.widgets.set_focus(Some(E_NAME), 0);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let (w, _) = cx.gui_size();
        let title = if self.draft.id.is_some() {
            "Edit profile"
        } else {
            "New profile"
        };
        cx.centered_text(title, w / 2, 12, WHITE, true);
        let (l, r) = (w / 2 - 154, w / 2 + 4);
        for (text, x, y) in [
            ("Name", l, 30),
            ("Icon", r, 30),
            ("Version", l, 66),
            ("Resolution (WxH)", r, 66),
            ("Game directory", l, 102),
            ("Mods folder", r, 102),
        ] {
            cx.text(text, x, y, GRAY, true);
        }
        let version = self.version.borrow().clone();
        let label = self.model.borrow().version_label(&version);
        if let Some(b) = self.widgets.button_mut(E_VERSION) {
            b.label = label;
        }
        let icon = self.draft.icon.clone();
        if let Some(b) = self.widgets.button_mut(E_ICON) {
            b.label = icon.replace('_', " ");
        }
        self.widgets.render(cx);
        draw_icon(cx, &icon, r + 3, 42, 16);
        let empty_dir = self.widgets.edit(E_GAME_DIR).is_some_and(EditBox::is_empty);
        if empty_dir {
            let default = format!("Default: {}", self.model.borrow().default_game_dir());
            cx.frame.push_scissor(l, 133, l + 304, 141);
            cx.text(&default, l, 133, DARK, false);
            cx.frame.pop_scissor();
        }
        if let Some(error) = &self.error {
            let error = error.clone();
            cx.centered_text(&error, w / 2, 172, RED, true);
        }
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if escape(input) {
            out.push(ScreenAction::Pop);
            return true;
        }
        if let UiInput::MousePress {
            x,
            y,
            button: MouseButton::Right,
            ..
        } = *input
            && let Some(b) = self.widgets.button_mut(E_ICON)
            && contains(b.rect, x, y)
        {
            self.cycle_icon(-1);
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        self.events(events, out);
        used
    }
}

const V_SNAPSHOTS: u32 = 1;
const V_CANCEL: u32 = 2;
const V_ROW: i32 = 20;
const V_W: i32 = 240;

/// Pick a version; versions mcr cannot play are listed greyed.
pub struct VersionSelectScreen {
    model: SharedLauncher,
    slot: Rc<RefCell<String>>,
    rows: Vec<VersionRow>,
    widgets: WidgetSet,
    gui: (i32, i32),
    list: ScrollList,
}

impl std::fmt::Debug for VersionSelectScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VersionSelectScreen")
            .field("rows", &self.rows.len())
            .finish()
    }
}

impl VersionSelectScreen {
    pub fn new(model: SharedLauncher, slot: Rc<RefCell<String>>) -> Self {
        Self {
            model,
            slot,
            rows: Vec::new(),
            widgets: WidgetSet::default(),
            gui: (0, 0),
            list: ScrollList::new([0, 32, 0, 0], V_ROW),
        }
    }

    pub fn rows(&self) -> &[VersionRow] {
        &self.rows
    }

    fn refresh(&mut self) {
        let mut rows = self.model.borrow().versions();
        rows.truncate(MAX_ROWS);
        self.rows = rows;
        self.list
            .set_rect([0, 32, self.gui.0, (self.gui.1 - 32 - 36).max(V_ROW)]);
        self.list.set_count(self.rows.len());
        let on = self.model.borrow().show_snapshots();
        if let Some(b) = self.widgets.button_mut(V_SNAPSHOTS) {
            b.label = format!("Snapshots: {}", if on { "ON" } else { "OFF" });
        }
    }

    fn left(&self) -> i32 {
        self.gui.0 / 2 - V_W / 2
    }
}

impl Screen for VersionSelectScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let y = height - 28;
        self.widgets
            .add(Button::new(V_SNAPSHOTS, [width / 2 - 154, y, 150, 20], ""));
        self.widgets.add(Button::new(
            V_CANCEL,
            [width / 2 + 4, y, 150, 20],
            "lang:gui.cancel",
        ));
        self.refresh();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let (w, _) = cx.gui_size();
        cx.centered_text("Select version", w / 2, 12, WHITE, true);
        let list = self.list.rect();
        let left = self.left();
        cx.frame.fill_rect(
            left - 2,
            list[1],
            left + V_W + 2,
            list[1] + list[3],
            0x8000_0000,
        );
        cx.frame.push_scissor(0, list[1], w, list[1] + list[3]);
        let current = self.slot.borrow().clone();
        let (mx, my) = cx.mouse;
        for index in self.list.visible_range() {
            let row = &self.rows[index];
            let top = self.list.item_top(index);
            let hovered = row.supported && contains([left, top, V_W, V_ROW], mx, my);
            if row.id == current || hovered {
                cx.frame
                    .fill_rect(left, top, left + V_W, top + V_ROW, 0x40FF_FFFF);
            }
            let color = if row.supported { WHITE } else { DARK };
            cx.text(&row.label, left + 4, top + 6, color, true);
            let tag = if !row.supported {
                ("not supported yet", DARK)
            } else if row.snapshot {
                ("snapshot", YELLOW)
            } else {
                ("", GREEN)
            };
            let tw = cx.text_width(tag.0);
            cx.text(tag.0, left + V_W - tw - 4, top + 6, tag.1, true);
        }
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, left + V_W + 4);
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if escape(input) {
            out.push(ScreenAction::Pop);
            return true;
        }
        let bar_x = self.left() + V_W + 4;
        if let UiInput::MousePress {
            x,
            y,
            button: MouseButton::Left,
            ..
        } = *input
        {
            let mut events = Vec::new();
            if !self.widgets.input(env, input, &mut events) {
                if self.list.input(input, bar_x) {
                    return true;
                }
                let left = self.left() as f32;
                if let Some(row) = self
                    .list
                    .item_at(x, y)
                    .filter(|_| (left..left + V_W as f32).contains(&x))
                    .and_then(|i| self.rows.get(i))
                {
                    if row.supported {
                        *self.slot.borrow_mut() = row.id.clone();
                        out.push(ScreenAction::ClickSound);
                        out.push(ScreenAction::Pop);
                    }
                    return true;
                }
                return false;
            }
            for WidgetEvent { id, kind } in events {
                if kind != WidgetEventKind::Pressed {
                    continue;
                }
                out.push(ScreenAction::ClickSound);
                match id {
                    V_SNAPSHOTS => {
                        let on = !self.model.borrow().show_snapshots();
                        self.model.borrow_mut().set_show_snapshots(on);
                        self.refresh();
                    }
                    V_CANCEL => out.push(ScreenAction::Pop),
                    _ => {}
                }
            }
            return true;
        }
        if self.list.input(input, bar_x) {
            return true;
        }
        let mut events = Vec::new();
        self.widgets.input(env, input, &mut events)
    }
}

const A_OFFLINE: u32 = 1;
const A_MICROSOFT: u32 = 2;
const A_REMOVE: u32 = 3;
const A_DONE: u32 = 4;
const A_ROW: i32 = 24;
const A_W: i32 = 240;

/// The account list: choose the active one, add offline or Microsoft accounts, sign out.
pub struct AccountsScreen {
    model: SharedLauncher,
    rows: Vec<AccountRow>,
    widgets: WidgetSet,
    gui: (i32, i32),
    list: ScrollList,
}

impl std::fmt::Debug for AccountsScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountsScreen")
            .field("rows", &self.rows.len())
            .finish()
    }
}

impl AccountsScreen {
    pub fn new(model: SharedLauncher) -> Self {
        Self {
            model,
            rows: Vec::new(),
            widgets: WidgetSet::default(),
            gui: (0, 0),
            list: ScrollList::new([0, 32, 0, 0], A_ROW),
        }
    }

    fn left(&self) -> i32 {
        self.gui.0 / 2 - A_W / 2
    }

    fn refresh(&mut self) {
        let mut rows = self.model.borrow().accounts();
        rows.truncate(MAX_ROWS);
        self.rows = rows;
        self.list
            .set_rect([0, 32, self.gui.0, (self.gui.1 - 32 - 60).max(A_ROW)]);
        self.list.set_count(self.rows.len());
        let removable = self.rows.iter().any(|r| r.active) && self.rows.len() > 1;
        if let Some(b) = self.widgets.button_mut(A_REMOVE) {
            b.active = removable;
        }
        let ms = self.model.borrow().microsoft_problem();
        if let Some(b) = self.widgets.button_mut(A_MICROSOFT) {
            b.active = ms.is_none();
            b.tooltip = ms.map(super::tooltip::TooltipLines::text);
        }
    }

    fn remove_confirmation(&self, row: &AccountRow) -> ConfirmScreen {
        let (model, key) = (self.model.clone(), row.key.clone());
        let message = if row.microsoft {
            format!(
                "{} will be signed out and its saved login removed from this computer.",
                row.name
            )
        } else {
            format!("The offline account {} will be removed.", row.name)
        };
        ConfirmScreen::new(
            "Sign out?",
            &message,
            vec![
                (
                    "Sign out".into(),
                    Box::new(move || {
                        model.borrow_mut().remove_account(&key);
                        vec![ScreenAction::Pop]
                    }),
                ),
                (
                    "lang:gui.cancel".into(),
                    Box::new(|| vec![ScreenAction::Pop]),
                ),
            ],
        )
    }
}

impl Screen for AccountsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let (l, r) = (width / 2 - 154, width / 2 + 4);
        let y = height - 52;
        self.widgets.add(Button::new(
            A_OFFLINE,
            [l, y, 150, 20],
            "Add offline account",
        ));
        self.widgets.add(Button::new(
            A_MICROSOFT,
            [r, y, 150, 20],
            "Add Microsoft account",
        ));
        self.widgets
            .add(Button::new(A_REMOVE, [l, y + 24, 150, 20], "Sign out"));
        self.widgets
            .add(Button::new(A_DONE, [r, y + 24, 150, 20], "lang:gui.done"));
        self.refresh();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let (w, _) = cx.gui_size();
        cx.centered_text("Accounts", w / 2, 12, WHITE, true);
        let list = self.list.rect();
        let left = self.left();
        cx.frame.fill_rect(
            left - 2,
            list[1],
            left + A_W + 2,
            list[1] + list[3],
            0x8000_0000,
        );
        cx.frame.push_scissor(0, list[1], w, list[1] + list[3]);
        for index in self.list.visible_range() {
            let row = &self.rows[index];
            let top = self.list.item_top(index);
            if row.active {
                cx.frame
                    .fill_rect(left, top, left + A_W, top + A_ROW, 0x40FF_FFFF);
            }
            cx.text(&row.name, left + 6, top + 8, WHITE, true);
            let (kind, color) = if row.microsoft {
                ("Microsoft", GREEN)
            } else {
                ("Offline", GRAY)
            };
            let tw = cx.text_width(kind);
            cx.text(kind, left + A_W - tw - 6, top + 8, color, true);
        }
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, left + A_W + 4);
        self.widgets.render(cx);
    }

    fn tick(&mut self, _env: &mut Env<'_>, _out: &mut Vec<ScreenAction>) {
        self.model.borrow_mut().tick();
        if self.model.borrow().accounts() != self.rows {
            self.refresh();
        }
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if escape(input) {
            out.push(ScreenAction::Pop);
            return true;
        }
        let bar_x = self.left() + A_W + 4;
        let mut events = Vec::new();
        if self.widgets.input(env, input, &mut events) {
            for WidgetEvent { id, kind } in events {
                if kind != WidgetEventKind::Pressed {
                    continue;
                }
                out.push(ScreenAction::ClickSound);
                match id {
                    A_OFFLINE => out.push(ScreenAction::Push(Box::new(OfflineAccountScreen::new(
                        self.model.clone(),
                    )))),
                    A_MICROSOFT => out.push(ScreenAction::Push(Box::new(SignInScreen::new(
                        self.model.clone(),
                    )))),
                    A_REMOVE => {
                        if let Some(row) = self.rows.iter().find(|r| r.active) {
                            out.push(ScreenAction::Push(Box::new(self.remove_confirmation(row))));
                        }
                    }
                    A_DONE => out.push(ScreenAction::Pop),
                    _ => {}
                }
            }
            return true;
        }
        if self.list.input(input, bar_x) {
            return true;
        }
        if let UiInput::MousePress {
            x,
            y,
            button: MouseButton::Left,
            ..
        } = *input
        {
            let left = self.left() as f32;
            if let Some(row) = self
                .list
                .item_at(x, y)
                .filter(|_| (left..left + A_W as f32).contains(&x))
                .and_then(|i| self.rows.get(i))
            {
                let key = row.key.clone();
                self.model.borrow_mut().set_active_account(&key);
                self.refresh();
                return true;
            }
        }
        false
    }
}

const O_NAME: u32 = 1;
const O_ADD: u32 = 2;
const O_CANCEL: u32 = 3;

/// Name an offline account; names are checked like vanilla's.
pub struct OfflineAccountScreen {
    model: SharedLauncher,
    widgets: WidgetSet,
    gui: (i32, i32),
    problem: Option<String>,
}

impl std::fmt::Debug for OfflineAccountScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OfflineAccountScreen").finish()
    }
}

impl OfflineAccountScreen {
    pub fn new(model: SharedLauncher) -> Self {
        Self {
            model,
            widgets: WidgetSet::default(),
            gui: (0, 0),
            problem: None,
        }
    }

    fn name(&self) -> String {
        self.widgets
            .edit(O_NAME)
            .map(|e| e.value().to_owned())
            .unwrap_or_default()
    }

    fn validate(&mut self) {
        let name = self.name();
        self.problem = self.model.borrow().offline_problem(&name);
        if let Some(b) = self.widgets.button_mut(O_ADD) {
            b.active = self.problem.is_none();
        }
    }

    fn add(&mut self, out: &mut Vec<ScreenAction>) {
        self.validate();
        if self.problem.is_none() {
            let name = self.name();
            self.model.borrow_mut().add_offline(&name);
            out.push(ScreenAction::Pop);
        }
    }
}

impl Screen for OfflineAccountScreen {
    fn init(&mut self, width: i32, height: i32) {
        let keep = self.name();
        self.gui = (width, height);
        self.widgets.clear();
        let mut name = EditBox::new(O_NAME, [width / 2 - 100, 60, 200, 20]).with_max_length(32);
        name.set_value(&keep);
        name.take_changed();
        self.widgets.add(name);
        let y = height / 4 + 96;
        self.widgets
            .add(Button::new(O_ADD, [width / 2 - 100, y, 200, 20], "Add"));
        self.widgets.add(Button::new(
            O_CANCEL,
            [width / 2 - 100, y + 24, 200, 20],
            "lang:gui.cancel",
        ));
        self.widgets.set_focus(Some(O_NAME), 0);
        self.validate();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let (w, _) = cx.gui_size();
        cx.centered_text("Add offline account", w / 2, 17, WHITE, true);
        cx.text("Player name", w / 2 - 100, 48, GRAY, true);
        cx.centered_text(
            "Offline accounts only join servers in offline mode",
            w / 2,
            90,
            DARK,
            true,
        );
        if let Some(problem) = &self.problem
            && !self.name().is_empty()
        {
            let problem = problem.clone();
            cx.centered_text(&problem, w / 2, 102, RED, true);
        }
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if escape(input) {
            out.push(ScreenAction::Pop);
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            match (id, kind) {
                (O_NAME, WidgetEventKind::Edited) => self.validate(),
                (O_ADD, WidgetEventKind::Pressed) | (O_NAME, WidgetEventKind::Submitted) => {
                    out.push(ScreenAction::ClickSound);
                    self.add(out);
                }
                (O_CANCEL, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                _ => {}
            }
        }
        self.validate();
        used
    }
}

const S_OPEN: u32 = 1;
const S_COPY: u32 = 2;
const S_CANCEL: u32 = 3;

/// The Microsoft device-code sign-in: shows the code and the link, and closes once signed in.
pub struct SignInScreen {
    model: SharedLauncher,
    state: SignInView,
    widgets: WidgetSet,
    gui: (i32, i32),
    started: bool,
}

impl std::fmt::Debug for SignInScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignInScreen")
            .field("started", &self.started)
            .finish()
    }
}

impl SignInScreen {
    pub fn new(model: SharedLauncher) -> Self {
        Self {
            model,
            state: SignInView::Idle,
            widgets: WidgetSet::default(),
            gui: (0, 0),
            started: false,
        }
    }

    fn sync(&mut self) {
        let has_code = matches!(self.state, SignInView::Code { .. });
        for id in [S_OPEN, S_COPY] {
            if let Some(b) = self.widgets.button_mut(id) {
                b.active = has_code;
            }
        }
        let done = matches!(self.state, SignInView::Failed(_) | SignInView::Done(_));
        if let Some(b) = self.widgets.button_mut(S_CANCEL) {
            b.label = if done {
                "lang:gui.back"
            } else {
                "lang:gui.cancel"
            }
            .to_owned();
        }
    }

    fn close(&mut self, out: &mut Vec<ScreenAction>) {
        if !matches!(self.state, SignInView::Done(_) | SignInView::Failed(_)) {
            self.model.borrow_mut().cancel_sign_in();
        }
        out.push(ScreenAction::Pop);
    }
}

impl Screen for SignInScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let y = height / 4 + 96;
        self.widgets.add(Button::new(
            S_OPEN,
            [width / 2 - 154, y, 150, 20],
            "Open link",
        ));
        self.widgets.add(Button::new(
            S_COPY,
            [width / 2 + 4, y, 150, 20],
            "Copy code",
        ));
        self.widgets.add(Button::new(
            S_CANCEL,
            [width / 2 - 100, y + 24, 200, 20],
            "lang:gui.cancel",
        ));
        if !self.started {
            self.started = true;
            self.model.borrow_mut().start_sign_in();
            self.state = self.model.borrow_mut().sign_in();
        }
        self.sync();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let (w, _) = cx.gui_size();
        cx.centered_text("Sign in with Microsoft", w / 2, 17, WHITE, true);
        match self.state.clone() {
            SignInView::Idle | SignInView::Starting => {
                cx.centered_text("Contacting Microsoft...", w / 2, 60, GRAY, true);
            }
            SignInView::Code { user_code, url } => {
                cx.centered_text("On any device, open", w / 2, 44, GRAY, true);
                cx.centered_text(&url, w / 2, 56, 0xFF55_55FF, true);
                cx.centered_text("and enter the code", w / 2, 68, GRAY, true);
                let tw = cx.text_width(&user_code);
                cx.frame
                    .fill_rect(w / 2 - tw - 8, 80, w / 2 + tw + 8, 104, 0xA000_0000);
                let cells = crate::parse_legacy(&user_code);
                cx.frame
                    .cells_scaled(&cells, (w / 2 - tw) as f32, 84.0, YELLOW, true, 2.0);
                cx.centered_text("Waiting for you to sign in...", w / 2, 110, DARK, true);
            }
            SignInView::Failed(reason) => {
                cx.centered_text("Sign-in failed", w / 2, 56, RED, true);
                cx.frame.push_scissor(0, 66, w, 78);
                cx.centered_text(&reason, w / 2, 68, GRAY, true);
                cx.frame.pop_scissor();
            }
            SignInView::Done(name) => {
                cx.centered_text(&format!("Signed in as {name}"), w / 2, 60, GREEN, true);
            }
        }
        self.widgets.render(cx);
    }

    fn tick(&mut self, _env: &mut Env<'_>, out: &mut Vec<ScreenAction>) {
        self.model.borrow_mut().tick();
        let state = self.model.borrow_mut().sign_in();
        if state != self.state {
            self.state = state;
            self.sync();
            if matches!(self.state, SignInView::Done(_)) {
                out.push(ScreenAction::Pop);
            }
        }
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if escape(input) {
            self.close(out);
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            if kind != WidgetEventKind::Pressed {
                continue;
            }
            out.push(ScreenAction::ClickSound);
            match (id, &self.state) {
                (S_OPEN, SignInView::Code { url, .. }) => {
                    out.push(ScreenAction::OpenUrl(url.clone()));
                }
                (S_COPY, SignInView::Code { user_code, .. }) => {
                    out.push(ScreenAction::CopyToClipboard(user_code.clone()));
                }
                (S_CANCEL, _) => self.close(out),
                _ => {}
            }
        }
        used
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::env::{MemoryClipboard, NoView};

    #[derive(Default)]
    struct Fake {
        profiles: Vec<LauncherProfileRow>,
        selected: String,
        deleted: Vec<String>,
        played: u32,
        saved: Vec<ProfileDraft>,
        accounts: Vec<AccountRow>,
        snapshots: bool,
        sign_in: Option<SignInView>,
        cancelled: bool,
    }

    fn row(id: &str, playable: bool, default: bool) -> LauncherProfileRow {
        LauncherProfileRow {
            id: id.into(),
            name: id.into(),
            icon: "grass_block".into(),
            version: "26.3".into(),
            playable,
            note: String::new(),
            last_played: "never".into(),
            mods: false,
            default,
        }
    }

    impl LauncherModel for Fake {
        fn profiles(&self) -> Vec<LauncherProfileRow> {
            self.profiles.clone()
        }
        fn selected(&self) -> String {
            self.selected.clone()
        }
        fn select(&mut self, id: &str) {
            self.selected = id.into();
        }
        fn can_add(&self) -> bool {
            true
        }
        fn draft(&self, id: Option<&str>) -> ProfileDraft {
            ProfileDraft {
                id: id.map(str::to_owned),
                name: "Draft".into(),
                icon: "grass_block".into(),
                version: "latest-release".into(),
                heap_mib: 2048,
                ..ProfileDraft::default()
            }
        }
        fn save_profile(&mut self, draft: ProfileDraft) -> Result<(), String> {
            if draft.resolution == "bad" {
                return Err("bad resolution".into());
            }
            self.saved.push(draft);
            Ok(())
        }
        fn duplicate(&mut self, _id: &str) {}
        fn delete(&mut self, id: &str) {
            self.deleted.push(id.into());
            self.profiles.retain(|p| p.id != id);
        }
        fn icons(&self) -> Vec<String> {
            vec!["grass_block".into(), "tnt".into()]
        }
        fn versions(&self) -> Vec<VersionRow> {
            let mut rows = vec![
                VersionRow {
                    id: "latest-release".into(),
                    label: "Latest release (26.3)".into(),
                    snapshot: false,
                    supported: true,
                },
                VersionRow {
                    id: "26.3".into(),
                    label: "26.3".into(),
                    snapshot: false,
                    supported: true,
                },
                VersionRow {
                    id: "26.2".into(),
                    label: "26.2".into(),
                    snapshot: false,
                    supported: false,
                },
            ];
            if self.snapshots {
                rows.push(VersionRow {
                    id: "26w50a".into(),
                    label: "26w50a".into(),
                    snapshot: true,
                    supported: false,
                });
            }
            rows
        }
        fn version_label(&self, version: &str) -> String {
            version.into()
        }
        fn show_snapshots(&self) -> bool {
            self.snapshots
        }
        fn set_show_snapshots(&mut self, on: bool) {
            self.snapshots = on;
        }
        fn heap_range(&self) -> (u32, u32) {
            (1024, 4096)
        }
        fn default_game_dir(&self) -> String {
            "run/client".into()
        }
        fn accounts(&self) -> Vec<AccountRow> {
            self.accounts.clone()
        }
        fn set_active_account(&mut self, key: &str) {
            for a in &mut self.accounts {
                a.active = a.key == key;
            }
        }
        fn offline_problem(&self, name: &str) -> Option<String> {
            (name.is_empty() || name.contains(' ')).then(|| "invalid".to_owned())
        }
        fn add_offline(&mut self, name: &str) {
            self.accounts.push(AccountRow {
                key: name.into(),
                name: name.into(),
                microsoft: false,
                active: true,
            });
        }
        fn remove_account(&mut self, key: &str) {
            self.accounts.retain(|a| a.key != key);
        }
        fn microsoft_problem(&self) -> Option<String> {
            None
        }
        fn start_sign_in(&mut self) {
            self.sign_in = Some(SignInView::Starting);
        }
        fn sign_in(&mut self) -> SignInView {
            self.sign_in.clone().unwrap_or(SignInView::Idle)
        }
        fn cancel_sign_in(&mut self) {
            self.cancelled = true;
        }
        fn play(&mut self) {
            self.played += 1;
        }
        fn launch(&mut self) -> LaunchView {
            LaunchView::Idle
        }
        fn notice(&self) -> Option<String> {
            None
        }
        fn tick(&mut self) {}
    }

    fn fake() -> Rc<RefCell<Fake>> {
        Rc::new(RefCell::new(Fake {
            profiles: vec![row("latest-release", true, true), row("old", false, false)],
            selected: "latest-release".into(),
            accounts: vec![AccountRow {
                key: "offline:player".into(),
                name: "Player".into(),
                microsoft: false,
                active: true,
            }],
            ..Fake::default()
        }))
    }

    struct Harness {
        stack: super::super::screen::ScreenStack,
        clipboard: MemoryClipboard,
        out: Vec<ScreenAction>,
    }

    impl Harness {
        fn new(screen: Box<dyn Screen>) -> Self {
            let mut stack = super::super::screen::ScreenStack::new(480, 270);
            stack.push(screen);
            Self {
                stack,
                clipboard: MemoryClipboard::default(),
                out: Vec::new(),
            }
        }

        fn click(&mut self, x: f32, y: f32) {
            for input in [
                UiInput::MouseMove { x, y },
                UiInput::MousePress {
                    x,
                    y,
                    button: MouseButton::Left,
                    mods: crate::Modifiers::NONE,
                    double: false,
                },
                UiInput::MouseRelease {
                    x,
                    y,
                    button: MouseButton::Left,
                    mods: crate::Modifiers::NONE,
                },
            ] {
                let mut env = Env {
                    view: &NoView,
                    clipboard: &mut self.clipboard,
                    now_ms: 10_000,
                };
                self.stack.input(&mut env, &input, &mut self.out);
            }
        }

        fn typed(&mut self, text: &str) {
            for c in text.chars() {
                let mut env = Env {
                    view: &NoView,
                    clipboard: &mut self.clipboard,
                    now_ms: 10_000,
                };
                self.stack.input(&mut env, &UiInput::Char(c), &mut self.out);
            }
        }

        fn tick(&mut self) {
            let mut env = Env {
                view: &NoView,
                clipboard: &mut self.clipboard,
                now_ms: 10_000,
            };
            self.stack.tick(&mut env, &mut self.out);
        }
    }

    const W: f32 = 480.0;
    const H: f32 = 270.0;

    fn play_xy() -> (f32, f32) {
        (W / 2.0 - 150.0 + 150.0, H - 70.0 + 6.0 + 26.0 + 12.0)
    }

    #[test]
    fn play_and_selection() {
        let model = fake();
        let mut h = Harness::new(Box::new(LauncherScreen::new(model.clone())));
        let (px, py) = play_xy();
        h.click(px, py);
        assert_eq!(model.borrow().played, 1);
        let row_y = (LIST_TOP + ROW_H + 10) as f32;
        h.click(W / 2.0, row_y);
        assert_eq!(model.borrow().selected, "old");
        h.click(px, py);
        assert_eq!(model.borrow().played, 1, "unsupported profiles cannot play");
    }

    #[test]
    fn delete_needs_confirmation_and_default_is_kept() {
        let model = fake();
        let mut h = Harness::new(Box::new(LauncherScreen::new(model.clone())));
        let delete = (W / 2.0 - 150.0 + 3.0 * 76.0 + 30.0, H - 70.0 + 16.0);
        h.click(delete.0, delete.1);
        assert_eq!(h.stack.len(), 1, "the default profile has no Delete");
        h.click(W / 2.0, (LIST_TOP + ROW_H + 10) as f32);
        h.click(delete.0, delete.1);
        assert_eq!(h.stack.len(), 2);
        assert!(model.borrow().deleted.is_empty());
        let mut env = Env {
            view: &NoView,
            clipboard: &mut h.clipboard,
            now_ms: 0,
        };
        h.stack.input(
            &mut env,
            &UiInput::Key {
                key: Key::Escape,
                mods: crate::Modifiers::NONE,
                repeat: false,
            },
            &mut h.out,
        );
        assert_eq!(h.stack.len(), 1);
        assert!(model.borrow().deleted.is_empty());
    }

    #[test]
    fn version_list_skips_unsupported_rows() {
        let model = fake();
        let slot = Rc::new(RefCell::new("latest-release".to_owned()));
        let shared: SharedLauncher = model.clone();
        let mut h = Harness::new(Box::new(VersionSelectScreen::new(shared, slot.clone())));
        h.click(W / 2.0, (32 + 2 * V_ROW + 5) as f32);
        assert_eq!(*slot.borrow(), "latest-release");
        assert_eq!(h.stack.len(), 1);
        h.click(W / 2.0 - 100.0, H - 20.0);
        assert!(model.borrow().snapshots);
        h.click(W / 2.0, (32 + V_ROW + 5) as f32);
        assert_eq!(*slot.borrow(), "26.3");
        assert_eq!(h.stack.len(), 0);
    }

    #[test]
    fn offline_name_must_be_valid() {
        let model = fake();
        let mut h = Harness::new(Box::new(OfflineAccountScreen::new(model.clone())));
        h.typed("bad name");
        let add = (W / 2.0, (270 / 4 + 96 + 10) as f32);
        h.click(add.0, add.1);
        assert_eq!(model.borrow().accounts.len(), 1);
        let mut h = Harness::new(Box::new(OfflineAccountScreen::new(model.clone())));
        h.typed("Steve");
        h.click(add.0, add.1);
        assert_eq!(model.borrow().accounts.len(), 2);
        assert_eq!(h.stack.len(), 0);
    }

    #[test]
    fn sign_in_shows_code_and_cancels() {
        let model = fake();
        let mut h = Harness::new(Box::new(SignInScreen::new(model.clone())));
        model.borrow_mut().sign_in = Some(SignInView::Code {
            user_code: "ABCD".into(),
            url: "https://www.microsoft.com/link".into(),
        });
        h.tick();
        let y = (270 / 4 + 96 + 10) as f32;
        h.click(W / 2.0 + 50.0, y);
        assert!(
            h.out
                .iter()
                .any(|a| matches!(a, ScreenAction::CopyToClipboard(c) if c == "ABCD"))
        );
        h.click(W / 2.0, y + 24.0);
        assert!(model.borrow().cancelled);
        assert_eq!(h.stack.len(), 0);
    }

    #[test]
    fn editor_reports_save_errors() {
        let model = fake();
        let shared: SharedLauncher = model.clone();
        let mut h = Harness::new(Box::new(ProfileEditScreen::new(shared, None)));
        h.click(W / 2.0 + 50.0, 86.0);
        h.typed("bad");
        h.click(W / 2.0 - 80.0, H - 18.0);
        assert!(model.borrow().saved.is_empty());
        assert_eq!(h.stack.len(), 1);
        h.click(W / 2.0 + 80.0, 50.0);
        let mut env = Env {
            view: &NoView,
            clipboard: &mut h.clipboard,
            now_ms: 0,
        };
        h.stack
            .input(&mut env, &UiInput::MouseMove { x: 0.0, y: 0.0 }, &mut h.out);
        h.click(W / 2.0 + 50.0, 86.0);
        for _ in 0..3 {
            let mut env = Env {
                view: &NoView,
                clipboard: &mut h.clipboard,
                now_ms: 0,
            };
            h.stack.input(
                &mut env,
                &UiInput::Key {
                    key: Key::Backspace,
                    mods: crate::Modifiers::NONE,
                    repeat: false,
                },
                &mut h.out,
            );
        }
        h.typed("1280x720");
        h.click(W / 2.0 - 80.0, H - 18.0);
        let saved = model.borrow().saved.clone();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].icon, "tnt");
        assert_eq!(saved[0].resolution, "1280x720");
        assert_eq!(h.stack.len(), 0);
    }
}
