//! Shared `options.txt` state and the Options screens.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use mcr_settings::{InputKey, MAPPINGS, Options, OptionsFile, SOUND_CATEGORIES};
use mcr_ui::{
    Button, CycleButton, Env, Key, LanguageInfo, MouseButton, RenderCx, Screen, ScreenAction,
    ScrollList, Slider, UiInput, WidgetEventKind, WidgetSet, draw_menu_background,
};

use crate::input::input_key_from_ui;
use crate::language::LanguageScreen;

#[derive(Debug)]
pub struct Settings {
    pub file: OptionsFile,
    path: PathBuf,
    /// Bumped on every edit so the running session knows to re-apply.
    pub version: u64,
    /// Java runs a dynamic light mod, so Video Settings offers mcr's switch for it; not saved.
    pub dynamic_lights_provider: bool,
}

pub type SharedSettings = Rc<RefCell<Settings>>;

impl Settings {
    pub fn load(game_dir: &std::path::Path) -> SharedSettings {
        let path = game_dir.join("options.txt");
        let file = OptionsFile::load(&path).unwrap_or_else(|e| {
            eprintln!("options.txt unreadable: {e}");
            OptionsFile::default()
        });
        Rc::new(RefCell::new(Self {
            file,
            path,
            version: 1,
            dynamic_lights_provider: false,
        }))
    }

    pub fn edit(&mut self, f: impl FnOnce(&mut OptionsFile)) {
        f(&mut self.file);
        self.version += 1;
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = self.file.save(&self.path) {
            eprintln!("options.txt not saved: {e}");
        }
    }
}

const DONE: u32 = 1000;
const VIDEO: u32 = 1;
const SOUND: u32 = 2;
const CONTROLS: u32 = 3;
const CHAT: u32 = 4;
const LANGUAGE: u32 = 5;

pub struct OptionsScreen {
    languages: Vec<LanguageInfo>,
    settings: SharedSettings,
    widgets: WidgetSet,
    gui: (i32, i32),
}

impl std::fmt::Debug for OptionsScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OptionsScreen").finish()
    }
}

impl OptionsScreen {
    pub fn new(settings: SharedSettings, languages: Vec<LanguageInfo>) -> Self {
        Self {
            languages,
            settings,
            widgets: WidgetSet::default(),
            gui: (0, 0),
        }
    }
}

impl Screen for OptionsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let x = width / 2 - 155;
        let y = height / 6 + 24;
        for (i, (id, label)) in [
            (VIDEO, "lang:options.video"),
            (SOUND, "lang:options.sounds"),
            (CONTROLS, "lang:options.controls"),
            (CHAT, "lang:options.chat"),
            (LANGUAGE, "lang:options.language"),
        ]
        .into_iter()
        .enumerate()
        {
            let (col, row) = (i as i32 % 2, i as i32 / 2);
            self.widgets.add(Button::new(
                id,
                [x + col * 160, y + row * 24, 150, 20],
                label,
            ));
        }
        self.widgets.add(Button::new(
            DONE,
            [width / 2 - 100, height / 6 + 168, 200, 20],
            "lang:gui.done",
        ));
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let title = cx.lang("options.title");
        cx.centered_text(&title, self.gui.0 / 2, 15, 0xFFFF_FFFF, true);
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
        for e in events.iter().filter(|e| e.kind == WidgetEventKind::Pressed) {
            out.push(ScreenAction::ClickSound);
            let settings = self.settings.clone();
            match e.id {
                VIDEO => out.push(ScreenAction::Push(Box::new(FormScreen::video(settings)))),
                SOUND => out.push(ScreenAction::Push(Box::new(FormScreen::sound(settings)))),
                CONTROLS => out.push(ScreenAction::Push(Box::new(ControlsScreen::new(settings)))),
                CHAT => out.push(ScreenAction::Push(Box::new(FormScreen::chat(settings)))),
                LANGUAGE => out.push(ScreenAction::Push(Box::new(LanguageScreen::new(
                    settings,
                    self.languages.clone(),
                )))),
                DONE => out.push(ScreenAction::Pop),
                _ => {}
            }
        }
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }
}

type Setter = Box<dyn Fn(&mut Options, f64)>;

enum Item {
    Slider {
        get: Box<dyn Fn(&Options) -> f64>,
        set: Setter,
        text: Box<dyn Fn(&Options) -> String>,
    },
    Toggle {
        label: &'static str,
        get: fn(&Options) -> bool,
        set: fn(&mut Options, bool),
    },
}

