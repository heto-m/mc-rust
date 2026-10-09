use mcr_buf::{Decode, Encode, Result, read_varint, take};

/// Vanilla `ByIdMap` lookup that substitutes the first value for unknown ids.
fn by_id_or_first<T: Copy>(values: &[T], id: i32) -> T {
    usize::try_from(id)
        .ok()
        .and_then(|i| values.get(i).copied())
        .unwrap_or(values[0])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Encode)]
pub enum Hand {
    #[default]
    Main,
    Off,
}

impl Decode<'_> for Hand {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        Ok(by_id_or_first(
            &[Self::Main, Self::Off],
            read_varint(input)?,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum SignSide {
    Back,
    Front,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode)]
pub enum Difficulty {
    Peaceful,
    Easy,
    Normal,
    Hard,
}

/// Vanilla wraps out-of-range difficulty ids modulo 4.
impl Decode<'_> for Difficulty {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let all = [Self::Peaceful, Self::Easy, Self::Normal, Self::Hard];
        Ok(all[read_varint(input)?.rem_euclid(4) as usize])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Encode)]
pub enum GameType {
    #[default]
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl Decode<'_> for GameType {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let all = [
            Self::Survival,
            Self::Creative,
            Self::Adventure,
            Self::Spectator,
        ];
        Ok(by_id_or_first(&all, read_varint(input)?))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Encode, Decode)]
pub struct Vec3i {
    #[mcr(varint)]
    pub x: i32,
    #[mcr(varint)]
    pub y: i32,
    #[mcr(varint)]
    pub z: i32,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MessageSignature(pub [u8; 256]);

impl std::fmt::Debug for MessageSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MessageSignature({:02x?}..)", &self.0[..4])
    }
}

impl Encode for MessageSignature {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0);
    }
}

impl Decode<'_> for MessageSignature {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let mut bytes = [0; 256];
        bytes.copy_from_slice(take(input, 256)?);
        Ok(Self(bytes))
    }
}

#[cfg(test)]
mod tests {
    use mcr_buf::from_bytes_exact;

    use super::*;
    use crate::test_util::roundtrip;

    #[test]
    fn enum_ids() {
        assert_eq!(roundtrip(&Hand::Off), [1]);
        assert_eq!(roundtrip(&Difficulty::Hard), [3]);
        assert_eq!(roundtrip(&GameType::Spectator), [3]);
        assert_eq!(
            roundtrip(&Vec3i {
                x: -1,
                y: 300,
                z: 0
            }),
            [0xff, 0xff, 0xff, 0xff, 0x0f, 0xac, 0x02, 0]
        );
    }

    #[test]
    fn out_of_range_ids_follow_vanilla() {
        assert_eq!(from_bytes_exact::<Hand>(&[2]).unwrap(), Hand::Main);
        assert_eq!(
            from_bytes_exact::<GameType>(&[9]).unwrap(),
            GameType::Survival
        );
        let minus_one = [0xff, 0xff, 0xff, 0xff, 0x0f];
        assert_eq!(
            from_bytes_exact::<GameType>(&minus_one).unwrap(),
            GameType::Survival
        );
        assert_eq!(
            from_bytes_exact::<Difficulty>(&[5]).unwrap(),
            Difficulty::Easy
        );
        assert_eq!(
            from_bytes_exact::<Difficulty>(&minus_one).unwrap(),
            Difficulty::Hard
        );
    }
}
