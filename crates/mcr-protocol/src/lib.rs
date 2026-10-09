//! Canonical packet definitions for the latest supported version (26.3, protocol 777).
//! Older versions translate onto these types; never add per-version fields here.

pub mod common;
pub mod configuration;
pub mod dispatch;
#[cfg(test)]
mod dispatch_tests;
mod error;
pub mod handshake;
mod ids;
pub mod login;
pub mod play;
pub mod status;
pub mod types;

pub use error::ProtocolError;
pub use ids::{PacketIds, PacketSet};
pub use mcr_data::{Direction, ProtocolState};

pub trait Packet {
    /// Namespaced name as in the data generator's `packets.json`, e.g. `minecraft:login_finished`.
    const NAME: &'static str;
    const DIRECTION: Direction;
}

/// `packet!(AddEntity, "add_entity", Clientbound);` or `packet!(Chat<'a>, ...)`.
/// Packets shared by configuration and play (keep-alive, ping, ...) are one type; states map
/// names to types in the dispatch tables.
#[macro_export]
macro_rules! packet {
    ($ty:ident $(<$lt:lifetime>)?, $name:literal, $dir:ident) => {
        impl$(<$lt>)? $crate::Packet for $ty$(<$lt>)? {
            const NAME: &'static str = concat!("minecraft:", $name);
            const DIRECTION: $crate::Direction = $crate::Direction::$dir;
        }
    };
}

#[cfg(test)]
pub(crate) mod test_util {
    use std::fmt::Debug;

    use mcr_buf::{Decode, Encode, from_bytes_exact, to_bytes};

    pub fn roundtrip<T>(value: &T) -> Vec<u8>
    where
        T: Encode + for<'a> Decode<'a> + PartialEq + Debug,
    {
        let bytes = to_bytes(value);
        assert_eq!(&from_bytes_exact::<T>(&bytes).unwrap(), value);
        bytes
    }
}
