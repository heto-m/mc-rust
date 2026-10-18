use mcr_buf::BlockPos;
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::serverbound::{Facing, PlayerActionKind};
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{
    AdventureModePredicate, BlockPredicate, DataComponent, DataComponentMatchers,
    DataComponentPatch, HolderSet, PropertyMatcher, StatePropertiesPredicate, ValueMatcher,
};
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::game::GameType;
use mcr_world::{BlockState, ChunkColumn, ChunkPos};
use uuid::Uuid;

use crate::client::Client;
use crate::entity::{AttributeState, Entity, attribute_range};
use crate::inventory::HOTBAR_START;
use crate::use_item::{UseResult, UseTarget};

const STONE: BlockPos = BlockPos { x: 1, y: 64, z: 1 };

fn state_of(c: &Client, name: &str) -> u32 {
    let b = c.state.data.block_by_name(name).unwrap();
    c.state.data.block_info(b).unwrap().default_state
}

fn client() -> Client {
    let mut c = Client::for_test();
    let cfg = *c.state.chunks.config();
    c.state
        .chunks
        .insert(ChunkPos::new(0, 0), ChunkColumn::empty(&cfg));
    let stone = state_of(&c, "minecraft:stone");
    for x in 0..8 {
        for z in 0..8 {
            c.state.chunks.set_block(x, 63, z, BlockState(stone));
        }
    }
    c.state.chunks.set_block(1, 64, 1, BlockState(stone));
    c.state.player.physics.pos = [4.5, 64.0, 4.5];
    c.state.player.physics.on_ground = true;
    c
}

fn hold(c: &mut Client, name: &str, components: Vec<DataComponent>) {
    let item = c.state.data.item_by_name(name).unwrap() as i32;
    let mut stack = ItemStack::new(item, 4);
    stack.components = DataComponentPatch {
        added: components,
        removed: vec![],
    };
    c.state.inventory.player.slots[HOTBAR_START] = stack;
}

fn predicate(c: &Client, block: &str, props: Option<(&str, &str)>) -> AdventureModePredicate {
    let id = c
        .state
        .data
        .block_by_name(&format!("minecraft:{block}"))
        .unwrap() as i32;
    AdventureModePredicate(vec![BlockPredicate {
        blocks: Some(HolderSet::Ids(vec![id])),
        properties: props.map(|(k, v)| {
            StatePropertiesPredicate(vec![PropertyMatcher {
                name: k.to_owned(),
                matcher: ValueMatcher::Exact(v.to_owned()),
            }])
        }),
        nbt: None,
        components: DataComponentMatchers::default(),
    }])
}

