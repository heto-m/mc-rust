use uuid::Uuid;

use crate::{
    BoundedString, ByteArray, Decode, DecodeError, Encode, MAX_STRING_CHARS, RestBytes, Result,
    decode_str, encode_str, prealloc_capacity, read_len, take, write_varint,
};

macro_rules! impl_be {
    ($($t:ty),*) => {$(
        impl Encode for $t {
            fn encode(&self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.to_be_bytes());
            }
        }

        impl Decode<'_> for $t {
            fn decode(input: &mut &[u8]) -> Result<Self> {
                let bytes = take(input, size_of::<$t>())?;
                Ok(<$t>::from_be_bytes(bytes.try_into().unwrap()))
            }
        }
    )*};
}

impl_be!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, f32, f64);

impl Encode for bool {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(u8::from(*self));
    }
}

impl Decode<'_> for bool {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        match u8::decode(input)? {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(DecodeError::InvalidValue {
                what: "bool",
                value: v.into(),
            }),
        }
    }
}

impl Encode for Uuid {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes());
    }
}

impl Decode<'_> for Uuid {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        Ok(Uuid::from_bytes(take(input, 16)?.try_into().unwrap()))
    }
}

impl Encode for str {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_str(self, out);
    }
}

impl Encode for String {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_str(self, out);
    }
}

impl<'a> Decode<'a> for &'a str {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        decode_str(input, MAX_STRING_CHARS)
    }
}

impl Decode<'_> for String {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        decode_str(input, MAX_STRING_CHARS).map(str::to_owned)
    }
}

impl<const MAX: usize> Encode for BoundedString<MAX> {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_str(&self.0, out);
    }
}

impl<const MAX: usize> Decode<'_> for BoundedString<MAX> {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        decode_str(input, MAX).map(|s| Self(s.to_owned()))
    }
}

impl Encode for ByteArray<'_> {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.0.len() as i32);
        out.extend_from_slice(self.0);
    }
}

impl<'a> Decode<'a> for ByteArray<'a> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        let len = read_len(input, input.len())?;
        take(input, len).map(Self)
    }
}

impl Encode for RestBytes<'_> {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.0);
    }
}

impl<'a> Decode<'a> for RestBytes<'a> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        Ok(Self(std::mem::take(input)))
    }
}

impl<T: Encode> Encode for Option<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Some(v) => {
                true.encode(out);
                v.encode(out);
            }
            None => false.encode(out),
        }
    }
}

impl<'a, T: Decode<'a>> Decode<'a> for Option<T> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        if bool::decode(input)? {
            T::decode(input).map(Some)
        } else {
            Ok(None)
        }
    }
}

impl<T: Encode> Encode for [T] {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.len() as i32);
        for v in self {
            v.encode(out);
        }
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        self.as_slice().encode(out);
    }
}

impl<'a, T: Decode<'a>> Decode<'a> for Vec<T> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        let len = read_len(input, input.len())?;
        let mut v = Vec::with_capacity(prealloc_capacity::<T>(len));
        for _ in 0..len {
            v.push(T::decode(input)?);
        }
        Ok(v)
    }
}

impl<T: Encode, const N: usize> Encode for [T; N] {
    fn encode(&self, out: &mut Vec<u8>) {
        for v in self {
            v.encode(out);
        }
    }
}

impl<'a, T: Decode<'a>, const N: usize> Decode<'a> for [T; N] {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        let mut v = Vec::with_capacity(N);
        for _ in 0..N {
            v.push(T::decode(input)?);
        }
        Ok(v.try_into().unwrap_or_else(|_| unreachable!()))
    }
}

impl<T: Encode + ?Sized> Encode for &T {
    fn encode(&self, out: &mut Vec<u8>) {
        (**self).encode(out);
    }
}

impl<T: Encode + ?Sized> Encode for Box<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        (**self).encode(out);
    }
}

impl<'a, T: Decode<'a>> Decode<'a> for Box<T> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        T::decode(input).map(Box::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VarInt, from_bytes_exact, to_bytes};

    fn roundtrip<T>(value: T)
    where
        T: Encode + for<'a> Decode<'a> + PartialEq + std::fmt::Debug,
    {
        let bytes = to_bytes(&value);
        assert_eq!(from_bytes_exact::<T>(&bytes).unwrap(), value);
    }

    #[test]
    fn primitives() {
        roundtrip(0x1234u16);
        roundtrip(-5i64);
        roundtrip(1.5f32);
        roundtrip(true);
        roundtrip(Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef));
        roundtrip(String::from("héllo ☃"));
        roundtrip(Some(VarInt(300)));
        roundtrip(None::<i32>);
        roundtrip(vec![VarInt(1), VarInt(-1)]);
        roundtrip(vec![[0u64; 64]; 3]);
        roundtrip([1u8, 2, 3]);
        assert_eq!(to_bytes(&25565u16), [0x63, 0xdd]);
    }

    #[test]
    fn borrowed() {
        let bytes = to_bytes(&ByteArray(&[1, 2, 3]));
        assert_eq!(from_bytes_exact::<ByteArray>(&bytes).unwrap().0, [1, 2, 3]);
        let s = to_bytes("abc");
        assert_eq!(from_bytes_exact::<&str>(&s).unwrap(), "abc");
    }

    #[test]
    fn rejects_bad_input() {
        assert!(matches!(
            bool::decode(&mut &[2u8][..]),
            Err(DecodeError::InvalidValue { .. })
        ));
        assert_eq!(
            i32::decode(&mut &[0u8; 3][..]),
            Err(DecodeError::UnexpectedEof)
        );
        let long = to_bytes("abcdef");
        assert!(matches!(
            BoundedString::<5>::decode(&mut long.as_slice()),
            Err(DecodeError::TooLong { .. })
        ));
        let hostile = [0xff, 0xff, 0xff, 0xff, 0x07];
        assert!(Vec::<u8>::decode(&mut &hostile[..]).is_err());
        let mut big_t = vec![0xff, 0xff, 0x7f];
        big_t.resize(big_t.len() + 2_097_151, 0);
        assert!(Vec::<[u64; 512]>::decode(&mut big_t.as_slice()).is_err());
    }
}
