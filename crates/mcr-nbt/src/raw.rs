//! Zero-copy, allocation-free access to encoded NBT. Values are located by skipping, never built.

use std::borrow::Cow;

use crate::decode::{Accounter, read_i32, read_name, read_u8, read_value, take};
use crate::{Limits, NbtError, Result, Value, mutf8, tag};

const MAX_SKIP_DEPTH: usize = 512;

fn fixed_size(ty: u8) -> Option<usize> {
    match ty {
        tag::BYTE => Some(1),
        tag::SHORT => Some(2),
        tag::INT | tag::FLOAT => Some(4),
        tag::LONG | tag::DOUBLE => Some(8),
        _ => None,
    }
}

fn skip(inp: &mut &[u8], ty: u8, depth: usize) -> Result<()> {
    if let Some(n) = fixed_size(ty) {
        take(inp, n)?;
        return Ok(());
    }
    match ty {
        tag::BYTE_ARRAY | tag::INT_ARRAY | tag::LONG_ARRAY => {
            let len = read_i32(inp)?;
            let len = usize::try_from(len).map_err(|_| NbtError::NegativeLength(len))?;
            let width = match ty {
                tag::BYTE_ARRAY => 1,
                tag::INT_ARRAY => 4,
                _ => 8,
            };
            take(inp, len.saturating_mul(width))?;
        }
        tag::STRING => {
            read_name(inp)?;
        }
        tag::LIST => {
            if depth >= MAX_SKIP_DEPTH {
                return Err(NbtError::DepthExceeded(MAX_SKIP_DEPTH));
            }
            let elem = read_u8(inp)?;
            let count = usize::try_from(read_i32(inp)?).unwrap_or(0);
            if count == 0 {
                return Ok(());
            }
            if elem == tag::END {
                return Err(NbtError::MissingListType);
            }
            if let Some(n) = fixed_size(elem) {
                take(inp, count.saturating_mul(n))?;
            } else {
                for _ in 0..count {
                    skip(inp, elem, depth + 1)?;
                }
            }
        }
        tag::COMPOUND => {
            if depth >= MAX_SKIP_DEPTH {
                return Err(NbtError::DepthExceeded(MAX_SKIP_DEPTH));
            }
            loop {
                let ty = read_u8(inp)?;
                if ty == tag::END {
                    break;
                }
                read_name(inp)?;
                skip(inp, ty, depth + 1)?;
            }
        }
        t => return Err(NbtError::InvalidTag(t)),
    }
    Ok(())
}

/// A tag payload; `data` starts at the payload and may extend past it.
#[derive(Debug, Clone, Copy)]
pub struct RawValue<'a> {
    ty: u8,
    data: &'a [u8],
}

#[derive(Debug, Clone, Copy)]
pub struct RawCompound<'a>(&'a [u8]);

#[derive(Debug, Clone, Copy)]
pub struct RawList<'a> {
    elem: u8,
    len: usize,
    data: &'a [u8],
}

macro_rules! be_array {
    ($name:ident, $t:ty) => {
        #[derive(Debug, Clone, Copy)]
        pub struct $name<'a>(&'a [u8]);

        impl<'a> $name<'a> {
            pub fn len(&self) -> usize {
                self.0.len() / size_of::<$t>()
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }

            pub fn get(&self, i: usize) -> Option<$t> {
                let n = size_of::<$t>();
                let c = self.0.get(i * n..(i + 1) * n)?;
                Some(<$t>::from_be_bytes(c.try_into().unwrap()))
            }

            pub fn iter(&self) -> impl ExactSizeIterator<Item = $t> + 'a {
                self.0
                    .chunks_exact(size_of::<$t>())
                    .map(|c| <$t>::from_be_bytes(c.try_into().unwrap()))
            }

            pub fn as_bytes(&self) -> &'a [u8] {
                self.0
            }
        }
    };
}

be_array!(IntArrayRef, i32);
be_array!(LongArrayRef, i64);

/// Network root: `None` for the End tag.
pub fn network_root(mut input: &[u8]) -> Result<Option<RawValue<'_>>> {
    let ty = read_u8(&mut input)?;
    if ty == tag::END {
        return Ok(None);
    }
    Ok(Some(RawValue { ty, data: input }))
}

