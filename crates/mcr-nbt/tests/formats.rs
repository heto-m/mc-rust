use mcr_buf::{Decode, Encode, from_bytes_exact, to_bytes};
use mcr_nbt::{
    Compound, Compression, Limits, List, NbtError, NetworkCompound, NetworkNbt, OptionalNetworkNbt,
    Value, read_disk, read_network, write_disk,
};

fn sample() -> Compound {
    let mut inner = Compound::new();
    inner.insert("emoji", "x\u{1F600}\0y");
    inner.insert("f", 1.5f32);
    let mut c = Compound::new();
    c.insert("b", 1i8);
    c.insert("s", -2i16);
    c.insert("i", 3i32);
    c.insert("l", i64::MAX);
    c.insert("d", std::f64::consts::PI);
    c.insert("ba", Value::ByteArray(vec![-1, 0, 1]));
    c.insert("ia", vec![1i32, -1]);
    c.insert("la", vec![i64::MIN, 0]);
    c.insert("ints", List::Int(vec![1, 2]));
    c.insert(
        "comps",
        List::Compound(vec![inner.clone(), Compound::new()]),
    );
    c.insert("lists", List::List(vec![List::Byte(vec![1]), List::Empty]));
    c.insert("inner", inner);
    c
}

#[test]
fn network_round_trip() {
    let v = Value::Compound(sample());
    let bytes = to_bytes(&NetworkNbt(v.clone()));
    assert_eq!(bytes[0], 10);
    assert_eq!(from_bytes_exact::<NetworkNbt>(&bytes).unwrap().0, v);
    let c = from_bytes_exact::<NetworkCompound>(&bytes).unwrap();
    assert_eq!(c.0, sample());
}

#[test]
fn network_accepts_non_compound_root() {
    let bytes = to_bytes(&NetworkNbt(Value::String("hi".into())));
    assert_eq!(bytes, [8, 0, 2, b'h', b'i']);
    assert!(NetworkCompound::decode(&mut &bytes[..]).is_err());
    assert_eq!(
        from_bytes_exact::<NetworkNbt>(&bytes).unwrap().0.as_str(),
        Some("hi")
    );
}

#[test]
fn end_tag_means_absent() {
    assert_eq!(to_bytes(&OptionalNetworkNbt(None)), [0]);
    let got = from_bytes_exact::<OptionalNetworkNbt>(&[0]).unwrap();
    assert_eq!(got.0, None);
    assert!(NetworkNbt::decode(&mut &[0u8][..]).is_err());
    let some = OptionalNetworkNbt(Some(Value::Int(5)));
    let mut out = Vec::new();
    some.encode(&mut out);
    assert_eq!(out, [3, 0, 0, 0, 5]);
    assert_eq!(from_bytes_exact::<OptionalNetworkNbt>(&out).unwrap(), some);
}

#[test]
fn disk_round_trip_all_compressions() {
    for comp in [Compression::None, Compression::Gzip, Compression::Zlib] {
        let bytes = write_disk("Level", &sample(), comp).unwrap();
        let (name, c) = read_disk(&bytes, Limits::UNCOMPRESSED).unwrap();
        assert_eq!(name, "Level");
        assert_eq!(c, sample());
    }
}

#[test]
fn disk_inflate_is_bounded() {
    let mut big = Compound::new();
    big.insert("z", Value::ByteArray(vec![0; 1 << 20]));
    let gz = write_disk("", &big, Compression::Gzip).unwrap();
    assert!(gz.len() < 16_384);
    let limits = Limits {
        max_bytes: 1024,
        ..Limits::UNCOMPRESSED
    };
    assert!(matches!(
        read_disk(&gz, limits),
        Err(NbtError::QuotaExceeded(_))
    ));
}

#[test]
fn deep_nesting_is_rejected() {
    let mut bytes = vec![10u8];
    for _ in 0..600 {
        bytes.extend_from_slice(&[10, 0, 0]);
    }
    assert!(matches!(
        read_network(&mut &bytes[..], Limits::UNLIMITED),
        Err(NbtError::DepthExceeded(512))
    ));
    let mut lists = vec![9u8];
    for _ in 0..100_000 {
        lists.extend_from_slice(&[9, 0, 0, 0, 1]);
    }
    assert!(read_network(&mut &lists[..], Limits::UNLIMITED).is_err());
}

#[test]
fn depth_512_is_allowed() {
    let mut bytes = vec![];
    for _ in 0..512 {
        bytes.extend_from_slice(&[10, 0, 0]);
    }
    bytes.extend(std::iter::repeat_n(0u8, 513));
    assert!(read_network(&mut &bytes[..], Limits::UNLIMITED).is_ok());
}

#[test]
fn huge_declared_lengths_fail_cheaply() {
    let huge = i32::MAX.to_be_bytes();
    for ty in [7u8, 11, 12] {
        let mut b = vec![ty];
        b.extend_from_slice(&huge);
        assert!(read_network(&mut &b[..], Limits::UNLIMITED).is_err());
    }
    for elem in [1u8, 3, 7, 8, 9, 10, 12] {
        let mut b = vec![9, elem];
        b.extend_from_slice(&huge);
        assert!(read_network(&mut &b[..], Limits::UNLIMITED).is_err());
        assert!(read_network(&mut &b[..], Limits::NETWORK).is_err());
    }
}

#[test]
fn quota_counts_lists() {
    let mut b = vec![9, 3];
    b.extend_from_slice(&1_000_000i32.to_be_bytes());
    b.extend(std::iter::repeat_n(0u8, 4_000_000));
    assert!(matches!(
        read_network(&mut &b[..], Limits::NETWORK),
        Err(NbtError::QuotaExceeded(_))
    ));
}

#[test]
fn malformed_list_headers() {
    assert!(matches!(
        read_network(&mut &[9u8, 0, 0, 0, 0, 0][..], Limits::NETWORK),
        Ok(Some(Value::List(l))) if l.is_empty()
    ));
    assert!(matches!(
        read_network(&mut &[9u8, 0, 0, 0, 0, 2][..], Limits::NETWORK),
        Err(NbtError::MissingListType)
    ));
    assert!(read_network(&mut &[13u8][..], Limits::NETWORK).is_err());
}

#[test]
fn every_truncation_errors_cleanly() {
    let bytes = to_bytes(&NetworkNbt(Value::Compound(sample())));
    for n in 1..bytes.len() {
        assert!(
            read_network(&mut &bytes[..n], Limits::NETWORK).is_err(),
            "{n}"
        );
    }
}

#[test]
fn pseudo_random_garbage_never_panics() {
    let mut x = 0x1234_5678_9abc_def0u64;
    for _ in 0..2000 {
        let len = {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % 64) as usize
        };
        let buf: Vec<u8> = (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x & 0xFF) as u8
            })
            .collect();
        let _ = read_network(&mut &buf[..], Limits::NETWORK);
        let _ = read_disk(&buf, Limits::NETWORK);
        if let Ok(Some(v)) = mcr_nbt::raw::network_root(&buf) {
            let _ = v.size();
        }
    }
}

#[test]
fn string_names_use_modified_utf8_on_the_wire() {
    let mut c = Compound::new();
    c.insert("\u{1F600}", 0i8);
    let bytes = write_disk("", &c, Compression::None).unwrap();
    let key = [0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80];
    assert!(bytes.windows(6).any(|w| w == key));
}
