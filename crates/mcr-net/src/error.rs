use std::io;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("connection closed by peer")]
    Eof,
    #[error("connection closed locally")]
    LocalClose,
    #[error("frame length prefix wider than 21 bits")]
    LengthTooWide,
    #[error("zero-length frame")]
    ZeroLength,
    #[error("frame of {0} bytes exceeds the 2097151 byte limit")]
    FrameTooLarge(usize),
    #[error("malformed varint")]
    BadVarInt,
    #[error("invalid data length {0}")]
    BadDataLength(i32),
    #[error("compressed size {size} is below threshold {threshold}")]
    BelowThreshold { size: usize, threshold: usize },
    #[error("uncompressed size {0} exceeds the 8388608 byte limit")]
    UncompressedTooLarge(usize),
    #[error("zlib stream is corrupt")]
    Inflate,
    #[error("zlib compression failed")]
    Deflate,
    #[error("inflated {actual} bytes but frame declared {declared}")]
    SizeMismatch { declared: usize, actual: usize },
    #[error("connection closed: {0}")]
    Disconnected(Arc<NetError>),
}
