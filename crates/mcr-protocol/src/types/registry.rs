use mcr_buf::{Decode, Encode, Identifier, Result, read_len, read_varint, write_varint};
use mcr_nbt::NetworkNbt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct KnownPack {
    pub namespace: String,
    pub id: String,
    pub version: String,
}

impl KnownPack {
    pub const VANILLA_NAMESPACE: &'static str = "minecraft";

    pub fn vanilla(id: &str, version: &str) -> Self {
        Self {
            namespace: Self::VANILLA_NAMESPACE.into(),
            id: id.into(),
            version: version.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct RegistryEntry {
    pub id: Identifier,
    pub data: Option<NetworkNbt>,
}

/// VarInt count followed by VarInt elements.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IdList(pub Vec<i32>);

impl Encode for IdList {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.0.len() as i32);
        for &id in &self.0 {
            write_varint(out, id);
        }
    }
}

impl Decode<'_> for IdList {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let len = read_len(input, input.len())?;
        let mut ids = Vec::with_capacity(len);
        for _ in 0..len {
            ids.push(read_varint(input)?);
        }
        Ok(Self(ids))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TagEntries {
    pub name: Identifier,
    pub ids: IdList,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RegistryTags {
    pub registry: Identifier,
    pub tags: Vec<TagEntries>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::roundtrip;

    #[test]
    fn registry_entry_bytes() {
        let none = RegistryEntry {
            id: Identifier::parse("a").unwrap(),
            data: None,
        };
        let mut expected = vec![11];
        expected.extend(*b"minecraft:a");
        expected.push(0);
        assert_eq!(roundtrip(&none), expected);

        let some = RegistryEntry {
            data: Some(NetworkNbt(mcr_nbt::Value::Byte(3))),
            ..none
        };
        let mut expected = vec![11];
        expected.extend(*b"minecraft:a");
        expected.extend([1, 1, 3]);
        assert_eq!(roundtrip(&some), expected);
    }

    #[test]
    fn tags_roundtrip() {
        roundtrip(&RegistryTags {
            registry: Identifier::parse("block").unwrap(),
            tags: vec![TagEntries {
                name: Identifier::parse("logs").unwrap(),
                ids: IdList(vec![1, 300, 5]),
            }],
        });
    }

    #[test]
    fn known_pack_roundtrip() {
        let mut expected = vec![9];
        expected.extend(*b"minecraft");
        expected.extend([4, b'c', b'o', b'r', b'e', 4, b'2', b'6', b'.', b'3']);
        assert_eq!(roundtrip(&KnownPack::vanilla("core", "26.3")), expected);
    }
}
