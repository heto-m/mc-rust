use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("{path}: invalid json: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
    #[error("{path}: invalid png: {message}")]
    Png { path: String, message: String },
    #[error("missing resource {0}")]
    Missing(String),
    #[error("{path}: {message}")]
    Invalid { path: String, message: String },
    #[error("model {model}: unresolved texture variable #{variable}")]
    UnresolvedTexture { model: String, variable: String },
    #[error("model parent chain too deep or cyclic at {0}")]
    ParentCycle(String),
    #[error("block {block}: no model matches properties [{properties}]")]
    NoVariant { block: String, properties: String },
}

pub type Result<T, E = AssetError> = std::result::Result<T, E>;

impl AssetError {
    pub(crate) fn invalid(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Invalid {
            path: path.into(),
            message: message.into(),
        }
    }
}
