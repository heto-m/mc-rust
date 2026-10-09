use crate::{Decode, DecodeError, Encode, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VarInt(pub i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VarLong(pub i64);

pub fn read_varint(input: &mut &[u8]) -> Result<i32> {
    let mut value: u32 = 0;
    for i in 0..5 {
        let Some((&byte, rest)) = input.split_first() else {
            return Err(DecodeError::UnexpectedEof);
        };
        *input = rest;
        value |= u32::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(value as i32);
        }
    }
    Err(DecodeError::VarIntTooLong(5))
}

pub fn read_varlong(input: &mut &[u8]) -> Result<i64> {
    let mut value: u64 = 0;
    for i in 0..10 {
        let Some((&byte, rest)) = input.split_first() else {
            return Err(DecodeError::UnexpectedEof);
        };
        *input = rest;
        value |= u64::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(value as i64);
        }
    }
    Err(DecodeError::VarIntTooLong(10))
}

pub fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut v = value as u32;
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

pub fn write_varlong(out: &mut Vec<u8>, value: i64) {
    let mut v = value as u64;
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

pub fn varint_len(value: i32) -> usize {
    match value as u32 {
        0..0x80 => 1,
        0x80..0x4000 => 2,
        0x4000..0x20_0000 => 3,
        0x20_0000..0x1000_0000 => 4,
        _ => 5,
    }
}

impl Encode for VarInt {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.0);
    }
}

impl Decode<'_> for VarInt {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        read_varint(input).map(Self)
    }
}

impl Encode for VarLong {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varlong(out, self.0);
    }
}

impl Decode<'_> for VarLong {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        read_varlong(input).map(Self)
    }
}

impl From<i32> for VarInt {
    fn from(v: i32) -> Self {
        Self(v)
    }
}

impl From<VarInt> for i32 {
    fn from(v: VarInt) -> Self {
        v.0
    }
}

impl From<i64> for VarLong {
    fn from(v: i64) -> Self {
        Self(v)
    }
}

impl From<VarLong> for i64 {
    fn from(v: VarLong) -> Self {
        v.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CASES: &[(i32, &[u8])] = &[
        (0, &[0x00]),
        (1, &[0x01]),
        (127, &[0x7f]),
        (128, &[0x80, 0x01]),
        (255, &[0xff, 0x01]),
        (25565, &[0xdd, 0xc7, 0x01]),
        (2097151, &[0xff, 0xff, 0x7f]),
        (i32::MAX, &[0xff, 0xff, 0xff, 0xff, 0x07]),
        (-1, &[0xff, 0xff, 0xff, 0xff, 0x0f]),
        (i32::MIN, &[0x80, 0x80, 0x80, 0x80, 0x08]),
    ];

    #[test]
    fn varint_known_values() {
        for &(value, bytes) in CASES {
            let mut out = Vec::new();
            write_varint(&mut out, value);
            assert_eq!(out, bytes, "encode {value}");
            assert_eq!(varint_len(value), bytes.len());
            let mut input = bytes;
            assert_eq!(read_varint(&mut input).unwrap(), value);
            assert!(input.is_empty());
        }
    }

    #[test]
    fn varint_too_long() {
        let mut input: &[u8] = &[0xff; 6];
        assert_eq!(read_varint(&mut input), Err(DecodeError::VarIntTooLong(5)));
    }

    #[test]
    fn varlong_roundtrip() {
        for v in [0, 1, -1, i64::MAX, i64::MIN, 9223372036854775807 / 3] {
            let mut out = Vec::new();
            write_varlong(&mut out, v);
            assert!(out.len() <= 10);
            assert_eq!(read_varlong(&mut out.as_slice()).unwrap(), v);
        }
    }
}
