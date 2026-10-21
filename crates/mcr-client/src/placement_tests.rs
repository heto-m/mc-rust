use mcr_buf::BlockPos;
use mcr_nbt::{Compound, List, NetworkCompound, Value};
use mcr_protocol::play::serverbound::Facing;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{
    AdventureModePredicate, BlockPredicate, DataComponent, DataComponentMatchers,
    DataComponentPatch, HolderSet,
};
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::game::GameType;
use mcr_world::{BlockState, ChunkColumn, ChunkPos};
use uuid::Uuid;

use crate::client::Client;
use crate::entity::Entity;
use crate::inventory::HOTBAR_START;
use crate::placement::{horizontal_direction, nearest_looking};
use crate::use_item::{UseResult, UseTarget};

const STONE: BlockPos = BlockPos { x: 1, y: 64, z: 1 };

fn default_of(c: &Client, name: &str) -> u32 {
    let b = c.state.data.block_by_name(name).unwrap();
    c.state.data.block_info(b).unwrap().default_state
}

fn props(c: &Client, pos: BlockPos) -> Vec<(String, String)> {
    let s = c.state.chunks.block(pos.x, pos.y, pos.z).unwrap().0;
    c.state
        .data
        .state_properties(s)
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn prop(c: &Client, pos: BlockPos, key: &str) -> String {
    props(c, pos)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
        .unwrap_or_default()
}

fn block_at(c: &Client, pos: BlockPos) -> String {
    let s = c.state.chunks.block(pos.x, pos.y, pos.z).unwrap().0;
    c.state.data.state_block_name(s).unwrap().to_owned()
}

fn client() -> Client {
    let mut c = Client::for_test();
    let cfg = *c.state.chunks.config();
    c.state
        .chunks
        .insert(ChunkPos::new(0, 0), ChunkColumn::empty(&cfg));
    let stone = default_of(&c, "minecraft:stone");
    for x in 0..8 {
        for z in 0..8 {
            c.state.chunks.set_block(x, 63, z, BlockState(stone));
        }
    }
    c.state.chunks.set_block(1, 64, 1, BlockState(stone));
    c.state.player.physics.pos = [5.5, 64.0, 5.5];
    c.state.player.physics.on_ground = true;
    c
}

fn hold(c: &mut Client, name: &str, components: Vec<DataComponent>) {
    let item = c.state.data.item_by_name(name).unwrap() as i32;
    let mut stack = ItemStack::new(item, 8);
    stack.components = DataComponentPatch {
        added: components,
        removed: vec![],
    };
    c.state.inventory.player.slots[HOTBAR_START] = stack;
}

fn click(c: &mut Client, pos: BlockPos, face: Facing, cursor: [f32; 3]) -> UseResult {
    c.start_use(UseTarget::Block {
        pos,
        face,
        cursor,
        inside: false,
    })
    .unwrap()
}

const EAST_OF_STONE: BlockPos = BlockPos { x: 2, y: 64, z: 1 };
const ABOVE_STONE: BlockPos = BlockPos { x: 1, y: 65, z: 1 };

#[test]
fn directions_follow_the_view() {
    assert_eq!(horizontal_direction(0.0), Facing::South);
    assert_eq!(horizontal_direction(90.0), Facing::West);
    assert_eq!(horizontal_direction(180.0), Facing::North);
    assert_eq!(horizontal_direction(-90.0), Facing::East);
    assert_eq!(horizontal_direction(f32::NAN), Facing::South);
    assert_eq!(nearest_looking(0.0, 80.0), Facing::Down);
    assert_eq!(nearest_looking(0.0, -80.0), Facing::Up);
    assert_eq!(nearest_looking(-90.0, 10.0), Facing::East);
}

#[test]
fn stair_facing_and_half_come_from_the_hit() {
    let mut c = client();
    c.state.player.physics.yaw = 180.0;
    hold(&mut c, "minecraft:oak_stairs", vec![]);
    assert_eq!(
        click(&mut c, STONE, Facing::East, [1.0, 0.75, 0.5]),
        UseResult::Success
    );
    assert_eq!(block_at(&c, EAST_OF_STONE), "minecraft:oak_stairs");
    assert_eq!(prop(&c, EAST_OF_STONE, "facing"), "north");
    assert_eq!(prop(&c, EAST_OF_STONE, "half"), "top");
    assert_eq!(prop(&c, EAST_OF_STONE, "shape"), "straight");
    assert_eq!(c.state.inventory.player.slots[HOTBAR_START].count, 7);

    c.state.player.physics.yaw = -90.0;
    assert_eq!(
        click(&mut c, STONE, Facing::Up, [0.5, 1.0, 0.5]),
        UseResult::Success
    );
    assert_eq!(prop(&c, ABOVE_STONE, "facing"), "east");
    assert_eq!(prop(&c, ABOVE_STONE, "half"), "bottom");
}

#[test]
fn stairs_take_corner_shapes_from_neighbours() {
    let mut c = client();
    let ground = BlockPos::new(3, 63, 3);
    hold(&mut c, "minecraft:oak_stairs", vec![]);
    c.state.player.physics.yaw = 180.0;
    click(&mut c, ground, Facing::Up, [0.5, 1.0, 0.5]);
    let first = BlockPos::new(3, 64, 3);
    assert_eq!(prop(&c, first, "facing"), "north");
    c.state.player.physics.yaw = -90.0;
    click(&mut c, BlockPos::new(2, 63, 3), Facing::Up, [0.5, 1.0, 0.5]);
    let second = BlockPos::new(2, 64, 3);
    assert_eq!(prop(&c, second, "facing"), "east");
    assert_eq!(prop(&c, second, "shape"), "outer_left");
}

#[test]
fn slab_onto_slab_merges() {
    let mut c = client();
    hold(&mut c, "minecraft:oak_slab", vec![]);
    click(&mut c, STONE, Facing::Up, [0.5, 1.0, 0.5]);
    assert_eq!(prop(&c, ABOVE_STONE, "type"), "bottom");
    assert_eq!(
        click(&mut c, ABOVE_STONE, Facing::Up, [0.5, 0.5, 0.5]),
        UseResult::Success
    );
    assert_eq!(prop(&c, ABOVE_STONE, "type"), "double");
    assert_eq!(
        c.state.chunks.block(1, 66, 1),
        Some(BlockState(0)),
        "merged in place"
    );

    click(&mut c, STONE, Facing::East, [1.0, 0.8, 0.5]);
    assert_eq!(prop(&c, EAST_OF_STONE, "type"), "top");
    click(&mut c, EAST_OF_STONE, Facing::Down, [0.5, 0.5, 0.5]);
    assert_eq!(prop(&c, EAST_OF_STONE, "type"), "double");
}

#[test]
fn a_different_slab_does_not_merge() {
    let mut c = client();
    hold(&mut c, "minecraft:oak_slab", vec![]);
    click(&mut c, STONE, Facing::Up, [0.5, 1.0, 0.5]);
    hold(&mut c, "minecraft:stone_slab", vec![]);
    click(&mut c, ABOVE_STONE, Facing::Up, [0.5, 0.5, 0.5]);
    assert_eq!(prop(&c, ABOVE_STONE, "type"), "bottom");
    assert_eq!(
        block_at(&c, BlockPos::new(1, 66, 1)),
        "minecraft:stone_slab"
    );
}

#[test]
fn log_axis_follows_the_clicked_face() {
    let mut c = client();
    hold(&mut c, "minecraft:oak_log", vec![]);
    click(&mut c, STONE, Facing::East, [1.0, 0.5, 0.5]);
    assert_eq!(prop(&c, EAST_OF_STONE, "axis"), "x");
    click(&mut c, STONE, Facing::Up, [0.5, 1.0, 0.5]);
    assert_eq!(prop(&c, ABOVE_STONE, "axis"), "y");
    click(&mut c, STONE, Facing::South, [0.5, 0.5, 1.0]);
    assert_eq!(prop(&c, BlockPos::new(1, 64, 2), "axis"), "z");
}

#[test]
fn facing_families() {
    let mut c = client();
    c.state.player.physics.yaw = 0.0;
    hold(&mut c, "minecraft:furnace", vec![]);
    click(&mut c, STONE, Facing::Up, [0.5, 1.0, 0.5]);
    assert_eq!(prop(&c, ABOVE_STONE, "facing"), "north");
    c.state.player.physics.pitch = 80.0;
    hold(&mut c, "minecraft:piston", vec![]);
    click(&mut c, STONE, Facing::East, [1.0, 0.5, 0.5]);
    assert_eq!(prop(&c, EAST_OF_STONE, "facing"), "up");
}

#[test]
fn obstruction_uses_the_placement_state() {
    let mut c = client();
    let kind = c
        .state
        .data
        .entity_type_by_name("minecraft:chicken")
        .unwrap() as i32;
    let mut chicken = Entity::new(9, Uuid::from_u128(9), kind, [2.5, 64.55, 1.5]);
    chicken.data.insert(9, EntityDataValue::Float(4.0));
    c.state.entities.insert(chicken);
    hold(&mut c, "minecraft:oak_slab", vec![]);
    assert_eq!(
        click(&mut c, STONE, Facing::East, [1.0, 0.8, 0.5]),
        UseResult::Consume,
        "a top slab would overlap the chicken"
    );
    assert_eq!(c.state.chunks.block(2, 64, 1), Some(BlockState(0)));
    assert_eq!(
        click(&mut c, STONE, Facing::East, [1.0, 0.2, 0.5]),
        UseResult::Success
    );
    assert_eq!(prop(&c, EAST_OF_STONE, "type"), "bottom");
}

#[test]
fn torches_are_not_predicted_on_walls() {
    let mut c = client();
    hold(&mut c, "minecraft:torch", vec![]);
    assert_eq!(
        click(&mut c, STONE, Facing::East, [1.0, 0.5, 0.5]),
        UseResult::Success
    );
    assert_eq!(c.state.chunks.block(2, 64, 1), Some(BlockState(0)));
    assert_eq!(c.state.inventory.player.slots[HOTBAR_START].count, 8);
}

#[test]
fn block_state_component_applies() {
    let mut c = client();
    hold(
        &mut c,
        "minecraft:oak_slab",
        vec![DataComponent::BlockState(vec![
            mcr_protocol::types::component::BlockStateProperty {
                name: "type".into(),
                value: "top".into(),
            },
        ])],
    );
    click(&mut c, STONE, Facing::Up, [0.5, 1.0, 0.5]);
    assert_eq!(prop(&c, ABOVE_STONE, "type"), "top");
}

fn chest_client(name: Option<&str>) -> (Client, BlockPos) {
    let mut c = client();
    let chest = default_of(&c, "minecraft:chest");
    let pos = BlockPos::new(3, 64, 3);
    c.state
        .chunks
        .set_block(pos.x, pos.y, pos.z, BlockState(chest));
    let mut data = Compound::new();
    if let Some(n) = name {
        data.insert("CustomName", Value::String(n.into()));
    }
    data.insert(
        "Tags",
        Value::List(List::String(vec!["a".into(), "b".into(), "c".into()])),
    );
    let kind = c
        .state
        .data
        .registry("minecraft:block_entity_type")
        .unwrap()
        .id_of("minecraft:chest")
        .unwrap() as i32;
    c.state.block_entities.insert(pos, (kind, data));
    c.state.player.game_mode = GameType::Adventure;
    (c, pos)
}

fn nbt_predicate(c: &Client, nbt: Option<Compound>) -> AdventureModePredicate {
    let id = c.state.data.block_by_name("minecraft:chest").unwrap() as i32;
    AdventureModePredicate(vec![BlockPredicate {
        blocks: Some(HolderSet::Ids(vec![id])),
        properties: None,
        nbt: nbt.map(NetworkCompound),
        components: DataComponentMatchers::default(),
    }])
}

fn can_break(c: &mut Client, nbt: Option<Compound>, pos: BlockPos) -> bool {
    let p = nbt_predicate(c, nbt);
    hold(
        c,
        "minecraft:iron_pickaxe",
        vec![DataComponent::CanBreak(p)],
    );
    !c.state.block_action_restricted(pos)
}

fn named(n: &str) -> Compound {
    let mut e = Compound::new();
    e.insert("CustomName", Value::String(n.into()));
    e
}

#[test]
fn nbt_predicates_match_block_entity_data() {
    let (mut c, pos) = chest_client(Some("vault"));
    assert!(can_break(&mut c, Some(named("vault")), pos));
    assert!(!can_break(&mut c, Some(named("other")), pos));

    let mut meta = Compound::new();
    meta.insert("id", Value::String("minecraft:chest".into()));
    meta.insert("x", Value::Int(3));
    assert!(can_break(&mut c, Some(meta), pos), "metadata is included");

    let mut partial = Compound::new();
    partial.insert(
        "Tags",
        Value::List(List::String(vec!["c".into(), "a".into()])),
    );
    assert!(
        can_break(&mut c, Some(partial), pos),
        "lists match partially"
    );
    let mut missing = Compound::new();
    missing.insert("Tags", Value::List(List::String(vec!["z".into()])));
    assert!(!can_break(&mut c, Some(missing), pos));

    let (mut c, pos) = chest_client(None);
    assert!(
        !can_break(&mut c, Some(named("vault")), pos),
        "an unnamed chest"
    );
    c.state.block_entities.clear();
    assert!(
        !can_break(&mut c, Some(Compound::new()), pos),
        "no block entity"
    );
}

#[test]
fn component_matchers_are_ignored_like_vanilla() {
    let (mut c, pos) = chest_client(None);
    let mut p = nbt_predicate(&c, None);
    p.0[0].components.exact = vec![DataComponent::MaxStackSize(mcr_buf::VarInt(1))];
    hold(
        &mut c,
        "minecraft:iron_pickaxe",
        vec![DataComponent::CanBreak(p)],
    );
    assert!(!c.state.block_action_restricted(pos));
}
