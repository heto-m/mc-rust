use super::*;
use crate::PacketIds;
use crate::version::tables::VersionTables;
use crate::version::translator::Maps;
use crate::version::{Session, Translated, Wire};

fn wire() -> Wire {
    Wire::Legacy(translator())
}

fn maps() -> &'static Maps {
    translator().maps()
}

fn data() -> &'static mcr_data::Version {
    mcr_data::v26_3()
}

fn canonical(registry: &str, name: &str) -> i32 {
    data().registry(registry).unwrap().id_of(name).unwrap() as i32
}

fn old_id(state: ProtocolState, direction: Direction, name: &str) -> i32 {
    let key = VersionTables::packet_key(
        ["handshake", "status", "login", "configuration", "play"][state as usize],
        ["clientbound", "serverbound"][direction as usize],
    );
    let names = translator().tables().old_names(&key);
    if names.is_empty() {
        return new_id(state, direction, name);
    }
    names
        .iter()
        .position(|n| n.strip_prefix("minecraft:") == Some(name))
        .unwrap_or_else(|| panic!("no 26.1.2 packet {name}")) as i32
}

fn new_id(state: ProtocolState, direction: Direction, name: &str) -> i32 {
    PacketIds::v26_3()
        .id_by_name(state, direction, name)
        .unwrap()
}

fn varint(v: i32) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, v);
    out
}

fn text(s: &str) -> Vec<u8> {
    mcr_buf::to_bytes(&crate::types::Component::text(s))
}

fn frames(t: Translated<'_>) -> Vec<(i32, Vec<u8>)> {
    match t {
        Translated::Packet(id, bytes) => vec![(id, bytes.to_vec())],
        Translated::Packets(frames) => frames.map(|(id, b)| (id, b.to_vec())).collect(),
        Translated::Drop => Vec::new(),
    }
}

fn inbound_in(state: ProtocolState, name: &str, payload: &[u8]) -> Option<(i32, Vec<u8>)> {
    let mut out = Vec::new();
    let t = wire()
        .inbound(
            state,
            old_id(state, C, name),
            payload,
            &mut Session::default(),
            &mut out,
        )
        .unwrap();
    frames(t).into_iter().next()
}

fn inbound(name: &str, payload: &[u8]) -> Option<(i32, Vec<u8>)> {
    inbound_in(Play, name, payload)
}

fn outbound(name: &str, payload: &[u8]) -> Option<(i32, Vec<u8>)> {
    let mut out = Vec::new();
    let t = wire()
        .outbound(
            Play,
            new_id(Play, S, name),
            payload,
            &mut Session::default(),
            &mut out,
        )
        .unwrap();
    frames(t).into_iter().next()
}

fn decode<'a, T: mcr_buf::Decode<'a>>(bytes: &'a [u8]) -> T {
    mcr_buf::from_bytes_exact(bytes).unwrap()
}

#[test]
fn tables_load() {
    let t = translator();
    assert_eq!((t.version_name(), t.protocol()), ("26.1.2", 775));
    assert!(maps().old_swing_animation.is_some());
    assert!(maps().old_map_color.is_some());
}

#[test]
fn registries_shift_against_26_3() {
    let stone = data().item_by_name("minecraft:stone").unwrap() as i32;
    let sulfur = data().item_by_name("minecraft:sulfur").unwrap() as i32;
    assert!(maps().item.back(stone).is_some());
    assert_eq!(maps().item.back(sulfur), None);
    let bed = translator()
        .tables()
        .old_names("registry/minecraft:block_entity_type")
        .iter()
        .position(|n| n == "minecraft:bed")
        .unwrap() as i32;
    assert_eq!(maps().block_entity.forward(bed), None);
}

#[test]
fn color_argument_is_team_color() {
    let old = translator()
        .tables()
        .old_names("registry/minecraft:command_argument_type")
        .iter()
        .position(|n| n == "minecraft:color")
        .unwrap() as i32;
    let team = canonical("minecraft:command_argument_type", "minecraft:team_color");
    assert_eq!(maps().argument.forward(old), Some(team));
    assert_eq!(maps().argument.back(team), Some(old));
}

