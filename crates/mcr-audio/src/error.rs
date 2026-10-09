use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("vorbis decode failed: {0}")]
    Decode(String),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("asset {0} not in index")]
    MissingAsset(String),
    #[error("audio device: {0}")]
    Device(String),
}
