#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontId(pub u8);

impl FontId {
    pub const DEFAULT: Self = Self(0);
    pub const UNIFORM: Self = Self(1);
    pub const ALT: Self = Self(2);
    pub const ILLAGERALT: Self = Self(3);
    pub const COUNT: usize = 4;

    pub fn from_name(name: &str) -> Self {
        match name.strip_prefix("minecraft:").unwrap_or(name) {
            "uniform" => Self::UNIFORM,
            "alt" => Self::ALT,
            "illageralt" => Self::ILLAGERALT,
            _ => Self::DEFAULT,
        }
    }

    pub fn path(self) -> &'static str {
        ["default", "uniform", "alt", "illageralt"][self.0 as usize % Self::COUNT]
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
