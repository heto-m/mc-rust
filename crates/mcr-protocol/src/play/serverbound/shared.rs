use mcr_buf::{Decode, DecodeError, Encode, Result, read_varint, take, write_varint};

pub use crate::types::game::{Difficulty, GameType, Hand, Vec3i};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Facing {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Facing {
    pub fn from_id(id: i32) -> Result<Self> {
        Ok(match id {
            0 => Self::Down,
            1 => Self::Up,
            2 => Self::North,
            3 => Self::South,
            4 => Self::West,
            5 => Self::East,
            _ => {
                return Err(DecodeError::InvalidValue {
                    what: "Facing",
                    value: id.into(),
                });
            }
        })
    }

    pub fn id(self) -> i32 {
        self as i32
    }
}

impl Encode for Facing {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.id());
    }
}

impl Decode<'_> for Facing {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        Self::from_id(read_varint(input)?)
    }
}

pub(crate) mod facing_byte {
    use super::*;

    pub fn encode(value: &Facing, out: &mut Vec<u8>) {
        out.push(value.id() as u8);
    }

    pub fn decode(input: &mut &[u8]) -> Result<Facing> {
        Facing::from_id(u8::decode(input)?.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum Rotation {
    None,
    Clockwise90,
    Clockwise180,
    CounterClockwise90,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct BoundedBytes<const MAX: usize>(pub Vec<u8>);

impl<const MAX: usize> Encode for BoundedBytes<MAX> {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.0.len() as i32);
        out.extend_from_slice(&self.0);
    }
}

impl<const MAX: usize> Decode<'_> for BoundedBytes<MAX> {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let len = mcr_buf::read_len(input, MAX)?;
        Ok(Self(take(input, len)?.to_vec()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FlagByte(pub u8);

impl FlagByte {
    pub fn bit(self, n: u8) -> bool {
        self.0 >> n & 1 == 1
    }

    pub fn with(self, n: u8, on: bool) -> Self {
        Self(self.0 | u8::from(on) << n)
    }
}

impl Encode for FlagByte {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.0);
    }
}

impl Decode<'_> for FlagByte {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        u8::decode(input).map(Self)
    }
}

#[cfg(test)]
macro_rules! roundtrips {
    ($($name:ident: $value:expr),* $(,)?) => {$(
        #[test]
        fn $name() {
            $crate::test_util::roundtrip(&$value);
        }
    )*};
}

#[cfg(test)]
pub(crate) use roundtrips;