fn unit(
    label: &'static str,
    get: fn(&Options) -> f64,
    set: fn(&mut Options, f64),
    percent: bool,
) -> Item {
    Item::Slider {
        get: Box::new(get),
        set: Box::new(set),
        text: Box::new(move |o| {
            if percent {
                format!("{label}: {}%", (get(o) * 100.0).round() as i32)
            } else {
                format!("{label}: {:.2}", get(o))
            }
        }),
    }
}

fn stepped(
    label: &'static str,
    (lo, hi): (i32, i32),
    get: fn(&Options) -> i32,
    set: fn(&mut Options, i32),
    suffix: &'static str,
) -> Item {
    let span = f64::from(hi - lo);
    Item::Slider {
        get: Box::new(move |o| f64::from(get(o) - lo) / span),
        set: Box::new(move |o, v| set(o, lo + (v * span).round() as i32)),
        text: Box::new(move |o| format!("{label}: {}{suffix}", get(o))),
    }
}

pub struct FormScreen {
    title: &'static str,
    settings: SharedSettings,
    items: Vec<Item>,
    widgets: WidgetSet,
    gui: (i32, i32),
}

impl std::fmt::Debug for FormScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormScreen")
            .field("title", &self.title)
            .finish()
    }
}

impl FormScreen {
    fn new(title: &'static str, settings: SharedSettings, items: Vec<Item>) -> Self {
        Self {
            title,
            settings,
            items,
            widgets: WidgetSet::default(),
            gui: (0, 0),
        }
    }

    pub fn video(settings: SharedSettings) -> Self {
        let mut items = vec![
            stepped(
                "Render Distance",
                (2, 32),
                |o| o.render_distance,
                |o, v| o.render_distance = v,
                " chunks",
            ),
            stepped(
                "Max Framerate",
                (10, 260),
                |o| o.max_fps,
                |o, v| o.max_fps = v,
                " fps",
            ),
            Item::Toggle {
                label: "VSync",
                get: |o| o.enable_vsync,
                set: |o, v| o.enable_vsync = v,
            },
            stepped(
                "GUI Scale (0 = auto)",
                (0, 4),
                |o| o.gui_scale,
                |o, v| o.gui_scale = v,
                "",
            ),
            stepped(
                "FOV",
                (30, 110),
                |o| o.fov.0,
                |o, v| o.fov = mcr_settings::value::Fov(v as _),
                "",
            ),
            unit("Brightness", |o| o.gamma, |o, v| o.gamma = v, true),
            Item::Toggle {
                label: "View Bobbing",
                get: |o| o.bob_view,
                set: |o, v| o.bob_view = v,
            },
            Item::Toggle {
                label: "Entity Shadows",
                get: |o| o.entity_shadows,
                set: |o, v| o.entity_shadows = v,
            },
        ];
        if settings.borrow().dynamic_lights_provider {
            items.push(Item::Toggle {
                label: "Mod Dynamic Lights",
                get: |o| o.mcr_dynamic_lights,
                set: |o, v| o.mcr_dynamic_lights = v,
            });
        }
        Self::new("Video Settings", settings, items)
    }

    pub fn sound(settings: SharedSettings) -> Self {
        let items = (0..SOUND_CATEGORIES.len())
            .map(|i| {
                let label: &'static str = SOUND_CATEGORIES[i];
                Item::Slider {
                    get: Box::new(move |o| o.sound_volumes[i]),
                    set: Box::new(move |o, v| o.sound_volumes[i] = v),
                    text: Box::new(move |o| {
                        format!(
                            "{}: {}%",
                            capitalise(label),
                            (o.sound_volumes[i] * 100.0).round() as i32
                        )
                    }),
                }
            })
            .collect();
        Self::new("Music & Sounds", settings, items)
    }

