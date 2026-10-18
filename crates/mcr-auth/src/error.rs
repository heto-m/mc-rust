use std::io;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("no Microsoft client id configured (set MCR_MSA_CLIENT_ID)")]
    MissingClientId,
    #[error("http transport error: {0}")]
    Transport(String),
    #[error("unexpected http status {status} from {url}")]
    Status { status: u16, url: String },
    #[error("invalid response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("device code login was declined")]
    LoginDeclined,
    #[error("device code expired before login completed")]
    LoginExpired,
    #[error("oauth error: {0}")]
    OAuth(String),
    #[error("sign-in was cancelled")]
    Cancelled,
    #[error("the account is not signed in")]
    NotSignedIn,
    #[error("refresh token rejected")]
    RefreshRejected,
    #[error("the Microsoft account has no Xbox account")]
    NoXboxAccount,
    #[error("Xbox Live is unavailable in this account's country")]
    XboxCountryBanned,
    #[error("the account needs adult verification")]
    XboxAdultVerification,
    #[error("the account belongs to a child and must be added to a family")]
    XboxChildAccount,
    #[error("xsts error {0}")]
    Xsts(u64),
    #[error("account does not own Minecraft: Java Edition")]
    NotOwned,
    #[error("invalid key material: {0}")]
    Key(String),
    #[error("invalid timestamp: {0}")]
    Time(String),
    #[error("invalid player name: {0}")]
    InvalidName(&'static str),
    #[error("cache io error: {0}")]
    Io(#[from] io::Error),
}