fn actions(sent: &[S<'_>]) -> Vec<PlayerActionKind> {
    sent.iter()
        .filter_map(|p| match p {
            S::PlayerAction(a) => Some(a.action),
            _ => None,
        })
        .collect()
}

#[test]
fn adventure_mines_only_with_a_matching_can_break() {
    let mut c = client();
    c.state.player.game_mode = GameType::Adventure;
    hold(&mut c, "minecraft:iron_pickaxe", vec![]);
    assert!(!c.start_destroy(STONE, Facing::Up).unwrap());
    assert!(actions(&c.take_sent()).is_empty());

    let wrong = predicate(&c, "dirt", None);
    hold(
        &mut c,
        "minecraft:iron_pickaxe",
        vec![DataComponent::CanBreak(wrong)],
    );
    assert!(!c.start_destroy(STONE, Facing::Up).unwrap());

    let right = predicate(&c, "stone", None);
    hold(
        &mut c,
        "minecraft:iron_pickaxe",
        vec![DataComponent::CanBreak(right)],
    );
    assert!(c.start_destroy(STONE, Facing::Up).unwrap());
    assert_eq!(
        actions(&c.take_sent()),
        [PlayerActionKind::StartDestroyBlock]
    );

    c.state.player.game_mode = GameType::Spectator;
    assert!(c.state.block_action_restricted(STONE));
}

#[test]
fn can_break_properties_must_match() {
    let mut c = client();
    c.state.player.game_mode = GameType::Adventure;
    let p = predicate(&c, "stone", Some(("axis", "y")));
    hold(
        &mut c,
        "minecraft:iron_pickaxe",
        vec![DataComponent::CanBreak(p)],
    );
    assert!(c.state.block_action_restricted(STONE), "stone has no axis");
}

#[test]
fn adventure_places_only_on_can_place_on() {
    let mut c = client();
    c.state.player.game_mode = GameType::Adventure;
    let on_stone = UseTarget::Block {
        pos: STONE,
        face: Facing::Up,
        cursor: [0.5, 1.0, 0.5],
        inside: false,
    };
    hold(&mut c, "minecraft:dirt", vec![]);
    assert_eq!(c.start_use(on_stone).unwrap(), UseResult::Pass);
    assert_eq!(c.state.chunks.block(1, 65, 1), Some(BlockState(0)));
    let p = predicate(&c, "stone", None);
    hold(&mut c, "minecraft:dirt", vec![DataComponent::CanPlaceOn(p)]);
    assert_eq!(c.start_use(on_stone).unwrap(), UseResult::Success);
    assert_eq!(
        c.state.chunks.block(1, 65, 1),
        Some(BlockState(state_of(&c, "minecraft:dirt")))
    );
}

#[test]
fn placement_is_blocked_by_other_entities() {
    let mut c = client();
    let kind = c.state.data.entity_type_by_name("minecraft:cow").unwrap() as i32;
    let mut cow = Entity::new(9, Uuid::from_u128(9), kind, [1.5, 65.0, 1.5]);
    cow.data.insert(9, EntityDataValue::Float(10.0));
    c.state.entities.insert(cow);
    hold(&mut c, "minecraft:dirt", vec![]);
    let result = c
        .start_use(UseTarget::Block {
            pos: STONE,
            face: Facing::Up,
            cursor: [0.5, 1.0, 0.5],
            inside: false,
        })
        .unwrap();
    assert_eq!(result, UseResult::Consume, "a failed placement");
    assert_eq!(c.state.chunks.block(1, 65, 1), Some(BlockState(0)));
    assert_eq!(c.state.inventory.player.slots[HOTBAR_START].count, 4);
}

#[test]
fn bare_hand_consume_table() {
    let mut c = client();
    hold(&mut c, "minecraft:snowball", vec![]);
    c.state.inventory.player.slots[HOTBAR_START] = ItemStack::default();
    let target = UseTarget::Block {
        pos: STONE,
        face: Facing::Up,
        cursor: [0.5, 1.0, 0.5],
        inside: false,
    };
    for (block, expect) in [
        ("minecraft:potted_poppy", UseResult::Success),
        ("minecraft:flower_pot", UseResult::Consume),
        ("minecraft:dragon_egg", UseResult::Success),
        ("minecraft:redstone_wire", UseResult::Success),
        ("minecraft:stone", UseResult::Pass),
    ] {
        let s = state_of(&c, block);
        c.state.chunks.set_block(1, 64, 1, BlockState(s));
        assert_eq!(c.start_use(target).unwrap(), expect, "{block}");
    }
    c.state.player.game_mode = GameType::Adventure;
    let wire = state_of(&c, "minecraft:redstone_wire");
    c.state.chunks.set_block(1, 64, 1, BlockState(wire));
    assert_eq!(c.start_use(target).unwrap(), UseResult::Pass, "adventure");
}

/// The player breaks the block at their feet's level, steps into the gap, and the server keeps it.
fn mine_and_step_in(c: &mut Client, teleport: bool) -> [f64; 3] {
    let pos = BlockPos::new(1, 64, 1);
    c.state.player.physics.pos = [2.5, 64.0, 1.5];
    let seq = c.next_sequence();
    c.state.predict_block(pos, BlockState(0), seq);
    if teleport {
        c.state.predictions.on_teleport();
    }
    c.state.player.physics.pos = [1.5, 64.0, 1.5];
    c.state.finish_predictions(seq);
    c.state.player.physics.pos
}

#[test]
fn reverted_prediction_moves_the_player_back() {
    let mut c = client();
    assert_eq!(mine_and_step_in(&mut c, false), [2.5, 64.0, 1.5]);
    assert_eq!(
        c.state.chunks.block(1, 64, 1),
        Some(BlockState(state_of(&c, "minecraft:stone")))
    );
}

#[test]
fn no_snap_back_after_a_teleport() {
    let mut c = client();
    assert_eq!(mine_and_step_in(&mut c, true), [1.5, 64.0, 1.5]);
}

#[test]
fn a_confirmed_prediction_never_moves_the_player() {
    let mut c = client();
    let pos = BlockPos::new(1, 64, 1);
    c.state.player.physics.pos = [2.5, 64.0, 1.5];
    let seq = c.next_sequence();
    c.state.predict_block(pos, BlockState(0), seq);
    c.state.predictions.hold(pos, 0);
    c.state.player.physics.pos = [1.5, 64.0, 1.5];
    c.state.finish_predictions(seq);
    assert_eq!(c.state.player.physics.pos, [1.5, 64.0, 1.5]);
}

#[test]
fn entity_interaction_sends_the_relative_location() {
    let mut c = client();
    c.start_use(UseTarget::Entity {
        id: 7,
        location: [0.1, 1.25, -0.2],
    })
    .unwrap();
    let sent = c.take_sent();
    let at = sent.iter().find_map(|p| match p {
        S::Interact(i) => Some([i.location.x, i.location.y, i.location.z]),
        _ => None,
    });
    let at = at.expect("interact sent");
    for (got, want) in at.into_iter().zip([0.1, 1.25, -0.2]) {
        assert!((got - want).abs() < 1.0e-3, "{at:?}");
    }
}

#[test]
fn attribute_values_are_sanitised() {
    let mut a = AttributeState::new(f64::NAN, Vec::new());
    a.range = attribute_range("minecraft:step_height");
    assert_eq!(a.value(None), 0.0);
    a.base = 1.0e12;
    assert_eq!(a.value(None), 10.0);
    a.range = attribute_range("minecraft:scale");
    a.base = 0.0;
    assert_eq!(a.value(None), 0.0625);
    a.range = None;
    a.base = f64::NAN;
    assert_eq!(a.value(None), 0.0);
}

#[test]
fn hostile_step_height_reaches_physics_bounded() {
    let mut c = client();
    let id = c
        .state
        .data
        .registry("minecraft:attribute")
        .and_then(|r| r.id_of("minecraft:step_height"))
        .unwrap() as i32;
    let mut a = AttributeState::new(1.0e300, Vec::new());
    a.range = attribute_range("minecraft:step_height");
    c.state.player.attributes.insert(id, a);
    c.state.refresh_physics_inputs();
    assert_eq!(c.state.player.physics.attributes.step_height, 10.0);
}

#[test]
fn boats_collide_and_mobs_push() {
    let mut c = client();
    let boat = c
        .state
        .data
        .entity_type_by_name("minecraft:oak_boat")
        .unwrap() as i32;
    c.state
        .entities
        .insert(Entity::new(5, Uuid::from_u128(5), boat, [4.5, 64.0, 6.0]));
    let cow = c.state.data.entity_type_by_name("minecraft:cow").unwrap() as i32;
    let mut cow = Entity::new(6, Uuid::from_u128(6), cow, [4.8, 64.0, 4.5]);
    cow.data.insert(9, EntityDataValue::Float(10.0));
    c.state.entities.insert(cow);
    let mut pushers = Vec::new();
    c.state.refresh_collision_env(&mut pushers);
    assert_eq!(c.state.player.physics.entity_colliders.len(), 1);
    assert_eq!(pushers, [[4.8, 64.0, 4.5]]);
    assert!(c.state.player.physics.border.is_some());
}