#[test]
fn login_gains_online_mode() {
    let login = crate::play::clientbound::player::Login {
        player_id: 7,
        hardcore: false,
        levels: vec![mcr_buf::Identifier::parse("minecraft:overworld").unwrap()],
        max_players: 20,
        chunk_radius: 10,
        simulation_distance: 10,
        reduced_debug_info: false,
        show_death_screen: true,
        do_limited_crafting: false,
        spawn_info: crate::play::clientbound::player::CommonPlayerSpawnInfo {
            dimension_type: 0,
            dimension: mcr_buf::Identifier::parse("minecraft:overworld").unwrap(),
            seed: 1,
            game_type: crate::types::game::GameType::Creative,
            previous_game_type: None,
            is_debug: false,
            is_flat: true,
            last_death_location: None,
            portal_cooldown: 0,
            sea_level: 63,
        },
        online_mode: true,
        enforces_secure_chat: true,
    };
    let canonical_bytes = mcr_buf::to_bytes(&login);
    // 26.1 has no online-mode byte, and a signed-byte previous game type (-1 = none).
    let mut old = canonical_bytes.clone();
    old.remove(old.len() - 2);
    let prev = old.len() - 7;
    assert_eq!(old[prev], 0);
    old[prev] = 0xff;
    let (id, bytes) = inbound("login", &old).unwrap();
    assert_eq!(id, new_id(Play, C, "login"));
    assert_eq!(bytes, canonical_bytes);
}

#[test]
fn login_finished_gains_a_session_id() {
    let mut p = vec![0; 16];
    p.extend(varint(3));
    p.extend(b"Bob");
    p.push(0);
    let (_, bytes) = inbound_in(ProtocolState::Login, "login_finished", &p).unwrap();
    let finished: crate::login::LoginFinished = decode(&bytes);
    assert_eq!(finished.profile.name, "Bob");
    assert!(finished.session_id.is_nil());
}

#[test]
fn team_parameters_reorder_and_colors_become_optional() {
    use crate::play::clientbound::ui::{SetPlayerTeam, TeamAction, TeamColor};
    let team = |color: i32, players: bool| {
        let mut p = varint(1);
        p.push(b't');
        p.push(if players { 0 } else { 2 });
        p.extend(text("Display"));
        p.push(3);
        p.extend(varint(1));
        p.extend(varint(2));
        p.extend(varint(color));
        p.extend(text("["));
        p.extend(text("]"));
        if players {
            p.extend(varint(1));
            p.extend(varint(3));
            p.extend(b"Bob");
        }
        p
    };
    let (_, bytes) = inbound("set_player_team", &team(12, true)).unwrap();
    let packet: SetPlayerTeam = decode(&bytes);
    let TeamAction::Add {
        parameters,
        players,
    } = packet.action
    else {
        panic!()
    };
    assert_eq!(parameters.color, Some(TeamColor::Red));
    assert_eq!(parameters.options, 3);
    assert_eq!(parameters.player_prefix.to_plain_string(), "[");
    assert_eq!(parameters.player_suffix.to_plain_string(), "]");
    assert_eq!(players, ["Bob"]);
    let (_, bytes) = inbound("set_player_team", &team(21, false)).unwrap();
    let packet: SetPlayerTeam = decode(&bytes);
    let TeamAction::Change(parameters) = packet.action else {
        panic!()
    };
    assert_eq!(parameters.color, None);
}

#[test]
fn spectator_action_becomes_spectate_entity() {
    let (id, bytes) = outbound("spectator_action", &[1, 42]).unwrap();
    assert_eq!(id, old_id(Play, S, "spectate_entity"));
    assert_eq!(bytes, [42]);
    assert_eq!(outbound("spectator_action", &[0]), None);
}

