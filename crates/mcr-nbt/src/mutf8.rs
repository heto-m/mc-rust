//! Java "Modified UTF-8": NUL as `C0 80`, supplementary characters as CESU-8 surrogate pairs.

use std::borrow::Cow;

use crate::{NbtError, Result};

/// Input that is already valid standard UTF-8 is accepted as-is, including 4-byte sequences.
pub fn decode(bytes: &[u8]) -> Result<Cow<'_, str>> {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok(Cow::Borrowed(s));
    }
    decode_slow(bytes).map(Cow::Owned)
}

fn decode_slow(bytes: &[u8]) -> Result<String> {
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let (unit, n) = match b {
            0x00..=0x7F => (u16::from(b), 1),
            0xC0..=0xDF => {
                let c1 = cont(bytes, i + 1)?;
                ((u16::from(b & 0x1F) << 6) | c1, 2)
            }
            0xE0..=0xEF => {
                let c1 = cont(bytes, i + 1)?;
                let c2 = cont(bytes, i + 2)?;
                ((u16::from(b & 0x0F) << 12) | (c1 << 6) | c2, 3)
            }
            _ => return Err(NbtError::InvalidMutf8),
        };
        units.push(unit);
        i += n;
    }
    String::from_utf16(&units).map_err(|_| NbtError::InvalidMutf8)
}

fn cont(bytes: &[u8], i: usize) -> Result<u16> {
    match bytes.get(i) {
        Some(&b) if b & 0xC0 == 0x80 => Ok(u16::from(b & 0x3F)),
        _ => Err(NbtError::InvalidMutf8),
    }
}

fn needs_transcode(s: &str) -> bool {
    s.bytes().any(|b| b == 0 || b >= 0xF0)
}

pub fn encoded_len(s: &str) -> usize {
    if !needs_transcode(s) {
        return s.len();
    }
    s.chars()
        .map(|c| match c.len_utf8() {
            _ if c == '\0' => 2,
            4 => 6,
            n => n,
        })
        .sum()
}

pub fn encode(s: &str) -> Cow<'_, [u8]> {
    if !needs_transcode(s) {
        return Cow::Borrowed(s.as_bytes());
    }
    let mut out = Vec::with_capacity(s.len() + 8);
    encode_into(s, &mut out);
    Cow::Owned(out)
}

pub fn encode_into(s: &str, out: &mut Vec<u8>) {
    if !needs_transcode(s) {
        out.extend_from_slice(s.as_bytes());
        return;
    }
    for c in s.chars() {
        match c.len_utf8() {
            _ if c == '\0' => out.extend_from_slice(&[0xC0, 0x80]),
            4 => {
                let mut units = [0u16; 2];
                for &u in c.encode_utf16(&mut units).iter() {
                    out.extend_from_slice(&[
                        0xE0 | (u >> 12) as u8,
                        0x80 | ((u >> 6) & 0x3F) as u8,
                        0x80 | (u & 0x3F) as u8,
                    ]);
                }
            }
            _ => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nul_uses_c080() {
        assert_eq!(&*encode("a\0b"), b"a\xC0\x80b");
        assert_eq!(decode(b"a\xC0\x80b").unwrap(), "a\0b");
    }

    #[test]
    fn supplementary_is_six_bytes() {
        let enc = encode("\u{1F600}");
        assert_eq!(&*enc, b"\xED\xA0\xBD\xED\xB8\x80");
        assert_eq!(encoded_len("\u{1F600}"), 6);
        assert_eq!(decode(&enc).unwrap(), "\u{1F600}");
    }

    #[test]
    fn plain_is_borrowed() {
        assert!(matches!(encode("héllo"), Cow::Borrowed(_)));
        assert!(matches!(
            decode("héllo".as_bytes()).unwrap(),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn rejects_malformed() {
        assert!(decode(b"\xED\xA0\xBD").is_err());
        assert!(decode(b"\xED\xB8\x80\xED\xA0\xBD").is_err());
        assert!(decode(b"\xC0").is_err());
        assert!(decode(b"\xE2\x82").is_err());
        assert!(decode(b"\xC0\x41").is_err());
        assert!(decode(b"\xFF").is_err());
    }

    #[test]
    fn mixed_round_trip() {
        let s = "a\0é€\u{10FFFF}\u{10000}z";
        assert_eq!(decode(&encode(s)).unwrap(), s);
        assert_eq!(encode(s).len(), encoded_len(s));
    }
}
