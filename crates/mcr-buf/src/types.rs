use std::fmt;

use crate::{
    Decode, DecodeError, Encode, Result, decode_str, encode_str, read_len, read_varint, take,
    write_varint,
};

#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct BitSet(Vec<u64>);

impl BitSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, bit: usize) -> bool {
        self.0
            .get(bit / 64)
            .is_some_and(|w| w >> (bit % 64) & 1 == 1)
    }

    pub fn set(&mut self, bit: usize, value: bool) {
        let word = bit / 64;
        if word >= self.0.len() {
            if !value {
                return;
            }
            self.0.resize(word + 1, 0);
        }
        let mask = 1u64 << (bit % 64);
        if value {
            self.0[word] |= mask;
        } else {
            self.0[word] &= !mask;
        }
    }

    pub fn words(&self) -> &[u64] {
        &self.0
    }

    pub fn from_words(words: Vec<u64>) -> Self {
        Self(words)
    }
}

impl Encode for BitSet {
    fn encode(&self, out: &mut Vec<u8>) {
        let used = self.0.iter().rposition(|&w| w != 0).map_or(0, |i| i + 1);
        write_varint(out, used as i32);
        for w in &self.0[..used] {
            w.encode(out);
        }
    }
}

impl Decode<'_> for BitSet {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let len = read_len(input, input.len() / 8)?;
        let mut words = Vec::with_capacity(len);
        for _ in 0..len {
            words.push(u64::decode(input)?);
        }
        Ok(Self(words))
    }
}

/// Exactly `ceil(BITS / 8)` bytes, least significant bit first.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FixedBitSet<const BITS: usize>(Vec<u8>);

impl<const BITS: usize> FixedBitSet<BITS> {
    const BYTES: usize = BITS.div_ceil(8);

    pub fn new() -> Self {
        Self(vec![0; Self::BYTES])
    }

    pub fn get(&self, bit: usize) -> bool {
        bit < BITS && self.0[bit / 8] >> (bit % 8) & 1 == 1
    }

    /// # Panics
    /// If `bit >= BITS`.
    pub fn set(&mut self, bit: usize, value: bool) {
        assert!(bit < BITS, "bit {bit} out of range for {BITS}-bit set");
        let mask = 1u8 << (bit % 8);
        if value {
            self.0[bit / 8] |= mask;
        } else {
            self.0[bit / 8] &= !mask;
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl<const BITS: usize> Default for FixedBitSet<BITS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const BITS: usize> Encode for FixedBitSet<BITS> {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0);
    }
}

impl<const BITS: usize> Decode<'_> for FixedBitSet<BITS> {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        Ok(Self(take(input, Self::BYTES)?.to_vec()))
    }
}

/// VarInt-length-prefixed list rejected above `MAX` elements.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LengthPrefixed<T, const MAX: usize>(pub Vec<T>);

impl<T, const MAX: usize> Default for LengthPrefixed<T, MAX> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T: Encode, const MAX: usize> Encode for LengthPrefixed<T, MAX> {
    fn encode(&self, out: &mut Vec<u8>) {
        self.0.encode(out);
    }
}

impl<'a, T: Decode<'a>, const MAX: usize> Decode<'a> for LengthPrefixed<T, MAX> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        let len = read_len(input, MAX)?;
        let mut v = Vec::with_capacity(crate::prealloc_capacity::<T>(len));
        for _ in 0..len {
            v.push(T::decode(input)?);
        }
        Ok(Self(v))
    }
}

pub const DEFAULT_NAMESPACE: &str = "minecraft";

/// Always stored as `namespace:path`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identifier {
    full: String,
    colon: usize,
}

fn namespace_char(c: u8) -> bool {
    matches!(c, b'_' | b'-' | b'.' | b'a'..=b'z' | b'0'..=b'9')
}

fn path_char(c: u8) -> bool {
    namespace_char(c) || c == b'/'
}

impl Identifier {
    pub fn new(namespace: &str, path: &str) -> Result<Self> {
        if namespace == ".."
            || !namespace.bytes().all(namespace_char)
            || !path.bytes().all(path_char)
        {
            return Err(DecodeError::Other(format!(
                "invalid identifier {namespace}:{path}"
            )));
        }
        Ok(Self {
            full: format!("{namespace}:{path}"),
            colon: namespace.len(),
        })
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.split_once(':') {
            Some((ns, path)) => Self::new(ns, path),
            None => Self::new(DEFAULT_NAMESPACE, s),
        }
    }

    pub fn namespace(&self) -> &str {
        &self.full[..self.colon]
    }

    pub fn path(&self) -> &str {
        &self.full[self.colon + 1..]
    }

    pub fn as_str(&self) -> &str {
        &self.full
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.full)
    }
}

impl Encode for Identifier {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_str(&self.full, out);
    }
}

impl Decode<'_> for Identifier {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        Self::parse(decode_str(input, crate::MAX_STRING_CHARS)?)
    }
}

/// x and z take 26 bits each, y the low 12; all signed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    const H_BITS: u32 = 26;
    const Y_BITS: u32 = 12;

    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub const fn pack(self) -> i64 {
        let x = (self.x as i64 & ((1 << Self::H_BITS) - 1)) << (Self::H_BITS + Self::Y_BITS);
        let z = (self.z as i64 & ((1 << Self::H_BITS) - 1)) << Self::Y_BITS;
        let y = self.y as i64 & ((1 << Self::Y_BITS) - 1);
        x | z | y
    }

    pub const fn unpack(v: i64) -> Self {
        Self {
            x: (v >> (Self::H_BITS + Self::Y_BITS)) as i32,
            y: (v << (64 - Self::Y_BITS) >> (64 - Self::Y_BITS)) as i32,
            z: (v << (64 - Self::H_BITS - Self::Y_BITS) >> (64 - Self::H_BITS)) as i32,
        }
    }
}

