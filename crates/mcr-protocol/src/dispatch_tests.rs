use std::collections::BTreeSet;

use crate::dispatch::*;
use crate::{Direction, PacketIds, PacketSet, ProtocolError, ProtocolState};

fn data_names(state: ProtocolState, direction: Direction) -> BTreeSet<String> {
    let version = mcr_data::v26_3();
    (0..)
        .map_while(|id| version.packet_name(state, direction, id))
        .map(str::to_owned)
        .collect()
}

fn assert_covers<S: PacketSet>() {
    let names: Vec<_> = S::NAMES.iter().map(|n| (*n).to_owned()).collect();
    let unique: BTreeSet<_> = names.iter().cloned().collect();
    assert_eq!(
        unique.len(),
        names.len(),
        "duplicate variant in {:?} {:?}",
        S::STATE,
        S::DIRECTION
    );
    assert_eq!(
        unique,
        data_names(S::STATE, S::DIRECTION),
        "{:?} {:?} does not match the packet report",
        S::STATE,
        S::DIRECTION
    );
}

#[test]
fn enums_cover_exactly_the_reported_packets() {
    assert_covers::<HandshakeServerbound>();
    assert_covers::<StatusClientbound>();
    assert_covers::<StatusServerbound>();
    assert_covers::<LoginClientbound>();
    assert_covers::<LoginServerbound>();
    assert_covers::<ConfigClientbound>();
    assert_covers::<ConfigServerbound>();
    assert_covers::<PlayClientbound>();
    assert_covers::<PlayServerbound>();
}

#[test]
fn ids_resolve_both_ways() {
    let ids = PacketIds::new(mcr_data::v26_3()).unwrap();
    assert_eq!(
        ids.id_by_name(
            ProtocolState::Login,
            Direction::Clientbound,
            "login_finished"
        ),
        Some(2)
    );
    let packet = LoginClientbound::decode(3, &[0x80, 0x02], &ids).unwrap();
    assert_eq!(packet.name(), "minecraft:login_compression");
    assert_eq!(packet.encode(&ids), (3, vec![0x80, 0x02]));
}

#[test]
fn trailing_bytes_name_the_packet() {
    let ids = PacketIds::v26_3();
    let err = LoginClientbound::decode(3, &[0x01, 0xff], ids).unwrap_err();
    assert_eq!(
        err,
        ProtocolError::TrailingBytes {
            packet: "minecraft:login_compression",
            state: ProtocolState::Login,
            direction: Direction::Clientbound,
            remaining: 1,
        }
    );
    assert!(err.to_string().contains("login_compression"));
}

#[test]
fn unknown_id_and_truncated_payload() {
    let ids = PacketIds::v26_3();
    assert!(matches!(
        LoginClientbound::decode(99, &[], ids),
        Err(ProtocolError::UnknownId { id: 99, .. })
    ));
    assert!(matches!(
        LoginClientbound::decode(-1, &[], ids),
        Err(ProtocolError::UnknownId { id: -1, .. })
    ));
    assert!(matches!(
        LoginClientbound::decode(3, &[], ids),
        Err(ProtocolError::Decode {
            packet: "minecraft:login_compression",
            ..
        })
    ));
}
