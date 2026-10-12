//! Who gets input in mod mode: Java while its GUI is composed, mcr for native overlays, and both
//! for gameplay keys while no Java screen is open.

use mcr_modbridge::ModInput;
use mcr_modbridge::input::{CursorKind, JavaInput, NativeKind, us_printable};
use mcr_settings::InputKey;
use mcr_ui::{ConfirmScreen, Key, MouseButton, ScreenAction};

use super::Shell;
use crate::input::input_key_from_ui;
use crate::mod_input::sdl_button;
use crate::settings::OptionsScreen;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Authority {
    /// No Java GUI (no mods, Java still booting or detached): everything is mcr's.
    #[default]
    Native,
    /// mcr's own screen is drawn above Java's GUI and takes all input.
    Overlay,
    /// A Java screen (or the title, or Java's loading overlay) takes all input.
    JavaScreen,
    /// In game with no screen: gameplay keys are mcr's, everything also reaches Java.
    JavaGame,
}

impl Authority {
    pub fn java(self) -> bool {
        matches!(self, Self::JavaScreen | Self::JavaGame)
    }
}

/// What the window should do for Java this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JavaWindow {
    pub cursor: CursorKind,
    /// Text input on, with the focused field's area in window pixels when known.
    pub text_input: Option<Option<[i32; 4]>>,
}

/// Keyboard keys physically down, by USB HID usage.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct DownKeys([u64; 8]);

impl DownKeys {
    fn set(&mut self, hid: i32, down: bool) {
        if let Ok(n @ 0..512) = usize::try_from(hid) {
            let (word, bit) = (n / 64, 1u64 << (n % 64));
            if down {
                self.0[word] |= bit;
            } else {
                self.0[word] &= !bit;
            }
        }
    }

    fn iter(&self) -> impl Iterator<Item = InputKey> + '_ {
        (0..512usize)
            .filter(|n| self.0[n / 64] & (1 << (n % 64)) != 0)
            .map(|n| InputKey::Keyboard(n as i32))
    }
}

/// Text that comes with a key press: with an IME active, only `Ime::Commit` delivers text.
fn key_text(ime_active: bool, text: Option<&str>) -> Option<&str> {
    if ime_active { None } else { text }
}

#[derive(Debug, Default)]
pub struct Route {
    input: Option<ModInput>,
    pub authority: Authority,
    /// Java's GUI is composed this frame.
    pub gui: bool,
    pub java: JavaInput,
    cursor: (f64, f64),
    down: DownKeys,
    ime_active: bool,
}

impl Route {
    pub fn new(input: Option<ModInput>) -> Self {
        Self {
            input,
            ..Self::default()
        }
    }
}

fn mcr_menu() -> ConfirmScreen {
    ConfirmScreen::new(
        "mcr",
        "",
        vec![
            (
                "lang:menu.options".into(),
                Box::new(|| vec![ScreenAction::OpenOptions]),
            ),
            ("Mods...".into(), Box::new(|| vec![ScreenAction::OpenMods])),
            ("lang:gui.done".into(), Box::new(|| vec![ScreenAction::Pop])),
        ],
    )
    .with_panel()
}

impl Shell {
    /// Re-reads Java's state and decides the authority; runs before input of a frame is routed.
    pub(super) fn update_route(&mut self) {
        let Some(input) = self.route.input.clone() else {
            return;
        };
        let gui = self.mod_frames().is_some_and(|f| f.lock().gui_active());
        let java = input.java();
        if java.quit {
            self.quit = true;
        }
        self.route.gui = gui;
        self.route.java = java;
        if let Some(kind) = input.take_native_request()
            && gui
        {
            self.open_native(kind);
        }
        let overlay = match &self.play {
            Some(play) => play.screen_open(),
            None => self.stack.len() > 1,
        };
        let next = if !gui {
            Authority::Native
        } else if overlay {
            Authority::Overlay
        } else if java.captures_input() || self.play.is_none() {
            Authority::JavaScreen
        } else {
            Authority::JavaGame
        };
        let prev = std::mem::replace(&mut self.route.authority, next);
        if prev != next {
            if prev == Authority::JavaGame
                && let Some(play) = &mut self.play
            {
                play.release_all();
            }
            if prev.java() && !next.java() {
                input.release_all();
            }
            if prev == Authority::JavaScreen
                && next == Authority::JavaGame
                && let Some(play) = &mut self.play
            {
                let held: Vec<InputKey> = self.route.down.iter().collect();
                play.resume_held(&held);
            }
        }
        let scale = (gui && java.gui_scale > 0).then_some(java.gui_scale as i32);
        let mut open_mods = false;
        if let Some(play) = &mut self.play {
            play.java_screen = next == Authority::JavaScreen;
            play.gui_scale_override = scale;
            open_mods = std::mem::take(&mut play.open_mods);
        }
        if open_mods {
            self.open_mods();
        }
    }

