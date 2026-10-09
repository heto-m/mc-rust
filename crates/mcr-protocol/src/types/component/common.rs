use mcr_buf::{Decode, DecodeError, Encode, Identifier, Result, VarInt, read_len, write_varint};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Unit;

impl Encode for Unit {
    fn encode(&self, _out: &mut Vec<u8>) {}
}

impl Decode<'_> for Unit {
    fn decode(_input: &mut &[u8]) -> Result<Self> {
        Ok(Self)
    }
}

/// Registry ids are server-synced, so a set names either a tag or explicit ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HolderSet {
    Tag(Identifier),
    Ids(Vec<i32>),
}

impl Encode for HolderSet {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Tag(tag) => {
                write_varint(out, 0);
                tag.encode(out);
            }
            Self::Ids(ids) => {
                write_varint(out, ids.len() as i32 + 1);
                for &id in ids {
                    write_varint(out, id);
                }
            }
        }
    }
}

impl Decode<'_> for HolderSet {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        match read_len(input, input.len() + 1)? {
            0 => Identifier::decode(input).map(Self::Tag),
            n => {
                let mut ids = Vec::with_capacity(mcr_buf::prealloc_capacity::<i32>(n - 1));
                for _ in 1..n {
                    ids.push(VarInt::decode(input)?.0);
                }
                Ok(Self::Ids(ids))
            }
        }
    }
}

/// Bool-prefixed union: `true` selects the left variant.
macro_rules! either_codec {
    ($ty:ident, $left:ident($lt:ty), $right:ident($rt:ty)) => {
        impl Encode for $ty {
            fn encode(&self, out: &mut Vec<u8>) {
                match self {
                    Self::$left(v) => {
                        true.encode(out);
                        v.encode(out);
                    }
                    Self::$right(v) => {
                        false.encode(out);
                        v.encode(out);
                    }
                }
            }
        }

        impl Decode<'_> for $ty {
            fn decode(input: &mut &[u8]) -> Result<Self> {
                if bool::decode(input)? {
                    <$lt>::decode(input).map(Self::$left)
                } else {
                    <$rt>::decode(input).map(Self::$right)
                }
            }
        }
    };
}
pub(super) use either_codec;

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvableInt {
    Constant(i32),
    Reference(Identifier),
}
either_codec!(ResolvableInt, Constant(i32), Reference(Identifier));

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvableFloat {
    Constant(f32),
    Reference(Identifier),
}
either_codec!(ResolvableFloat, Constant(f32), Reference(Identifier));

#[derive(Debug, Clone, PartialEq, mcr_buf::Encode, mcr_buf::Decode)]
pub struct Filterable<T> {
    pub raw: T,
    pub filtered: Option<T>,
}

#[derive(Debug, Clone, PartialEq, Eq, mcr_buf::Encode, mcr_buf::Decode)]
pub struct GlobalPos {
    pub dimension: Identifier,
    pub pos: mcr_buf::BlockPos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum Rarity {
    Common = 0,
    Uncommon = 1,
    Rare = 2,
    Epic = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum DyeColor {
    White = 0,
    Orange = 1,
    Magenta = 2,
    LightBlue = 3,
    Yellow = 4,
    Lime = 5,
    Pink = 6,
    Gray = 7,
    LightGray = 8,
    Cyan = 9,
    Purple = 10,
    Blue = 11,
    Brown = 12,
    Green = 13,
    Red = 14,
    Black = 15,
}

/// Wire ids are not declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum EquipmentSlot {
    MainHand = 0,
    Feet = 1,
    Legs = 2,
    Chest = 3,
    Head = 4,
    OffHand = 5,
    Body = 6,
    Saddle = 7,
}

impl EquipmentSlot {
    /// Declaration order, which `set_equipment` writes instead of the component id.
    pub const ALL: [Self; 8] = [
        Self::MainHand,
        Self::OffHand,
        Self::Feet,
        Self::Legs,
        Self::Chest,
        Self::Head,
        Self::Body,
        Self::Saddle,
    ];

    pub fn ordinal(self) -> u8 {
        Self::ALL.iter().position(|&s| s == self).unwrap() as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum EquipmentSlotGroup {
    Any = 0,
    MainHand = 1,
    OffHand = 2,
    Hand = 3,
    Feet = 4,
    Legs = 5,
    Chest = 6,
    Head = 7,
    Armor = 8,
    Body = 9,
    Saddle = 10,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ItemUseAnimation {
    None = 0,
    Eat = 1,
    Drink = 2,
    Block = 3,
    Bow = 4,
    Trident = 5,
    Crossbow = 6,
    Spyglass = 7,
    TootHorn = 8,
    Brush = 9,
    Bundle = 10,
    Spear = 11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum SwingAnimationType {
    None = 0,
    Whack = 1,
    Stab = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum AttributeOperation {
    AddValue = 0,
    AddMultipliedBase = 1,
    AddMultipliedTotal = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum FireworkShape {
    SmallBall = 0,
    LargeBall = 1,
    Star = 2,
    Creeper = 3,
    Burst = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum MapPostProcessing {
    Lock = 0,
    Scale = 1,
}

pub(super) struct DepthGuard;

thread_local! {
    static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

const MAX_DEPTH: u32 = 32;

impl DepthGuard {
    pub(super) fn enter() -> Result<Self> {
        DEPTH.with(|d| {
            if d.get() >= MAX_DEPTH {
                return Err(DecodeError::Other("component nesting too deep".into()));
            }
            d.set(d.get() + 1);
            Ok(Self)
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get() - 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::roundtrip;

    #[test]
    fn holder_set_forms() {
        let tag = HolderSet::Tag(Identifier::parse("planks").unwrap());
        let mut expected = vec![0, 16];
        expected.extend_from_slice(b"minecraft:planks");
        assert_eq!(roundtrip(&tag), expected);
        assert_eq!(roundtrip(&HolderSet::Ids(vec![5, 300])), [3, 5, 0xac, 2]);
        assert_eq!(roundtrip(&HolderSet::Ids(vec![])), [1]);
    }

    #[test]
    fn holder_set_rejects_oversized_count() {
        assert!(HolderSet::decode(&mut &[0xff, 0xff, 0xff, 0x7f][..]).is_err());
    }
}
