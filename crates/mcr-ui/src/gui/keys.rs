//! The key bindings container screens honour (`keyHotbarSlots`, `keySwapOffhand`, `keyDrop`, ...).

use super::input::{Key, MouseButton};
use crate::text::Lang;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyInput {
    Unbound,
    Key(Key),
    Mouse(MouseButton),
}

/// A bound key with its `options.txt` name, such as `key.keyboard.e`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenKey {
    pub input: KeyInput,
    pub name: &'static str,
}

impl ScreenKey {
    pub const fn key(key: Key, name: &'static str) -> Self {
        Self {
            input: KeyInput::Key(key),
            name,
        }
    }

    pub fn is_key(&self, key: Key) -> bool {
        self.input == KeyInput::Key(key)
    }

    pub fn is_mouse(&self, button: MouseButton) -> bool {
        self.input == KeyInput::Mouse(button)
    }

    /// `InputConstants.Key#getDisplayName`: the translation when there is one, else the key's own label.
    pub fn label(&self, lang: &Lang) -> String {
        if let Some(text) = lang.get(self.name) {
            return text.to_owned();
        }
        let suffix = self.name.rsplit('.').next().unwrap_or(self.name);
        if suffix.chars().count() == 1 {
            suffix.to_uppercase()
        } else {
            self.name.to_owned()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenKeys {
    pub hotbar: [ScreenKey; 9],
    pub swap_offhand: ScreenKey,
    pub drop: ScreenKey,
    pub inventory: ScreenKey,
    pub chat: ScreenKey,
    pub save_hotbar: ScreenKey,
}

impl Default for ScreenKeys {
    fn default() -> Self {
        const NAMES: [&str; 9] = [
            "key.keyboard.1",
            "key.keyboard.2",
            "key.keyboard.3",
            "key.keyboard.4",
            "key.keyboard.5",
            "key.keyboard.6",
            "key.keyboard.7",
            "key.keyboard.8",
            "key.keyboard.9",
        ];
        Self {
            hotbar: std::array::from_fn(|i| ScreenKey::key(Key::Digit(i as u8 + 1), NAMES[i])),
            swap_offhand: ScreenKey::key(Key::Letter('f'), "key.keyboard.f"),
            drop: ScreenKey::key(Key::Letter('q'), "key.keyboard.q"),
            inventory: ScreenKey::key(Key::Letter('e'), "key.keyboard.e"),
            chat: ScreenKey::key(Key::Letter('t'), "key.keyboard.t"),
            save_hotbar: ScreenKey::key(Key::Letter('c'), "key.keyboard.c"),
        }
    }
}

impl ScreenKeys {
    pub fn hotbar_key(&self, key: Key) -> Option<usize> {
        self.hotbar.iter().position(|k| k.is_key(key))
    }

    pub fn hotbar_mouse(&self, button: MouseButton) -> Option<usize> {
        self.hotbar.iter().position(|k| k.is_mouse(button))
    }
}
