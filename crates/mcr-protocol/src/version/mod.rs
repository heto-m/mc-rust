//! The game versions mcr can play: the canonical 26.3 set plus older versions translated onto it.
//!
//! Everything version-specific lives under this module. The rest of the client decodes and encodes only
//! canonical packets; [`Wire`] sits between those and the socket.

pub(crate) mod io;
pub mod tables;
mod translator;
mod v26_1_2;
mod v26_2;

pub use translator::{Frames, Session, TranslateError, Translated, Translator};

use crate::{Direction, ProtocolState};

pub const GAME_VERSION: &str = "26.3";
pub const PROTOCOL: i32 = 777;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionInfo {
    pub name: &'static str,
    pub protocol: i32,
}

/// Every version with a wire, newest first.
pub const VERSIONS: &[VersionInfo] = &[
    VersionInfo {
        name: GAME_VERSION,
        protocol: PROTOCOL,
    },
    VersionInfo {
        name: "26.2",
        protocol: 776,
    },
    VersionInfo {
        name: "26.1.2",
        protocol: 775,
    },
    VersionInfo {
        name: "26.1.1",
        protocol: 775,
    },
    VersionInfo {
        name: "26.1",
        protocol: 775,
    },
];

/// The versions the launcher and server list offer, newest first.
pub const SUPPORTED_VERSIONS: &[&str] = &[GAME_VERSION, "26.2", "26.1.2", "26.1.1", "26.1"];

pub fn is_supported(id: &str) -> bool {
    SUPPORTED_VERSIONS.contains(&id)
}

/// Protocol numbers of [`SUPPORTED_VERSIONS`], newest first.
pub fn supported_protocols() -> impl Iterator<Item = i32> {
    VERSIONS
        .iter()
        .filter(|v| is_supported(v.name))
        .map(|v| v.protocol)
}

pub fn version_by_protocol(protocol: i32) -> Option<&'static VersionInfo> {
    VERSIONS.iter().find(|v| v.protocol == protocol)
}

/// How frames travel between the socket and the canonical codecs.
#[derive(Debug, Clone, Copy)]
pub enum Wire {
    Canonical,
    Legacy(&'static Translator),
}

impl Wire {
    /// The wire for a supported protocol.
    pub fn for_protocol(protocol: i32) -> Option<Self> {
        let version = version_by_protocol(protocol).filter(|v| is_supported(v.name))?;
        Self::any_for_protocol(version.protocol)
    }

    /// Like [`for_protocol`](Self::for_protocol), but also for versions not offered yet.
    pub fn any_for_protocol(protocol: i32) -> Option<Self> {
        match protocol {
            PROTOCOL => Some(Self::Canonical),
            776 => Some(Self::Legacy(v26_2::translator())),
            775 => Some(Self::Legacy(v26_1_2::translator())),
            _ => None,
        }
    }

    pub fn is_canonical(self) -> bool {
        matches!(self, Self::Canonical)
    }

    pub fn protocol(self) -> i32 {
        match self {
            Self::Canonical => PROTOCOL,
            Self::Legacy(t) => t.protocol(),
        }
    }

    pub fn version_name(self) -> &'static str {
        match self {
            Self::Canonical => GAME_VERSION,
            Self::Legacy(t) => t.version_name(),
        }
    }

    /// Turns a frame from the server into canonical clientbound frames; `out` holds rewritten payloads.
    #[inline]
    pub fn inbound<'a>(
        self,
        state: ProtocolState,
        id: i32,
        payload: &'a [u8],
        session: &mut Session,
        out: &'a mut Vec<u8>,
    ) -> Result<Translated<'a>, TranslateError> {
        match self {
            Self::Canonical => Ok(Translated::Packet(id, payload)),
            Self::Legacy(t) => {
                t.translate(state, Direction::Clientbound, id, payload, session, out)
            }
        }
    }

    /// Turns a canonical serverbound frame into the frames this wire sends.
    #[inline]
    pub fn outbound<'a>(
        self,
        state: ProtocolState,
        id: i32,
        payload: &'a [u8],
        session: &mut Session,
        out: &'a mut Vec<u8>,
    ) -> Result<Translated<'a>, TranslateError> {
        match self {
            Self::Canonical => Ok(Translated::Packet(id, payload)),
            Self::Legacy(t) => {
                t.translate(state, Direction::Serverbound, id, payload, session, out)
            }
        }
    }

    /// The id a canonical clientbound packet arrives under on this wire.
    pub fn wire_clientbound_id(self, state: ProtocolState, canonical: i32) -> Option<i32> {
        match self {
            Self::Canonical => Some(canonical),
            Self::Legacy(t) => t.wire_clientbound_id(state, canonical),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_version_is_supported() {
        assert!(is_supported(GAME_VERSION));
        assert!(!is_supported(""));
        assert_eq!(supported_protocols().next(), Some(PROTOCOL));
        assert_eq!(i64::from(PROTOCOL), i64::from(mcr_data::v26_3().protocol()));
    }

    #[test]
    fn wire_lookup() {
        assert!(Wire::for_protocol(PROTOCOL).unwrap().is_canonical());
        assert!(Wire::for_protocol(770).is_none());
        let old = Wire::any_for_protocol(776).unwrap();
        assert_eq!((old.protocol(), old.version_name()), (776, "26.2"));
        assert_eq!(Wire::for_protocol(776).is_some(), is_supported("26.2"));
        let older = Wire::any_for_protocol(775).unwrap();
        assert_eq!((older.protocol(), older.version_name()), (775, "26.1.2"));
        assert_eq!(version_by_protocol(775).map(|v| v.name), Some("26.1.2"));
    }

    #[test]
    fn canonical_wire_passes_frames_through() {
        let payload = [1u8, 2, 3];
        let mut scratch = Vec::new();
        let mut session = Session::default();
        let out = Wire::Canonical
            .inbound(ProtocolState::Play, 7, &payload, &mut session, &mut scratch)
            .unwrap();
        let Translated::Packet(id, bytes) = out else {
            panic!("dropped")
        };
        assert_eq!(id, 7);
        assert!(std::ptr::eq(bytes, &payload[..]));
        assert!(scratch.is_empty());
    }
}
