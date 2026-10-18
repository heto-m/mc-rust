use mcr_buf::{Encode, IdOr, Identifier, LengthPrefixed, VarInt, write_varint};

use super::*;
use crate::PacketIds;
use crate::types::component::{
    BlocksAttacks, DataComponent, DataComponentPatch, HolderSet, Instrument, ItemDamageFunction,
    TrimMaterial, component_type_id,
};
use crate::types::item::{ItemStack, ItemStackTemplate, UntrustedItemStack};
use crate::version::io::Depth;
use crate::version::tables::VersionTables;
use crate::version::translator::{MAX_SHIFTED_ENTITIES, Maps};
use crate::version::v26_2::items::Codec;
use crate::version::{Session, TranslateError, Translated, Wire};

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

fn old_name_id(key: &str, name: &str) -> i32 {
    translator()
        .tables()
        .old_names(key)
        .iter()
        .position(|n| n.strip_prefix("minecraft:") == Some(name))
        .unwrap_or_else(|| panic!("no 1.21.11 {name} in {key}")) as i32
}

fn old_id(state: ProtocolState, direction: Direction, name: &str) -> i32 {
    let key = VersionTables::packet_key(
        ["handshake", "status", "login", "configuration", "play"][state as usize],
        ["clientbound", "serverbound"][direction as usize],
    );
    if translator().tables().old_names(&key).is_empty() {
        return new_id(state, direction, name);
    }
    old_name_id(&key, name)
}

fn new_id(state: ProtocolState, direction: Direction, name: &str) -> i32 {
    PacketIds::v26_3()
        .id_by_name(state, direction, name)
        .unwrap()
}

fn old_component(name: &str) -> i32 {
    old_name_id("registry/minecraft:data_component_type", name)
}

fn new_item(name: &str) -> i32 {
    data().item_by_name(&format!("minecraft:{name}")).unwrap() as i32
}

fn old_item(name: &str) -> i32 {
    maps().item.back(new_item(name)).unwrap()
}

fn old_entity(name: &str) -> i32 {
    let new = data()
        .entity_type_by_name(&format!("minecraft:{name}"))
        .unwrap() as i32;
    maps().entity.back(new).unwrap()
}

fn varint(v: i32) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, v);
    out
}

fn decode<'a, T: mcr_buf::Decode<'a>>(bytes: &'a [u8]) -> T {
    mcr_buf::from_bytes_exact(bytes).unwrap()
}

fn frames(t: Translated<'_>) -> Vec<(i32, Vec<u8>)> {
    match t {
        Translated::Packet(id, bytes) => vec![(id, bytes.to_vec())],
        Translated::Packets(frames) => frames.map(|(id, b)| (id, b.to_vec())).collect(),
        Translated::Drop => Vec::new(),
    }
}

fn inbound_with(
    session: &mut Session,
    name: &str,
    payload: &[u8],
) -> std::result::Result<Vec<(i32, Vec<u8>)>, TranslateError> {
    let mut out = Vec::new();
    let t = wire().inbound(Play, old_id(Play, C, name), payload, session, &mut out)?;
    Ok(frames(t))
}

fn inbound(name: &str, payload: &[u8]) -> std::result::Result<Vec<u8>, TranslateError> {
    let mut all = inbound_with(&mut Session::default(), name, payload)?;
    assert_eq!(all.len(), 1, "{name} became {} packets", all.len());
    let (id, bytes) = all.remove(0);
    assert_eq!(id, new_id(Play, C, name));
    Ok(bytes)
}

fn outbound_with(session: &mut Session, name: &str, payload: &[u8]) -> Vec<(i32, Vec<u8>)> {
    let mut out = Vec::new();
    let t = wire()
        .outbound(Play, new_id(Play, S, name), payload, session, &mut out)
        .unwrap();
    frames(t)
}

/// 1.21.11's non-empty stack: count, item, patch.
fn old_stack(item: &str, count: i32, added: &[(i32, Vec<u8>)]) -> Vec<u8> {
    let mut b = varint(count);
    b.extend(varint(old_item(item)));
    b.extend(varint(added.len() as i32));
    b.extend(varint(0));
    for (id, value) in added {
        b.extend(varint(*id));
        b.extend(value);
    }
    b
}

fn template(item: &str, count: i32) -> ItemStackTemplate {
    ItemStackTemplate {
        item: new_item(item),
        count,
        components: Default::default(),
    }
}

