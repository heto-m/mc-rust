use mcr_buf::{
    Decode, DecodeError, Encode, Identifier, Result, prealloc_capacity, read_len, read_varint,
    write_varint,
};

/// Vanilla `ByteBufCodecs.map`: VarInt count, then key and value per entry, in order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pairs<K, V>(pub Vec<(K, V)>);

impl<K: Encode, V: Encode> Encode for Pairs<K, V> {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.0.len() as i32);
        for (k, v) in &self.0 {
            k.encode(out);
            v.encode(out);
        }
    }
}

impl<'a, K: Decode<'a>, V: Decode<'a>> Decode<'a> for Pairs<K, V> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        let len = read_len(input, input.len())?;
        let mut pairs = Vec::with_capacity(prealloc_capacity::<(K, V)>(len));
        for _ in 0..len {
            pairs.push((K::decode(input)?, V::decode(input)?));
        }
        Ok(Self(pairs))
    }
}

/// An item holder set: a tag reference or an explicit list of item registry ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HolderSet {
    Tag(Identifier),
    Direct(Vec<i32>),
}

impl Encode for HolderSet {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Tag(tag) => {
                write_varint(out, 0);
                tag.encode(out);
            }
            Self::Direct(ids) => {
                write_varint(out, ids.len() as i32 + 1);
                for &id in ids {
                    write_varint(out, id);
                }
            }
        }
    }
}

impl Decode<'_> for HolderSet {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let count = read_varint(input)?;
        if count == 0 {
            return Identifier::decode(input).map(Self::Tag);
        }
        let len = usize::try_from(count - 1).map_err(|_| DecodeError::NegativeLength(count))?;
        if len > input.len() {
            return Err(DecodeError::UnexpectedEof);
        }
        let mut ids = Vec::with_capacity(prealloc_capacity::<i32>(len));
        for _ in 0..len {
            ids.push(read_varint(input)?);
        }
        Ok(Self::Direct(ids))
    }
}

/// Stand-in for vanilla's `DataComponentPatch`, which needs the data component registry.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct ComponentPatch {}

impl Encode for ComponentPatch {
    fn encode(&self, _out: &mut Vec<u8>) {
        todo!("data component patch")
    }
}

impl Decode<'_> for ComponentPatch {
    fn decode(_input: &mut &[u8]) -> Result<Self> {
        todo!("data component patch")
    }
}

/// Stand-in for vanilla's `DataComponentExactPredicate`.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct ComponentPredicate {}

impl Encode for ComponentPredicate {
    fn encode(&self, _out: &mut Vec<u8>) {
        todo!("data component predicate")
    }
}

impl Decode<'_> for ComponentPredicate {
    fn decode(_input: &mut &[u8]) -> Result<Self> {
        todo!("data component predicate")
    }
}

/// An item stack without the empty case: item id, then count, then components.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ItemStackTemplate {
    #[mcr(varint)]
    pub item: i32,
    #[mcr(varint)]
    pub count: i32,
    pub components: ComponentPatch,
}

/// `OPTIONAL_VAR_INT`: 0 is absent, otherwise the value plus one.
pub mod optional_varint {
    use super::*;

    pub fn encode(value: &Option<i32>, out: &mut Vec<u8>) {
        write_varint(out, value.map_or(0, |v| v + 1));
    }

    pub fn decode(input: &mut &[u8]) -> Result<Option<i32>> {
        Ok(match read_varint(input)? {
            0 => None,
            n => Some(n - 1),
        })
    }
}

#[cfg(test)]
mod tests {
    use mcr_buf::{VarInt, to_bytes};

    use super::*;
    use crate::test_util::roundtrip;

    #[test]
    fn pairs_keep_order() {
        let pairs = Pairs(vec![(VarInt(2), true), (VarInt(1), false)]);
        assert_eq!(roundtrip(&pairs), [2, 2, 1, 1, 0]);
    }

    #[test]
    fn holder_set_forms() {
        let tag = HolderSet::Tag(Identifier::parse("planks").unwrap());
        let mut expected = vec![0, 16];
        expected.extend_from_slice(b"minecraft:planks");
        assert_eq!(roundtrip(&tag), expected);
        assert_eq!(roundtrip(&HolderSet::Direct(vec![5, 300])), [3, 5, 0xac, 2]);
        assert_eq!(roundtrip(&HolderSet::Direct(vec![])), [1]);
    }

    #[test]
    fn holder_set_rejects_oversized_count() {
        assert!(HolderSet::decode(&mut &[0xff, 0xff, 0xff, 0x7f][..]).is_err());
    }

    #[test]
    fn optional_varint_codec() {
        assert_eq!(to_bytes(&VarInt(0)), [0]);
        let mut out = Vec::new();
        optional_varint::encode(&Some(0), &mut out);
        optional_varint::encode(&None, &mut out);
        assert_eq!(out, [1, 0]);
        assert_eq!(optional_varint::decode(&mut &[3u8][..]).unwrap(), Some(2));
    }
}