    pub fn chat(settings: SharedSettings) -> Self {
        let items = vec![
            Item::Toggle {
                label: "Colors",
                get: |o| o.chat_colors,
                set: |o, v| o.chat_colors = v,
            },
            Item::Toggle {
                label: "Web Links",
                get: |o| o.chat_links,
                set: |o, v| o.chat_links = v,
            },
            Item::Toggle {
                label: "Prompt on Links",
                get: |o| o.chat_links_prompt,
                set: |o, v| o.chat_links_prompt = v,
            },
            Item::Toggle {
                label: "Auto-Suggest Commands",
                get: |o| o.auto_suggestions,
                set: |o, v| o.auto_suggestions = v,
            },
            unit(
                "Opacity",
                |o| o.chat_opacity,
                |o, v| o.chat_opacity = v,
                true,
            ),
            unit(
                "Text Background",
                |o| o.text_background_opacity,
                |o, v| o.text_background_opacity = v,
                true,
            ),
            unit("Width", |o| o.chat_width, |o, v| o.chat_width = v, true),
            unit(
                "Height (focused)",
                |o| o.chat_height_focused,
                |o, v| o.chat_height_focused = v,
                true,
            ),
            unit(
                "Height (unfocused)",
                |o| o.chat_height_unfocused,
                |o, v| o.chat_height_unfocused = v,
                true,
            ),
        ];
        Self::new("Chat Settings", settings, items)
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

impl Screen for FormScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let opts = self.settings.borrow().file.options.clone();
        for (i, item) in self.items.iter().enumerate() {
            let (col, row) = (i as i32 % 2, i as i32 / 2);
            let rect = [width / 2 - 155 + col * 160, 36 + row * 24, 150, 20];
            match item {
                Item::Slider { get, text, .. } => {
                    self.widgets
                        .add(Slider::new(i as u32, rect, text(&opts), get(&opts) as f32));
                }
                Item::Toggle { label, get, .. } => {
                    let index = usize::from(!get(&opts));
                    self.widgets.add(CycleButton::new(
                        i as u32,
                        rect,
                        *label,
                        vec!["ON".into(), "OFF".into()],
                        index,
                    ));
                }
            }
        }
        self.widgets.add(Button::new(
            DONE,
            [width / 2 - 100, height - 27, 200, 20],
            "lang:gui.done",
        ));
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        cx.centered_text(self.title, self.gui.0 / 2, 15, 0xFFFF_FFFF, true);
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
        for e in events {
            let Some(item) = self.items.get(e.id as usize) else {
                if e.id == DONE && e.kind == WidgetEventKind::Pressed {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                continue;
            };
            match (item, e.kind) {
                (Item::Slider { set, text, .. }, WidgetEventKind::SliderChanged(v)) => {
                    let label = {
                        let mut s = self.settings.borrow_mut();
                        s.edit(|f| set(&mut f.options, f64::from(v)));
                        text(&s.file.options)
                    };
                    if let Some(w) = self.widgets.slider_mut(e.id) {
                        w.label = label;
                    }
                }
                (Item::Toggle { set, .. }, WidgetEventKind::Cycled(i)) => {
                    self.settings
                        .borrow_mut()
                        .edit(|f| set(&mut f.options, i == 0));
                    out.push(ScreenAction::ClickSound);
                }
                _ => {}
            }
        }
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }
}

const ROW: i32 = 24;

pub struct ControlsScreen {
    settings: SharedSettings,
    widgets: WidgetSet,
    list: ScrollList,
    gui: (i32, i32),
    listening: Option<usize>,
    mouse: (f32, f32),
}

impl std::fmt::Debug for ControlsScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ControlsScreen").finish()
    }
}

impl ControlsScreen {
    pub fn new(settings: SharedSettings) -> Self {
        Self {
            settings,
            widgets: WidgetSet::default(),
            list: ScrollList::new([0, 32, 0, 0], ROW),
            gui: (0, 0),
            listening: None,
            mouse: (0.0, 0.0),
        }
    }

    fn bind(&mut self, key: InputKey) {
        if let Some(i) = self.listening.take() {
            self.settings.borrow_mut().edit(|f| f.bindings.set(i, key));
        }
    }

    fn button_rect(&self, index: usize) -> [i32; 4] {
        [self.gui.0 / 2 + 5, self.list.item_top(index), 150, 20]
    }
}

