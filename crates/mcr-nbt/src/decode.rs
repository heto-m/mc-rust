use crate::{Compound, List, NbtError, Result, Value, mutf8, tag};

/// Mirrors vanilla `NbtAccounter`: nesting depth plus an approximate heap-byte quota.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_depth: usize,
    pub max_bytes: usize,
}

impl Limits {
    pub const NETWORK: Limits = Limits {
        max_depth: 512,
        max_bytes: 2 * 1024 * 1024,
    };
    pub const UNCOMPRESSED: Limits = Limits {
        max_depth: 512,
        max_bytes: 100 * 1024 * 1024,
    };
    pub const UNLIMITED: Limits = Limits {
        max_depth: 512,
        max_bytes: usize::MAX,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Self::NETWORK
    }
}

pub(crate) struct Accounter {
    limits: Limits,
    used: usize,
    depth: usize,
}

impl Accounter {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            used: 0,
            depth: 0,
        }
    }

    fn account(&mut self, n: usize) -> Result<()> {
        match self.used.checked_add(n) {
            Some(u) if u <= self.limits.max_bytes => {
                self.used = u;
                Ok(())
            }
            _ => Err(NbtError::QuotaExceeded(self.limits.max_bytes)),
        }
    }

    fn push(&mut self) -> Result<()> {
        if self.depth >= self.limits.max_depth {
            return Err(NbtError::DepthExceeded(self.limits.max_depth));
        }
        self.depth += 1;
        Ok(())
    }
}

pub(crate) fn take<'a>(inp: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    if inp.len() < n {
        return Err(NbtError::UnexpectedEof);
    }
    let (head, tail) = inp.split_at(n);
    *inp = tail;
    Ok(head)
}

pub(crate) fn read_u8(inp: &mut &[u8]) -> Result<u8> {
    Ok(take(inp, 1)?[0])
}

pub(crate) fn read_u16(inp: &mut &[u8]) -> Result<u16> {
    Ok(u16::from_be_bytes(take(inp, 2)?.try_into().unwrap()))
}

pub(crate) fn read_i32(inp: &mut &[u8]) -> Result<i32> {
    Ok(i32::from_be_bytes(take(inp, 4)?.try_into().unwrap()))
}

pub(crate) fn read_name<'a>(inp: &mut &'a [u8]) -> Result<&'a [u8]> {
    let len = usize::from(read_u16(inp)?);
    take(inp, len)
}

fn read_string(inp: &mut &[u8], acc: &mut Accounter) -> Result<String> {
    acc.account(36)?;
    let raw = read_name(inp)?;
    acc.account(raw.len() * 2)?;
    Ok(mutf8::decode(raw)?.into_owned())
}

fn array_len(inp: &mut &[u8], elem: usize, acc: &mut Accounter) -> Result<usize> {
    let len = read_i32(inp)?;
    let len = usize::try_from(len).map_err(|_| NbtError::NegativeLength(len))?;
    acc.account(len.saturating_mul(elem))?;
    Ok(len)
}

macro_rules! be_vec {
    ($inp:expr, $len:expr, $t:ty) => {{
        const N: usize = size_of::<$t>();
        let bytes = take($inp, $len * N)?;
        bytes
            .chunks_exact(N)
            .map(|c| <$t>::from_be_bytes(c.try_into().unwrap()))
            .collect::<Vec<$t>>()
    }};
}