impl Encode for BlockPos {
    fn encode(&self, out: &mut Vec<u8>) {
        self.pack().encode(out);
    }
}

impl Decode<'_> for BlockPos {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        i64::decode(input).map(Self::unpack)
    }
}

/// Rotation in 1/256 of a full turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Angle(pub u8);

impl Angle {
    pub fn from_degrees(degrees: f32) -> Self {
        Self((degrees * 256.0 / 360.0).floor() as i32 as u8)
    }

    pub fn degrees(self) -> f32 {
        f32::from(self.0) * 360.0 / 256.0
    }
}

impl Encode for Angle {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.0);
    }
}

impl Decode<'_> for Angle {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        u8::decode(input).map(Self)
    }
}

/// Registry id, or an inline value when the wire id is 0.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IdOr<T> {
    Id(i32),
    Inline(T),
}

impl<T: Encode> Encode for IdOr<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Id(id) => write_varint(out, id + 1),
            Self::Inline(v) => {
                write_varint(out, 0);
                v.encode(out);
            }
        }
    }
}

impl<'a, T: Decode<'a>> Decode<'a> for IdOr<T> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        match read_varint(input)? {
            0 => T::decode(input).map(Self::Inline),
            n if n > 0 => Ok(Self::Id(n - 1)),
            n => Err(DecodeError::InvalidValue {
                what: "registry id",
                value: n.into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VarInt, from_bytes_exact, to_bytes};

    #[test]
    fn bitset_bytes() {
        let mut b = BitSet::new();
        b.set(0, true);
        b.set(65, true);
        let bytes = to_bytes(&b);
        assert_eq!(bytes, [2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2]);
        let back = from_bytes_exact::<BitSet>(&bytes).unwrap();
        assert!(back.get(0) && back.get(65) && !back.get(1));
        assert_eq!(to_bytes(&BitSet::new()), [0]);
    }

    #[test]
    fn bitset_rejects_hostile_length() {
        assert!(BitSet::decode(&mut &[0x7f][..]).is_err());
    }

    #[test]
    fn fixed_bitset_bytes() {
        let mut b = FixedBitSet::<10>::new();
        b.set(9, true);
        b.set(0, true);
        assert_eq!(to_bytes(&b), [1, 2]);
        assert_eq!(from_bytes_exact::<FixedBitSet<10>>(&[1, 2]).unwrap(), b);
        assert!(FixedBitSet::<10>::decode(&mut &[1u8][..]).is_err());
    }

    #[test]
    fn block_pos_layout() {
        // Fixture: x=18357644, y=831, z=-20882616 from the wiki's position example.
        let pos = BlockPos::new(18_357_644, 831, -20_882_616);
        assert_eq!(pos.pack(), 0x4607_632C_15B4_833F);
        assert_eq!(BlockPos::unpack(0x4607_632C_15B4_833F), pos);
        assert_eq!(
            to_bytes(&BlockPos::new(1, 0, 0)),
            [0, 0, 0, 0x40, 0, 0, 0, 0]
        );
        assert_eq!(BlockPos::new(0, 0, 1).pack(), 1 << 12);
        assert_eq!(BlockPos::new(0, 5, 0).pack(), 5);
        assert_eq!(BlockPos::new(-1, -1, -1).pack(), -1);
        let min = BlockPos::new(-33_554_432, -2048, -33_554_432);
        assert_eq!(BlockPos::unpack(min.pack()), min);
    }

    #[test]
    fn identifier() {
        let id = Identifier::parse("stone").unwrap();
        assert_eq!(id.as_str(), "minecraft:stone");
        assert_eq!((id.namespace(), id.path()), ("minecraft", "stone"));
        let bytes = to_bytes(&id);
        assert_eq!(from_bytes_exact::<Identifier>(&bytes).unwrap(), id);
        assert!(Identifier::parse("Bad:x").is_err());
        assert!(Identifier::parse("a:b c").is_err());
        assert!(Identifier::parse("a/b:c").is_err());
        assert!(Identifier::parse("ns:a/b").is_ok());
    }

    #[test]
    fn length_prefixed() {
        let v = LengthPrefixed::<u8, 2>(vec![1, 2]);
        assert_eq!(
            from_bytes_exact::<LengthPrefixed<u8, 2>>(&to_bytes(&v)).unwrap(),
            v
        );
        assert_eq!(
            LengthPrefixed::<u8, 2>::decode(&mut &[3, 1, 2, 3][..]),
            Err(DecodeError::TooLong { len: 3, max: 2 })
        );
    }

    #[test]
    fn angle() {
        assert_eq!(Angle::from_degrees(90.0), Angle(64));
        assert_eq!(Angle(128).degrees(), 180.0);
        assert_eq!(Angle::from_degrees(-90.0), Angle(192));
    }

    #[test]
    fn id_or() {
        assert_eq!(to_bytes(&IdOr::<VarInt>::Id(4)), [5]);
        assert_eq!(to_bytes(&IdOr::Inline(VarInt(7))), [0, 7]);
        assert_eq!(
            from_bytes_exact::<IdOr<VarInt>>(&[0, 7]).unwrap(),
            IdOr::Inline(VarInt(7))
        );
        assert_eq!(from_bytes_exact::<IdOr<VarInt>>(&[5]).unwrap(), IdOr::Id(4));
    }
}
