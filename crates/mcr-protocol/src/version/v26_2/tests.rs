use super::*;
use crate::PacketIds;
use crate::version::io::Depth;
use crate::version::translator::Maps;
use crate::version::{Session, TranslateError, Translated, Wire};

fn maps() -> &'static Maps {
    translator().maps()
}

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

pub(super) fn frames(t: Translated<'_>) -> Vec<(i32, Vec<u8>)> {
    match t {
        Translated::Packet(id, bytes) => vec![(id, bytes.to_vec())],
        Translated::Packets(frames) => frames.map(|(id, b)| (id, b.to_vec())).collect(),
        Translated::Drop => Vec::new(),
    }
}

/// Every packet a 26.2 clientbound play packet becomes, through `session`.
pub(super) fn inbound_with(
    session: &mut Session,
    name: &str,
    payload: &[u8],
) -> std::result::Result<Vec<(i32, Vec<u8>)>, TranslateError> {
    let id = old_id(Play, C, name);
    let mut scratch = Vec::new();
    Ok(frames(wire().inbound(
        Play,
        id,
        payload,
        session,
        &mut scratch,
    )?))
}

/// Translates a 26.2 clientbound play packet; `None` when dropped.
pub(super) fn inbound(
    name: &str,
    payload: &[u8],
) -> std::result::Result<Option<(i32, Vec<u8>)>, TranslateError> {
    Ok(inbound_with(&mut Session::default(), name, payload)?
        .into_iter()
        .next())
}