    /// Java's "mcr" button: mcr's menu, settings or Mods screen above Java's GUI.
    fn open_native(&mut self, kind: NativeKind) {
        match kind {
            NativeKind::Mods => self.open_mods(),
            NativeKind::Menu => self.show_native(Box::new(mcr_menu())),
            NativeKind::Settings => {
                let screen = OptionsScreen::new(self.settings.clone(), self.languages());
                self.show_native(Box::new(screen));
            }
        }
    }

    fn show_native(&mut self, screen: Box<dyn mcr_ui::Screen>) {
        match &mut self.play {
            Some(play) => play.show_screen(screen),
            None => self.push(screen),
        }
    }

    /// The GUI scale setting native screens use: Java's own while its GUI is composed.
    pub(super) fn gui_scale_setting(&self) -> i32 {
        if self.route.gui && self.route.java.gui_scale > 0 {
            self.route.java.gui_scale as i32
        } else {
            self.settings.borrow().file.options.gui_scale
        }
    }

    fn java_input(&self) -> Option<ModInput> {
        if self.route.authority.java() {
            self.route.input.clone()
        } else {
            None
        }
    }

    /// Hands a key or button to Java; `true` when mcr's own handling is skipped.
    pub(super) fn route_key(
        &mut self,
        key: InputKey,
        printable: Option<char>,
        pressed: bool,
        repeat: bool,
        text: Option<&str>,
    ) -> bool {
        if let (Some(_), InputKey::Keyboard(hid)) = (&self.route.input, key) {
            self.route.down.set(hid, pressed);
        }
        let Some(input) = self.java_input() else {
            return false;
        };
        match key {
            InputKey::Keyboard(hid) => {
                let Ok(scancode) = u32::try_from(hid) else {
                    return true;
                };
                input.key(scancode, printable, pressed, repeat);
                if pressed
                    && self.route.java.text_input
                    && let Some(text) = key_text(self.route.ime_active, text)
                {
                    input.text(text);
                }
            }
            InputKey::Mouse(_) => {
                if let Some(button) = sdl_button(key) {
                    input.button(button, pressed);
                }
            }
            _ => {}
        }
        self.route.authority == Authority::JavaScreen || repeat
    }

    pub(super) fn route_cursor(&mut self, px: f64, py: f64) -> bool {
        let Some(input) = self.java_input() else {
            return false;
        };
        let (lx, ly) = self.route.cursor;
        if !self.route.java.grabbed {
            input.motion(px, py, px - lx, py - ly);
        }
        self.route.cursor = (px, py);
        self.route.authority == Authority::JavaScreen
    }

    pub(super) fn route_motion(&mut self, dx: f64, dy: f64) -> bool {
        let Some(input) = self.java_input() else {
            return false;
        };
        if self.route.java.grabbed {
            let (x, y) = self.route.cursor;
            input.motion(x, y, dx, dy);
        }
        self.route.authority == Authority::JavaScreen
    }

    /// A wheel turn in SDL units (`dx`, `dy`) and mcr's own hotbar steps.
    pub fn wheel(&mut self, dx: f64, dy: f64, steps: f32) {
        if let Some(input) = self.java_input() {
            input.wheel(dx, dy);
            if self.route.authority == Authority::JavaScreen {
                return;
            }
        }
        self.scroll(steps);
    }

    pub(super) fn route_text(&mut self, text: &str) -> bool {
        let Some(input) = self.java_input() else {
            return false;
        };
        input.text(text);
        true
    }