fn cursor(stack: &[u8]) -> ItemStack {
    let bytes = inbound("set_cursor_item", stack).unwrap();
    decode::<crate::play::clientbound::ui::SetCursorItem>(&bytes).contents
}

#[test]
fn tables_load() {
    let t = translator();
    assert_eq!((t.version_name(), t.protocol()), ("1.21.11", 774));
    assert_eq!(maps().era, Era::Stacks);
    assert!(maps().old_swing_animation.is_some());
    assert!(maps().old_map_color.is_some());
}

#[test]
fn registries_shift_against_26_3() {
    assert_eq!(maps().item.back(new_item("golden_dandelion")), None);
    assert_ne!(old_item("stone"), -1);
    let team = canonical("minecraft:command_argument_type", "minecraft:team_color");
    let color = old_name_id("registry/minecraft:command_argument_type", "color");
    assert_eq!(maps().argument.forward(color), Some(team));
    // 1.21.11's item stack display is its fourth slot display type, 26.3's sixth.
    assert_eq!(maps().slot_display.forward(3), Some(5));
    assert_eq!(maps().slot_display.forward(7), Some(10));
}

#[test]
fn set_time_becomes_a_clock_update() {
    use crate::play::clientbound::world::{ClockUpdate, SetTime};
    let mut p = 1234i64.to_be_bytes().to_vec();
    p.extend(6000i64.to_be_bytes());
    p.push(1);
    let time: SetTime = decode(&inbound("set_time", &p).unwrap());
    assert_eq!(time.game_time, 1234);
    assert_eq!(
        time.clock_updates,
        [ClockUpdate {
            clock: 0,
            total_ticks: 6000,
            partial_tick: 0.0,
            rate: 1.0
        }]
    );
    *p.last_mut().unwrap() = 0;
    let time: SetTime = decode(&inbound("set_time", &p).unwrap());
    assert_eq!(time.clock_updates[0].rate, 0.0);
    p.pop();
    assert!(inbound("set_time", &p).is_err());
}

#[test]
fn containers_hold_stacks_count_first() {
    let mut container = varint(3);
    container.extend(old_stack("diamond", 3, &[]));
    container.push(0);
    container.extend(old_stack("stone", 64, &[]));
    let stack = cursor(&old_stack(
        "shulker_box",
        1,
        &[(old_component("container"), container)],
    ));
    let DataComponent::Container(c) = &stack.components.added[0] else {
        panic!("{:?}", stack.components.added)
    };
    assert_eq!(
        c.0,
        [
            Some(template("diamond", 3)),
            None,
            Some(template("stone", 64))
        ]
    );
}

#[test]
fn bundles_projectiles_and_remainders_become_templates() {
    let mut bundle = varint(2);
    bundle.extend(old_stack("apple", 5, &[]));
    bundle.extend(old_stack("arrow", 1, &[]));
    let mut charged = varint(1);
    charged.extend(old_stack("firework_rocket", 1, &[]));
    let remainder = old_stack("bowl", 1, &[]);
    let stack = cursor(&old_stack(
        "bundle",
        1,
        &[
            (old_component("bundle_contents"), bundle),
            (old_component("charged_projectiles"), charged),
            (old_component("use_remainder"), remainder),
        ],
    ));
    let added = &stack.components.added;
    assert_eq!(
        added[0],
        DataComponent::BundleContents(vec![template("apple", 5), template("arrow", 1)])
    );
    assert_eq!(
        added[1],
        DataComponent::ChargedProjectiles(LengthPrefixed(vec![template("firework_rocket", 1)]))
    );
    assert_eq!(added[2], DataComponent::UseRemainder(template("bowl", 1)));
    // A template cannot be empty; 1.21.11's non-optional stacks never were.
    let mut empty = varint(1);
    empty.push(0);
    assert!(
        inbound(
            "set_cursor_item",
            &old_stack("bundle", 1, &[(old_component("bundle_contents"), empty)])
        )
        .is_err()
    );
}

