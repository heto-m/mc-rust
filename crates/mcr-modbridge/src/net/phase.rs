use std::sync::OnceLock;

use mcr_client::Phase;
use mcr_protocol::{Direction, PacketIds, ProtocolState};

use crate::abi::{PHASE_CONFIGURATION, PHASE_LOGIN, PHASE_PLAY};

/// The protocol phase as the bridge ABI encodes it; Java uses the same numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NetPhase {
    Login,
    Configuration,
    Play,
}

impl NetPhase {
    pub fn tag(self) -> u8 {
        match self {
            Self::Login => PHASE_LOGIN,
            Self::Configuration => PHASE_CONFIGURATION,
            Self::Play => PHASE_PLAY,
        }
    }

    pub fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            PHASE_LOGIN => Some(Self::Login),
            PHASE_CONFIGURATION => Some(Self::Configuration),
            PHASE_PLAY => Some(Self::Play),
            _ => None,
        }
    }

    pub fn of_client(phase: Phase) -> Option<Self> {
        match phase {
            Phase::Login => Some(Self::Login),
            Phase::Configuration => Some(Self::Configuration),
            Phase::Play => Some(Self::Play),
            Phase::Closed => None,
        }
    }

    pub fn state(self) -> ProtocolState {
        match self {
            Self::Login => ProtocolState::Login,
            Self::Configuration => ProtocolState::Configuration,
            Self::Play => ProtocolState::Play,
        }
    }
}

/// Clientbound ids that move vanilla's inbound decoder, plus the login packets Java never sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionIds {
    pub login_finished: i32,
    pub finish_configuration: i32,
    pub start_configuration: i32,
    pub encryption_request: i32,
    pub login_compression: i32,
}

impl TransitionIds {
    pub fn resolve(ids: &PacketIds) -> Option<Self> {
        let cb = |state, name| ids.id_by_name(state, Direction::Clientbound, name);
        Some(Self {
            login_finished: cb(ProtocolState::Login, "login_finished")?,
            finish_configuration: cb(ProtocolState::Configuration, "finish_configuration")?,
            start_configuration: cb(ProtocolState::Play, "start_configuration")?,
            encryption_request: cb(ProtocolState::Login, "hello")?,
            login_compression: cb(ProtocolState::Login, "login_compression")?,
        })
    }

    pub fn v26_3() -> &'static Self {
        static IDS: OnceLock<TransitionIds> = OnceLock::new();
        IDS.get_or_init(|| {
            Self::resolve(PacketIds::v26_3()).expect("26.3 has every transition packet")
        })
    }
}

/// Follows the phase from clientbound packets alone, on the reader thread. A transition packet
/// still belongs to the phase it arrives in; the next frame is in the new one.
#[derive(Debug, Clone, Copy)]
pub struct PhaseTracker {
    phase: NetPhase,
    ids: TransitionIds,
}

impl PhaseTracker {
    pub fn new(ids: TransitionIds) -> Self {
        Self {
            phase: NetPhase::Login,
            ids,
        }
    }

    pub fn phase(&self) -> NetPhase {
        self.phase
    }

    /// The phase `id` arrived in, after which the tracker moves on if `id` is a transition.
    pub fn observe(&mut self, id: i32) -> NetPhase {
        let current = self.phase;
        self.phase = match current {
            NetPhase::Login if id == self.ids.login_finished => NetPhase::Configuration,
            NetPhase::Configuration if id == self.ids.finish_configuration => NetPhase::Play,
            NetPhase::Play if id == self.ids.start_configuration => NetPhase::Configuration,
            other => other,
        };
        current
    }

    /// Java's pipeline has neither an encryption nor a compression stage.
    pub fn hidden_from_java(&self, phase: NetPhase, id: i32) -> bool {
        phase == NetPhase::Login
            && (id == self.ids.encryption_request || id == self.ids.login_compression)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(state: ProtocolState, name: &str) -> i32 {
        PacketIds::v26_3()
            .id_by_name(state, Direction::Clientbound, name)
            .unwrap()
    }

    #[test]
    fn recorded_login_config_play_config_sequence() {
        use ProtocolState::{Configuration as C, Login as L, Play as P};
        let sequence = [
            (L, "login_compression", NetPhase::Login),
            (L, "login_finished", NetPhase::Login),
            (C, "custom_payload", NetPhase::Configuration),
            (C, "update_enabled_features", NetPhase::Configuration),
            (C, "select_known_packs", NetPhase::Configuration),
            (C, "registry_data", NetPhase::Configuration),
            (C, "update_tags", NetPhase::Configuration),
            (C, "finish_configuration", NetPhase::Configuration),
            (P, "login", NetPhase::Play),
            (P, "keep_alive", NetPhase::Play),
            (P, "level_chunk_with_light", NetPhase::Play),
            (P, "start_configuration", NetPhase::Play),
            (C, "keep_alive", NetPhase::Configuration),
            (C, "finish_configuration", NetPhase::Configuration),
            (P, "respawn", NetPhase::Play),
        ];
        let mut tracker = PhaseTracker::new(*TransitionIds::v26_3());
        for (state, name, want) in sequence {
            assert_eq!(tracker.observe(id(state, name)), want, "{name}");
        }
        assert_eq!(tracker.phase(), NetPhase::Play);
    }

    #[test]
    fn ids_only_transition_in_their_own_phase() {
        let ids = *TransitionIds::v26_3();
        let mut tracker = PhaseTracker::new(ids);
        assert_eq!(tracker.observe(ids.start_configuration), NetPhase::Login);
        assert_eq!(tracker.observe(ids.finish_configuration), NetPhase::Login);
        assert_eq!(tracker.phase(), NetPhase::Login);
    }

    #[test]
    fn login_crypto_and_compression_are_hidden() {
        let ids = *TransitionIds::v26_3();
        let tracker = PhaseTracker::new(ids);
        assert!(tracker.hidden_from_java(NetPhase::Login, ids.login_compression));
        assert!(tracker.hidden_from_java(NetPhase::Login, ids.encryption_request));
        assert!(!tracker.hidden_from_java(NetPhase::Login, ids.login_finished));
        assert!(!tracker.hidden_from_java(NetPhase::Play, ids.login_compression));
    }

    #[test]
    fn tags_round_trip() {
        for p in [NetPhase::Login, NetPhase::Configuration, NetPhase::Play] {
            assert_eq!(NetPhase::from_tag(p.tag()), Some(p));
        }
        assert_eq!(NetPhase::from_tag(0), None);
        assert_eq!(NetPhase::of_client(Phase::Closed), None);
    }
}
