//! Configuration-only packets; the ones shared with play live in [`crate::common`].

use mcr_buf::{Decode, Encode, Identifier, LengthPrefixed};

use crate::packet;
use crate::types::registry::{KnownPack, RegistryEntry};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CodeOfConduct {
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
pub struct ClientboundFinishConfiguration;

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct RegistryData {
    pub registry: Identifier,
    pub entries: Vec<RegistryEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
pub struct ResetChat;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ClientboundSelectKnownPacks {
    pub packs: Vec<KnownPack>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct UpdateEnabledFeatures {
    pub features: Vec<Identifier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
pub struct AcceptCodeOfConduct;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
pub struct ServerboundFinishConfiguration;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ServerboundSelectKnownPacks {
    pub packs: LengthPrefixed<KnownPack, 64>,
}

packet!(CodeOfConduct, "code_of_conduct", Clientbound);
packet!(
    ClientboundFinishConfiguration,
    "finish_configuration",
    Clientbound
);
packet!(RegistryData, "registry_data", Clientbound);
packet!(ResetChat, "reset_chat", Clientbound);
packet!(
    ClientboundSelectKnownPacks,
    "select_known_packs",
    Clientbound
);
packet!(
    UpdateEnabledFeatures,
    "update_enabled_features",
    Clientbound
);
packet!(AcceptCodeOfConduct, "accept_code_of_conduct", Serverbound);
packet!(
    ServerboundFinishConfiguration,
    "finish_configuration",
    Serverbound
);
packet!(
    ServerboundSelectKnownPacks,
    "select_known_packs",
    Serverbound
);

#[cfg(test)]
mod tests {
    use mcr_buf::{from_bytes_exact, to_bytes};
    use mcr_nbt::{NetworkNbt, Value};

    use super::*;
    use crate::test_util::roundtrip;

    #[test]
    fn unit_packets_are_empty() {
        assert!(roundtrip(&ClientboundFinishConfiguration).is_empty());
        assert!(roundtrip(&ServerboundFinishConfiguration).is_empty());
        assert!(roundtrip(&ResetChat).is_empty());
        assert!(roundtrip(&AcceptCodeOfConduct).is_empty());
    }

    #[test]
    fn registry_data_bytes() {
        let packet = RegistryData {
            registry: Identifier::parse("r").unwrap(),
            entries: vec![
                RegistryEntry {
                    id: Identifier::parse("x").unwrap(),
                    data: None,
                },
                RegistryEntry {
                    id: Identifier::parse("y").unwrap(),
                    data: Some(NetworkNbt(Value::Int(1))),
                },
            ],
        };
        let bytes = roundtrip(&packet);
        assert_eq!(&bytes[..11], b"\x0bminecraft:");
        assert_eq!(bytes[bytes.len() - 6..], [1, 3, 0, 0, 0, 1]);
    }

    #[test]
    fn known_packs() {
        let packs = vec![KnownPack::vanilla("core", "26.3")];
        let bytes = roundtrip(&ClientboundSelectKnownPacks {
            packs: packs.clone(),
        });
        assert_eq!(&bytes[..2], [1, 9]);
        roundtrip(&ServerboundSelectKnownPacks {
            packs: LengthPrefixed(packs),
        });
        let too_many = to_bytes(&ClientboundSelectKnownPacks {
            packs: vec![KnownPack::vanilla("a", "b"); 65],
        });
        assert!(from_bytes_exact::<ServerboundSelectKnownPacks>(&too_many).is_err());
    }

    #[test]
    fn features_and_conduct() {
        roundtrip(&UpdateEnabledFeatures {
            features: vec![Identifier::parse("vanilla").unwrap()],
        });
        assert_eq!(roundtrip(&CodeOfConduct { text: "x".into() }), [1, b'x']);
    }
}