impl Screen for ControlsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.list
            .set_rect([0, 32, width, (height - 32 - 36).max(0)]);
        self.list.set_count(MAPPINGS.len());
        self.widgets.clear();
        self.widgets.add(Button::new(
            DONE,
            [width / 2 - 100, height - 27, 200, 20],
            "lang:gui.done",
        ));
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        cx.centered_text("Controls", self.gui.0 / 2, 12, 0xFFFF_FFFF, true);
        let [lx, ly, lw, lh] = self.list.rect();
        cx.frame.push_scissor(lx, ly, lx + lw, ly + lh);
        for index in self.list.visible_range() {
            let y = self.list.item_top(index);
            let bindings = self.settings.borrow().file.bindings.clone();
            let name = cx.lang(MAPPINGS[index].name);
            cx.text(&name, self.gui.0 / 2 - 155, y + 6, 0xFFFF_FFFF, true);
            let rect = self.button_rect(index);
            let hot = mcr_ui::contains(rect, self.mouse.0, self.mouse.1);
            cx.sprite(
                if hot {
                    "widget/button_highlighted"
                } else {
                    "widget/button"
                },
                rect[0],
                rect[1],
                rect[2],
                rect[3],
            );
            let label = if self.listening == Some(index) {
                "> ??? <".to_owned()
            } else {
                let key = bindings.input(index);
                if key == InputKey::Unbound {
                    "Not bound".to_owned()
                } else {
                    key_label(cx, key)
                }
            };
            let w = cx.text_width(&label);
            cx.text(
                &label,
                rect[0] + (rect[2] - w) / 2,
                y + 6,
                0xFFFF_FFFF,
                true,
            );
        }
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, self.gui.0 / 2 + 160);
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        match *input {
            UiInput::MouseMove { x, y } => self.mouse = (x, y),
            UiInput::Key { key, .. } if self.listening.is_some() => {
                if key == Key::Escape {
                    self.bind(InputKey::Unbound);
                } else {
                    self.bind(input_key_from_ui(key));
                }
                return true;
            }
            UiInput::MousePress { x, y, button, .. } if self.listening.is_some() => {
                let _ = (x, y);
                self.bind(match button {
                    MouseButton::Left => InputKey::Mouse(1),
                    MouseButton::Middle => InputKey::Mouse(2),
                    MouseButton::Right => InputKey::Mouse(3),
                    MouseButton::Other(n) => InputKey::Mouse(i32::from(n) + 1),
                });
                return true;
            }
            UiInput::MousePress {
                x,
                y,
                button: MouseButton::Left,
                ..
            } => {
                if mcr_ui::contains(self.list.rect(), x, y)
                    && let Some(i) = self.list.item_at(x, y)
                    && mcr_ui::contains(self.button_rect(i), x, y)
                {
                    self.listening = Some(i);
                    out.push(ScreenAction::ClickSound);
                    return true;
                }
            }
            UiInput::Key {
                key: Key::Escape, ..
            } => {
                out.push(ScreenAction::Pop);
                return true;
            }
            _ => {}
        }
        if self.list.input(input, self.gui.0 / 2 + 160) {
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        if events
            .iter()
            .any(|e| e.id == DONE && e.kind == WidgetEventKind::Pressed)
        {
            out.push(ScreenAction::ClickSound);
            out.push(ScreenAction::Pop);
        }
        used
    }
}

fn key_label(cx: &mut RenderCx<'_, '_>, key: InputKey) -> String {
    let name = key.name();
    let translated = cx.lang(name);
    if translated == name {
        name.rsplit('.').next().unwrap_or(name).to_uppercase()
    } else {
        translated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_bump_the_version_and_persist() {
        let dir = std::env::temp_dir().join(format!("mcr-settings-{}", std::process::id()));
        let settings = Settings::load(&dir);
        let before = settings.borrow().version;
        settings
            .borrow_mut()
            .edit(|f| f.options.render_distance = 5);
        assert!(settings.borrow().version > before);
        let again = Settings::load(&dir);
        assert_eq!(again.borrow().file.options.render_distance, 5);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn dynamic_lights_switch_only_with_a_provider() {
        let dir = std::env::temp_dir().join(format!("mcr-settings-dl-{}", std::process::id()));
        let settings = Settings::load(&dir);
        let labels = |s: &SharedSettings| -> Vec<&'static str> {
            FormScreen::video(s.clone())
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Toggle { label, .. } => Some(*label),
                    Item::Slider { .. } => None,
                })
                .collect()
        };
        assert!(!labels(&settings).contains(&"Mod Dynamic Lights"));
        settings.borrow_mut().dynamic_lights_provider = true;
        assert!(labels(&settings).contains(&"Mod Dynamic Lights"));
        assert!(settings.borrow().file.options.mcr_dynamic_lights);
        let _ = std::fs::remove_dir_all(dir);
    }
}
