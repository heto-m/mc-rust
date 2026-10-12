//! winit's events in SDL3's terms for Java in mod mode, and Java's window wishes back in winit's.

use std::sync::{Arc, Mutex, PoisonError};

use mcr_modbridge::input::{
    ClipboardHost, CursorKind, MAX_BUTTON, MAX_CLIPBOARD_BYTES, MemoryClipboard,
};
use mcr_settings::InputKey;
use winit::event::{KeyEvent, MouseScrollDelta};
use winit::keyboard::{Key, NamedKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::CursorIcon;

/// SDL wheel units per pixel of a precise scroll delta, as SDL's platform backends scale them.
const PIXELS_PER_WHEEL_STEP: f64 = 120.0;

/// The character the layout gives a key without modifiers, which becomes its SDL keycode.
pub fn printable(event: &KeyEvent) -> Option<char> {
    match event.key_without_modifiers() {
        Key::Character(s) => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        }
        Key::Named(NamedKey::Space) => Some(' '),
        _ => None,
    }
}

/// SDL wheel units: notches stay notches, pixel deltas are scaled down.
pub fn wheel(delta: MouseScrollDelta) -> (f64, f64) {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => (f64::from(x), f64::from(y)),
        MouseScrollDelta::PixelDelta(p) => {
            (p.x / PIXELS_PER_WHEEL_STEP, p.y / PIXELS_PER_WHEEL_STEP)
        }
    }
}

/// SDL's button numbers equal mcr's mouse key numbers (1 left, 2 middle, 3 right, 4 and 5 side).
pub fn sdl_button(key: InputKey) -> Option<u8> {
    match key {
        InputKey::Mouse(n) => u8::try_from(n)
            .ok()
            .filter(|b| (1..=MAX_BUTTON).contains(b)),
        _ => None,
    }
}

pub fn cursor_icon(kind: CursorKind) -> CursorIcon {
    match kind {
        CursorKind::Default | CursorKind::Arrow => CursorIcon::Default,
        CursorKind::IBeam => CursorIcon::Text,
        CursorKind::Crosshair => CursorIcon::Crosshair,
        CursorKind::PointingHand => CursorIcon::Pointer,
        CursorKind::ResizeNs => CursorIcon::NsResize,
        CursorKind::ResizeEw => CursorIcon::EwResize,
        CursorKind::ResizeAll => CursorIcon::Move,
        CursorKind::NotAllowed => CursorIcon::NotAllowed,
    }
}

/// The system clipboard, or a private one where none is reachable (headless runs).
pub struct SystemClipboard {
    system: Option<Mutex<arboard::Clipboard>>,
    fallback: MemoryClipboard,
}

impl SystemClipboard {
    pub fn open() -> Arc<Self> {
        let system = match arboard::Clipboard::new() {
            Ok(c) => Some(Mutex::new(c)),
            Err(e) => {
                eprintln!("[mods] no system clipboard ({e}); Java gets a private one");
                None
            }
        };
        Arc::new(Self {
            system,
            fallback: MemoryClipboard::default(),
        })
    }
}

