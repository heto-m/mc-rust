use super::*;
use crate::PacketIds;
use crate::version::{TranslateError, Translated, Wire};

pub(super) fn wire() -> Wire {
    Wire::Legacy(translator())
}

pub(super) fn old_id(state: ProtocolState, direction: Direction, name: &str) -> i32 {
    let t = translator();
    let key = crate::version::tables::VersionTables::packet_key(
        ["handshake", "status", "login", "configuration", "play"][state as usize],
        ["clientbound", "serverbound"][direction as usize],
    );
    let names = t_names(t, &key);
    if names.is_empty() {
        return PacketIds::v26_3()
            .id_by_name(state, direction, name)
            .unwrap();
    }
    names
        .iter()
        .position(|n| n.strip_prefix("minecraft:") == Some(name))
        .unwrap_or_else(|| panic!("no 26.2 packet {name}")) as i32
}

fn t_names<'a>(t: &'a Translator, key: &str) -> &'a [String] {
    t.tables().old_names(key)
}

pub(super) fn new_id(state: ProtocolState, direction: Direction, name: &str) -> i32 {
    PacketIds::v26_3()
        .id_by_name(state, direction, name)
        .unwrap()
}

/// Translates a 26.2 clientbound play packet; `None` when dropped.
pub(super) fn inbound(
    name: &str,
    payload: &[u8],
) -> std::result::Result<Option<(i32, Vec<u8>)>, TranslateError> {
    let id = old_id(Play, C, name);
    let mut scratch = Vec::new();
    Ok(match wire().inbound(Play, id, payload, &mut scratch)? {
        Translated::Packet(id, bytes) => Some((id, bytes.to_vec())),
        Translated::Drop => None,
    })
}

pub(super) fn outbound(
    name: &str,
    payload: &[u8],
) -> std::result::Result<Option<(i32, Vec<u8>)>, TranslateError> {
    let id = new_id(Play, S, name);
    let mut scratch = Vec::new();
    Ok(match wire().outbound(Play, id, payload, &mut scratch)? {
        Translated::Packet(id, bytes) => Some((id, bytes.to_vec())),
        Translated::Drop => None,
    })
}

#[test]
fn tables_load() {
    let t = translator();
    assert_eq!((t.version_name(), t.protocol()), ("26.2", 776));
}

#[test]
fn handshake_carries_old_protocol() {
    let intention = crate::handshake::Intention {
        protocol_version: 777,
        host: "localhost".into(),
        port: 25572,
        intent: crate::handshake::Intent::Login,
    };
    let bytes = mcr_buf::to_bytes(&intention);
    let mut scratch = Vec::new();
    let Translated::Packet(id, out) = wire().outbound(Handshake, 0, &bytes, &mut scratch).unwrap()
    else {
        panic!("dropped")
    };
    let back: crate::handshake::Intention = mcr_buf::from_bytes_exact(out).unwrap();
    assert_eq!((id, back.protocol_version, back.port), (0, 776, 25572));
}

#[test]
fn shifted_ids_map_by_name() {
    let (id, bytes) = inbound("set_time", &[1, 2, 3]).unwrap().unwrap();
    assert_eq!(id, new_id(Play, C, "set_time"));
    assert_eq!(bytes, [1, 2, 3]);
    assert_ne!(old_id(Play, C, "set_time"), id);
    assert_eq!(
        wire().wire_clientbound_id(Play, id),
        Some(old_id(Play, C, "set_time"))
    );
}

#[test]
fn unknown_wire_id_is_an_error() {
    let mut scratch = Vec::new();
    assert!(wire().inbound(Play, 10_000, &[], &mut scratch).is_err());
    assert!(wire().inbound(Play, -1, &[], &mut scratch).is_err());
}

#[test]
fn animate_swings_become_swing_animation() {
    let (id, bytes) = inbound("animate", &[7, 3]).unwrap().unwrap();
    assert_eq!(id, new_id(Play, C, "swing_animation"));
    let swing: crate::play::clientbound::entity::EntitySwing =
        mcr_buf::from_bytes_exact(&bytes).unwrap();
    assert_eq!(swing.entity_id, 7);
    assert_eq!(swing.hand, crate::types::game::Hand::Off);
    let (id, bytes) = inbound("animate", &[7, 4]).unwrap().unwrap();
    assert_eq!((id, bytes), (new_id(Play, C, "animate"), vec![7, 1]));
    assert_eq!(inbound("animate", &[7, 2]).unwrap().unwrap().1, [7, 0]);
    assert_eq!(inbound("animate", &[7, 9]).unwrap(), None);
    assert!(inbound("animate", &[7]).is_err());
}