#[test]
fn either_holders_drop_their_key_form() {
    let horn = 3;
    let mut holder = vec![1];
    holder.extend(varint(horn + 1));
    let mut key = vec![0];
    key.extend(mcr_buf::to_bytes(
        &Identifier::parse("minecraft:ponder_goat_horn").unwrap(),
    ));
    let stack = cursor(&old_stack(
        "goat_horn",
        1,
        &[
            (old_component("instrument"), holder),
            (old_component("damage"), varint(3)),
        ],
    ));
    assert_eq!(
        stack.components.added,
        [
            DataComponent::Instrument(IdOr::Id(horn)),
            DataComponent::Damage(VarInt(3))
        ]
    );
    let stack = cursor(&old_stack(
        "goat_horn",
        1,
        &[
            (old_component("instrument"), key),
            (old_component("damage"), varint(3)),
        ],
    ));
    assert_eq!(stack.components.added, [DataComponent::Damage(VarInt(3))]);
    let mut variant = vec![1];
    variant.extend(varint(2));
    let stack = cursor(&old_stack(
        "egg",
        1,
        &[(old_component("chicken/variant"), variant)],
    ));
    assert_eq!(
        stack.components.added,
        [DataComponent::ChickenVariant(VarInt(2))]
    );
}

#[test]
fn tag_keys_become_holder_sets() {
    let tag = Identifier::parse("minecraft:is_fire").unwrap();
    let mut blocks = 0.5f32.to_be_bytes().to_vec();
    blocks.extend(1f32.to_be_bytes());
    blocks.push(0);
    for v in [1f32, 0.0, 1.0] {
        blocks.extend(v.to_be_bytes());
    }
    blocks.push(1);
    blocks.extend(mcr_buf::to_bytes(&tag));
    blocks.extend([0, 0]);
    let stack = cursor(&old_stack(
        "shield",
        1,
        &[
            (old_component("damage_resistant"), mcr_buf::to_bytes(&tag)),
            (old_component("blocks_attacks"), blocks),
        ],
    ));
    let added = &stack.components.added;
    assert_eq!(
        added[0],
        DataComponent::DamageResistant(HolderSet::Tag(tag.clone()))
    );
    let DataComponent::BlocksAttacks(b) = &added[1] else {
        panic!()
    };
    assert_eq!(b.bypassed_by, Some(HolderSet::Tag(tag)));
}

fn patch_bytes(added: Vec<DataComponent>) -> Vec<u8> {
    let mut out = Vec::new();
    DataComponentPatch {
        added,
        removed: vec![component_type_id("food").unwrap()],
    }
    .encode_with(&mut out, false);
    out
}

/// Every value 26.1 reshaped, canonical to 1.21.11 and back.
#[test]
fn reshaped_values_round_trip_through_1_21_11() {
    let tag = Identifier::parse("minecraft:is_fire").unwrap();
    let mut boxed = template("stone", 2);
    boxed.components.added = vec![DataComponent::Damage(VarInt(1))];
    let sound = IdOr::Id(canonical(
        "minecraft:sound_event",
        "minecraft:block.stone.break",
    ));
    let canonical_patch = patch_bytes(vec![
        DataComponent::Container(LengthPrefixed(vec![Some(boxed.clone()), None])),
        DataComponent::BundleContents(vec![boxed.clone()]),
        DataComponent::ChargedProjectiles(LengthPrefixed(vec![template("arrow", 1)])),
        DataComponent::UseRemainder(template("bowl", 1)),
        DataComponent::Instrument(IdOr::Inline(Instrument {
            sound_event: sound,
            use_duration: 7.0,
            range: 256.0,
            durability_damage: 0,
            description: crate::types::Component::text("Horn"),
        })),
        DataComponent::ProvidesTrimMaterial(IdOr::Inline(TrimMaterial {
            palette_id: Identifier::parse("minecraft:trim/gold").unwrap(),
            description: crate::types::Component::text("Gold"),
        })),
        DataComponent::JukeboxPlayable(IdOr::Id(4)),
        DataComponent::ChickenVariant(VarInt(1)),
        DataComponent::ZombieNautilusVariant(VarInt(0)),
        DataComponent::DamageType(VarInt(5)),
        DataComponent::DamageResistant(HolderSet::Tag(tag.clone())),
        DataComponent::ProvidesBannerPatterns(HolderSet::Tag(tag.clone())),
        DataComponent::BlocksAttacks(BlocksAttacks {
            block_delay_seconds: 0.25,
            disable_cooldown_scale: 1.0,
            damage_reductions: Vec::new(),
            item_damage: ItemDamageFunction {
                threshold: 1.0,
                base: 0.0,
                factor: 1.0,
            },
            bypassed_by: Some(HolderSet::Tag(tag)),
            block_sound: None,
            disable_sound: None,
        }),
    ]);
    let (fwd, back) = (Codec::fwd(maps()), Codec::back(maps()));
    let mut old = Vec::new();
    back.patch(&mut canonical_patch.as_slice(), &mut old, Depth::top())
        .unwrap();
    let mut again = Vec::new();
    fwd.patch(&mut old.as_slice(), &mut again, Depth::top())
        .unwrap();
    assert_eq!(again, canonical_patch);
}

