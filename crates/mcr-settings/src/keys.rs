use crate::key_table;

/// A physical input as vanilla names it. Keyboard codes are GLFW key codes, mouse codes are GLFW button indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputKey {
    Unbound,
    Keyboard(i32),
    Mouse(i32),
}

impl InputKey {
    pub fn name(self) -> &'static str {
        let (table, code) = match self {
            Self::Unbound => return "key.keyboard.unknown",
            Self::Keyboard(c) => (key_table::KEYBOARD, c),
            Self::Mouse(c) => (key_table::MOUSE, c),
        };
        table
            .iter()
            .find(|(_, v)| *v == code)
            .map_or("key.keyboard.unknown", |(n, _)| n)
    }

    pub fn from_name(name: &str) -> Option<Self> {
        if let Some(&(_, code)) = key_table::KEYBOARD.iter().find(|(n, _)| *n == name) {
            return Some(if code == 0 {
                Self::Unbound
            } else {
                Self::Keyboard(code)
            });
        }
        key_table::MOUSE
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, code)| Self::Mouse(code))
    }
}
