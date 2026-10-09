use mcr_buf::{Decode, Encode, FixedBitSet, LengthPrefixed, Result, take};
use uuid::Uuid;

use super::shared::BoundedBytes;
use crate::packet;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MessageSignature(pub [u8; 256]);

impl std::fmt::Debug for MessageSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MessageSignature({:02x?}..)", &self.0[..4])
    }
}

impl Encode for MessageSignature {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0);
    }
}

impl Decode<'_> for MessageSignature {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        Ok(Self(take(input, 256)?.try_into().unwrap()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct LastSeenUpdate {
    #[mcr(varint)]
    pub offset: i32,
    pub acknowledged: FixedBitSet<20>,
    pub checksum: u8,
}

impl LastSeenUpdate {
    pub const IGNORE_CHECKSUM: u8 = 0;

    pub fn new(offset: i32) -> Self {
        Self {
            offset,
            acknowledged: FixedBitSet::new(),
            checksum: Self::IGNORE_CHECKSUM,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct ArgumentSignature {
    #[mcr(max_len = 16)]
    pub name: String,
    pub signature: MessageSignature,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct ChatCommand {
    pub command: String,
}

packet!(ChatCommand, "chat_command", Serverbound);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct ChatCommandSigned {
    pub command: String,
    pub timestamp_ms: i64,
    pub salt: i64,
    pub argument_signatures: LengthPrefixed<ArgumentSignature, 8>,
    pub last_seen: LastSeenUpdate,
}

packet!(ChatCommandSigned, "chat_command_signed", Serverbound);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct Chat {
    #[mcr(max_len = 256)]
    pub message: String,
    pub timestamp_ms: i64,
    pub salt: i64,
    pub signature: Option<MessageSignature>,
    pub last_seen: LastSeenUpdate,
}

packet!(Chat, "chat", Serverbound);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct ChatSessionUpdate {
    pub session_id: Uuid,
    pub expires_at_ms: i64,
    pub public_key: BoundedBytes<512>,
    pub key_signature: BoundedBytes<4096>,
}

packet!(ChatSessionUpdate, "chat_session_update", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct ChatAck {
    #[mcr(varint)]
    pub offset: i32,
}

packet!(ChatAck, "chat_ack", Serverbound);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct CommandSuggestion {
    #[mcr(varint)]
    pub id: i32,
    #[mcr(max_len = 32500)]
    pub command: String,
}

packet!(CommandSuggestion, "command_suggestion", Serverbound);

#[cfg(test)]
mod tests {
    use mcr_buf::{DecodeError, to_bytes};

    use super::*;
    use crate::play::serverbound::shared::roundtrips;
    use crate::test_util::roundtrip;

    fn sig(fill: u8) -> MessageSignature {
        MessageSignature([fill; 256])
    }

    fn update() -> LastSeenUpdate {
        let mut acknowledged = FixedBitSet::new();
        acknowledged.set(0, true);
        acknowledged.set(19, true);
        LastSeenUpdate {
            offset: 3,
            acknowledged,
            checksum: 7,
        }
    }

    roundtrips! {
        command: ChatCommand { command: "say hi".into() },
        signed: ChatCommandSigned {
            command: "msg a b".into(),
            timestamp_ms: 1_700_000_000_000,
            salt: -5,
            argument_signatures: LengthPrefixed(vec![ArgumentSignature { name: "message".into(), signature: sig(2) }]),
            last_seen: update(),
        },
        session: ChatSessionUpdate {
            session_id: Uuid::from_u128(9),
            expires_at_ms: 42,
            public_key: BoundedBytes(vec![1; 294]),
            key_signature: BoundedBytes(vec![2; 512]),
        },
        ack: ChatAck { offset: 5 },
        suggestion: CommandSuggestion { id: 1, command: "/gam".into() },
    }

    #[test]
    fn unsigned_chat_bytes() {
        let packet = Chat {
            message: "hi".into(),
            timestamp_ms: 1,
            salt: 2,
            signature: None,
            last_seen: LastSeenUpdate::new(0),
        };
        let bytes = roundtrip(&packet);
        let mut expected = vec![2, b'h', b'i'];
        expected.extend_from_slice(&1i64.to_be_bytes());
        expected.extend_from_slice(&2i64.to_be_bytes());
        expected.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn signed_chat_layout() {
        let packet = Chat {
            message: "x".into(),
            timestamp_ms: 0,
            salt: 0,
            signature: Some(sig(0xab)),
            last_seen: update(),
        };
        let bytes = roundtrip(&packet);
        assert_eq!(bytes.len(), 2 + 16 + 1 + 256 + 1 + 3 + 1);
        assert_eq!(bytes[18], 1);
        assert_eq!(&bytes[bytes.len() - 5..], [3, 0x01, 0, 0x08, 7]);
    }

    #[test]
    fn long_message_rejected() {
        let bytes = to_bytes(&Chat {
            message: "a".repeat(257),
            timestamp_ms: 0,
            salt: 0,
            signature: None,
            last_seen: LastSeenUpdate::new(0),
        });
        assert!(matches!(
            Chat::decode(&mut bytes.as_slice()),
            Err(DecodeError::TooLong { max: 256, .. })
        ));
    }

    #[test]
    fn too_many_argument_signatures() {
        let packet = ChatCommandSigned {
            command: String::new(),
            timestamp_ms: 0,
            salt: 0,
            argument_signatures: LengthPrefixed(
                (0..9)
                    .map(|_| ArgumentSignature {
                        name: "a".into(),
                        signature: sig(0),
                    })
                    .collect(),
            ),
            last_seen: LastSeenUpdate::new(0),
        };
        assert!(ChatCommandSigned::decode(&mut to_bytes(&packet).as_slice()).is_err());
    }
}