#[test]
fn creative_stacks_go_back_as_1_21_11_stacks() {
    let mut stack = ItemStack::new(new_item("shulker_box"), 1);
    stack.components.added = vec![
        DataComponent::Container(LengthPrefixed(vec![None, Some(template("diamond", 3))])),
        DataComponent::DamageResistant(HolderSet::Ids(vec![1, 2])),
    ];
    let mut p = vec![0, 5];
    p.extend(mcr_buf::to_bytes(&UntrustedItemStack(stack)));
    let mut session = Session::default();
    let out = outbound_with(&mut session, "set_creative_mode_slot", &p);
    let mut container = varint(2);
    container.push(0);
    container.extend(old_stack("diamond", 3, &[]));
    let mut expected = vec![0, 5, 1];
    expected.extend(varint(old_item("shulker_box")));
    // The direct holder set has no tag key, so damage resistance is left out.
    expected.extend([1, 0]);
    expected.extend(varint(old_component("container")));
    expected.extend(varint(container.len() as i32));
    expected.extend(container);
    assert_eq!(out, [(old_id(Play, S, "set_creative_mode_slot"), expected)]);
}

#[test]
fn item_particles_and_advancement_icons_were_stacks() {
    let particle = maps().particle.back(crate::types::particle::ITEM).unwrap();
    let mut p = varint(particle);
    p.extend(old_stack("diamond", 1, &[]));
    let mut out = Vec::new();
    crate::version::v26_2::world::particle(maps(), &mut p.as_slice(), &mut out, Depth::top())
        .unwrap();
    let mut expected = varint(crate::types::particle::ITEM);
    expected.extend(mcr_buf::to_bytes(&template("diamond", 1)));
    assert_eq!(out, expected);

    let mut p = vec![0];
    p.extend(varint(1));
    p.extend(mcr_buf::to_bytes(&Identifier::parse("a:b").unwrap()));
    p.push(0);
    p.push(1);
    p.extend(mcr_buf::to_bytes(&crate::types::Component::text("T")));
    p.extend(mcr_buf::to_bytes(&crate::types::Component::text("D")));
    p.extend(old_stack("diamond", 1, &[]));
    p.extend(varint(0));
    p.extend(0i32.to_be_bytes());
    p.extend([0; 8]);
    p.extend(varint(0));
    p.push(0);
    p.extend(varint(0));
    p.extend(varint(0));
    p.push(0);
    let bytes = inbound("update_advancements", &p).unwrap();
    let adv: crate::play::clientbound::ui::UpdateAdvancements = decode(&bytes);
    let icon = &adv.added[0].advancement.display.as_ref().unwrap().icon;
    assert_eq!(*icon, template("diamond", 1));
}

#[test]
fn slot_display_ids_shift() {
    // A stonecutter recipe: input tag display, result item stack display, station item display.
    let mut p = varint(1);
    p.extend(varint(7));
    p.extend(varint(3));
    p.extend(varint(4));
    p.extend(mcr_buf::to_bytes(
        &Identifier::parse("minecraft:logs").unwrap(),
    ));
    p.extend(varint(3));
    p.extend(old_stack("stick", 4, &[]));
    p.extend(varint(2));
    p.extend(varint(old_item("stonecutter")));
    p.extend(varint(0));
    p.extend(varint(0));
    p.push(0);
    p.push(0);
    p.push(0);
    let bytes = inbound("recipe_book_add", &p).unwrap();
    let mut expected = varint(1);
    expected.extend(varint(7));
    expected.extend(varint(3));
    expected.extend(varint(6));
    expected.extend(varint(0));
    expected.extend(mcr_buf::to_bytes(
        &Identifier::parse("minecraft:logs").unwrap(),
    ));
    expected.extend(varint(5));
    expected.extend(mcr_buf::to_bytes(&template("stick", 4)));
    expected.extend(varint(4));
    expected.extend(varint(new_item("stonecutter")));
    expected.extend(varint(0));
    expected.extend(varint(0));
    expected.extend([0, 0, 0]);
    assert_eq!(bytes, expected);
}