#[test]
fn attributes_remap() {
    let gravity = canonical("minecraft:attribute", "minecraft:gravity");
    let old = maps().attribute.back(gravity).unwrap();
    assert_ne!(old, gravity);
    let mut p = varint(5);
    p.extend(varint(1));
    p.extend(varint(old));
    p.extend(0.08f64.to_be_bytes());
    p.extend(varint(0));
    let (_, bytes) = inbound("update_attributes", &p).unwrap();
    let update: crate::play::clientbound::entity::UpdateAttributes = decode(&bytes);
    assert_eq!(update.attributes.0[0].attribute, gravity);
}

#[test]
fn bed_block_entities_are_left_out() {
    let bed = translator()
        .tables()
        .old_names("registry/minecraft:block_entity_type")
        .iter()
        .position(|n| n == "minecraft:bed")
        .unwrap() as i32;
    let chest = canonical("minecraft:block_entity_type", "minecraft:chest");
    let old_chest = maps().block_entity.back(chest).unwrap();
    let mut p = vec![0, 0, 0, 1, 0, 0, 0, 2];
    p.extend([0, 0]);
    p.extend(varint(2));
    p.extend([0x12, 0, 64]);
    p.extend(varint(bed));
    p.extend([10, 0]);
    p.extend([0x13, 0, 64]);
    p.extend(varint(old_chest));
    p.extend([10, 0]);
    p.extend([0, 0, 0, 0, 0, 0]);
    let (_, bytes) = inbound("level_chunk_with_light", &p).unwrap();
    let mut expected = vec![0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 1, 0x13, 0, 64];
    expected.extend(varint(chest));
    expected.extend([10, 0]);
    expected.extend([0, 0, 0, 0, 0, 0]);
    assert_eq!(bytes, expected);
    let mut p = vec![0; 8];
    p.extend(varint(bed));
    p.extend([10, 0]);
    assert_eq!(inbound("block_entity_data", &p), None);
}

#[test]
fn item_stacks_translate() {
    let sword = data().item_by_name("minecraft:diamond_sword").unwrap() as i32;
    let mut p = varint(1);
    p.extend(varint(maps().item.back(sword).unwrap()));
    p.extend([0, 0]);
    let (_, bytes) = inbound("set_cursor_item", &p).unwrap();
    let cursor: crate::play::clientbound::ui::SetCursorItem = decode(&bytes);
    assert_eq!(cursor.contents.item, sword);
}

#[test]
fn known_core_pack_defaults_to_26_1_2() {
    let pack = crate::types::registry::KnownPack::vanilla("core", "26.3");
    let mut p = varint(1);
    mcr_buf::Encode::encode(&pack, &mut p);
    let mut out = Vec::new();
    let t = wire()
        .outbound(
            ProtocolState::Configuration,
            new_id(ProtocolState::Configuration, S, "select_known_packs"),
            &p,
            &mut Session::default(),
            &mut out,
        )
        .unwrap();
    let (_, bytes) = frames(t).remove(0);
    let back: mcr_buf::LengthPrefixed<crate::types::registry::KnownPack, 64> = decode(&bytes);
    assert_eq!(back.0[0].version, "26.1.2");
}

#[test]
fn hostile_payloads_never_panic() {
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for rule in v26_2::RULES.iter().chain(RULES) {
        for len in [0usize, 1, 3, 8, 40, 300] {
            for _ in 0..40 {
                let payload: Vec<u8> = (0..len).map(|_| next() as u8).collect();
                let (mut out, mut session) = (Vec::new(), Session::default());
                let _ = match rule.direction {
                    C => wire().inbound(
                        rule.state,
                        old_id(rule.state, C, rule.name),
                        &payload,
                        &mut session,
                        &mut out,
                    ),
                    S => wire().outbound(
                        rule.state,
                        new_id(rule.state, S, rule.name),
                        &payload,
                        &mut session,
                        &mut out,
                    ),
                };
            }
        }
    }
}