pub(crate) fn read_value(inp: &mut &[u8], ty: u8, acc: &mut Accounter) -> Result<Value> {
    Ok(match ty {
        tag::BYTE => {
            acc.account(9)?;
            Value::Byte(read_u8(inp)? as i8)
        }
        tag::SHORT => {
            acc.account(10)?;
            Value::Short(read_u16(inp)? as i16)
        }
        tag::INT => {
            acc.account(12)?;
            Value::Int(read_i32(inp)?)
        }
        tag::LONG => {
            acc.account(16)?;
            Value::Long(i64::from_be_bytes(take(inp, 8)?.try_into().unwrap()))
        }
        tag::FLOAT => {
            acc.account(12)?;
            Value::Float(f32::from_bits(read_i32(inp)? as u32))
        }
        tag::DOUBLE => {
            acc.account(16)?;
            Value::Double(f64::from_be_bytes(take(inp, 8)?.try_into().unwrap()))
        }
        tag::BYTE_ARRAY => {
            acc.account(24)?;
            let len = array_len(inp, 1, acc)?;
            Value::ByteArray(take(inp, len)?.iter().map(|&b| b as i8).collect())
        }
        tag::STRING => Value::String(read_string(inp, acc)?),
        tag::LIST => Value::List(read_list(inp, acc)?),
        tag::COMPOUND => Value::Compound(read_compound(inp, acc)?),
        tag::INT_ARRAY => {
            acc.account(24)?;
            let len = array_len(inp, 4, acc)?;
            Value::IntArray(be_vec!(inp, len, i32))
        }
        tag::LONG_ARRAY => {
            acc.account(24)?;
            let len = array_len(inp, 8, acc)?;
            Value::LongArray(be_vec!(inp, len, i64))
        }
        t => return Err(NbtError::InvalidTag(t)),
    })
}

pub(crate) fn read_compound(inp: &mut &[u8], acc: &mut Accounter) -> Result<Compound> {
    acc.push()?;
    acc.account(48)?;
    let mut c = Compound::new();
    loop {
        let ty = read_u8(inp)?;
        if ty == tag::END {
            break;
        }
        let name = read_name(inp)?;
        acc.account(28 + name.len() * 2)?;
        let key = mutf8::decode(name)?.into_owned();
        let v = read_value(inp, ty, acc)?;
        if c.insert(key, v).is_none() {
            acc.account(36)?;
        }
    }
    acc.depth -= 1;
    Ok(c)
}

fn read_list(inp: &mut &[u8], acc: &mut Accounter) -> Result<List> {
    acc.push()?;
    acc.account(36)?;
    let ty = read_u8(inp)?;
    let count = read_i32(inp)?;
    let count = usize::try_from(count).unwrap_or(0);
    if count == 0 {
        acc.depth -= 1;
        return Ok(List::Empty);
    }
    let min_elem = match ty {
        tag::END => return Err(NbtError::MissingListType),
        tag::BYTE => 1,
        tag::SHORT => 2,
        tag::INT | tag::FLOAT => 4,
        tag::LONG | tag::DOUBLE => 8,
        tag::BYTE_ARRAY | tag::INT_ARRAY | tag::LONG_ARRAY => 4,
        tag::STRING => 2,
        tag::LIST => 5,
        tag::COMPOUND => 1,
        t => return Err(NbtError::InvalidTag(t)),
    };
    acc.account(count.saturating_mul(4))?;
    if count.saturating_mul(min_elem) > inp.len() {
        return Err(NbtError::UnexpectedEof);
    }
    macro_rules! each {
        ($variant:ident) => {{
            let mut v = Vec::with_capacity(count.min(1024));
            for _ in 0..count {
                match read_value(inp, ty, acc)? {
                    Value::$variant(x) => v.push(x),
                    _ => unreachable!(),
                }
            }
            List::$variant(v)
        }};
    }
    let list = match ty {
        tag::BYTE => {
            acc.account(count * 9)?;
            List::Byte(take(inp, count)?.iter().map(|&b| b as i8).collect())
        }
        tag::SHORT => {
            acc.account(count * 10)?;
            List::Short(be_vec!(inp, count, i16))
        }
        tag::INT => {
            acc.account(count * 12)?;
            List::Int(be_vec!(inp, count, i32))
        }
        tag::LONG => {
            acc.account(count * 16)?;
            List::Long(be_vec!(inp, count, i64))
        }
        tag::FLOAT => {
            acc.account(count * 12)?;
            List::Float(be_vec!(inp, count, f32))
        }
        tag::DOUBLE => {
            acc.account(count * 16)?;
            List::Double(be_vec!(inp, count, f64))
        }
        tag::BYTE_ARRAY => each!(ByteArray),
        tag::STRING => each!(String),
        tag::COMPOUND => each!(Compound),
        tag::INT_ARRAY => each!(IntArray),
        tag::LONG_ARRAY => each!(LongArray),
        _ => {
            let mut v = Vec::with_capacity(count.min(1024));
            for _ in 0..count {
                v.push(read_list(inp, acc)?);
            }
            List::List(v)
        }
    };
    acc.depth -= 1;
    Ok(list)
}
