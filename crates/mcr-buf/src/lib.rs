//! Wire-format primitives shared by every protocol version.

mod impls;
mod varint;

pub use varint::{
    VarInt, VarLong, read_varint, read_varlong, varint_len, write_varint, write_varlong,
};

pub type Result<T> = std::result::Result<T, DecodeError>;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("varint exceeds {0} bytes")]
    VarIntTooLong(usize),
    #[error("negative length {0}")]
    NegativeLength(i32),
    #[error("length {len} exceeds limit {max}")]
    TooLong { len: usize, max: usize },
    #[error("invalid utf-8 in string")]
    InvalidUtf8,
    #[error("invalid {what}: {value}")]
    InvalidValue { what: &'static str, value: i64 },
    #[error("{0}")]
    Other(String),
}

pub trait Encode {
    fn encode(&self, out: &mut Vec<u8>);
}

/// `'a` lets borrowed types (`&'a str`, [`ByteArray`]) decode without copying.
pub trait Decode<'a>: Sized {
    fn decode(input: &mut &'a [u8]) -> Result<Self>;
}

pub fn take<'a>(input: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    if input.len() < n {
        return Err(DecodeError::UnexpectedEof);
    }
    let (head, tail) = input.split_at(n);
    *input = tail;
    Ok(head)
}

pub fn read_len(input: &mut &[u8], max: usize) -> Result<usize> {
    let len = read_varint(input)?;
    let len = usize::try_from(len).map_err(|_| DecodeError::NegativeLength(len))?;
    if len > max {
        return Err(DecodeError::TooLong { len, max });
    }
    Ok(len)
}

pub const MAX_STRING_CHARS: usize = 32767;

/// VarInt-length-prefixed bytes, borrowed from the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ByteArray<'a>(pub &'a [u8]);

/// Everything left in the packet, unprefixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RestBytes<'a>(pub &'a [u8]);

/// String with a protocol-specific maximum length in UTF-16 code units.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct BoundedString<const MAX: usize>(pub String);

pub fn decode_str<'a>(input: &mut &'a [u8], max_chars: usize) -> Result<&'a str> {
    let len = read_len(input, max_chars * 3)?;
    let s = std::str::from_utf8(take(input, len)?).map_err(|_| DecodeError::InvalidUtf8)?;
    let chars = s.encode_utf16().count();
    if chars > max_chars {
        return Err(DecodeError::TooLong {
            len: chars,
            max: max_chars,
        });
    }
    Ok(s)
}

pub fn encode_str(s: &str, out: &mut Vec<u8>) {
    write_varint(out, s.len() as i32);
    out.extend_from_slice(s.as_bytes());
}

pub fn to_bytes<T: Encode + ?Sized>(value: &T) -> Vec<u8> {
    let mut out = Vec::new();
    value.encode(&mut out);
    out
}

/// Decodes `T` and fails if any input is left over.
pub fn from_bytes_exact<'a, T: Decode<'a>>(mut input: &'a [u8]) -> Result<T> {
    let value = T::decode(&mut input)?;
    if !input.is_empty() {
        return Err(DecodeError::Other(format!(
            "{} trailing bytes",
            input.len()
        )));
    }
    Ok(value)
}
