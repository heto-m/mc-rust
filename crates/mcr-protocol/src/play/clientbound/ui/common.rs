use mcr_buf::{Decode, Encode, Result, prealloc_capacity, read_len, read_varint, write_varint};

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
    fn optional_varint_codec() {
        assert_eq!(to_bytes(&VarInt(0)), [0]);
        let mut out = Vec::new();
        optional_varint::encode(&Some(0), &mut out);
        optional_varint::encode(&None, &mut out);
        assert_eq!(out, [1, 0]);
        assert_eq!(optional_varint::decode(&mut &[3u8][..]).unwrap(), Some(2));
    }
}