impl ClipboardHost for SystemClipboard {
    fn get(&self) -> Option<String> {
        let Some(system) = &self.system else {
            return self.fallback.get();
        };
        let text = system
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_text()
            .ok()?;
        let mut end = text.len().min(MAX_CLIPBOARD_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Some(text[..end].to_owned())
    }

    fn set(&self, text: &str) {
        match &self.system {
            Some(system) => {
                let _ = system
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .set_text(text.to_owned());
            }
            None => self.fallback.set(text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{key_from_winit, mouse_from_winit};
    use mcr_modbridge::input::{SDLK_SCANCODE_MASK, keymod, keymod_bit, sdl_keycode};
    use winit::event::MouseButton;
    use winit::keyboard::KeyCode;

    fn sdl(code: KeyCode, printable: Option<char>) -> (u32, i32) {
        let InputKey::Keyboard(hid) = key_from_winit(code) else {
            panic!("{code:?} is unmapped");
        };
        let scancode = u32::try_from(hid).unwrap();
        (scancode, sdl_keycode(scancode, printable))
    }

    #[test]
    fn letters_and_digits() {
        assert_eq!(sdl(KeyCode::KeyA, Some('a')), (4, 'a' as i32));
        assert_eq!(sdl(KeyCode::KeyZ, Some('z')), (29, 'z' as i32));
        assert_eq!(sdl(KeyCode::Digit1, Some('1')), (30, '1' as i32));
        assert_eq!(sdl(KeyCode::Digit0, Some('0')), (39, '0' as i32));
        assert_eq!(sdl(KeyCode::Space, Some(' ')), (44, 0x20));
        assert_eq!(sdl(KeyCode::Enter, None), (40, 0x0D));
        assert_eq!(sdl(KeyCode::Escape, None), (41, 0x1B));
    }

    #[test]
    fn function_keys() {
        assert_eq!(sdl(KeyCode::F1, None), (58, SDLK_SCANCODE_MASK | 58));
        assert_eq!(sdl(KeyCode::F3, None), (60, SDLK_SCANCODE_MASK | 60));
        assert_eq!(sdl(KeyCode::F12, None), (69, SDLK_SCANCODE_MASK | 69));
        assert_eq!(sdl(KeyCode::F24, None), (115, SDLK_SCANCODE_MASK | 115));
    }

    #[test]
    fn numpad_keys_keep_their_scancodes() {
        assert_eq!(sdl(KeyCode::Numpad1, Some('1')), (89, 0x4000_0059));
        assert_eq!(sdl(KeyCode::Numpad0, Some('0')), (98, 0x4000_0062));
        assert_eq!(sdl(KeyCode::NumpadEnter, None), (88, 0x4000_0058));
        assert_eq!(sdl(KeyCode::NumpadAdd, Some('+')), (87, 0x4000_0057));
        assert_eq!(sdl(KeyCode::NumLock, None), (83, 0x4000_0053));
    }

    #[test]
    fn modifier_sides() {
        let bit = |code| match key_from_winit(code) {
            InputKey::Keyboard(hid) => keymod_bit(hid as u32),
            _ => 0,
        };
        assert_eq!(bit(KeyCode::ShiftLeft), keymod::LSHIFT);
        assert_eq!(bit(KeyCode::ShiftRight), keymod::RSHIFT);
        assert_eq!(bit(KeyCode::ControlLeft), keymod::LCTRL);
        assert_eq!(bit(KeyCode::ControlRight), keymod::RCTRL);
        assert_eq!(bit(KeyCode::AltLeft), keymod::LALT);
        assert_eq!(bit(KeyCode::AltRight), keymod::RALT);
        assert_eq!(bit(KeyCode::SuperLeft), keymod::LGUI);
        assert_eq!(bit(KeyCode::SuperRight), keymod::RGUI);
        assert_eq!(bit(KeyCode::KeyA), 0);
    }

    #[test]
    fn non_us_printable_keys() {
        assert_eq!(sdl(KeyCode::KeyY, Some('z')), (28, 'z' as i32), "QWERTZ");
        assert_eq!(sdl(KeyCode::Semicolon, Some('ö')), (51, 'ö' as i32));
        assert_eq!(sdl(KeyCode::KeyQ, Some('A')), (20, 'a' as i32), "AZERTY");
        assert_eq!(sdl(KeyCode::IntlBackslash, Some('<')), (100, '<' as i32));
    }

    #[test]
    fn buttons_and_wheel() {
        assert_eq!(sdl_button(mouse_from_winit(MouseButton::Left)), Some(1));
        assert_eq!(sdl_button(mouse_from_winit(MouseButton::Middle)), Some(2));
        assert_eq!(sdl_button(mouse_from_winit(MouseButton::Right)), Some(3));
        assert_eq!(sdl_button(mouse_from_winit(MouseButton::Back)), Some(4));
        assert_eq!(sdl_button(mouse_from_winit(MouseButton::Forward)), Some(5));
        assert_eq!(sdl_button(mouse_from_winit(MouseButton::Other(400))), None);
        assert_eq!(sdl_button(InputKey::Keyboard(4)), None);
        assert_eq!(wheel(MouseScrollDelta::LineDelta(0.0, -2.0)), (0.0, -2.0));
        let px = winit::dpi::PhysicalPosition::new(240.0, 60.0);
        assert_eq!(wheel(MouseScrollDelta::PixelDelta(px)), (2.0, 0.5));
    }

    #[test]
    fn cursors() {
        assert_eq!(cursor_icon(CursorKind::IBeam), CursorIcon::Text);
        assert_eq!(cursor_icon(CursorKind::PointingHand), CursorIcon::Pointer);
        assert_eq!(cursor_icon(CursorKind::Default), CursorIcon::Default);
    }
}
