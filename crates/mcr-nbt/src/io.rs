use std::io::{Read, Write};

use flate2::Compression as Level;
use flate2::read::{GzDecoder, ZlibDecoder};
use flate2::write::{GzEncoder, ZlibEncoder};
use mcr_buf::{Decode, Encode};

use crate::decode::{Accounter, read_compound, read_name, read_u8, read_value};
use crate::encode::{write_compound, write_name, write_value};
use crate::{Compound, Limits, NbtError, Result, Value, mutf8, tag};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Gzip,
    Zlib,
}

/// Network NBT: type byte then payload with no root name. `None` is the End tag.
pub fn read_network(input: &mut &[u8], limits: Limits) -> Result<Option<Value>> {
    let ty = read_u8(input)?;
    if ty == tag::END {
        return Ok(None);
    }
    read_value(input, ty, &mut Accounter::new(limits)).map(Some)
}

pub fn write_network(out: &mut Vec<u8>, value: Option<&Value>) {
    match value {
        None => out.push(tag::END),
        Some(v) => {
            out.push(v.tag());
            write_value(out, v);
        }
    }
}

/// Any root tag; the End tag is an error.
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkNbt(pub Value);

/// Root must be a compound; the End tag is an error.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NetworkCompound(pub Compound);

/// Any root tag, with the End tag meaning absent.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OptionalNetworkNbt(pub Option<Value>);

impl NetworkNbt {
    pub fn decode_with(input: &mut &[u8], limits: Limits) -> Result<Self> {
        read_network(input, limits)?
            .map(Self)
            .ok_or(NbtError::UnexpectedEnd)
    }
}

impl NetworkCompound {
    pub fn decode_with(input: &mut &[u8], limits: Limits) -> Result<Self> {
        match NetworkNbt::decode_with(input, limits)?.0 {
            Value::Compound(c) => Ok(Self(c)),
            _ => Err(NbtError::NotCompound),
        }
    }
}

impl OptionalNetworkNbt {
    pub fn decode_with(input: &mut &[u8], limits: Limits) -> Result<Self> {
        read_network(input, limits).map(Self)
    }
}

impl Decode<'_> for NetworkNbt {
    fn decode(input: &mut &[u8]) -> mcr_buf::Result<Self> {
        Ok(Self::decode_with(input, Limits::NETWORK)?)
    }
}

impl Decode<'_> for NetworkCompound {
    fn decode(input: &mut &[u8]) -> mcr_buf::Result<Self> {
        Ok(Self::decode_with(input, Limits::NETWORK)?)
    }
}

impl Decode<'_> for OptionalNetworkNbt {
    fn decode(input: &mut &[u8]) -> mcr_buf::Result<Self> {
        Ok(Self::decode_with(input, Limits::NETWORK)?)
    }
}

impl Encode for NetworkNbt {
    fn encode(&self, out: &mut Vec<u8>) {
        write_network(out, Some(&self.0));
    }
}

impl Encode for NetworkCompound {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(tag::COMPOUND);
        write_compound(out, &self.0);
    }
}

impl Encode for OptionalNetworkNbt {
    fn encode(&self, out: &mut Vec<u8>) {
        write_network(out, self.0.as_ref());
    }
}

fn inflate<R: Read>(reader: R, limits: Limits) -> Result<Vec<u8>> {
    let cap = u64::try_from(limits.max_bytes).unwrap_or(u64::MAX);
    let mut out = Vec::new();
    reader.take(cap.saturating_add(1)).read_to_end(&mut out)?;
    if out.len() as u64 > cap {
        return Err(NbtError::QuotaExceeded(limits.max_bytes));
    }
    Ok(out)
}

/// Reads a named root compound; gzip and zlib are detected from the magic bytes.
/// `limits.max_bytes` also bounds the inflated size.
pub fn read_disk(bytes: &[u8], limits: Limits) -> Result<(String, Compound)> {
    let inflated;
    let mut data = match bytes {
        [0x1f, 0x8b, ..] => {
            inflated = inflate(GzDecoder::new(bytes), limits)?;
            &inflated[..]
        }
        [0x78, ..] => {
            inflated = inflate(ZlibDecoder::new(bytes), limits)?;
            &inflated[..]
        }
        _ => bytes,
    };
    if read_u8(&mut data)? != tag::COMPOUND {
        return Err(NbtError::NotCompound);
    }
    let name = mutf8::decode(read_name(&mut data)?)?.into_owned();
    let compound = read_compound(&mut data, &mut Accounter::new(limits))?;
    Ok((name, compound))
}

pub fn write_disk(name: &str, root: &Compound, compression: Compression) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    raw.push(tag::COMPOUND);
    write_name(&mut raw, name);
    write_compound(&mut raw, root);
    Ok(match compression {
        Compression::None => raw,
        Compression::Gzip => {
            let mut e = GzEncoder::new(Vec::new(), Level::fast());
            e.write_all(&raw)?;
            e.finish()?
        }
        Compression::Zlib => {
            let mut e = ZlibEncoder::new(Vec::new(), Level::fast());
            e.write_all(&raw)?;
            e.finish()?
        }
    })
}