/// Disk root (already decompressed): the root name and value.
pub fn disk_root(mut input: &[u8]) -> Result<(Cow<'_, str>, RawValue<'_>)> {
    let ty = read_u8(&mut input)?;
    if ty == tag::END {
        return Err(NbtError::UnexpectedEnd);
    }
    let name = mutf8::decode(read_name(&mut input)?)?;
    Ok((name, RawValue { ty, data: input }))
}

impl<'a> RawValue<'a> {
    pub fn new(ty: u8, data: &'a [u8]) -> Self {
        Self { ty, data }
    }

    pub fn tag(&self) -> u8 {
        self.ty
    }

    /// Encoded payload length; validates structure and nesting depth.
    pub fn size(&self) -> Result<usize> {
        let mut rest = self.data;
        skip(&mut rest, self.ty, 0)?;
        Ok(self.data.len() - rest.len())
    }

    pub fn to_value(&self, limits: Limits) -> Result<Value> {
        let mut data = self.data;
        read_value(&mut data, self.ty, &mut Accounter::new(limits))
    }

    fn fixed<const N: usize>(&self, ty: u8) -> Option<[u8; N]> {
        if self.ty != ty {
            return None;
        }
        self.data.get(..N)?.try_into().ok()
    }

    pub fn as_i8(&self) -> Option<i8> {
        self.fixed::<1>(tag::BYTE).map(|b| b[0] as i8)
    }

    pub fn as_i16(&self) -> Option<i16> {
        self.fixed(tag::SHORT).map(i16::from_be_bytes)
    }

    pub fn as_i32(&self) -> Option<i32> {
        self.fixed(tag::INT).map(i32::from_be_bytes)
    }

    pub fn as_i64(&self) -> Option<i64> {
        self.fixed(tag::LONG).map(i64::from_be_bytes)
    }

    pub fn as_f32(&self) -> Option<f32> {
        self.fixed(tag::FLOAT).map(f32::from_be_bytes)
    }

    pub fn as_f64(&self) -> Option<f64> {
        self.fixed(tag::DOUBLE).map(f64::from_be_bytes)
    }

    pub fn as_str(&self) -> Option<Cow<'a, str>> {
        if self.ty != tag::STRING {
            return None;
        }
        let mut d = self.data;
        mutf8::decode(read_name(&mut d).ok()?).ok()
    }

    fn array(&self, ty: u8, width: usize) -> Option<&'a [u8]> {
        if self.ty != ty {
            return None;
        }
        let mut d = self.data;
        let len = usize::try_from(read_i32(&mut d).ok()?).ok()?;
        take(&mut d, len.checked_mul(width)?).ok()
    }

    pub fn as_byte_array(&self) -> Option<&'a [u8]> {
        self.array(tag::BYTE_ARRAY, 1)
    }

    pub fn as_int_array(&self) -> Option<IntArrayRef<'a>> {
        self.array(tag::INT_ARRAY, 4).map(IntArrayRef)
    }

    pub fn as_long_array(&self) -> Option<LongArrayRef<'a>> {
        self.array(tag::LONG_ARRAY, 8).map(LongArrayRef)
    }

    pub fn as_compound(&self) -> Option<RawCompound<'a>> {
        (self.ty == tag::COMPOUND).then_some(RawCompound(self.data))
    }

    pub fn as_list(&self) -> Option<RawList<'a>> {
        if self.ty != tag::LIST {
            return None;
        }
        let mut d = self.data;
        let elem = read_u8(&mut d).ok()?;
        let len = usize::try_from(read_i32(&mut d).ok()?).unwrap_or(0);
        Some(RawList {
            elem,
            len: if elem == tag::END { 0 } else { len },
            data: d,
        })
    }
}