#[test]
fn player_action_skips_new_ordinal() {
    let (id, bytes) = outbound("player_action", &[3, 9, 9]).unwrap().unwrap();
    assert_eq!(
        (id, bytes),
        (old_id(Play, S, "player_action"), vec![2, 9, 9])
    );
    assert_eq!(
        outbound("player_action", &[0, 9]).unwrap().unwrap().1,
        [0, 9]
    );
    assert_eq!(outbound("player_action", &[1, 9]).unwrap(), None);
}

#[test]
fn punch_becomes_main_hand_swing() {
    let (id, bytes) = outbound("punch", &[]).unwrap().unwrap();
    assert_eq!((id, bytes), (old_id(Play, S, "swing"), vec![0]));
}

#[test]
fn identical_states_pass_through() {
    let mut scratch = Vec::new();
    let payload = 5i64.to_be_bytes();
    let out = wire()
        .inbound(ProtocolState::Status, 1, &payload, &mut scratch)
        .unwrap();
    assert_eq!(out, Translated::Packet(1, &payload[..]));
}

fn data() -> &'static mcr_data::Version {
    mcr_data::v26_3()
}

fn new_item(name: &str) -> i32 {
    data().item_by_name(name).unwrap() as i32
}

fn old_item(name: &str) -> i32 {
    maps().item.back(new_item(name)).unwrap()
}

fn new_state(block: &str) -> i32 {
    let b = data().block_by_name(block).unwrap();
    data().block_info(b).unwrap().default_state as i32
}

fn old_state(block: &str) -> i32 {
    maps().states.back(new_state(block)).unwrap()
}

fn varint(v: i32) -> Vec<u8> {
    let mut out = Vec::new();
    mcr_buf::write_varint(&mut out, v);
    out
}

fn decode<'a, T: mcr_buf::Decode<'a>>(bytes: &'a [u8]) -> T {
    mcr_buf::from_bytes_exact(bytes).unwrap()
}

#[test]
fn registries_actually_shift() {
    assert_ne!(
        old_item("minecraft:diamond_sword"),
        new_item("minecraft:diamond_sword")
    );
    assert_ne!(old_state("minecraft:stone"), 0);
    assert_eq!(maps().component.forward(super::items::old_ids::SWING), None);
    assert_eq!(
        maps().component.forward(super::items::old_ids::MAP_COLOR),
        None
    );
}

fn old_stack_bytes(item: i32, count: i32, components: &[u8], n_added: i32) -> Vec<u8> {
    let mut b = varint(count);
    b.extend(varint(item));
    b.extend(varint(n_added));
    b.extend(varint(0));
    b.extend_from_slice(components);
    b
}

#[test]
fn item_stack_components_translate() {
    use crate::types::component::{DataComponent, SwingAnimationType};
    use crate::types::item::ItemStack;
    let mut comps = varint(3);
    comps.extend(varint(7));
    comps.extend(varint(super::items::old_ids::SWING));
    comps.extend([2, 9]);
    comps.extend(varint(super::items::old_ids::MAP_COLOR));
    comps.extend([0, 0, 0, 5]);
    let mut container = varint(super::items::old_ids::CONTAINER);
    container.extend([1, 1]);
    container.extend(varint(old_item("minecraft:stone")));
    container.extend([2, 0, 0]);
    comps.extend(container);
    let payload = {
        let mut p = varint(0);
        p.extend(varint(1));
        p.extend([0, 5]);
        p.extend(old_stack_bytes(
            old_item("minecraft:diamond_sword"),
            1,
            &comps,
            4,
        ));
        p
    };
    let (id, bytes) = inbound("container_set_slot", &payload).unwrap().unwrap();
    assert_eq!(id, new_id(Play, C, "container_set_slot"));
    let slot: crate::play::clientbound::ui::ContainerSetSlot = decode(&bytes);
    let stack: &ItemStack = &slot.item;
    assert_eq!(stack.item, new_item("minecraft:diamond_sword"));
    let added = &stack.components.added;
    assert_eq!(added.len(), 4);
    assert_eq!(added[0], DataComponent::Damage(mcr_buf::VarInt(7)));
    let DataComponent::AttackAnimation(a) = &added[1] else {
        panic!("{:?}", added[1])
    };
    assert_eq!((a.kind, a.duration), (SwingAnimationType::Stab, 9));
    assert!(matches!(added[2], DataComponent::InteractAnimation(_)));
    let DataComponent::Container(c) = &added[3] else {
        panic!()
    };
    assert_eq!(c.0[0].as_ref().unwrap().item, new_item("minecraft:stone"));
}

