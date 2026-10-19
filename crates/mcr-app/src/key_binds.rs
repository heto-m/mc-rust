//! `KeyBindsScreen` and its `KeyBindsList`: mappings under category headers, each with a reset button.

use mcr_settings::{Category, InputKey, KeyBindings, MAPPINGS};
use mcr_ui::{
    Button, Env, Key, MouseButton, RenderCx, Screen, ScreenAction, ScrollList, TooltipLines,
    UiInput, WidgetEventKind, WidgetSet, contains, draw_menu_background, tile_texture,
};

use crate::input::input_key_from_ui;
use crate::options_list::{FOOTER, HEADER};
use crate::settings::SharedSettings;

const ROW: i32 = 20;
const ROW_WIDTH: i32 = 340;
const CHANGE_WIDTH: i32 = 75;
const RESET_WIDTH: i32 = 50;
const RESET_ALL: u32 = 1 << 20;
const DONE: u32 = RESET_ALL + 1;
const WHITE: u32 = 0xFFFF_FFFF;
const COLLISION: u32 = 0xFFFF_FF00;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    Category(Category),
    Key(usize),
}

/// Vanilla's `KeyMapping#compareTo`: category order, then explicit order, then translated name.
fn rows(mut name: impl FnMut(&str) -> String) -> Vec<Row> {
    let mut keys: Vec<(usize, i32, String, usize)> = MAPPINGS
        .iter()
        .enumerate()
        .map(|(i, m)| (m.category.rank(), m.order(), name(m.name), i))
        .collect();
    keys.sort();
    let mut out = Vec::with_capacity(keys.len() + Category::SORTED.len());
    let mut last = None;
    for (_, _, _, i) in keys {
        let category = MAPPINGS[i].category;
        if last != Some(category) {
            last = Some(category);
            out.push(Row::Category(category));
        }
        out.push(Row::Key(i));
    }
    out
}

pub fn key_label(cx: &mut RenderCx<'_, '_>, key: InputKey) -> String {
    let name = key.name();
    let translated = cx.lang(name);
    if translated == name {
        name.rsplit('.').next().unwrap_or(name).to_uppercase()
    } else {
        translated
    }
}

/// `KeyBindsList.KeyEntry#updateChangeButtonMessage`.
fn change_label(key: &str, collides: bool, selected: bool) -> String {
    let mut label = if collides {
        format!("§e[ §f{key}§e ]")
    } else {
        key.to_owned()
    };
    if selected {
        label = format!("§e> §f§n{label}§r§e <");
    }
    label
}

pub struct KeyBindsScreen {
    settings: SharedSettings,
    rows: Vec<Row>,
    widgets: WidgetSet,
    footer: WidgetSet,
    list: ScrollList,
    gui: (i32, i32),
    selected: Option<usize>,
}

impl std::fmt::Debug for KeyBindsScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyBindsScreen")
            .field("selected", &self.selected)
            .finish_non_exhaustive()
    }
}

impl KeyBindsScreen {
    pub fn new(settings: SharedSettings) -> Self {
        Self {
            settings,
            rows: Vec::new(),
            widgets: WidgetSet::default(),
            footer: WidgetSet::default(),
            list: ScrollList::new([0, HEADER, 0, 0], ROW),
            gui: (0, 0),
            selected: None,
        }
    }

    fn bindings(&self) -> KeyBindings {
        self.settings.borrow().file.bindings.clone()
    }

    fn bind(&mut self, key: InputKey) {
        if let Some(i) = self.selected.take() {
            self.settings.borrow_mut().edit(|f| f.bindings.set(i, key));
        }
    }

    fn bar_x(&self) -> i32 {
        self.gui.0 / 2 + ROW_WIDTH / 2 + 8
    }

    fn reset_x(&self) -> i32 {
        self.bar_x() - RESET_WIDTH - 10
    }

