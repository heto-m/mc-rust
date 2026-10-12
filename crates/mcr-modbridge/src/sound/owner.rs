use mcr_client::ClientEvent;

/// Which side plays a sound in mod mode, so each sound plays exactly once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundOwner {
    /// mcr's own predictions and the local player's movement, which Java's shadow player never plays.
    Mcr,
    /// Everything the server sends, plus music, ambience, weather, entity loops, GUI and mod sounds.
    Java,
}

/// The owner of the sounds a client event can cause; `None` for events that cause none.
pub fn event_owner(event: &ClientEvent) -> Option<SoundOwner> {
    match event {
        ClientEvent::LevelEvent {
            predicted: true, ..
        } => Some(SoundOwner::Mcr),
        ClientEvent::LevelEvent { .. }
        | ClientEvent::Sound { .. }
        | ClientEvent::LocalSound { .. }
        | ClientEvent::Explosion { .. }
        | ClientEvent::ItemTaken { .. }
        | ClientEvent::EntityEvent { .. }
        | ClientEvent::EntityDamaged { .. } => Some(SoundOwner::Java),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcr_buf::BlockPos;

    fn level_event(predicted: bool) -> ClientEvent {
        ClientEvent::LevelEvent {
            event: 2001,
            pos: BlockPos { x: 0, y: 64, z: 0 },
            data: 1,
            global: false,
            predicted,
        }
    }

    #[test]
    fn predicted_breaks_are_mcr_and_server_events_are_java() {
        assert_eq!(event_owner(&level_event(true)), Some(SoundOwner::Mcr));
        assert_eq!(event_owner(&level_event(false)), Some(SoundOwner::Java));
        assert_eq!(event_owner(&ClientEvent::ChatCleared), None);
    }
}