impl<'a> RawCompound<'a> {
    pub fn iter(&self) -> RawCompoundIter<'a> {
        RawCompoundIter {
            data: self.0,
            done: false,
        }
    }

    /// Scans entries in order; stops at the match, so later entries are never touched.
    pub fn get(&self, key: &str) -> Result<Option<RawValue<'a>>> {
        let key = mutf8::encode(key);
        for entry in self.iter() {
            let (name, value) = entry?;
            if name == &*key {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    pub fn get_i32(&self, key: &str) -> Option<i32> {
        self.get(key).ok()??.as_i32()
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key).ok()??.as_i64()
    }

    pub fn get_str(&self, key: &str) -> Option<Cow<'a, str>> {
        self.get(key).ok()??.as_str()
    }

    pub fn get_compound(&self, key: &str) -> Option<RawCompound<'a>> {
        self.get(key).ok()??.as_compound()
    }

    pub fn get_long_array(&self, key: &str) -> Option<LongArrayRef<'a>> {
        self.get(key).ok()??.as_long_array()
    }
}

#[derive(Debug, Clone)]
pub struct RawCompoundIter<'a> {
    data: &'a [u8],
    done: bool,
}

impl<'a> Iterator for RawCompoundIter<'a> {
    /// Raw Modified UTF-8 name bytes and the value.
    type Item = Result<(&'a [u8], RawValue<'a>)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let step = |d: &mut &'a [u8]| -> Result<Option<(&'a [u8], RawValue<'a>)>> {
            let ty = read_u8(d)?;
            if ty == tag::END {
                return Ok(None);
            }
            let name = read_name(d)?;
            let value = RawValue { ty, data: d };
            skip(d, ty, 1)?;
            Ok(Some((name, value)))
        };
        match step(&mut self.data) {
            Ok(Some(e)) => Some(Ok(e)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}

impl<'a> RawList<'a> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn element_tag(&self) -> u8 {
        self.elem
    }

    pub fn iter(&self) -> impl Iterator<Item = Result<RawValue<'a>>> + 'a {
        let (elem, mut data) = (self.elem, self.data);
        (0..self.len).map(move |_| {
            let value = RawValue { ty: elem, data };
            skip(&mut data, elem, 1)?;
            Ok(value)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Compound, NetworkNbt};
    use mcr_buf::to_bytes;

    fn sample() -> Vec<u8> {
        let mut inner = Compound::new();
        inner.insert("deep", 7i64);
        let mut c = Compound::new();
        c.insert("name", "hé\0llo");
        c.insert("n", 42i32);
        c.insert("list", crate::List::Int(vec![1, 2, 3]));
        c.insert("inner", inner);
        c.insert("longs", vec![5i64, 6]);
        to_bytes(&NetworkNbt(Value::Compound(c)))
    }

    #[test]
    fn extracts_keys_without_owning() {
        let bytes = sample();
        let root = network_root(&bytes)
            .unwrap()
            .unwrap()
            .as_compound()
            .unwrap();
        assert_eq!(root.get_i32("n"), Some(42));
        assert_eq!(root.get_str("name").as_deref(), Some("hé\0llo"));
        assert_eq!(root.get_compound("inner").unwrap().get_i64("deep"), Some(7));
        let longs = root.get_long_array("longs").unwrap();
        assert_eq!(longs.iter().collect::<Vec<_>>(), [5, 6]);
        assert!(root.get("missing").unwrap().is_none());
        let list = root.get("list").unwrap().unwrap().as_list().unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list.iter().last().unwrap().unwrap().as_i32(), Some(3));
    }

    #[test]
    fn size_and_to_value_agree() {
        let bytes = sample();
        let v = network_root(&bytes).unwrap().unwrap();
        assert_eq!(v.size().unwrap(), bytes.len() - 1);
        let owned = v.to_value(Limits::NETWORK).unwrap();
        assert_eq!(owned.as_compound().unwrap().get_i32("n"), Some(42));
    }

    #[test]
    fn truncated_and_deep_do_not_panic() {
        let bytes = sample();
        for n in 0..bytes.len() {
            if let Ok(Some(v)) = network_root(&bytes[..n]) {
                let _ = v.size();
                if let Some(c) = v.as_compound() {
                    let _ = c.get("longs");
                    let _ = c.iter().count();
                }
            }
        }
        let mut deep = vec![tag::LIST];
        for _ in 0..5000 {
            deep.extend_from_slice(&[tag::LIST, 0, 0, 0, 1]);
        }
        let v = network_root(&deep).unwrap().unwrap();
        assert!(matches!(v.size(), Err(NbtError::DepthExceeded(_))));
    }
}
