#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SoundSource {
    Master,
    Music,
    Records,
    Weather,
    Blocks,
    Hostile,
    Neutral,
    Players,
    Ambient,
    Voice,
    Ui,
}

impl SoundSource {
    pub const COUNT: usize = 11;

    pub const ALL: [SoundSource; Self::COUNT] = [
        Self::Master,
        Self::Music,
        Self::Records,
        Self::Weather,
        Self::Blocks,
        Self::Hostile,
        Self::Neutral,
        Self::Players,
        Self::Ambient,
        Self::Voice,
        Self::Ui,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Master => "master",
            Self::Music => "music",
            Self::Records => "record",
            Self::Weather => "weather",
            Self::Blocks => "block",
            Self::Hostile => "hostile",
            Self::Neutral => "neutral",
            Self::Players => "player",
            Self::Ambient => "ambient",
            Self::Voice => "voice",
            Self::Ui => "ui",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_indices_are_dense() {
        for (i, s) in SoundSource::ALL.into_iter().enumerate() {
            assert_eq!(s.index(), i);
            assert_eq!(SoundSource::from_name(s.name()), Some(s));
        }
        assert_eq!(SoundSource::from_name("nope"), None);
    }
}