pub(super) fn outbound(
    name: &str,
    payload: &[u8],
) -> std::result::Result<Option<(i32, Vec<u8>)>, TranslateError> {
    let id = new_id(Play, S, name);
    let mut scratch = Vec::new();
    let mut session = Session::default();
    Ok(
        frames(wire().outbound(Play, id, payload, &mut session, &mut scratch)?)
            .into_iter()
            .next(),
    )
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
    let mut session = Session::default();
    let Translated::Packet(id, out) = wire()
        .outbound(Handshake, 0, &bytes, &mut session, &mut scratch)
        .unwrap()
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
    let mut session = Session::default();
    assert!(
        wire()
            .inbound(Play, 10_000, &[], &mut session, &mut scratch)
            .is_err()
    );
    assert!(
        wire()
            .inbound(Play, -1, &[], &mut session, &mut scratch)
            .is_err()
    );
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
    let mut session = Session::default();
    let payload = 5i64.to_be_bytes();
    let out = wire()
        .inbound(
            ProtocolState::Status,
            1,
            &payload,
            &mut session,
            &mut scratch,
        )
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
    let swing = super::items::old_component_id("swing_animation");
    let map_color = super::items::old_component_id("map_color");
    assert_eq!(maps().component.forward(swing), None);
    assert_eq!(maps().component.forward(map_color), None);
    assert_eq!(
        (maps().old_swing_animation, maps().old_map_color),
        (Some(swing), Some(map_color))
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
    comps.extend(varint(super::items::old_component_id("swing_animation")));
    comps.extend([2, 9]);
    comps.extend(varint(super::items::old_component_id("map_color")));
    comps.extend([0, 0, 0, 5]);
    let mut container = varint(super::items::old_component_id("container"));
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
    let mut session = Session::default();
    let answer = |session: &mut Session, scratch: &mut Vec<u8>| {
        let Translated::Packet(_, bytes) = wire()
            .outbound(Configuration, id, &p, session, scratch)
            .unwrap()
        else {
            panic!()
        };
        let back: mcr_buf::LengthPrefixed<crate::types::registry::KnownPack, 64> = decode(bytes);
        back.0[0].version.clone()
    };
    assert_eq!(answer(&mut session, &mut scratch), "26.2");
    let offer = crate::types::registry::KnownPack::vanilla("core", "26.2-pre1");
    let mut o = varint(1);
    mcr_buf::Encode::encode(&offer, &mut o);
    let offer_id = old_id(Configuration, C, "select_known_packs");
    let mut buf = Vec::new();
    let same = wire()
        .inbound(Configuration, offer_id, &o, &mut session, &mut buf)
        .unwrap();
    assert_eq!(
        same,
        Translated::Packet(new_id(Configuration, C, "select_known_packs"), &o[..])
    );
    assert_eq!(answer(&mut session, &mut scratch), "26.2-pre1");
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
                let mut session = Session::default();
                let id = match rule.direction {
                    C => old_id(rule.state, C, rule.name),
                    S => new_id(rule.state, S, rule.name),
                };
                let w = wire();
                let _ = match rule.direction {
                    C => w.inbound(rule.state, id, &payload, &mut session, &mut scratch),
                    S => w.outbound(rule.state, id, &payload, &mut session, &mut scratch),
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

#[test]
fn nested_block_predicates_hit_the_depth_limit() {
    let new = crate::types::component::COMPONENT_NAMES
        .iter()
        .position(|n| *n == "can_break")
        .unwrap() as i32;
    let old = maps().component.back(new).unwrap();
    let levels = 64;
    let mut comps = Vec::new();
    for _ in 0..levels {
        comps.extend(varint(old));
        comps.extend([1, 0, 0, 0, 1]);
    }
    comps.extend(varint(old));
    comps.extend([1, 0, 0, 0, 0, 0]);
    comps.extend(std::iter::repeat_n(0, levels));
    let bytes = old_stack_bytes(old_item("minecraft:diamond_pickaxe"), 1, &comps, 1);
    let mut w = Vec::new();
    let c = super::items::Codec::fwd(maps());
    assert!(
        c.stack(&mut bytes.as_slice(), &mut w, Depth::top())
            .is_err()
    );
}

fn canonical(registry: &str, name: &str) -> i32 {
    data()
        .registry(registry)
        .unwrap()
        .id_of(name)
        .unwrap_or_else(|| panic!("no {name} in {registry}")) as i32
}

fn component_id(name: &str) -> i32 {
    crate::types::component::component_type_id(name).unwrap()
}

fn text(s: &str) -> Vec<u8> {
    mcr_buf::to_bytes(&crate::types::Component::text(s))
}

fn patch_bytes(added: Vec<crate::types::component::DataComponent>, removed: Vec<i32>) -> Vec<u8> {
    let mut out = Vec::new();
    crate::types::component::DataComponentPatch { added, removed }.encode_with(&mut out, false);
    out
}

fn template(item: &str) -> crate::types::item::ItemStackTemplate {
    crate::types::item::ItemStackTemplate {
        item: new_item(item),
        count: 1,
        components: Default::default(),
    }
}

/// Every changed component a creative stack can carry, canonical to 26.2 and back.
#[test]
fn changed_components_round_trip_through_26_2() {
    use crate::types::component::*;
    use mcr_buf::{IdOr, LengthPrefixed, VarInt};
    use mcr_nbt::{NetworkNbt, Value};
    let sound = IdOr::Id(canonical(
        "minecraft:sound_event",
        "minecraft:block.stone.break",
    ));
    let mut boxed = template("minecraft:stone");
    boxed.components.added = vec![DataComponent::BreakSound(sound.clone())];
    let mut potions = mcr_nbt::Compound::new();
    potions.insert("potions", Value::String("minecraft:swiftness".into()));
    let added = vec![
        DataComponent::CustomName(crate::types::Component::text("Helm")),
        DataComponent::Trim(ArmorTrim {
            material: IdOr::Inline(TrimMaterial {
                palette_id: mcr_buf::Identifier::parse("minecraft:trim/gold").unwrap(),
                description: crate::types::Component::text("Gold"),
            }),
            pattern: IdOr::Id(3),
        }),
        DataComponent::Instrument(IdOr::Inline(Instrument {
            sound_event: sound.clone(),
            use_duration: 7.0,
            range: 256.0,
            durability_damage: 0,
            description: crate::types::Component::text("Horn"),
        })),
        DataComponent::PotDecorations(PotDecorations {
            back: Some(template("minecraft:angler_pottery_sherd")),
            left: None,
            right: None,
            front: Some(template("minecraft:brewer_pottery_sherd")),
        }),
        DataComponent::AttackAnimation(SwingAnimation {
            kind: SwingAnimationType::Stab,
            duration: 9,
        }),
        DataComponent::InteractAnimation(SwingAnimation {
            kind: SwingAnimationType::Stab,
            duration: 9,
        }),
        DataComponent::Container(LengthPrefixed(vec![Some(boxed), None])),
        DataComponent::TooltipDisplay(TooltipDisplay {
            hide_tooltip: false,
            hidden_components: vec![VarInt(component_id("trim"))],
        }),
        DataComponent::CanBreak(AdventureModePredicate(vec![BlockPredicate {
            blocks: Some(HolderSet::Ids(vec![
                data().block_by_name("minecraft:stone").unwrap() as i32,
            ])),
            properties: None,
            nbt: None,
            components: DataComponentMatchers {
                exact: vec![DataComponent::Damage(VarInt(1))],
                partial: LengthPrefixed(vec![ComponentPredicate {
                    kind: PredicateType::Predicate(canonical(
                        "minecraft:data_component_predicate_type",
                        "minecraft:potion_contents",
                    )),
                    value: NetworkNbt(Value::Compound(potions)),
                }]),
            },
        }])),
    ];
    let canonical_patch = patch_bytes(added, vec![component_id("food")]);
    let (fwd, back) = (
        super::items::Codec::fwd(maps()),
        super::items::Codec::back(maps()),
    );
    let mut old = Vec::new();
    back.patch(&mut canonical_patch.as_slice(), &mut old, Depth::top())
        .unwrap();
    assert_ne!(old, canonical_patch);
    let mut again = Vec::new();
    fwd.patch(&mut old.as_slice(), &mut again, Depth::top())
        .unwrap();
    assert_eq!(again, canonical_patch);
}

#[test]
fn creative_stacks_keep_changed_components() {
    use crate::types::component::*;
    use crate::types::item::{ItemStack, UntrustedItemStack};
    use mcr_buf::IdOr;
    let mut stack = ItemStack::new(new_item("minecraft:diamond_helmet"), 1);
    stack.components.added = vec![
        DataComponent::CustomName(crate::types::Component::text("Helm")),
        DataComponent::Trim(ArmorTrim {
            material: IdOr::Inline(TrimMaterial {
                palette_id: mcr_buf::Identifier::parse("minecraft:trim/gold").unwrap(),
                description: crate::types::Component::text("Gold"),
            }),
            pattern: IdOr::Id(3),
        }),
    ];
    let mut p = vec![0, 5];
    p.extend(mcr_buf::to_bytes(&UntrustedItemStack(stack)));
    let (_, bytes) = outbound("set_creative_mode_slot", &p).unwrap().unwrap();
    let mut trim = vec![0];
    mcr_buf::encode_str("gold", &mut trim);
    trim.push(0);
    trim.extend(text("Gold"));
    trim.push(4);
    let name = text("Helm");
    let mut expected = vec![0, 5, 1];
    expected.extend(varint(old_item("minecraft:diamond_helmet")));
    expected.extend([2, 0]);
    expected.extend(varint(super::items::old_component_id("custom_name")));
    expected.extend(varint(name.len() as i32));
    expected.extend(&name);
    expected.extend(varint(super::items::old_component_id("trim")));
    expected.extend(varint(trim.len() as i32));
    expected.extend(&trim);
    assert_eq!(bytes, expected);
}

fn container_set_data(window: i32, id: i16, value: i16) -> Vec<u8> {
    mcr_buf::to_bytes(&crate::play::clientbound::ui::ContainerSetData {
        container_id: window,
        id,
        value,
    })
}

#[test]
fn brewing_stands_get_their_totals_once() {
    let menu = |name: &str| maps().menu.back(canonical("minecraft:menu", name)).unwrap();
    let open = |window: i32, kind: &str| {
        let mut p = varint(window);
        p.extend(varint(menu(kind)));
        p.extend(text("Brewing Stand"));
        p
    };
    let mut session = Session::default();
    let opened = inbound_with(
        &mut session,
        "open_screen",
        &open(3, "minecraft:brewing_stand"),
    )
    .unwrap();
    assert_eq!(opened.len(), 1);
    let first = inbound_with(
        &mut session,
        "container_set_data",
        &container_set_data(3, 1, 20),
    )
    .unwrap();
    let data_id = new_id(Play, C, "container_set_data");
    assert_eq!(
        first,
        [
            (data_id, container_set_data(3, 1, 20)),
            (data_id, container_set_data(3, 2, 400)),
            (data_id, container_set_data(3, 3, 20)),
        ]
    );
    let again = inbound_with(
        &mut session,
        "container_set_data",
        &container_set_data(3, 0, 399),
    )
    .unwrap();
    assert_eq!(again.len(), 1);
    inbound_with(&mut session, "open_screen", &open(4, "minecraft:furnace")).unwrap();
    let furnace = inbound_with(
        &mut session,
        "container_set_data",
        &container_set_data(4, 1, 20),
    )
    .unwrap();
    assert_eq!(furnace.len(), 1);
}

fn level_event(event: i32, pos: mcr_buf::BlockPos, data: i32) -> Vec<u8> {
    mcr_buf::to_bytes(&crate::play::clientbound::world::LevelEvent {
        event,
        pos,
        data,
        global: false,
    })
}

#[test]
fn moved_level_event_sounds_follow_their_particles() {
    use crate::play::clientbound::world::{Sound, SoundSource};
    let pos = mcr_buf::BlockPos::new(-3, 70, 12);
    let event_id = new_id(Play, C, "level_event");
    for (event, sound) in [(2002, 1053), (2007, 1054)] {
        let out = inbound_with(
            &mut Session::default(),
            "level_event",
            &level_event(event, pos, 0xff00ff),
        )
        .unwrap();
        assert_eq!(
            out,
            [
                (event_id, level_event(event, pos, 0xff00ff)),
                (event_id, level_event(sound, pos, 0)),
            ]
        );
    }
    let out = inbound_with(
        &mut Session::default(),
        "level_event",
        &level_event(3003, pos, 0),
    )
    .unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[1].0, new_id(Play, C, "sound"));
    let sound: Sound = decode(&out[1].1);
    assert_eq!(
        sound.sound,
        mcr_buf::IdOr::Id(canonical(
            "minecraft:sound_event",
            "minecraft:item.honeycomb.wax_on"
        ))
    );
    assert_eq!(sound.source, SoundSource::Blocks);
    assert_eq!((sound.x, sound.y, sound.z), (-20, 564, 100));
    let plain = inbound_with(
        &mut Session::default(),
        "level_event",
        &level_event(1000, pos, 0),
    )
    .unwrap();
    assert_eq!(plain.len(), 1);
}

fn pot_tag() -> Vec<u8> {
    use mcr_nbt::{Compound, List, Value};
    let mut tag = Compound::new();
    let sherds = ["brick", "brick", "brick", "angler_pottery_sherd"]
        .map(|s| format!("minecraft:{s}"))
        .to_vec();
    tag.insert("sherds", Value::List(List::String(sherds)));
    let mut out = Vec::new();
    mcr_nbt::write_network(&mut out, Some(&Value::Compound(tag)));
    out
}

fn assert_pot_front(mut tag: &[u8]) {
    let value = mcr_nbt::read_network(&mut tag, mcr_nbt::Limits::NETWORK)
        .unwrap()
        .unwrap();
    let mcr_nbt::Value::Compound(c) = value else {
        panic!()
    };
    let sides = c.get_compound("sherds").unwrap();
    assert_eq!(sides.len(), 1);
    let front = sides.get_compound("front").unwrap();
    assert_eq!(front.get_str("id"), Some("minecraft:angler_pottery_sherd"));
    assert_eq!(front.get_i32("count"), Some(1));
    assert!(tag.is_empty());
}

#[test]
fn pot_sherds_become_sides() {
    let pot = maps()
        .block_entity
        .back(canonical(
            "minecraft:block_entity_type",
            "minecraft:decorated_pot",
        ))
        .unwrap();
    let mut p = vec![0; 8];
    p.extend(varint(pot));
    p.extend(pot_tag());
    let (id, bytes) = inbound("block_entity_data", &p).unwrap().unwrap();
    assert_eq!(id, new_id(Play, C, "block_entity_data"));
    assert_eq!(&bytes[..8], &[0; 8]);
    assert_eq!(
        bytes[8] as i32,
        canonical("minecraft:block_entity_type", "minecraft:decorated_pot")
    );
    assert_pot_front(&bytes[9..]);

    let mut p = vec![0, 0, 0, 1, 0, 0, 0, 2];
    p.extend([0, 0]);
    p.extend(varint(1));
    p.extend([0x12, 0, 64]);
    p.extend(varint(pot));
    p.extend(pot_tag());
    p.extend([0, 0, 0, 0, 0, 0]);
    let (_, bytes) = inbound("level_chunk_with_light", &p).unwrap().unwrap();
    let tail = &bytes[8 + 2 + 1 + 3 + 1..];
    assert_pot_front(&tail[..tail.len() - 6]);
}

/// Item particles nest stacks; their depth counts from the caller, not from zero.
#[test]
fn nested_particle_items_hit_the_depth_limit() {
    let container = super::items::old_component_id("container");
    let stone = old_item("minecraft:stone");
    let item_particle = maps().particle.back(crate::types::particle::ITEM).unwrap();
    let mut nested = Vec::new();
    for _ in 0..64 {
        nested.extend(varint(stone));
        nested.extend([1, 1, 0]);
        nested.extend(varint(container));
        nested.extend([1, 1]);
    }
    nested.extend(varint(stone));
    nested.extend([1, 0, 0]);
    let mut p = varint(1);
    p.push(0);
    p.extend(varint(16));
    p.extend(varint(item_particle));
    p.extend(&nested);
    p.push(0xff);
    assert!(inbound("set_entity_data", &p).is_err());
    let mut shallow = varint(1);
    shallow.push(0);
    shallow.extend(varint(16));
    shallow.extend(varint(item_particle));
    shallow.extend(varint(stone));
    shallow.extend([1, 0, 0, 0xff]);
    assert!(inbound("set_entity_data", &shallow).unwrap().is_some());
}

/// Deeply nested text and NBT are rejected by the raw skipper without building trees.
#[test]
fn deep_text_and_nbt_fail_cleanly() {
    let mut deep = vec![mcr_nbt::tag::LIST];
    for _ in 0..100_000 {
        deep.extend([mcr_nbt::tag::LIST, 0, 0, 0, 1]);
    }
    deep.extend([0, 0, 0, 0, 0]);
    let mut p = vec![0];
    p.extend(varint(1));
    p.extend(mcr_buf::to_bytes(
        &mcr_buf::Identifier::parse("a:b").unwrap(),
    ));
    p.push(0);
    p.push(1);
    p.extend(&deep);
    assert!(inbound("update_advancements", &p).is_err());

    let bees = super::items::old_component_id("entity_data");
    let mut comps = varint(bees);
    comps.extend(varint(
        maps()
            .entity
            .back(data().entity_type_by_name("minecraft:pig").unwrap() as i32)
            .unwrap(),
    ));
    let mut nbt = vec![mcr_nbt::tag::COMPOUND];
    for _ in 0..100_000 {
        nbt.extend([mcr_nbt::tag::COMPOUND, 0, 0]);
    }
    comps.extend(&nbt);
    let p = old_stack_bytes(old_item("minecraft:pig_spawn_egg"), 1, &comps, 1);
    assert!(inbound("set_cursor_item", &p).is_err());
}

#[test]
fn nested_slot_displays_hit_the_depth_limit() {
    let mut display = vec![2; 10_000];
    display.push(0);
    let mut p = varint(1);
    p.extend(varint(1));
    p.push(3);
    p.extend(&display);
    assert!(inbound("recipe_book_add", &p).is_err());
}

#[test]
fn huge_counts_with_tiny_payloads_fail_fast() {
    let mut p = varint(0);
    p.extend(varint(0));
    p.extend(varint(i32::MAX));
    assert!(inbound("container_set_content", &p).is_err());
    let mut p = varint(1);
    p.extend(varint(old_item("minecraft:stone")));
    p.extend(varint(i32::MAX));
    p.extend(varint(i32::MAX));
    assert!(inbound("set_cursor_item", &p).is_err());
    let mut p = vec![0; 8];
    p.extend(varint(0));
    p.extend(varint(0));
    p.extend(varint(i32::MAX));
    assert!(inbound("level_chunk_with_light", &p).is_err());
}
