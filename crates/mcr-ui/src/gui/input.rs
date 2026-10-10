#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u8),
}

impl MouseButton {
    /// Vanilla's button numbering: left 0, right 1, middle 2.
    pub fn index(self) -> i32 {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Middle => 2,
            Self::Other(n) => i32::from(n),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        shift: false,
        ctrl: false,
        alt: false,
    };
    pub const SHIFT: Self = Self {
        shift: true,
        ctrl: false,
        alt: false,
    };
    pub const CTRL: Self = Self {
        shift: false,
        ctrl: true,
        alt: false,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Escape,
    Enter,
    Tab,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
    Digit(u8),
    Letter(char),
    Function(u8),
    Other(u32),
}

/// Input in GUI pixels (physical position divided by the frame scale), independent of the window crate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiInput {
    MouseMove {
        x: f32,
        y: f32,
    },
    MousePress {
        x: f32,
        y: f32,
        button: MouseButton,
        mods: Modifiers,
        double: bool,
    },
    MouseRelease {
        x: f32,
        y: f32,
        button: MouseButton,
        mods: Modifiers,
    },
    Scroll {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    Key {
        key: Key,
        mods: Modifiers,
        repeat: bool,
    },
    Char(char),
}

impl UiInput {
    pub fn press(x: f32, y: f32, button: MouseButton) -> Self {
        Self::MousePress {
            x,
            y,
            button,
            mods: Modifiers::NONE,
            double: false,
        }
    }

    pub fn release(x: f32, y: f32, button: MouseButton) -> Self {
        Self::MouseRelease {
            x,
            y,
            button,
            mods: Modifiers::NONE,
        }
    }

    pub fn key(key: Key) -> Self {
        Self::Key {
            key,
            mods: Modifiers::NONE,
            repeat: false,
        }
    }

    pub fn key_mods(key: Key, mods: Modifiers) -> Self {
        Self::Key {
            key,
            mods,
            repeat: false,
        }
    }
}

/// Turns presses into double clicks the way vanilla's 250 ms window does.
#[derive(Clone, Copy, Debug, Default)]
pub struct DoubleClickTracker {
    last: Option<(MouseButton, u64)>,
}

impl DoubleClickTracker {
    const WINDOW_MS: u64 = 250;

    pub fn press(&mut self, button: MouseButton, now_ms: u64) -> bool {
        let double = matches!(self.last, Some((b, t)) if b == button && now_ms.saturating_sub(t) < Self::WINDOW_MS);
        self.last = if double { None } else { Some((button, now_ms)) };
        double
    }
}

pub fn contains(rect: [i32; 4], x: f32, y: f32) -> bool {
    x >= rect[0] as f32
        && y >= rect[1] as f32
        && x < (rect[0] + rect[2]) as f32
        && y < (rect[1] + rect[3]) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_click_window() {
        let mut t = DoubleClickTracker::default();
        assert!(!t.press(MouseButton::Left, 1000));
        assert!(t.press(MouseButton::Left, 1200));
        assert!(!t.press(MouseButton::Left, 1300));
        assert!(!t.press(MouseButton::Right, 1400));
        assert!(!t.press(MouseButton::Right, 1700));
    }
}
