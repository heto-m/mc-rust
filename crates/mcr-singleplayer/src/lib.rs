//! Singleplayer: Mojang's server jar as a managed loopback child process playing vanilla worlds.

mod error;
pub mod fabric;
pub mod flat;
mod http;
pub mod icon;
pub mod java;
pub mod lan;
pub mod layout;
pub mod level;
pub mod memory;
pub mod packs;
pub mod props;
pub mod provision;
pub mod recreate;
pub mod safe;
pub mod server;
pub mod stale;

pub use error::{Result, SpError};
pub use layout::Layout;
