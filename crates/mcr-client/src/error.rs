use mcr_protocol::ProtocolError;
use mcr_protocol::types::Component;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("network: {0}")]
    Net(#[from] mcr_net::NetError),
    #[error("protocol: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("decode: {0}")]
    Decode(#[from] mcr_buf::DecodeError),
    #[error("auth: {0}")]
    Auth(#[from] mcr_auth::AuthError),
    #[error("world: {0}")]
    World(#[from] mcr_world::WorldError),
    #[error("disconnected: {}", crate::events::sanitize_for_log(&.0.to_plain_string()))]
    Disconnected(Box<Component>),
    #[error("connection closed")]
    Closed,
    #[error("connection lost: {0}")]
    Connection(String),
    #[error("timed out while {0}")]
    Timeout(&'static str),
    #[error("{0}")]
    Invalid(String),
    #[error("server requires online mode; no account configured")]
    OnlineModeRequired,
    /// The server handed the player to another server; follow with [`crate::ClientConfig::for_transfer`].
    #[error("transferred to {}:{}", crate::events::sanitize_for_log(&.0.host), .0.port)]
    Transferred(Box<crate::config::TransferTarget>),
}

pub type Result<T> = std::result::Result<T, ClientError>;
