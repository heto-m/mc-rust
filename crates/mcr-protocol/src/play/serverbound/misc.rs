use mcr_buf::{BlockPos, Decode, Encode, Identifier, LengthPrefixed, Result, VarInt, write_varint};
use uuid::Uuid;

use super::shared::{Difficulty, GameType};
use crate::packet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct BlockEntityTagQuery {
    #[mcr(varint)]
    pub transaction_id: i32,
    pub pos: BlockPos,
}

packet!(BlockEntityTagQuery, "block_entity_tag_query", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct EntityTagQuery {
    #[mcr(varint)]
    pub transaction_id: i32,
    #[mcr(varint)]
    pub entity_id: i32,
}

packet!(EntityTagQuery, "entity_tag_query", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct ChangeDifficulty {
    pub difficulty: Difficulty,
}

packet!(ChangeDifficulty, "change_difficulty", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct ChangeGameMode {
    pub mode: GameType,
}

packet!(ChangeGameMode, "change_game_mode", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct LockDifficulty {
    pub locked: bool,
}

packet!(LockDifficulty, "lock_difficulty", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct ChunkBatchReceived {
    pub desired_chunks_per_tick: f32,
}

packet!(ChunkBatchReceived, "chunk_batch_received", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum ClientCommandAction {
    PerformRespawn,
    RequestStats,
    RequestGameruleValues,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct ClientCommand {
    pub action: ClientCommandAction,
}

packet!(ClientCommand, "client_command", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Encode, Decode)]
pub struct ClientTickEnd;

packet!(ClientTickEnd, "client_tick_end", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Encode, Decode)]
pub struct ConfigurationAcknowledged;

packet!(
    ConfigurationAcknowledged,
    "configuration_acknowledged",
    Serverbound
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Encode, Decode)]
pub struct PlayerLoaded;

packet!(PlayerLoaded, "player_loaded", Serverbound);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Encode, Decode)]
pub struct DebugSubscriptionRequest {
    pub subscriptions: LengthPrefixed<VarInt, 32>,
}

packet!(
    DebugSubscriptionRequest,
    "debug_subscription_request",
    Serverbound
);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub enum SeenAdvancements {
    OpenedTab(Identifier),
    ClosedScreen,
}

packet!(SeenAdvancements, "seen_advancements", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct TeleportToEntity {
    pub target: Uuid,
}

packet!(TeleportToEntity, "teleport_to_entity", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SpectatorAction {
    pub spectate_entity_id: Option<i32>,
}

impl Encode for SpectatorAction {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.spectate_entity_id.map_or(0, |id| id + 1));
    }
}

impl Decode<'_> for SpectatorAction {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let raw = mcr_buf::read_varint(input)?;
        Ok(Self {
            spectate_entity_id: (raw != 0).then(|| raw - 1),
        })
    }
}

packet!(SpectatorAction, "spectator_action", Serverbound);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct GameRuleEntry {
    pub rule: Identifier,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Encode, Decode)]
pub struct SetGameRule {
    pub entries: Vec<GameRuleEntry>,
}

packet!(SetGameRule, "set_game_rule", Serverbound);

#[cfg(test)]
mod tests {
    use mcr_buf::{from_bytes_exact, to_bytes};

    use super::*;
    use crate::play::serverbound::shared::roundtrips;
    use crate::status::PingRequest;
    use crate::test_util::roundtrip;

