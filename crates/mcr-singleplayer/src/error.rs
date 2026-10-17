pub type Result<T> = std::result::Result<T, SpError>;

#[derive(Debug, thiserror::Error)]
pub enum SpError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("nbt: {0}")]
    Nbt(#[from] mcr_nbt::NbtError),
    #[error("download failed: {0}")]
    Http(String),
    #[error("{0}")]
    Data(String),
    #[error("SHA-1 mismatch for {0}")]
    Hash(String),
    #[error("{0} is larger than the {1} byte limit")]
    TooLarge(String, u64),
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Refused(String),
}

impl SpError {
    pub(crate) fn data(message: impl Into<String>) -> Self {
        Self::Data(message.into())
    }
}
