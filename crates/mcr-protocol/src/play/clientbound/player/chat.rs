use mcr_buf::{
    BitSet, Decode, DecodeError, Encode, IdOr, LengthPrefixed, Result, read_varint, write_varint,
};
use mcr_nbt::NetworkNbt;
use uuid::Uuid;

use crate::{
    packet,
    types::{Component, game::MessageSignature},
};

/// A cache index into the last 128 signatures, or the full signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackedSignature {
    Cached(i32),
    Full(Box<MessageSignature>),
}

impl Encode for PackedSignature {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Cached(id) => write_varint(out, id + 1),
            Self::Full(signature) => {
                write_varint(out, 0);
                signature.encode(out);
            }
        }
    }
}

impl Decode<'_> for PackedSignature {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        match read_varint(input)? {
            0 => MessageSignature::decode(input).map(|s| Self::Full(Box::new(s))),
            n if n > 0 => Ok(Self::Cached(n - 1)),
            n => Err(DecodeError::InvalidValue {
                what: "packed signature",
                value: n.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SignedMessageBody {
    #[mcr(max_len = 256)]
    pub content: String,
    pub timestamp_millis: i64,
    pub salt: i64,
    pub last_seen: LengthPrefixed<PackedSignature, 20>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum FilterMask {
    PassThrough,
    FullyFiltered,
    PartiallyFiltered(BitSet),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ChatParameter {
    Sender,
    Target,
    Content,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ChatTypeDecoration {
    pub translation_key: String,
    pub parameters: Vec<ChatParameter>,
    pub style: NetworkNbt,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ChatType {
    pub chat: ChatTypeDecoration,
    pub narration: ChatTypeDecoration,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct BoundChatType {
    pub chat_type: IdOr<ChatType>,
    pub name: Component,
    pub target_name: Option<Component>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PlayerChat {
    #[mcr(varint)]
    pub global_index: i32,
    pub sender: Uuid,
    #[mcr(varint)]
    pub index: i32,
    pub signature: Option<MessageSignature>,
    pub body: SignedMessageBody,
    pub unsigned_content: Option<Component>,
    pub filter_mask: FilterMask,
    pub chat_type: BoundChatType,
}

packet!(PlayerChat, "player_chat", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DisguisedChat {
    pub message: Component,
    pub chat_type: BoundChatType,
}

packet!(DisguisedChat, "disguised_chat", Clientbound);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct DeleteChat {
    pub message_signature: PackedSignature,
}

packet!(DeleteChat, "delete_chat", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SystemChat {
    pub content: Component,
    pub overlay: bool,
}

packet!(SystemChat, "system_chat", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum CompletionAction {
    Add,
    Remove,
    Set,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CustomChatCompletions {
    pub action: CompletionAction,
    pub entries: Vec<String>,
}

packet!(
    CustomChatCompletions,
    "custom_chat_completions",
    Clientbound
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::roundtrip;

    fn full() -> MessageSignature {
        MessageSignature([7; 256])
    }

    #[test]
    fn packed_signature_forms() {
        assert_eq!(roundtrip(&PackedSignature::Cached(4)), [5]);
        let bytes = roundtrip(&PackedSignature::Full(Box::new(full())));
        assert_eq!(bytes.len(), 257);
        assert_eq!(bytes[0], 0);
        assert!(PackedSignature::decode(&mut &[0xff, 0xff, 0xff, 0xff, 0x0f][..]).is_err());
    }

    #[test]
    fn delete_chat_bytes() {
        assert_eq!(
            roundtrip(&DeleteChat {
                message_signature: PackedSignature::Cached(0),
            }),
            [1]
        );
        let mut expected = vec![0];
        expected.extend_from_slice(&[7; 256]);
        assert_eq!(
            roundtrip(&DeleteChat {
                message_signature: PackedSignature::Full(Box::new(full())),
            }),
            expected
        );
    }

    #[test]
    fn player_chat_bytes() {
        let mut packet = PlayerChat {
            global_index: 1,
            sender: Uuid::from_u128(9),
            index: 2,
            signature: None,
            body: SignedMessageBody {
                content: "hi".into(),
                timestamp_millis: 1,
                salt: 2,
                last_seen: LengthPrefixed(vec![]),
            },
            unsigned_content: None,
            filter_mask: FilterMask::PassThrough,
            chat_type: BoundChatType {
                chat_type: IdOr::Id(0),
                name: Component::text("A"),
                target_name: Some(Component::text("B")),
            },
        };
        let mut expected = vec![1];
        expected.extend_from_slice(&[0; 15]);
        expected.push(9);
        #[rustfmt::skip]
        expected.extend_from_slice(&[
            2,
            0,
            2, b'h', b'i', 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0,
            0,
            0,
            1, 8, 0, 1, b'A', 1, 8, 0, 1, b'B',
        ]);
        assert_eq!(roundtrip(&packet), expected);

        packet.signature = Some(full());
        packet.unsigned_content = Some(Component::text("u"));
        let bytes = roundtrip(&packet);
        assert_eq!(&bytes[18..20], [1, 7]);
        assert_eq!(&bytes[275..295], &expected[19..39]);
        assert_eq!(
            &bytes[295..],
            [&[1, 8, 0, 1, b'u'][..], &expected[40..]].concat()
        );
    }

    #[test]
    fn body_bytes() {
        let body = SignedMessageBody {
            content: "hi".into(),
            timestamp_millis: 1,
            salt: 2,
            last_seen: LengthPrefixed(vec![PackedSignature::Cached(1)]),
        };
        #[rustfmt::skip]
        assert_eq!(roundtrip(&body), [
            2, b'h', b'i',
            0, 0, 0, 0, 0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 2,
            1, 2,
        ]);
        let too_many = SignedMessageBody {
            last_seen: LengthPrefixed(vec![PackedSignature::Cached(0); 21]),
            ..body
        };
        assert!(
            mcr_buf::from_bytes_exact::<SignedMessageBody>(&mcr_buf::to_bytes(&too_many)).is_err()
        );
    }

    #[test]
    fn filter_mask_forms() {
        assert_eq!(roundtrip(&FilterMask::PassThrough), [0]);
        assert_eq!(roundtrip(&FilterMask::FullyFiltered), [1]);
        let mut bits = BitSet::new();
        bits.set(0, true);
        assert_eq!(roundtrip(&FilterMask::PartiallyFiltered(bits)), [2, 1, 1]);
    }

    #[test]
    fn completions() {
        assert_eq!(
            roundtrip(&CustomChatCompletions {
                action: CompletionAction::Set,
                entries: vec!["a".into()],
            }),
            [2, 1, 1, b'a']
        );
    }

    #[test]
    fn signed_chat_roundtrip() {
        let chat_type = BoundChatType {
            chat_type: IdOr::Id(0),
            name: Component::default(),
            target_name: None,
        };
        roundtrip(&PlayerChat {
            global_index: 1,
            sender: Uuid::from_u128(9),
            index: 2,
            signature: Some(full()),
            body: SignedMessageBody {
                content: "hello".into(),
                timestamp_millis: 5,
                salt: 6,
                last_seen: LengthPrefixed(vec![
                    PackedSignature::Cached(3),
                    PackedSignature::Full(Box::new(full())),
                ]),
            },
            unsigned_content: Some(Component::default()),
            filter_mask: FilterMask::FullyFiltered,
            chat_type: chat_type.clone(),
        });
        roundtrip(&DisguisedChat {
            message: Component::default(),
            chat_type,
        });
        roundtrip(&SystemChat {
            content: Component::default(),
            overlay: true,
        });
    }
}
