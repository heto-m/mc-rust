use mcr_buf::BlockPos;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{
    DataComponent, DataComponentPatch, HolderSet, Tool, ToolRule,
};
use mcr_world::{BlockState, ChunkColumn, ChunkPos};
use uuid::Uuid;

use crate::config::ClientConfig;
use crate::state::ClientState;

fn state() -> (ClientState, BlockPos, u16) {
    let mut s = ClientState::new(&ClientConfig::default(), Uuid::nil());
    let cfg = *s.chunks.config();
    s.chunks
        .insert(ChunkPos::new(0, 0), ChunkColumn::empty(&cfg));
    let stone = s.data.block_by_name("minecraft:stone").unwrap();
    let state = s.data.block_info(stone).unwrap().default_state as u16;
    s.chunks.set_block(1, 64, 1, BlockState(state));
    s.player.physics.on_ground = true;
    (s, BlockPos::new(1, 64, 1), state)
}

#[test]
fn bare_hand_on_stone_is_slow() {
    let (s, pos, _) = state();
    let p = s.destroy_progress(pos).unwrap();
    assert!((p - 1.0 / 1.5 / 100.0).abs() < 1e-6);
}

#[test]
fn tool_rule_speed_and_drops_apply() {
    let (mut s, pos, state) = state();
    let block = s.data.block_state(u32::from(state)).unwrap().block as i32;
    let pick = s.data.item_by_name("minecraft:iron_pickaxe").unwrap() as i32;
    let mut stack = ItemStack::new(pick, 1);
    stack.components = DataComponentPatch {
        added: vec![DataComponent::Tool(Tool {
            rules: vec![ToolRule {
                blocks: HolderSet::Ids(vec![block]),
                speed: Some(6.0),
                correct_for_drops: Some(true),
            }],
            default_mining_speed: 1.0,
            damage_per_block: 1,
            can_destroy_blocks_in_creative: true,
        })],
        removed: vec![],
    };
    s.inventory.player.slots[36] = stack;
    let p = s.destroy_progress(pos).unwrap();
    assert!((p - 6.0 / 1.5 / 30.0).abs() < 1e-6);
    s.player.physics.on_ground = false;
    let p = s.destroy_progress(pos).unwrap();
    assert!((p - 6.0 / 5.0 / 1.5 / 30.0).abs() < 1e-6);
}

#[test]
fn default_item_tool_comes_from_tables() {
    let (mut s, pos, _) = state();
    let pick = s.data.item_by_name("minecraft:iron_pickaxe").unwrap();
    s.inventory.player.slots[36] = ItemStack::new(pick as i32, 1);
    assert!(s.data.item_tool(pick).unwrap().rules().count() >= 2);
    assert!(s.destroy_progress(pos).is_some());
}

#[test]
fn unacknowledged_prediction_rolls_back() {
    let (mut s, pos, state) = state();
    s.predict_block(pos, BlockState::AIR, 5);
    assert_eq!(s.chunks.block(1, 64, 1), Some(BlockState::AIR));
    s.finish_predictions(4);
    assert_eq!(s.chunks.block(1, 64, 1), Some(BlockState::AIR));
    s.finish_predictions(5);
    assert_eq!(s.chunks.block(1, 64, 1), Some(BlockState(state)));
}

#[test]
fn server_update_during_prediction_wins_at_ack() {
    let (mut s, pos, _) = state();
    s.predict_block(pos, BlockState::AIR, 7);
    assert!(s.predictions.hold(pos, 1));
    s.finish_predictions(7);
    assert_eq!(s.chunks.block(1, 64, 1), Some(BlockState(1)));
}
