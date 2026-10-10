pub trait OptionValue: Sized {
    fn parse(raw: &str) -> Option<Self>;
    fn format(&self) -> String;
    fn clamp_to(self, _bounds: Option<(f64, f64)>) -> Self {
        self
    }
}

pub(crate) fn unquote(raw: &str) -> &str {
    raw.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw)
}

impl OptionValue for bool {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    fn format(&self) -> String {
        self.to_string()
    }
}

impl OptionValue for i32 {
    fn parse(raw: &str) -> Option<Self> {
        raw.parse().ok()
    }

    fn format(&self) -> String {
        self.to_string()
    }

    fn clamp_to(self, bounds: Option<(f64, f64)>) -> Self {
        match bounds {
            Some((lo, hi)) => (self as f64).clamp(lo, hi) as i32,
            None => self,
        }
    }
}

impl OptionValue for f64 {
    fn parse(raw: &str) -> Option<Self> {
        raw.parse().ok().filter(|v: &f64| v.is_finite())
    }

    fn format(&self) -> String {
        format!("{self:?}")
    }

    fn clamp_to(self, bounds: Option<(f64, f64)>) -> Self {
        match bounds {
            Some((lo, hi)) => self.clamp(lo, hi),
            None => self,
        }
    }
}

/// Raw text, as vanilla writes `lang` and `lastServer`: no quoting.
impl OptionValue for String {
    fn parse(raw: &str) -> Option<Self> {
        Some(raw.to_owned())
    }

    fn format(&self) -> String {
        self.clone()
    }
}

/// A string Gson-encoded by vanilla, so it is written inside double quotes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuotedString(pub String);

impl OptionValue for QuotedString {
    fn parse(raw: &str) -> Option<Self> {
        Some(Self(unquote(raw).to_owned()))
    }

    fn format(&self) -> String {
        let mut out = String::with_capacity(self.0.len() + 2);
        out.push('"');
        for c in self.0.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
}

/// A JSON array of strings, as `resourcePacks` is stored.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StringList(pub Vec<String>);

impl OptionValue for StringList {
    fn parse(raw: &str) -> Option<Self> {
        serde_json::from_str(raw).ok().map(Self)
    }

    fn format(&self) -> String {
        serde_json::to_string(&self.0).unwrap_or_else(|_| "[]".to_owned())
    }
}

/// Field of the field-of-view slider. Stored as `(fov - 70) / 40`, truncated to int like vanilla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fov(pub i32);

impl OptionValue for Fov {
    fn parse(raw: &str) -> Option<Self> {
        let v: f64 = raw.parse().ok().filter(|v: &f64| v.is_finite())?;
        Some(Self((v * 40.0 + 70.0) as i32))
    }

    fn format(&self) -> String {
        format!("{:?}", (self.0 as f64 - 70.0) / 40.0)
    }

    fn clamp_to(self, bounds: Option<(f64, f64)>) -> Self {
        match bounds {
            Some((lo, hi)) => Self((self.0 as f64).clamp(lo, hi) as i32),
            None => self,
        }
    }
}

macro_rules! named_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $s),+
                }
            }
        }

        impl OptionValue for $name {
            fn parse(raw: &str) -> Option<Self> {
                let raw = $crate::value::unquote(raw);
                Self::ALL.iter().copied().find(|v| v.name() == raw)
            }

            fn format(&self) -> String {
                format!("\"{}\"", self.name())
            }
        }
    };
}

macro_rules! legacy_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $id:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub fn id(self) -> i32 {
                match self {
                    $(Self::$variant => $id),+
                }
            }
        }

        impl OptionValue for $name {
            fn parse(raw: &str) -> Option<Self> {
                let id: i32 = raw.parse().ok()?;
                Self::ALL.iter().copied().find(|v| v.id() == id)
            }

            fn format(&self) -> String {
                self.id().to_string()
            }
        }
    };
}

named_enum! {
    /// `options.graphics.preset`
    GraphicsPreset {
        Fast = "fast",
        Fancy = "fancy",
        Fabulous = "fabulous",
        Custom = "custom",
    }
}

named_enum! {
    /// `options.graphicsApi`
    PreferredGraphicsApi {
        Default = "default",
        OpenGl = "opengl",
        Vulkan = "vulkan",
    }
}

named_enum! {
    /// `options.inactivityFpsLimit`
    InactivityFpsLimit {
        Minimized = "minimized",
        Afk = "afk",
    }
}

named_enum! {
    /// `options.mainHand`
    HumanoidArm {
        Left = "left",
        Right = "right",
    }
}

named_enum! {
    /// `options.sharePresence`
    PresenceSharing {
        None = "none",
        Limited = "limited",
        All = "all",
    }
}

named_enum! {
    /// `options.musicToast`
    MusicToastState {
        Never = "never",
        Pause = "pause",
        PauseAndToast = "pause_and_toast",
    }
}

named_enum! {
    /// `options.music_frequency`
    MusicFrequency {
        Default = "DEFAULT",
        Frequent = "FREQUENT",
        Constant = "CONSTANT",
    }
}

/// `options.renderClouds`: vanilla writes the serialized name, and also accepts a bare boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudStatus {
    Off,
    Fast,
    Fancy,
}

impl CloudStatus {
    pub const ALL: &'static [Self] = &[Self::Off, Self::Fast, Self::Fancy];

    fn name(self) -> &'static str {
        match self {
            Self::Off => "false",
            Self::Fast => "fast",
            Self::Fancy => "true",
        }
    }
}

impl OptionValue for CloudStatus {
    fn parse(raw: &str) -> Option<Self> {
        let raw = unquote(raw);
        Self::ALL.iter().copied().find(|v| v.name() == raw)
    }

    fn format(&self) -> String {
        format!("\"{}\"", self.name())
    }
}

legacy_enum! {
    /// `options.particles`
    ParticleStatus {
        All = 0,
        Decreased = 1,
        Minimal = 2,
    }
}

legacy_enum! {
    /// `options.narrator`
    NarratorStatus {
        Off = 0,
        All = 1,
        Chat = 2,
        System = 3,
    }
}

legacy_enum! {
    /// `options.chat.visibility`
    ChatVisibility {
        Full = 0,
        System = 1,
        Hidden = 2,
    }
}

legacy_enum! {
    /// `options.attackIndicator`
    AttackIndicator {
        Off = 0,
        Crosshair = 1,
        Hotbar = 2,
    }
}

legacy_enum! {
    /// `options.prioritizeChunkUpdates`
    PrioritizeChunkUpdates {
        None = 0,
        PlayerAffected = 1,
        Nearby = 2,
    }
}

legacy_enum! {
    /// `options.textureFiltering`
    TextureFiltering {
        None = 0,
        Rgss = 1,
        Anisotropic = 2,
    }
}
