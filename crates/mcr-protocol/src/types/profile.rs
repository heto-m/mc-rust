use mcr_buf::{BoundedString, Decode, Encode, LengthPrefixed, Result, decode_str, encode_str};
use uuid::Uuid;

pub const MAX_NAME_CHARS: usize = 16;
pub const MAX_PROPERTIES: usize = 16;
const MAX_PROPERTY_NAME_CHARS: usize = 64;
const MAX_PROPERTY_VALUE_CHARS: usize = 32767;
const MAX_SIGNATURE_CHARS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileProperty {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

impl Encode for ProfileProperty {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_str(&self.name, out);
        encode_str(&self.value, out);
        self.signature.encode(out);
    }
}

impl Decode<'_> for ProfileProperty {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let name = decode_str(input, MAX_PROPERTY_NAME_CHARS)?.to_owned();
        let value = decode_str(input, MAX_PROPERTY_VALUE_CHARS)?.to_owned();
        let signature = Option::<BoundedString<MAX_SIGNATURE_CHARS>>::decode(input)?.map(|s| s.0);
        Ok(Self {
            name,
            value,
            signature,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameProfile {
    pub id: Uuid,
    pub name: String,
    pub properties: Vec<ProfileProperty>,
}

impl Encode for GameProfile {
    fn encode(&self, out: &mut Vec<u8>) {
        self.id.encode(out);
        encode_str(&self.name, out);
        self.properties.encode(out);
    }
}

impl Decode<'_> for GameProfile {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let id = Uuid::decode(input)?;
        let name = decode_str(input, MAX_NAME_CHARS)?.to_owned();
        let properties = LengthPrefixed::<ProfileProperty, MAX_PROPERTIES>::decode(input)?.0;
        Ok(Self {
            id,
            name,
            properties,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::roundtrip;

    #[test]
    fn profile_roundtrip_and_bytes() {
        let profile = GameProfile {
            id: Uuid::from_u128(1),
            name: "Ab".into(),
            properties: vec![
                ProfileProperty {
                    name: "textures".into(),
                    value: "v".into(),
                    signature: Some("s".into()),
                },
                ProfileProperty {
                    name: "x".into(),
                    value: "y".into(),
                    signature: None,
                },
            ],
        };
        let bytes = roundtrip(&profile);
        let mut expected = Uuid::from_u128(1).as_bytes().to_vec();
        expected.extend([2, b'A', b'b', 2, 8]);
        expected.extend(*b"textures");
        expected.extend([1, b'v', 1, 1, b's', 1, b'x', 1, b'y', 0]);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn rejects_long_name() {
        let profile = GameProfile {
            id: Uuid::nil(),
            name: "a".repeat(17),
            properties: vec![],
        };
        let bytes = mcr_buf::to_bytes(&profile);
        assert!(mcr_buf::from_bytes_exact::<GameProfile>(&bytes).is_err());
    }
}
