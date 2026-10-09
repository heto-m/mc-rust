use mcr_buf::{
    Decode, Encode, FixedBitSet, LengthPrefixed, Result, read_len, read_varint, write_varint,
};
use uuid::Uuid;

use super::GameType;
use crate::{packet, types::Component};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProfileProperty {
    #[mcr(max_len = 64)]
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Encode, Decode)]
pub struct PlayerProfile {
    #[mcr(max_len = 16)]
    pub name: String,
    pub properties: LengthPrefixed<ProfileProperty, 16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChatSession {
    pub session_id: Uuid,
    pub expires_at_millis: i64,
    pub public_key: LengthPrefixed<u8, 512>,
    pub key_signature: LengthPrefixed<u8, 4096>,
}

/// Bit positions in [`PlayerInfoUpdate::actions`]; entry fields follow in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PlayerInfoAction {
    AddPlayer,
    InitializeChat,
    UpdateGameMode,
    UpdateListed,
    UpdateLatency,
    UpdateDisplayName,
    UpdateListOrder,
    UpdateHat,
}

impl PlayerInfoAction {
    pub const ALL: [Self; 8] = [
        Self::AddPlayer,
        Self::InitializeChat,
        Self::UpdateGameMode,
        Self::UpdateListed,
        Self::UpdateLatency,
        Self::UpdateDisplayName,
        Self::UpdateListOrder,
        Self::UpdateHat,
    ];

    pub fn bit(self) -> usize {
        self as usize
    }
}

/// Fields not selected by the packet's action set are ignored on encode and left at their defaults on decode.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInfoEntry {
    pub uuid: Uuid,
    pub profile: PlayerProfile,
    pub chat_session: Option<RemoteChatSession>,
    pub game_mode: GameType,
    pub listed: bool,
    pub latency: i32,
    pub display_name: Option<Component>,
    pub list_order: i32,
    pub show_hat: bool,
}

