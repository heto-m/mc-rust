use mcr_buf::DecodeError;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WorldError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error("invalid world config: {0}")]
    InvalidConfig(&'static str),
    #[error("id {id} out of range for registry of {count}")]
    IdOutOfRange { id: u32, count: u32 },
    #[error("palette index {index} out of range for palette of {len}")]
    PaletteIndex { index: u32, len: u32 },
    #[error("palette of {len} entries does not fit {bits} bits")]
    PaletteTooLarge { len: usize, bits: u8 },
    #[error("{what}: expected {expected}, got {got}")]
    BadLength {
        what: &'static str,
        expected: usize,
        got: usize,
    },
    #[error("{0} trailing bytes after chunk sections")]
    TrailingBytes(usize),
    #[error("malformed nbt: {0}")]
    Nbt(&'static str),
}
