use mcr_assets::AssetError;

#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error(transparent)]
    Asset(#[from] AssetError),
    #[error("{path}: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, UiError>;
