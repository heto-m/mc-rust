use mcr_net::FrameTap;

use crate::client::Phase;
use crate::player::LocalPlayer;

/// Serverbound traffic a mirror client (the mod bridge) hands to the real connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relayed<'a> {
    /// A raw frame, already filtered for the phase passed to [`Relay::poll`].
    Frame {
        id: i32,
        payload: &'a [u8],
    },
    Chat(&'a str),
    /// Without the leading slash.
    Command(&'a str),
    /// The mirror closed its side; the client closes the connection.
    Disconnect(&'a str),
}

/// A mirror of this connection that may send through it. Called on the game thread only.
pub trait Relay: Send + std::fmt::Debug {
    fn poll(&mut self, phase: Phase, sink: &mut dyn FnMut(Relayed<'_>));
    /// While true the mirror answers login custom queries and the client stays silent.
    fn answers_custom_queries(&self) -> bool;
    /// After every client tick in play, with the local player as the server now knows it.
    fn tick_end(&mut self, _tick: u64, _player: &LocalPlayer) {}
    /// The connection is gone (network error or local close).
    fn closed(&mut self, _reason: &str) {}
}

#[derive(Default)]
pub struct ClientHooks {
    pub frame_tap: Option<FrameTap>,
    pub relay: Option<Box<dyn Relay>>,
}

impl std::fmt::Debug for ClientHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientHooks")
            .field("frame_tap", &self.frame_tap.is_some())
            .field("relay", &self.relay)
            .finish()
    }
}
