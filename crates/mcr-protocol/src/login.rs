use mcr_buf::{ByteArray, Decode, Encode, Identifier, RestBytes};
use uuid::Uuid;

use crate::packet;
use crate::types::profile::GameProfile;

/// `reason` is the raw JSON text component; the wire format is JSON, not NBT.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LoginDisconnect {
    #[mcr(max_len = 262144)]
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ClientboundHello<'a> {
    #[mcr(max_len = 20)]
    pub server_id: String,
    pub public_key: ByteArray<'a>,
    pub challenge: ByteArray<'a>,
    pub should_authenticate: bool,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct LoginFinished {
    pub profile: GameProfile,
    pub session_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct LoginCompression {
    #[mcr(varint)]
    pub threshold: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CustomQuery<'a> {
    #[mcr(varint)]
    pub transaction_id: i32,
    pub channel: Identifier,
    pub data: RestBytes<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ServerboundHello {
    #[mcr(max_len = 16)]
    pub name: String,
    pub profile_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Key<'a> {
    pub shared_secret: ByteArray<'a>,
    pub encrypted_challenge: ByteArray<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CustomQueryAnswer<'a> {
    #[mcr(varint)]
    pub transaction_id: i32,
    pub data: Option<RestBytes<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
pub struct LoginAcknowledged;

packet!(LoginDisconnect, "login_disconnect", Clientbound);
packet!(ClientboundHello<'a>, "hello", Clientbound);
packet!(LoginFinished, "login_finished", Clientbound);
packet!(LoginCompression, "login_compression", Clientbound);
packet!(CustomQuery<'a>, "custom_query", Clientbound);
packet!(ServerboundHello, "hello", Serverbound);
packet!(Key<'a>, "key", Serverbound);
packet!(CustomQueryAnswer<'a>, "custom_query_answer", Serverbound);
packet!(LoginAcknowledged, "login_acknowledged", Serverbound);

#[cfg(test)]
mod tests {
    use mcr_buf::{from_bytes_exact, to_bytes};

    use super::*;
    use crate::common::roundtrip_borrowed as roundtrip;

    #[test]
    fn hello_bytes() {
        let bytes = roundtrip!(&ClientboundHello {
            server_id: String::new(),
            public_key: ByteArray(&[1, 2]),
            challenge: ByteArray(&[3]),
            should_authenticate: true,
        });
        assert_eq!(bytes, [0, 2, 1, 2, 1, 3, 1]);
    }

    #[test]
    fn serverbound_hello_bytes() {
        let packet = ServerboundHello {
            name: "Bob".into(),
            profile_id: Uuid::from_u128(5),
        };
        let mut expected = vec![3, b'B', b'o', b'b'];
        expected.extend(Uuid::from_u128(5).as_bytes());
        assert_eq!(roundtrip!(&packet), expected);
        let long = to_bytes(&ServerboundHello {
            name: "x".repeat(17),
            profile_id: Uuid::nil(),
        });
        assert!(from_bytes_exact::<ServerboundHello>(&long).is_err());
    }

    #[test]
    fn simple_packets() {
        assert_eq!(roundtrip!(&LoginCompression { threshold: 256 }), [0x80, 2]);
        assert_eq!(roundtrip!(&LoginAcknowledged), Vec::<u8>::new());
        roundtrip!(&LoginDisconnect {
            reason: "\"bye\"".into(),
        });
        roundtrip!(&Key {
            shared_secret: ByteArray(&[9; 4]),
            encrypted_challenge: ByteArray(&[]),
        });
    }

    #[test]
    fn query_packets() {
        let query = CustomQuery {
            transaction_id: 3,
            channel: Identifier::parse("a:b").unwrap(),
            data: RestBytes(&[7, 7]),
        };
        assert_eq!(roundtrip!(&query), [3, 3, b'a', b':', b'b', 7, 7]);
        assert_eq!(
            roundtrip!(&CustomQueryAnswer {
                transaction_id: 1,
                data: None
            }),
            [1, 0]
        );
        assert_eq!(
            roundtrip!(&CustomQueryAnswer {
                transaction_id: 1,
                data: Some(RestBytes(&[9]))
            }),
            [1, 1, 9]
        );
    }

    #[test]
    fn login_finished() {
        roundtrip!(&LoginFinished {
            profile: GameProfile {
                id: Uuid::from_u128(9),
                name: "N".into(),
                properties: vec![],
            },
            session_id: Uuid::from_u128(10),
        });
    }
}