    fn rebuild(&mut self) {
        self.widgets.clear();
        let (w, h) = self.gui;
        for row in &self.rows {
            if let Row::Key(i) = *row {
                self.widgets
                    .add(Button::new(2 * i as u32, [0, 0, CHANGE_WIDTH, ROW], ""));
                self.widgets.add(Button::new(
                    2 * i as u32 + 1,
                    [0, 0, RESET_WIDTH, ROW],
                    "lang:controls.reset",
                ));
            }
        }
        self.footer.clear();
        self.footer.add(Button::new(
            RESET_ALL,
            [w / 2 - 154, h - FOOTER + 6, 150, ROW],
            "lang:controls.resetAll",
        ));
        self.footer.add(Button::new(
            DONE,
            [w / 2 + 4, h - FOOTER + 6, 150, ROW],
            "lang:gui.done",
        ));
        self.place();
    }

    fn place(&mut self) {
        let [_, top, _, height] = self.list.rect();
        let bindings = self.bindings();
        let reset_x = self.reset_x();
        for (index, row) in self.rows.iter().enumerate() {
            let Row::Key(i) = *row else { continue };
            let y = self.list.item_top(index);
            let visible = y + ROW > top && y < top + height;
            if let Some(b) = self.widgets.button_mut(2 * i as u32) {
                b.rect = [reset_x - 5 - CHANGE_WIDTH, y, CHANGE_WIDTH, ROW];
                b.visible = visible;
            }
            if let Some(b) = self.widgets.button_mut(2 * i as u32 + 1) {
                b.rect = [reset_x, y, RESET_WIDTH, ROW];
                b.visible = visible;
                b.active = !bindings.is_default(i);
            }
        }
        if let Some(b) = self.footer.button_mut(RESET_ALL) {
            b.active = !bindings.all_default();
        }
    }
}

