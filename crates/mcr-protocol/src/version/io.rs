//! Byte-level helpers for rewrites: read the source layout, write the target layout, copy what is shared.

use mcr_buf::{Decode, DecodeError, Result, read_len, read_varint, take, write_varint};

/// Copies everything left.
pub fn rest(r: &mut &[u8], w: &mut Vec<u8>) {
    w.extend_from_slice(r);
    *r = &[];
}

pub fn bytes(r: &mut &[u8], w: &mut Vec<u8>, n: usize) -> Result<()> {
    w.extend_from_slice(take(r, n)?);
    Ok(())
}

/// Validates one `T` with its canonical decoder and copies its bytes unchanged.
pub fn copy<'a, T: Decode<'a>>(r: &mut &'a [u8], w: &mut Vec<u8>) -> Result<()> {
    let start = *r;
    T::decode(r)?;
    w.extend_from_slice(&start[..start.len() - r.len()]);
    Ok(())
}

pub fn varint(r: &mut &[u8], w: &mut Vec<u8>) -> Result<i32> {
    let v = read_varint(r)?;
    write_varint(w, v);
    Ok(v)
}

pub fn len(r: &mut &[u8], w: &mut Vec<u8>, max: usize) -> Result<usize> {
    let n = read_len(r, max)?;
    write_varint(w, n as i32);
    Ok(n)
}

pub fn bool(r: &mut &[u8], w: &mut Vec<u8>) -> Result<bool> {
    let b = take(r, 1)?[0];
    w.push(b);
    match b {
        0 => Ok(false),
        1 => Ok(true),
        v => Err(DecodeError::InvalidValue {
            what: "bool",
            value: v.into(),
        }),
    }
}

/// Rewrites a VarInt registry id; `None` from `map` is an error naming `what`.
pub fn remap(
    r: &mut &[u8],
    w: &mut Vec<u8>,
    what: &'static str,
    map: impl FnOnce(i32) -> Option<i32>,
) -> Result<i32> {
    let old = read_varint(r)?;
    let new = map(old).ok_or(unknown(what, old))?;
    write_varint(w, new);
    Ok(new)
}

/// Copies one network NBT tag without building it; the End tag is allowed only when `optional`.
pub fn nbt(r: &mut &[u8], w: &mut Vec<u8>, optional: bool) -> Result<()> {
    nbt_of(r, w, optional, None)
}

/// Like [`nbt`], but the root must be a compound.
pub fn compound(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    nbt_of(r, w, false, Some(mcr_nbt::tag::COMPOUND))
}

/// A text component: any NBT root but End.
pub fn text(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    nbt(r, w, false)
}

fn nbt_of(r: &mut &[u8], w: &mut Vec<u8>, optional: bool, root: Option<u8>) -> Result<()> {
    let nbt_error = |e: mcr_nbt::NbtError| DecodeError::Other(format!("nbt: {e}"));
    let Some(value) = mcr_nbt::raw::network_root(r).map_err(nbt_error)? else {
        if !optional {
            return Err(DecodeError::Other("nbt: unexpected end tag".into()));
        }
        bytes(r, w, 1)?;
        return Ok(());
    };
    if root.is_some_and(|t| t != value.tag()) {
        return Err(DecodeError::Other("nbt: root is not a compound".into()));
    }
    let size = value.size().map_err(nbt_error)?;
    bytes(r, w, 1 + size)
}

pub fn unknown(what: &'static str, value: i32) -> DecodeError {
    DecodeError::InvalidValue {
        what,
        value: value.into(),
    }
}

/// Bounds recursion through nested values (stacks inside components inside stacks).
#[derive(Debug, Clone, Copy)]
pub struct Depth(u8);

impl Depth {
    pub const MAX: u8 = 16;

    pub fn top() -> Self {
        Self(0)
    }

    pub fn deeper(self) -> Result<Self> {
        if self.0 >= Self::MAX {
            return Err(DecodeError::Other("nesting too deep".into()));
        }
        Ok(Self(self.0 + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_keeps_bytes_and_validates() {
        let mut w = Vec::new();
        let mut r: &[u8] = &[0x80, 0x01, 9];
        copy::<mcr_buf::VarInt>(&mut r, &mut w).unwrap();
        assert_eq!((w.as_slice(), r), (&[0x80, 0x01][..], &[9][..]));
        assert!(copy::<mcr_buf::VarInt>(&mut &[0x80][..], &mut w).is_err());
    }

    #[test]
    fn remap_and_bounds() {
        let mut w = Vec::new();
        remap(&mut &[3][..], &mut w, "x", |v| Some(v + 200)).unwrap();
        assert_eq!(w, [0xcb, 0x01]);
        assert!(remap(&mut &[3][..], &mut w, "x", |_| None).is_err());
        assert!(len(&mut &[5][..], &mut w, 4).is_err());
        assert!(bool(&mut &[2][..], &mut w).is_err());
        let mut d = Depth::top();
        for _ in 0..Depth::MAX {
            d = d.deeper().unwrap();
        }
        assert!(d.deeper().is_err());
    }

    #[test]
    fn nbt_copies_without_decoding_and_bounds_depth() {
        let mut w = Vec::new();
        let tag = [mcr_nbt::tag::STRING, 0, 2, b'h', b'i', 9];
        let mut r = &tag[..];
        text(&mut r, &mut w).unwrap();
        assert_eq!((w.as_slice(), r), (&tag[..5], &[9][..]));
        assert!(compound(&mut &tag[..], &mut Vec::new()).is_err());
        assert!(nbt(&mut &[0u8][..], &mut Vec::new(), false).is_err());
        let mut w = Vec::new();
        nbt(&mut &[0u8][..], &mut w, true).unwrap();
        assert_eq!(w, [0]);
        // Lists of lists nested far past the NBT depth limit.
        let mut deep = vec![mcr_nbt::tag::LIST];
        for _ in 0..2000 {
            deep.extend_from_slice(&[mcr_nbt::tag::LIST, 0, 0, 0, 1]);
        }
        deep.extend_from_slice(&[mcr_nbt::tag::END, 0, 0, 0, 0]);
        assert!(nbt(&mut deep.as_slice(), &mut Vec::new(), false).is_err());
        assert!(nbt(&mut &tag[..3], &mut Vec::new(), false).is_err());
    }
}
