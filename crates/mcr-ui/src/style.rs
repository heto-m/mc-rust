use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontId(pub u8);

const MAX_UNKNOWN_FONTS: usize = 24;

fn unknown_fonts() -> &'static Mutex<Vec<&'static str>> {
    static NAMES: OnceLock<Mutex<Vec<&'static str>>> = OnceLock::new();
    NAMES.get_or_init(Mutex::default)
}

impl FontId {
    pub const DEFAULT: Self = Self(0);
    pub const UNIFORM: Self = Self(1);
    pub const ALT: Self = Self(2);
    pub const ILLAGERALT: Self = Self(3);
    pub const COUNT: usize = 4;
    /// Every font without a definition; its glyphs are all the missing glyph.
    pub const MISSING: Self = Self(Self::COUNT as u8);

    /// Names without a definition get distinct ids, up to a bound, so a server cannot grow the table.
    pub fn from_name(name: &str) -> Self {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        match name {
            "default" => Self::DEFAULT,
            "uniform" => Self::UNIFORM,
            "alt" => Self::ALT,
            "illageralt" => Self::ILLAGERALT,
            _ => {
                let mut names = unknown_fonts().lock().unwrap_or_else(|e| e.into_inner());
                match names.iter().position(|n| *n == name) {
                    Some(i) => Self(Self::MISSING.0 + i as u8),
                    None if names.len() < MAX_UNKNOWN_FONTS && name.len() <= 256 => {
                        names.push(Box::leak(name.to_owned().into_boxed_str()));
                        Self(Self::MISSING.0 + (names.len() - 1) as u8)
                    }
                    None => Self::MISSING,
                }
            }
        }
    }

    pub fn is_missing(self) -> bool {
        usize::from(self.0) >= Self::COUNT
    }

    pub fn path(self) -> &'static str {
        match usize::from(self.0) {
            0 => "default",
            1 => "uniform",
            2 => "alt",
            3 => "illageralt",
            i => {
                let names = unknown_fonts().lock().unwrap_or_else(|e| e.into_inner());
                names.get(i - Self::COUNT).copied().unwrap_or("missing")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags(pub u8);

impl Flags {
    pub const BOLD: u8 = 1;
    pub const ITALIC: u8 = 2;
    pub const UNDERLINE: u8 = 4;
    pub const STRIKETHROUGH: u8 = 8;
    pub const OBFUSCATED: u8 = 16;

    pub fn has(self, bit: u8) -> bool {
        self.0 & bit != 0
    }

    pub fn set(&mut self, bit: u8, on: bool) {
        if on {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub color: Option<u32>,
    pub shadow: Option<u32>,
    pub flags: Flags,
    pub font: FontId,
}

impl Style {
    pub fn bold(self) -> bool {
        self.flags.has(Flags::BOLD)
    }

    pub fn italic(self) -> bool {
        self.flags.has(Flags::ITALIC)
    }

    pub fn underlined(self) -> bool {
        self.flags.has(Flags::UNDERLINE)
    }

    pub fn strikethrough(self) -> bool {
        self.flags.has(Flags::STRIKETHROUGH)
    }

    pub fn obfuscated(self) -> bool {
        self.flags.has(Flags::OBFUSCATED)
    }

    pub fn with_color(mut self, rgb: u32) -> Self {
        self.color = Some(rgb & 0xFF_FFFF);
        self
    }

    pub fn with_flag(mut self, bit: u8) -> Self {
        self.flags.set(bit, true);
        self
    }

    /// `None` for `r` and unknown codes; the caller owns reset semantics.
    pub fn apply_legacy(self, code: char) -> Option<Self> {
        let code = code.to_ascii_lowercase();
        if let Some(i) = "0123456789abcdef".find(code) {
            return Some(Self {
                color: Some(LEGACY_COLORS[i]),
                flags: Flags(0),
                ..self
            });
        }
        let bit = match code {
            'k' => Flags::OBFUSCATED,
            'l' => Flags::BOLD,
            'm' => Flags::STRIKETHROUGH,
            'n' => Flags::UNDERLINE,
            'o' => Flags::ITALIC,
            _ => return None,
        };
        Some(self.with_flag(bit))
    }
}

pub const LEGACY_COLORS: [u32; 16] = [
    0x000000, 0x0000AA, 0x00AA00, 0x00AAAA, 0xAA0000, 0xAA00AA, 0xFFAA00, 0xAAAAAA, 0x555555,
    0x5555FF, 0x55FF55, 0x55FFFF, 0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

pub fn scale_rgb(argb: u32, factor: f32) -> u32 {
    let ch = |shift: u32| (((argb >> shift) & 0xFF) as f32 * factor) as u32;
    (argb & 0xFF00_0000) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

pub fn with_alpha(argb: u32, alpha: f32) -> u32 {
    (((alpha.clamp(0.0, 1.0) * 255.0) as u32) << 24) | (argb & 0xFF_FFFF)
}
