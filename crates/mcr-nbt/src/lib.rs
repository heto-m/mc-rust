//! NBT: owned model, borrowed zero-copy reader, Modified UTF-8, disk/network formats and SNBT.

mod compound;
mod decode;
mod encode;
mod io;
pub mod json;
pub mod mutf8;
pub mod raw;
pub mod snbt;
mod value;

pub use compound::Compound;
pub use decode::Limits;
pub use io::{
    Compression, NetworkCompound, NetworkNbt, OptionalNetworkNbt, read_disk, read_network,
    write_disk, write_network,
};
pub use value::{List, Value};

pub mod tag {
    pub const END: u8 = 0;
    pub const BYTE: u8 = 1;
    pub const SHORT: u8 = 2;
    pub const INT: u8 = 3;
    pub const LONG: u8 = 4;
    pub const FLOAT: u8 = 5;
    pub const DOUBLE: u8 = 6;
    pub const BYTE_ARRAY: u8 = 7;
    pub const STRING: u8 = 8;
    pub const LIST: u8 = 9;
    pub const COMPOUND: u8 = 10;
    pub const INT_ARRAY: u8 = 11;
    pub const LONG_ARRAY: u8 = 12;
}

pub type Result<T> = std::result::Result<T, NbtError>;

#[derive(Debug, thiserror::Error)]
pub enum NbtError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("nesting deeper than {0}")]
    DepthExceeded(usize),
    #[error("data exceeds quota of {0} bytes")]
    QuotaExceeded(usize),
    #[error("invalid tag id {0}")]
    InvalidTag(u8),
    #[error("negative array length {0}")]
    NegativeLength(i32),
    #[error("non-empty list without element type")]
    MissingListType,
    #[error("invalid modified utf-8")]
    InvalidMutf8,
    #[error("root tag is not a compound")]
    NotCompound,
    #[error("unexpected end tag")]
    UnexpectedEnd,
    #[error("{0} trailing bytes")]
    TrailingBytes(usize),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<NbtError> for mcr_buf::DecodeError {
    fn from(e: NbtError) -> Self {
        match e {
            NbtError::UnexpectedEof => Self::UnexpectedEof,
            e => Self::Other(e.to_string()),
        }
    }
}
