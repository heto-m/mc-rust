use mcr_buf::DecodeError;

use crate::{Direction, ProtocolState};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("{state:?} {direction:?}: unknown packet id {id}")]
    UnknownId {
        state: ProtocolState,
        direction: Direction,
        id: i32,
    },
    #[error("{state:?} {direction:?} {packet}: {source}")]
    Decode {
        packet: &'static str,
        state: ProtocolState,
        direction: Direction,
        source: DecodeError,
    },
    #[error("{state:?} {direction:?} {packet}: {remaining} trailing bytes")]
    TrailingBytes {
        packet: &'static str,
        state: ProtocolState,
        direction: Direction,
        remaining: usize,
    },
    #[error("{state:?} {direction:?}: version data has no packet {packet}")]
    MissingPacket {
        packet: &'static str,
        state: ProtocolState,
        direction: Direction,
    },
}
