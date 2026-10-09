use mcr_buf::{Decode, DecodeError, Encode, VarInt, from_bytes_exact, to_bytes};

fn roundtrip<T>(value: &T)
where
    T: Encode + for<'a> Decode<'a> + PartialEq + std::fmt::Debug,
{
    let bytes = to_bytes(value);
    assert_eq!(&from_bytes_exact::<T>(&bytes).unwrap(), value);
}

fn roundtrip_borrowed<'a, T>(value: &T, bytes: &'a mut Vec<u8>)
where
    T: Encode + Decode<'a> + PartialEq + std::fmt::Debug,
{
    value.encode(bytes);
    assert_eq!(&from_bytes_exact::<T>(bytes).unwrap(), value);
}

#[derive(Debug, PartialEq, Encode, Decode)]
struct Named {
    a: u16,
    #[mcr(varint)]
    b: i32,
    #[mcr(varlong)]
    c: i64,
    d: Option<bool>,
}

#[derive(Debug, PartialEq, Encode, Decode)]
struct Tuple(u8, String);

#[derive(Debug, PartialEq, Encode, Decode)]
struct Unit;

#[derive(Debug, PartialEq, Encode, Decode)]
struct Borrowed<'a> {
    #[mcr(max_len = 5)]
    name: &'a str,
    rest: mcr_buf::RestBytes<'a>,
}

#[derive(Debug, PartialEq, Encode, Decode)]
struct Generic<T> {
    inner: T,
    list: Vec<T>,
}

#[derive(Debug, PartialEq, Encode, Decode)]
struct Owned {
    #[mcr(max_len = 3)]
    s: String,
}

mod half {
    use mcr_buf::{Decode, Encode, Result};

    pub fn encode(v: &u32, out: &mut Vec<u8>) {
        (v / 2).encode(out);
    }

    pub fn decode(input: &mut &[u8]) -> Result<u32> {
        u32::decode(input).map(|v| v * 2)
    }
}

#[derive(Debug, PartialEq, Encode, Decode)]
struct WithField {
    #[mcr(with = half)]
    v: u32,
}

#[derive(Debug, PartialEq, Encode, Decode)]
enum Plain {
    A,
    B,
    C,
}

#[derive(Debug, PartialEq, Encode, Decode)]
#[mcr(tag = u8)]
enum Explicit {
    A = 3,
    B,
    C = 10,
}

#[derive(Debug, PartialEq, Encode, Decode)]
#[mcr(tag = i32)]
enum Data<'a> {
    Unit,
    Tuple(u8, #[mcr(varint)] i32),
    Named { name: &'a str, n: VarInt },
}

#[derive(Debug, PartialEq, Encode, Decode)]
#[mcr(tag = varint)]
enum Neg {
    A = -1,
    B = 300,
}

#[test]
fn structs() {
    roundtrip(&Named {
        a: 7,
        b: -3,
        c: 1 << 40,
        d: Some(true),
    });
    roundtrip(&Tuple(1, "x".into()));
    roundtrip(&Unit);
    roundtrip(&Generic {
        inner: 5u8,
        list: vec![1, 2],
    });
    assert_eq!(to_bytes(&Unit), []);
}

#[test]
fn struct_bytes() {
    let n = Named {
        a: 0x0102,
        b: 300,
        c: 1,
        d: None,
    };
    assert_eq!(to_bytes(&n), [1, 2, 0xac, 0x02, 1, 0]);
}

#[test]
fn borrowed_and_max_len() {
    let bytes = to_bytes(&Borrowed {
        name: "abc",
        rest: mcr_buf::RestBytes(&[9, 9]),
    });
    let b = from_bytes_exact::<Borrowed>(&bytes).unwrap();
    assert_eq!((b.name, b.rest.0), ("abc", &[9, 9][..]));

    let long = to_bytes("abcdef");
    assert!(matches!(
        Borrowed::decode(&mut long.as_slice()),
        Err(DecodeError::TooLong { max: 5, .. })
    ));
    assert!(matches!(
        Owned::decode(&mut to_bytes("abcd").as_slice()),
        Err(DecodeError::TooLong { max: 3, .. })
    ));
    roundtrip(&Owned { s: "abc".into() });
}

#[test]
fn with_module() {
    let v = WithField { v: 10 };
    assert_eq!(to_bytes(&v), [0, 0, 0, 5]);
    roundtrip(&v);
}

#[test]
fn enum_tags() {
    assert_eq!(to_bytes(&Plain::C), [2]);
    assert_eq!(to_bytes(&Explicit::A), [3]);
    assert_eq!(to_bytes(&Explicit::B), [4]);
    assert_eq!(to_bytes(&Explicit::C), [10]);
    assert_eq!(to_bytes(&Neg::A), [0xff, 0xff, 0xff, 0xff, 0x0f]);
    roundtrip(&Neg::A);
    roundtrip(&Neg::B);
    roundtrip(&Plain::B);
    roundtrip(&Explicit::C);
}

#[test]
fn enum_data() {
    assert_eq!(to_bytes(&Data::Tuple(1, 2)), [0, 0, 0, 1, 1, 2]);
    roundtrip_borrowed(&Data::Unit, &mut Vec::new());
    roundtrip_borrowed(&Data::Tuple(9, -1), &mut Vec::new());
    roundtrip_borrowed(
        &Data::Named {
            name: "hi",
            n: VarInt(12),
        },
        &mut Vec::new(),
    );
}

#[test]
fn unknown_tag() {
    assert_eq!(
        Plain::decode(&mut &[3u8][..]),
        Err(DecodeError::InvalidValue {
            what: "Plain",
            value: 3
        })
    );
    assert_eq!(
        Explicit::decode(&mut &[5u8][..]),
        Err(DecodeError::InvalidValue {
            what: "Explicit",
            value: 5
        })
    );
    assert_eq!(Plain::decode(&mut &[][..]), Err(DecodeError::UnexpectedEof));
}