    pub(super) fn route_ui_key(&mut self, key: Key) -> bool {
        if self.java_input().is_none() {
            return false;
        }
        let k = input_key_from_ui(key);
        let printable = match k {
            InputKey::Keyboard(hid) => u32::try_from(hid).ok().and_then(us_printable),
            _ => None,
        };
        self.route_key(k, printable, true, false, None);
        self.route_key(k, printable, false, false, None);
        true
    }

    /// A click at GUI coordinates, scaled by Java's GUI scale (scripted input).
    pub(super) fn route_click(&mut self, x: f32, y: f32, button: MouseButton) -> bool {
        if self.java_input().is_none() {
            return false;
        }
        let scale = f64::from(self.route.java.gui_scale.max(1));
        let (px, py) = (f64::from(x) * scale, f64::from(y) * scale);
        self.route_cursor(px, py);
        let key = InputKey::Mouse(match button {
            MouseButton::Left => 1,
            MouseButton::Middle => 2,
            MouseButton::Right => 3,
            MouseButton::Other(n) => i32::from(n),
        });
        self.route_key(key, None, true, false, None);
        self.route_key(key, None, false, false, None);
        true
    }

    pub(super) fn route_hover(&mut self, x: f32, y: f32) -> bool {
        if self.java_input().is_none() {
            return false;
        }
        let scale = f64::from(self.route.java.gui_scale.max(1));
        self.route_cursor(f64::from(x) * scale, f64::from(y) * scale);
        true
    }

    pub fn ime_active(&mut self, active: bool) {
        self.route.ime_active = active;
    }

    /// Committed IME text.
    pub fn ime_commit(&mut self, text: &str) {
        if self.route.java.text_input {
            self.route_text(text);
        }
    }

    /// IME composition; `cursor` is a byte range of `text` as winit reports it.
    pub fn ime_preedit(&mut self, text: &str, cursor: Option<(usize, usize)>) {
        let Some(input) = self.java_input() else {
            return;
        };
        let count = |a: usize, b: usize| {
            text.get(a..b)
                .map_or(0, |s| i32::try_from(s.chars().count()).unwrap_or(i32::MAX))
        };
        let (start, length) = match cursor {
            Some((a, b)) if a <= b => (count(0, a), count(a, b)),
            _ => (-1, 0),
        };
        input.preedit(text, start, length);
    }

    pub fn dropped_file(&mut self, path: &std::path::Path) {
        if let Some(input) = self.java_input() {
            input.drop_files(vec![path.to_string_lossy().into_owned()]);
        }
    }

    pub fn focus(&mut self, focused: bool) {
        if let Some(input) = &self.route.input {
            input.set_focused(focused);
        }
        if !focused {
            self.lose_focus();
        }
    }

    /// Grab wish in mod mode: Java's while it has input, none under a native overlay.
    pub(super) fn route_grab(&self) -> Option<bool> {
        match self.route.authority {
            Authority::Native => None,
            Authority::Overlay => Some(false),
            Authority::JavaScreen | Authority::JavaGame => Some(self.route.java.grabbed),
        }
    }

    /// Cursor shape and text input Java wants on the window, while it has input.
    pub fn java_window(&self) -> Option<JavaWindow> {
        self.route.authority.java().then(|| JavaWindow {
            cursor: self.route.java.cursor,
            text_input: self
                .route
                .java
                .text_input
                .then_some(self.route.java.text_area),
        })
    }

    pub fn mod_authority(&self) -> Authority {
        self.route.authority
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ime_commit_is_the_only_text_source_while_active() {
        assert_eq!(key_text(false, Some("a")), Some("a"));
        assert_eq!(key_text(true, Some("a")), None);
        assert_eq!(key_text(true, None), None);
    }

    #[test]
    fn down_keys_track_presses_and_ignore_out_of_range() {
        let mut down = DownKeys::default();
        down.set(26, true);
        down.set(4, true);
        down.set(26, false);
        down.set(-1, true);
        down.set(5000, true);
        let keys: Vec<_> = down.iter().collect();
        assert_eq!(keys, vec![InputKey::Keyboard(4)]);
    }
}