impl Screen for KeyBindsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.list
            .set_rect([0, HEADER, width, (height - HEADER - FOOTER).max(0)]);
        self.list.set_count(self.rows.len());
        self.rebuild();
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        if self.rows.is_empty() {
            self.rows = rows(|key| cx.lang(key));
            self.list.set_count(self.rows.len());
            self.rebuild();
        }
        self.place();
        let (w, h) = self.gui;
        draw_menu_background(cx);
        tile_texture(
            cx,
            "menu_list_background",
            [0, HEADER, w, (h - HEADER - FOOTER).max(0)],
        );
        for (name, y) in [
            ("header_separator", HEADER - 2),
            ("footer_separator", h - FOOTER),
        ] {
            if let Some(id) = cx.assets.texture(name) {
                for x in (0..w).step_by(32) {
                    cx.frame.sprite(id, x, y, 32, 2);
                }
            }
        }
        let title = cx.lang("controls.keybinds.title");
        cx.centered_text(&title, w / 2, 12, WHITE, true);
        let bindings = self.bindings();
        let left = w / 2 - ROW_WIDTH / 2;
        let mut stripes = Vec::new();
        for (index, row) in self.rows.iter().enumerate() {
            let Row::Key(i) = *row else { continue };
            let others = bindings.collisions(i);
            let key = key_label(cx, bindings.input(i));
            let label = change_label(&key, !others.is_empty(), self.selected == Some(i));
            let tooltip = (!others.is_empty()).then(|| {
                let names: Vec<String> =
                    others.iter().map(|&o| cx.lang(MAPPINGS[o].name)).collect();
                let text = cx.lang_fmt("controls.keybinds.duplicateKeybinds", &[&names.join(", ")]);
                TooltipLines::wrapped(cx, &text)
            });
            if let Some(b) = self.widgets.button_mut(2 * i as u32) {
                b.label = label;
                b.tooltip = tooltip;
                if !others.is_empty() {
                    stripes.push((b.rect[0] - 6, self.list.item_top(index)));
                }
            }
        }
        let [lx, ly, lw, lh] = self.list.rect();
        cx.frame.push_scissor(lx, ly, lx + lw, ly + lh);
        for index in self.list.visible_range() {
            let y = self.list.item_top(index);
            match self.rows[index] {
                Row::Category(c) => {
                    let text = format!("§n§l{}", cx.lang(c.label_key()));
                    cx.centered_text(&text, w / 2, y + 2 + 16 - 9, WHITE, true);
                }
                Row::Key(i) => {
                    let name = cx.lang(MAPPINGS[i].name);
                    cx.text(&name, left + 2, y + 2 + 8 - 4, WHITE, true);
                }
            }
        }
        for (x, y) in stripes {
            cx.frame.fill_rect(x, y + 1, x + 3, y + 18, COLLISION);
        }
        self.widgets.render(cx);
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, self.bar_x());
        self.footer.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if self.selected.is_some() {
            match *input {
                UiInput::Key { key, .. } => {
                    let bound = if key == Key::Escape {
                        InputKey::Unbound
                    } else {
                        input_key_from_ui(key)
                    };
                    self.bind(bound);
                    return true;
                }
                UiInput::MousePress { button, .. } => {
                    self.bind(match button {
                        MouseButton::Left => InputKey::Mouse(1),
                        MouseButton::Middle => InputKey::Mouse(2),
                        MouseButton::Right => InputKey::Mouse(3),
                        MouseButton::Other(n) => InputKey::Mouse(i32::from(n) + 1),
                    });
                    return true;
                }
                _ => {}
            }
        }
        if let UiInput::Key {
            key: Key::Escape, ..
        } = input
        {
            out.push(ScreenAction::Pop);
            return true;
        }
        if self.list.input(input, self.bar_x()) {
            self.place();
            return true;
        }
        let in_list = match *input {
            UiInput::MousePress { x, y, .. } => contains(self.list.rect(), x, y),
            _ => true,
        };
        let mut events = Vec::new();
        let mut used = in_list && self.widgets.input(env, input, &mut events);
        used |= self.footer.input(env, input, &mut events);
        for e in events.iter().filter(|e| e.kind == WidgetEventKind::Pressed) {
            out.push(ScreenAction::ClickSound);
            match e.id {
                RESET_ALL => self.settings.borrow_mut().edit(|f| f.bindings.reset_all()),
                DONE => out.push(ScreenAction::Pop),
                id if id % 2 == 0 => self.selected = Some(id as usize / 2),
                id => {
                    let i = id as usize / 2;
                    self.settings
                        .borrow_mut()
                        .edit(|f| f.bindings.set(i, MAPPINGS[i].default));
                }
            }
        }
        self.place();
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }

    fn captures_keys(&self) -> bool {
        self.selected.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_group_by_category_in_vanilla_order() {
        let rows = rows(|k| k.to_owned());
        assert_eq!(rows[0], Row::Category(Category::Movement));
        let categories: Vec<Category> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Category(c) => Some(*c),
                Row::Key(_) => None,
            })
            .collect();
        assert_eq!(categories, Category::SORTED.to_vec());
        let debug = rows
            .iter()
            .position(|r| *r == Row::Category(Category::Debug))
            .unwrap();
        let overlay = mcr_settings::bindings::mapping_index("key.debug.overlay").unwrap();
        assert_eq!(rows[debug + 1], Row::Key(overlay));
        assert_eq!(rows.len(), MAPPINGS.len() + Category::SORTED.len());
    }

    #[test]
    fn conflicts_are_bracketed_and_the_listening_key_marked() {
        assert_eq!(change_label("W", false, false), "W");
        assert_eq!(change_label("W", true, false), "§e[ §fW§e ]");
        assert!(change_label("W", false, true).starts_with("§e> "));
    }

    #[test]
    fn rebinding_onto_a_used_key_collides_until_reset() {
        let dir = std::env::temp_dir().join(format!("mcr-keybinds-{}", std::process::id()));
        let settings = crate::settings::Settings::load(&dir);
        let mut screen = KeyBindsScreen::new(settings.clone());
        let jump = mcr_settings::bindings::mapping_index("key.jump").unwrap();
        let forward = mcr_settings::bindings::mapping_index("key.forward").unwrap();
        screen.selected = Some(jump);
        screen.bind(MAPPINGS[forward].default);
        assert_eq!(
            settings.borrow().file.bindings.collisions(forward),
            vec![jump]
        );
        settings.borrow_mut().edit(|f| f.bindings.reset_all());
        assert!(
            settings
                .borrow()
                .file
                .bindings
                .collisions(forward)
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