fn spawn(session: &mut Session, id: i32, kind: &str) {
    let mut p = varint(id);
    p.extend([0; 16]);
    p.extend(varint(old_entity(kind)));
    p.extend([0; 24]);
    p.push(0);
    p.extend([0; 3]);
    p.extend(varint(0));
    inbound_with(session, "add_entity", &p).unwrap();
}

fn entity_data(session: &mut Session, id: i32, entries: &[(u8, i32, &[u8])]) -> Vec<u8> {
    let mut p = varint(id);
    for (index, serializer, value) in entries {
        p.push(*index);
        p.extend(varint(*serializer));
        p.extend(*value);
    }
    p.push(0xff);
    inbound_with(session, "set_entity_data", &p)
        .unwrap()
        .remove(0)
        .1
}

#[test]
fn ageable_indices_shift_past_age_locked() {
    let mut session = Session::default();
    spawn(&mut session, 7, "sheep");
    spawn(&mut session, 8, "zombie");
    // Sheep: baby flag at 16, wool byte at 17.
    let sheep = entity_data(&mut session, 7, &[(16, 8, &[1]), (17, 0, &[3])]);
    let mut expected = varint(7);
    expected.extend([16, 8, 1, 18, 0, 3, 0xff]);
    assert_eq!(sheep, expected);
    let zombie = entity_data(&mut session, 8, &[(17, 1, &[5])]);
    let mut expected = varint(8);
    expected.extend([17, 1, 5, 0xff]);
    assert_eq!(zombie, expected);
    // Index 254 would become the end marker.
    let mut p = varint(7);
    p.extend([254, 0, 1, 0xff]);
    assert!(inbound_with(&mut session, "set_entity_data", &p).is_err());

    let mut remove = varint(1);
    remove.extend(varint(7));
    inbound_with(&mut session, "remove_entities", &remove).unwrap();
    let sheep = entity_data(&mut session, 7, &[(17, 0, &[3])]);
    assert_eq!(&sheep[1..], [17, 0, 3, 0xff]);
}

#[test]
fn respawn_forgets_tracked_entities() {
    let mut session = Session::default();
    spawn(&mut session, 9, "cow");
    let spawn_info = crate::play::clientbound::player::CommonPlayerSpawnInfo {
        dimension_type: 0,
        dimension: Identifier::parse("minecraft:overworld").unwrap(),
        seed: 1,
        game_type: crate::types::game::GameType::Survival,
        previous_game_type: None,
        is_debug: false,
        is_flat: false,
        last_death_location: None,
        portal_cooldown: 0,
        sea_level: 63,
    };
    let mut p = mcr_buf::to_bytes(&spawn_info);
    let prev = p.len() - 1 - 2 - 1 - 1 - 1;
    p[prev] = 0xff;
    p.push(0);
    inbound_with(&mut session, "respawn", &p).unwrap();
    assert!(session.state.shifted_entities.is_empty());
}

#[test]
fn shifted_serializers_remap() {
    let mut session = Session::default();
    spawn(&mut session, 3, "cow");
    // Cow variant: 1.21.11 serializer 22 at index 17; 26.3 serializer 23 at index 18.
    let cow = entity_data(&mut session, 3, &[(17, 22, &[2])]);
    assert_eq!(&cow[1..], [18, 23, 2, 0xff]);
    // Humanoid arm (38 -> 42), quaternion (36 -> 40) on a non-ageable entity.
    let mut quat = Vec::new();
    for v in [0f32, 0.0, 0.0, 1.0] {
        quat.extend(v.to_be_bytes());
    }
    let out = entity_data(&mut session, 4, &[(15, 38, &[1]), (13, 36, &quat)]);
    let mut expected = varint(4);
    expected.extend([15, 42, 1, 13, 40]);
    expected.extend(&quat);
    expected.push(0xff);
    assert_eq!(out, expected);
    let mut p = varint(4);
    p.extend([8, 39, 0, 0xff]);
    assert!(inbound_with(&mut session, "set_entity_data", &p).is_err());
}

#[test]
fn tracked_entities_are_capped() {
    let mut session = Session::default();
    for id in 0..MAX_SHIFTED_ENTITIES as i32 + 100 {
        session.state.track_shifted(id);
    }
    assert_eq!(session.state.shifted_entities.len(), MAX_SHIFTED_ENTITIES);
}