#[test]
fn chunk_sections_remap_and_widen() {
    use mcr_world::{ChunkData, LightData, WorldConfig};
    let stone = old_state("minecraft:stone");
    let dirt = old_state("minecraft:dirt");
    let mut sections = Vec::new();
    // Single value.
    sections.extend([0, 1, 0, 0, 0]);
    sections.extend(varint(stone));
    sections.extend([0, 0]);
    // Indirect, 4 bits: stone then dirt alternating.
    sections.extend([0x10, 0, 0, 0, 4]);
    sections.extend(varint(2));
    sections.extend(varint(stone));
    sections.extend(varint(dirt));
    for _ in 0..256 {
        sections.extend(0x1010_1010_1010_1010u64.to_be_bytes());
    }
    sections.extend([0, 0]);
    // Direct, 15 bits: every entry dirt.
    sections.extend([0x10, 0, 0, 0, 15]);
    let mut word = 0u64;
    for k in 0..4 {
        word |= (dirt as u64) << (k * 15);
    }
    for _ in 0..1024 {
        sections.extend(word.to_be_bytes());
    }
    sections.extend([0, 0]);
    let mut payload = vec![0, 0, 0, 1, 0, 0, 0, 2];
    payload.extend(varint(0));
    payload.extend(varint(sections.len() as i32));
    payload.extend(&sections);
    payload.extend(varint(0));
    // Light: sky mask bit 1 as one long, three empty masks, one sky array, no block arrays.
    payload.extend(varint(1));
    payload.extend(2u64.to_be_bytes());
    payload.extend([0, 0, 0]);
    payload.extend(varint(1));
    payload.extend(varint(2048));
    payload.extend([0x11; 2048]);
    payload.extend(varint(0));
    let (_, bytes) = inbound("level_chunk_with_light", &payload)
        .unwrap()
        .unwrap();
    let cfg = WorldConfig::new(data().state_count() as u32, 64, 0, 48).unwrap();
    let mut input = &bytes[8..];
    let chunk = ChunkData::decode(&mut input, &cfg).unwrap();
    let (s, d) = (
        new_state("minecraft:stone") as u16,
        new_state("minecraft:dirt") as u16,
    );
    assert_eq!(chunk.sections[0].block(3, 3, 3).0, s);
    assert_eq!(chunk.sections[1].block(0, 0, 0).0, s);
    assert_eq!(chunk.sections[1].block(1, 0, 0).0, d);
    assert_eq!(chunk.sections[2].block(15, 15, 15).0, d);
    assert_eq!(chunk.sections[2].blocks().bits(), 16);
    let light = LightData::decode(&mut input, &cfg).unwrap();
    assert_eq!(light.sky.len(), 1);
    assert_eq!(light.sky[0].section, 1);
    assert!(input.is_empty());
}

#[test]
fn entity_movement_and_spawn() {
    let (_, bytes) = inbound("move_entity_pos", &[5, 0, 1, 0, 2, 0, 3, 1])
        .unwrap()
        .unwrap();
    assert_eq!(bytes, [5, 1, 0, 1, 0, 2, 0, 3]);
    let (_, bytes) = inbound("move_entity_rot", &[5, 10, 20, 0])
        .unwrap()
        .unwrap();
    assert_eq!(bytes, [5, 0, 10, 20]);
    let zombie_old = maps()
        .entity
        .back(data().entity_type_by_name("minecraft:zombie").unwrap() as i32)
        .unwrap();
    let mut p = varint(9);
    p.extend([0; 16]);
    p.extend(varint(zombie_old));
    p.extend([0; 24]);
    p.extend([0, 0, 0, 0, 0]);
    let (_, bytes) = inbound("add_entity", &p).unwrap().unwrap();
    let e: crate::play::clientbound::entity::AddEntity = decode(&bytes);
    assert_eq!(
        e.entity_type,
        data().entity_type_by_name("minecraft:zombie").unwrap() as i32
    );
}

#[test]
fn respawn_previous_game_type() {
    let mut p = varint(0);
    p.extend([9, b'm', b':', b'o', b'v', b'e', b'r', b'w', b'l', b'd']);
    p.extend([0; 8]);
    p.extend([1, 0xff, 0, 0, 0]);
    p.extend(varint(0));
    p.extend(varint(63));
    p.push(3);
    let (_, bytes) = inbound("respawn", &p).unwrap().unwrap();
    let r: crate::play::clientbound::player::Respawn = decode(&bytes);
    assert_eq!(r.spawn_info.previous_game_type, None);
    assert_eq!(r.spawn_info.sea_level, 63);
}

#[test]
fn particles_move_first() {
    let flame_new = data()
        .registry("minecraft:particle_type")
        .unwrap()
        .id_of("minecraft:flame")
        .unwrap() as i32;
    let flame_old = maps().particle.back(flame_new).unwrap();
    let mut p = vec![1, 0];
    p.extend([0; 24]);
    p.extend([0; 12]);
    p.extend(0.5f32.to_be_bytes());
    p.extend(7i32.to_be_bytes());
    p.extend(varint(flame_old));
    let (_, bytes) = inbound("level_particles", &p).unwrap().unwrap();
    let lp: crate::play::clientbound::world::LevelParticles = decode(&bytes);
    assert_eq!(lp.particle, crate::types::Particle::Simple(flame_new));
    assert_eq!(
        (lp.count, lp.z_max_speed, lp.override_limiter),
        (7, 0.5, true)
    );
}

