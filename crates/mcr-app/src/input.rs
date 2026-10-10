use mcr_settings::InputKey;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

pub const KEY_ESCAPE: i32 = 41;
pub const KEY_ENTER: i32 = 40;
pub const KEY_BACKSPACE: i32 = 42;
pub const KEY_LEFT_CTRL: i32 = 224;

/// `mcr-settings` identifies keys by USB HID usage, so a physical key code maps one to one.
pub fn key_from_winit(code: KeyCode) -> InputKey {
    use KeyCode::*;
    let hid = match code {
        KeyA => 4,
        KeyB => 5,
        KeyC => 6,
        KeyD => 7,
        KeyE => 8,
        KeyF => 9,
        KeyG => 10,
        KeyH => 11,
        KeyI => 12,
        KeyJ => 13,
        KeyK => 14,
        KeyL => 15,
        KeyM => 16,
        KeyN => 17,
        KeyO => 18,
        KeyP => 19,
        KeyQ => 20,
        KeyR => 21,
        KeyS => 22,
        KeyT => 23,
        KeyU => 24,
        KeyV => 25,
        KeyW => 26,
        KeyX => 27,
        KeyY => 28,
        KeyZ => 29,
        Digit1 => 30,
        Digit2 => 31,
        Digit3 => 32,
        Digit4 => 33,
        Digit5 => 34,
        Digit6 => 35,
        Digit7 => 36,
        Digit8 => 37,
        Digit9 => 38,
        Digit0 => 39,
        Enter => KEY_ENTER,
        Escape => KEY_ESCAPE,
        Backspace => KEY_BACKSPACE,
        Tab => 43,
        Space => 44,
        Minus => 45,
        Equal => 46,
        BracketLeft => 47,
        BracketRight => 48,
        Backslash => 49,
        Semicolon => 51,
        Quote => 52,
        Backquote => 53,
        Comma => 54,
        Period => 55,
        Slash => 56,
        CapsLock => 57,
        F1 => 58,
        F2 => 59,
        F3 => 60,
        F4 => 61,
        F5 => 62,
        F6 => 63,
        F7 => 64,
        F8 => 65,
        F9 => 66,
        F10 => 67,
        F11 => 68,
        F12 => 69,
        Insert => 73,
        Home => 74,
        PageUp => 75,
        Delete => 76,
        End => 77,
        PageDown => 78,
        ArrowRight => 79,
        ArrowLeft => 80,
        ArrowDown => 81,
        ArrowUp => 82,
        ControlLeft => KEY_LEFT_CTRL,
        ShiftLeft => 225,
        AltLeft => 226,
        SuperLeft => 227,
        ControlRight => 228,
        ShiftRight => 229,
        AltRight => 230,
        SuperRight => 231,
        _ => return InputKey::Unbound,
    };
    InputKey::Keyboard(hid)
}

pub fn mouse_from_winit(button: MouseButton) -> InputKey {
    match button {
        MouseButton::Left => InputKey::Mouse(1),
        MouseButton::Right => InputKey::Mouse(3),
        MouseButton::Middle => InputKey::Mouse(2),
        MouseButton::Back => InputKey::Mouse(4),
        MouseButton::Forward => InputKey::Mouse(5),
        MouseButton::Other(n) => InputKey::Mouse(i32::from(n) + 1),
    }
}

pub const MAX_CHAT_CHARS: usize = 256;

/// One-line text field; the chat screen replaces it later.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LineEditor {
    text: String,
}

impl LineEditor {
    pub fn with_text(text: &str) -> Self {
        Self {
            text: text.chars().take(MAX_CHAT_CHARS).collect(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn insert(&mut self, typed: &str) {
        for c in typed.chars().filter(|c| !c.is_control()) {
            if self.text.chars().count() < MAX_CHAT_CHARS {
                self.text.push(c);
            }
        }
    }

    pub fn backspace(&mut self) {
        self.text.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hid_codes_match_the_settings_table() {
        assert_eq!(
            key_from_winit(KeyCode::KeyW),
            InputKey::from_name("key.keyboard.w").unwrap()
        );
        assert_eq!(
            key_from_winit(KeyCode::Space),
            InputKey::from_name("key.keyboard.space").unwrap()
        );
        assert_eq!(
            key_from_winit(KeyCode::F3),
            InputKey::from_name("key.keyboard.f3").unwrap()
        );
        assert_eq!(
            key_from_winit(KeyCode::ShiftLeft),
            InputKey::from_name("key.keyboard.left.shift").unwrap()
        );
        assert_eq!(
            mouse_from_winit(MouseButton::Right),
            InputKey::from_name("key.mouse.right").unwrap()
        );
        assert_eq!(
            key_from_winit(KeyCode::Slash),
            InputKey::from_name("key.keyboard.slash").unwrap()
        );
    }

    #[test]
    fn editor_drops_control_characters_and_bounds_length() {
        let mut e = LineEditor::default();
        e.insert("a\u{1b}[b\n");
        assert_eq!(e.text(), "a[b");
        e.insert(&"x".repeat(1000));
        assert_eq!(e.text().chars().count(), MAX_CHAT_CHARS);
        e.backspace();
        assert_eq!(e.text().chars().count(), MAX_CHAT_CHARS - 1);
    }
}