#[test]
fn interact_splits_into_interact_at_and_interact() {
    use crate::play::serverbound::Interact;
    let packet = Interact {
        entity_id: 42,
        hand: crate::types::game::Hand::Off,
        location: crate::types::Vec3d {
            x: 0.5,
            y: 1.25,
            z: -0.5,
        },
        using_secondary_action: true,
    };
    let bytes = mcr_buf::to_bytes(&packet);
    let out = outbound_with(&mut Session::default(), "interact", &bytes);
    let sent: Interact = decode(&bytes);
    let id = old_id(Play, S, "interact");
    let mut at = varint(42);
    at.extend(varint(2));
    for v in [sent.location.x, sent.location.y, sent.location.z] {
        at.extend((v as f32).to_be_bytes());
    }
    at.extend([1, 1]);
    assert_eq!(out, [(id, at), (id, vec![42, 0, 1, 1])]);
}

#[test]
fn attacks_and_spectating_use_the_attack_action() {
    let id = old_id(Play, S, "interact");
    let mut session = Session::default();
    assert_eq!(
        outbound_with(&mut session, "attack", &[42]),
        [(id, vec![42, 1, 0])]
    );
    let input = outbound_with(&mut session, "player_input", &[0x20]);
    assert_eq!(input, [(old_id(Play, S, "player_input"), vec![0x20])]);
    assert_eq!(
        outbound_with(&mut session, "attack", &[42]),
        [(id, vec![42, 1, 1])]
    );
    assert_eq!(
        outbound_with(&mut session, "spectator_action", &[1, 9]),
        [(id, vec![9, 1, 1])]
    );
    assert!(outbound_with(&mut session, "spectator_action", &[0]).is_empty());
}

#[test]
fn game_rule_traffic_is_dropped() {
    let mut session = Session::default();
    assert!(outbound_with(&mut session, "client_command", &[2]).is_empty());
    assert_eq!(
        outbound_with(&mut session, "client_command", &[0]),
        [(old_id(Play, S, "client_command"), vec![0])]
    );
    let mut p = varint(1);
    mcr_buf::encode_str("minecraft:keep_inventory", &mut p);
    mcr_buf::encode_str("true", &mut p);
    assert!(outbound_with(&mut session, "set_game_rule", &p).is_empty());
}

#[test]
fn known_core_pack_defaults_to_1_21_11() {
    let pack = crate::types::registry::KnownPack::vanilla("core", "26.3");
    let mut p = varint(1);
    pack.encode(&mut p);
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
    let back: LengthPrefixed<crate::types::registry::KnownPack, 64> = decode(&bytes);
    assert_eq!(back.0[0].version, "1.21.11");
}

/// Containers of 1.21.11 stacks nest like 26.1's templates and hit the same depth limit.
#[test]
fn nested_containers_hit_the_depth_limit() {
    let container = old_component("container");
    let nest = |levels: usize| {
        let mut inner = old_stack("stone", 1, &[]);
        for _ in 0..levels {
            let mut value = varint(1);
            value.extend(&inner);
            inner = old_stack("shulker_box", 1, &[(container, value)]);
        }
        inner
    };
    assert!(inbound("set_cursor_item", &nest(8)).is_ok());
    assert!(inbound("set_cursor_item", &nest(64)).is_err());
    let particle = maps().particle.back(crate::types::particle::ITEM).unwrap();
    let mut p = varint(1);
    p.push(0);
    p.extend(varint(16));
    p.extend(varint(particle));
    p.extend(nest(64));
    p.push(0xff);
    assert!(inbound("set_entity_data", &p).is_err());
}

#[test]
fn huge_counts_with_tiny_payloads_fail_fast() {
    let mut p = varint(i32::MAX);
    p.extend(varint(1));
    assert!(inbound("remove_entities", &p).is_err());
    let mut value = varint(i32::MAX);
    value.extend(old_stack("stone", 1, &[]));
    let p = old_stack("bundle", 1, &[(old_component("bundle_contents"), value)]);
    assert!(inbound("set_cursor_item", &p).is_err());
    let mut value = varint(200);
    value.push(0);
    let p = old_stack("shulker_box", 1, &[(old_component("container"), value)]);
    assert!(inbound("set_cursor_item", &p).is_err());
}