#[test]
fn sign_and_teleport_go_back() {
    let mut p = vec![0; 8];
    for line in ["a", "", "", ""] {
        p.extend(varint(line.len() as i32));
        p.extend(line.as_bytes());
    }
    p.extend(varint(1));
    let (_, bytes) = outbound("sign_update", &p).unwrap().unwrap();
    assert_eq!(&bytes[8..11], [1, 1, b'a']);
    let mut p = varint(4);
    p.extend([0; 32]);
    assert_eq!(
        outbound("accept_teleportation", &p).unwrap().unwrap().1,
        [4]
    );
}

#[test]
fn creative_slot_with_new_item_is_dropped() {
    let new_only = new_item("minecraft:black_wool_slab");
    assert!(maps().item.back(new_only).is_none());
    let mut p = vec![0, 36];
    p.extend(old_stack_bytes(new_only, 1, &[], 0));
    assert_eq!(outbound("set_creative_mode_slot", &p).unwrap(), None);
    let mut p = vec![0, 36];
    p.extend(old_stack_bytes(new_item("minecraft:stone"), 64, &[], 0));
    let (_, bytes) = outbound("set_creative_mode_slot", &p).unwrap().unwrap();
    let mut expected = varint(64);
    expected.extend(varint(old_item("minecraft:stone")));
    expected.extend([0, 0]);
    assert_eq!(&bytes[2..], &expected[..]);
}

#[test]
fn known_core_pack_claims_old_version() {
    let pack = crate::types::registry::KnownPack::vanilla("core", "26.3");
    let mut p = varint(1);
    mcr_buf::Encode::encode(&pack, &mut p);
    let id = new_id(Configuration, S, "select_known_packs");
    let mut scratch = Vec::new();
    let Translated::Packet(_, bytes) = wire()
        .outbound(Configuration, id, &p, &mut scratch)
        .unwrap()
    else {
        panic!()
    };
    let back: mcr_buf::LengthPrefixed<crate::types::registry::KnownPack, 64> = decode(bytes);
    assert_eq!(back.0[0].version, "26.2");
}

#[test]
fn hostile_payloads_never_panic() {
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for rule in RULES {
        for len in [0usize, 1, 3, 8, 40, 300] {
            for _ in 0..40 {
                let payload: Vec<u8> = (0..len).map(|_| next() as u8).collect();
                let mut scratch = Vec::new();
                let id = match rule.direction {
                    C if rule.state == Play => old_id(rule.state, C, rule.name),
                    C => new_id(rule.state, C, rule.name),
                    S => new_id(rule.state, S, rule.name),
                };
                let w = wire();
                let _ = match rule.direction {
                    C => w.inbound(rule.state, id, &payload, &mut scratch),
                    S => w.outbound(rule.state, id, &payload, &mut scratch),
                };
            }
        }
    }
}

#[test]
fn truncated_chunk_fails_cleanly() {
    let mut p = vec![0; 8];
    p.extend(varint(0));
    p.extend(varint(5000));
    p.extend([0x10, 0]);
    assert!(inbound("level_chunk_with_light", &p).is_err());
}

#[test]
fn brewing_property_sets_are_synthesized() {
    let mut payload = varint(1);
    payload.extend(mcr_buf::to_bytes(
        &mcr_buf::Identifier::parse("minecraft:furnace_input").unwrap(),
    ));
    payload.extend(varint(1));
    payload.extend(varint(old_item("minecraft:diamond_sword")));
    payload.extend(varint(0));
    let (_, out) = inbound("update_recipes", &payload).unwrap().unwrap();
    let recipes: crate::play::clientbound::ui::UpdateRecipes = decode(&out);
    let set = |name: &str| {
        recipes
            .item_sets
            .0
            .iter()
            .find(|(n, _)| n.as_str() == name)
            .map(|(_, ids)| ids.iter().map(|v| v.0).collect::<Vec<_>>())
            .unwrap()
    };
    assert_eq!(
        set("minecraft:furnace_input"),
        [new_item("minecraft:diamond_sword")]
    );
    assert!(set("minecraft:brewing_input").contains(&new_item("minecraft:glass_bottle")));
    assert!(set("minecraft:brewing_reagent").contains(&new_item("minecraft:nether_wart")));
    assert_eq!(set("minecraft:brewing_reagent").len(), 21);
}