impl PlayerInfoEntry {
    pub fn new(uuid: Uuid) -> Self {
        Self {
            uuid,
            profile: PlayerProfile::default(),
            chat_session: None,
            game_mode: GameType::Survival,
            listed: false,
            latency: 0,
            display_name: None,
            list_order: 0,
            show_hat: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInfoUpdate {
    pub actions: FixedBitSet<8>,
    pub entries: Vec<PlayerInfoEntry>,
}

packet!(PlayerInfoUpdate, "player_info_update", Clientbound);

impl PlayerInfoUpdate {
    pub fn with_actions(actions: &[PlayerInfoAction], entries: Vec<PlayerInfoEntry>) -> Self {
        let mut set = FixedBitSet::new();
        for action in actions {
            set.set(action.bit(), true);
        }
        Self {
            actions: set,
            entries,
        }
    }
}

impl Encode for PlayerInfoUpdate {
    fn encode(&self, out: &mut Vec<u8>) {
        self.actions.encode(out);
        write_varint(out, self.entries.len() as i32);
        for entry in &self.entries {
            entry.uuid.encode(out);
            for action in PlayerInfoAction::ALL {
                if !self.actions.get(action.bit()) {
                    continue;
                }
                match action {
                    PlayerInfoAction::AddPlayer => entry.profile.encode(out),
                    PlayerInfoAction::InitializeChat => entry.chat_session.encode(out),
                    PlayerInfoAction::UpdateGameMode => entry.game_mode.encode(out),
                    PlayerInfoAction::UpdateListed => entry.listed.encode(out),
                    PlayerInfoAction::UpdateLatency => write_varint(out, entry.latency),
                    PlayerInfoAction::UpdateDisplayName => entry.display_name.encode(out),
                    PlayerInfoAction::UpdateListOrder => write_varint(out, entry.list_order),
                    PlayerInfoAction::UpdateHat => entry.show_hat.encode(out),
                }
            }
        }
    }
}

impl Decode<'_> for PlayerInfoUpdate {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let actions = FixedBitSet::<8>::decode(input)?;
        let count = read_len(input, input.len())?;
        let mut entries = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let mut entry = PlayerInfoEntry::new(Uuid::decode(input)?);
            for action in PlayerInfoAction::ALL {
                if !actions.get(action.bit()) {
                    continue;
                }
                match action {
                    PlayerInfoAction::AddPlayer => entry.profile = PlayerProfile::decode(input)?,
                    PlayerInfoAction::InitializeChat => {
                        entry.chat_session = Option::decode(input)?;
                    }
                    PlayerInfoAction::UpdateGameMode => entry.game_mode = GameType::decode(input)?,
                    PlayerInfoAction::UpdateListed => entry.listed = bool::decode(input)?,
                    PlayerInfoAction::UpdateLatency => entry.latency = read_varint(input)?,
                    PlayerInfoAction::UpdateDisplayName => {
                        entry.display_name = Option::decode(input)?;
                    }
                    PlayerInfoAction::UpdateListOrder => entry.list_order = read_varint(input)?,
                    PlayerInfoAction::UpdateHat => entry.show_hat = bool::decode(input)?,
                }
            }
            entries.push(entry);
        }
        Ok(Self { actions, entries })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PlayerInfoRemove {
    pub profile_ids: Vec<Uuid>,
}

packet!(PlayerInfoRemove, "player_info_remove", Clientbound);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::roundtrip;

    fn uuid_bytes() -> Vec<u8> {
        let mut bytes = vec![0; 15];
        bytes.push(1);
        bytes
    }

    #[test]
    fn add_player_bytes() {
        let mut entry = PlayerInfoEntry::new(Uuid::from_u128(1));
        entry.profile = PlayerProfile {
            name: "Ab".into(),
            properties: LengthPrefixed(vec![ProfileProperty {
                name: "textures".into(),
                value: "v".into(),
                signature: Some("s".into()),
            }]),
        };
        entry.game_mode = GameType::Creative;
        entry.listed = true;
        entry.latency = 300;
        entry.list_order = 2;
        entry.show_hat = true;
        let packet = PlayerInfoUpdate::with_actions(
            &[
                PlayerInfoAction::AddPlayer,
                PlayerInfoAction::UpdateGameMode,
                PlayerInfoAction::UpdateListed,
                PlayerInfoAction::UpdateLatency,
                PlayerInfoAction::UpdateListOrder,
                PlayerInfoAction::UpdateHat,
            ],
            vec![entry],
        );
        let mut expected = vec![0b1101_1101, 1];
        expected.extend(uuid_bytes());
        expected.extend_from_slice(&[2, b'A', b'b', 1, 8]);
        expected.extend_from_slice(b"textures");
        expected.extend_from_slice(&[1, b'v', 1, 1, b's']);
        expected.extend_from_slice(&[1, 1, 0xac, 2, 2, 1]);
        assert_eq!(roundtrip(&packet), expected);
    }

    #[test]
    fn null_valued_actions() {
        let packet = PlayerInfoUpdate::with_actions(
            &[
                PlayerInfoAction::InitializeChat,
                PlayerInfoAction::UpdateDisplayName,
            ],
            vec![PlayerInfoEntry::new(Uuid::from_u128(1))],
        );
        let mut expected = vec![0b0010_0010, 1];
        expected.extend(uuid_bytes());
        expected.extend_from_slice(&[0, 0]);
        assert_eq!(roundtrip(&packet), expected);
    }

    #[test]
    fn chat_session_bytes() {
        let mut entry = PlayerInfoEntry::new(Uuid::from_u128(1));
        entry.chat_session = Some(RemoteChatSession {
            session_id: Uuid::from_u128(2),
            expires_at_millis: 3,
            public_key: LengthPrefixed(vec![9]),
            key_signature: LengthPrefixed(vec![]),
        });
        let packet =
            PlayerInfoUpdate::with_actions(&[PlayerInfoAction::InitializeChat], vec![entry]);
        let bytes = roundtrip(&packet);
        assert_eq!(bytes.len(), 1 + 1 + 16 + 1 + 16 + 8 + 2 + 1);
    }

    #[test]
    fn empty_and_remove() {
        let packet = PlayerInfoUpdate::with_actions(&[PlayerInfoAction::UpdateHat], vec![]);
        assert_eq!(roundtrip(&packet), [0x80, 0]);
        let mut expected = vec![1];
        expected.extend(uuid_bytes());
        assert_eq!(
            roundtrip(&PlayerInfoRemove {
                profile_ids: vec![Uuid::from_u128(1)],
            }),
            expected
        );
    }

    #[test]
    #[ignore = "needs text components"]
    fn display_name_roundtrip() {
        let mut entry = PlayerInfoEntry::new(Uuid::from_u128(1));
        entry.display_name = Some(Component::default());
        roundtrip(&PlayerInfoUpdate::with_actions(
            &[PlayerInfoAction::UpdateDisplayName],
            vec![entry],
        ));
    }
}