#[test]
fn hostile_payloads_never_panic() {
    let mut seed = 0x6a09_e667_f3bc_c908u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for rule in v26_2::RULES.iter().chain(v26_1_2::RULES).chain(RULES) {
        for len in [0usize, 1, 3, 8, 40, 300] {
            for _ in 0..40 {
                let payload: Vec<u8> = (0..len).map(|_| next() as u8).collect();
                let (mut out, mut session) = (Vec::new(), Session::default());
                session.state.track_shifted(1);
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

fn new_state(block: &str) -> i32 {
    let b = data().block_by_name(block).unwrap();
    data().block_info(b).unwrap().default_state as i32
}

fn old_state(block: &str) -> i32 {
    maps().states.back(new_state(block)).unwrap()
}

/// 1.21.11 sections carry only a block count; 26.1 added the fluid count after it.
#[test]
fn chunk_sections_gain_fluid_counts() {
    use mcr_world::{ChunkData, WorldConfig};
    let (stone, water) = (old_state("minecraft:stone"), old_state("minecraft:water"));
    let mut sections = Vec::new();
    // Single value: all water.
    sections.extend([0x10, 0, 0]);
    sections.extend(varint(water));
    sections.extend([0, 0]);
    // Indirect, 4 bits: stone then water alternating.
    sections.extend([0x10, 0, 4]);
    sections.extend(varint(2));
    sections.extend(varint(stone));
    sections.extend(varint(water));
    for _ in 0..256 {
        sections.extend(0x1010_1010_1010_1010u64.to_be_bytes());
    }
    sections.extend([0, 0]);
    // Direct, 15 bits: one long of water entries, the rest stone.
    sections.extend([0x10, 0, 15]);
    let word = |s: i32| (0..4).fold(0u64, |w, k| w | (s as u64) << (k * 15));
    sections.extend(word(water).to_be_bytes());
    for _ in 1..1024 {
        sections.extend(word(stone).to_be_bytes());
    }
    sections.extend([0, 0]);
    // Single value: stone, no fluid.
    sections.extend([0x10, 0, 0]);
    sections.extend(varint(stone));
    sections.extend([0, 0]);
    let mut payload = vec![0, 0, 0, 1, 0, 0, 0, 2];
    payload.extend(varint(0));
    payload.extend(varint(sections.len() as i32));
    payload.extend(&sections);
    payload.extend(varint(0));
    payload.extend([0, 0, 0, 0, 0, 0]);
    let bytes = inbound("level_chunk_with_light", &payload).unwrap();
    let cfg = WorldConfig::new(data().state_count() as u32, 64, 0, 64).unwrap();
    let chunk = ChunkData::decode(&mut &bytes[8..], &cfg).unwrap();
    let counts: Vec<u16> = chunk.sections[..4]
        .iter()
        .map(|s| s.fluid_count())
        .collect();
    assert_eq!(counts, [4096, 2048, 4, 0]);
    assert_eq!(
        chunk.sections[1].block(1, 0, 0).0,
        new_state("minecraft:water") as u16
    );
}

/// 26.1's loom tags name item sets 1.21.11 hard-coded; the client's loom rules read them.
#[test]
fn loom_item_tags_are_synthesized() {
    let mut p = varint(1);
    mcr_buf::encode_str("minecraft:item", &mut p);
    p.extend(varint(1));
    p.extend(mcr_buf::to_bytes(
        &Identifier::parse("minecraft:planks").unwrap(),
    ));
    p.extend(varint(1));
    p.extend(varint(old_item("oak_planks")));
    let mut out = Vec::new();
    let t = wire()
        .inbound(
            ProtocolState::Configuration,
            old_id(ProtocolState::Configuration, C, "update_tags"),
            &p,
            &mut Session::default(),
            &mut out,
        )
        .unwrap();
    let (_, bytes) = frames(t).remove(0);
    let tags: crate::common::UpdateTags = decode(&bytes);
    let item = &tags.registries[0];
    let names: Vec<&str> = item.tags.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "minecraft:planks",
            "minecraft:dyes",
            "minecraft:loom_dyes",
            "minecraft:loom_patterns"
        ]
    );
    assert_eq!(item.tags[0].ids.0, [new_item("oak_planks")]);
    assert_eq!(item.tags[2].ids.0.len(), 16);
    assert!(
        item.tags[3]
            .ids
            .0
            .contains(&new_item("creeper_banner_pattern"))
    );
}