    roundtrips! {
        block_query: BlockEntityTagQuery { transaction_id: 3, pos: BlockPos::new(1, 2, 3) },
        entity_query: EntityTagQuery { transaction_id: 3, entity_id: 7 },
        difficulty: ChangeDifficulty { difficulty: Difficulty::Hard },
        game_mode: ChangeGameMode { mode: GameType::Spectator },
        lock: LockDifficulty { locked: true },
        batch: ChunkBatchReceived { desired_chunks_per_tick: 7.5 },
        client_command: ClientCommand { action: ClientCommandAction::RequestStats },
        tick_end: ClientTickEnd,
        config_ack: ConfigurationAcknowledged,
        loaded: PlayerLoaded,
        ping: PingRequest { time: -9 },
        debug: DebugSubscriptionRequest { subscriptions: LengthPrefixed(vec![VarInt(1), VarInt(200)]) },
        advancements_open: SeenAdvancements::OpenedTab(Identifier::parse("minecraft:story/root").unwrap()),
        advancements_close: SeenAdvancements::ClosedScreen,
        teleport: TeleportToEntity { target: Uuid::from_u128(5) },
        spectator: SpectatorAction { spectate_entity_id: Some(4) },
        game_rules: SetGameRule { entries: vec![GameRuleEntry { rule: Identifier::parse("keep_inventory").unwrap(), value: "true".into() }] },
    }

    #[test]
    fn unit_packets_are_empty() {
        assert!(to_bytes(&ClientTickEnd).is_empty());
        assert!(to_bytes(&PlayerLoaded).is_empty());
        assert!(to_bytes(&ConfigurationAcknowledged).is_empty());
    }

    #[test]
    fn spectator_optional_varint() {
        assert_eq!(roundtrip(&SpectatorAction::default()), [0]);
        assert_eq!(
            roundtrip(&SpectatorAction {
                spectate_entity_id: Some(0)
            }),
            [1]
        );
    }

    #[test]
    fn advancements_tag() {
        assert_eq!(roundtrip(&SeenAdvancements::ClosedScreen), [1]);
        let open = to_bytes(&SeenAdvancements::OpenedTab(
            Identifier::parse("a:b").unwrap(),
        ));
        assert_eq!(open, [0, 3, b'a', b':', b'b']);
    }

    #[test]
    fn small_packet_bytes() {
        assert_eq!(
            roundtrip(&BlockEntityTagQuery {
                transaction_id: 3,
                pos: BlockPos::new(0, 5, 0)
            }),
            [3, 0, 0, 0, 0, 0, 0, 0, 5]
        );
        assert_eq!(
            roundtrip(&EntityTagQuery {
                transaction_id: 3,
                entity_id: 300
            }),
            [3, 0xac, 0x02]
        );
        assert_eq!(
            roundtrip(&ChangeGameMode {
                mode: GameType::Spectator
            }),
            [3]
        );
        assert_eq!(roundtrip(&LockDifficulty { locked: true }), [1]);
        assert_eq!(
            roundtrip(&ChunkBatchReceived {
                desired_chunks_per_tick: 7.5
            }),
            [0x40, 0xf0, 0, 0]
        );
        assert_eq!(
            roundtrip(&ClientCommand {
                action: ClientCommandAction::RequestGameruleValues
            }),
            [2]
        );
        assert_eq!(
            roundtrip(&PingRequest { time: -9 }),
            [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xf7]
        );
        let mut uuid = [0; 16];
        uuid[15] = 5;
        assert_eq!(
            roundtrip(&TeleportToEntity {
                target: Uuid::from_u128(5)
            }),
            uuid
        );
    }

    #[test]
    fn debug_subscriptions_bytes_and_limit() {
        let packet = DebugSubscriptionRequest {
            subscriptions: LengthPrefixed(vec![VarInt(1), VarInt(200)]),
        };
        assert_eq!(roundtrip(&packet), [2, 1, 0xc8, 0x01]);
        let mut too_many = vec![33];
        too_many.resize(34, 0);
        assert!(from_bytes_exact::<DebugSubscriptionRequest>(&too_many).is_err());
    }

    #[test]
    fn game_rule_bytes() {
        let packet = SetGameRule {
            entries: vec![GameRuleEntry {
                rule: Identifier::parse("a:b").unwrap(),
                value: "1".into(),
            }],
        };
        assert_eq!(roundtrip(&packet), [1, 3, b'a', b':', b'b', 1, b'1']);
    }

    #[test]
    fn difficulty_byte() {
        assert_eq!(
            to_bytes(&ChangeDifficulty {
                difficulty: Difficulty::Hard
            }),
            [3]
        );
    }
}
