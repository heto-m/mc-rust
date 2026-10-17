//! Singleplayer: Mojang's server jar as a managed loopback child process playing vanilla worlds.

mod error;
mod http;
pub mod java;
pub mod layout;
pub mod level;
pub mod memory;
pub mod props;
pub mod provision;
pub mod server;
pub mod stale;

pub use error::{Result, SpError};
pub use layout::Layout;
