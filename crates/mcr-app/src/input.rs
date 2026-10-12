use mcr_settings::InputKey;
use mcr_ui::{Key, Modifiers, MouseButton as UiMouseButton, UiInput};
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
        IntlBackslash => 100,
        PrintScreen => 70,
        ScrollLock => 71,
        Pause => 72,
        NumLock => 83,
        NumpadDivide => 84,
        NumpadMultiply => 85,
        NumpadSubtract => 86,
        NumpadAdd => 87,
        NumpadEnter => 88,
        Numpad1 => 89,
        Numpad2 => 90,
        Numpad3 => 91,
        Numpad4 => 92,
        Numpad5 => 93,
        Numpad6 => 94,
        Numpad7 => 95,
        Numpad8 => 96,
        Numpad9 => 97,
        Numpad0 => 98,
        NumpadDecimal => 99,
        ContextMenu => 101,
        NumpadEqual => 103,
        F13 => 104,
        F14 => 105,
        F15 => 106,
        F16 => 107,
        F17 => 108,
        F18 => 109,
        F19 => 110,
        F20 => 111,
        F21 => 112,
        F22 => 113,
        F23 => 114,
        F24 => 115,
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

pub fn track_modifiers(mods: &mut Modifiers, key: InputKey, pressed: bool) {
    if let InputKey::Keyboard(code) = key {
        match code {
            224 | 228 => mods.ctrl = pressed,
            225 | 229 => mods.shift = pressed,
            226 | 230 => mods.alt = pressed,
            _ => {}
        }
    }
}

/// The UI events for one key or mouse button transition; typed text follows the key press.
pub fn ui_inputs(
    key: InputKey,
    pressed: bool,
    text: Option<&str>,
    mods: Modifiers,
    mouse: (f32, f32),
) -> Vec<UiInput> {
    let (x, y) = mouse;
    let mut out = Vec::new();
    match key {
        InputKey::Mouse(n) => {
            let button = match n {
                1 => UiMouseButton::Left,
                2 => UiMouseButton::Middle,
                3 => UiMouseButton::Right,
                n => UiMouseButton::Other(n.clamp(0, 255) as u8),
            };
            out.push(if pressed {
                UiInput::MousePress {
                    x,
                    y,
                    button,
                    mods,
                    double: false,
                }
            } else {
                UiInput::MouseRelease { x, y, button, mods }
            });
            return out;
        }
        InputKey::Keyboard(_) if pressed => out.push(UiInput::Key {
            key: ui_key_from_input(key),
            mods,
            repeat: false,
        }),
        _ => {}
    }
    if pressed && !mods.ctrl {
        out.extend(
            text.unwrap_or("")
                .chars()
                .filter(|c| !c.is_control())
                .map(UiInput::Char),
        );
    }
    out
}

/// USB HID usage of a keyboard key to the UI crate's key enum.
pub fn ui_key_from_input(key: InputKey) -> Key {
    let InputKey::Keyboard(hid) = key else {
        return Key::Other(0);
    };
    match hid {
        4..=29 => Key::Letter((b'a' + (hid - 4) as u8) as char),
        30..=38 => Key::Digit((hid - 29) as u8),
        39 => Key::Digit(0),
        KEY_ENTER => Key::Enter,
        KEY_ESCAPE => Key::Escape,
        KEY_BACKSPACE => Key::Backspace,
        43 => Key::Tab,
        44 => Key::Space,
        58..=69 => Key::Function((hid - 57) as u8),
        74 => Key::Home,
        75 => Key::PageUp,
        76 => Key::Delete,
        77 => Key::End,
        78 => Key::PageDown,
        79 => Key::Right,
        80 => Key::Left,
        81 => Key::Down,
        82 => Key::Up,
        other => Key::Other(other as u32),
    }
}

pub fn input_key_from_ui(key: Key) -> InputKey {
    let hid = match key {
        Key::Letter(c) if c.is_ascii_lowercase() => 4 + (c as i32 - 'a' as i32),
        Key::Letter(_) => return InputKey::Unbound,
        Key::Digit(0) => 39,
        Key::Digit(n) => 29 + i32::from(n),
        Key::Enter => KEY_ENTER,
        Key::Escape => KEY_ESCAPE,
        Key::Backspace => KEY_BACKSPACE,
        Key::Tab => 43,
        Key::Space => 44,
        Key::Function(n) => 57 + i32::from(n),
        Key::Home => 74,
        Key::PageUp => 75,
        Key::Delete => 76,
        Key::End => 77,
        Key::PageDown => 78,
        Key::Right => 79,
        Key::Left => 80,
        Key::Down => 81,
        Key::Up => 82,
        Key::Other(n) if n > 0 => n as i32,
        Key::Other(_) => return InputKey::Unbound,
    };
    InputKey::Keyboard(hid)
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
    fn ui_keys_round_trip() {
        for hid in (4..=44).chain(58..=69).chain(74..=82) {
            let k = InputKey::Keyboard(hid);
            assert_eq!(input_key_from_ui(ui_key_from_input(k)), k, "{hid}");
        }
    }
}
