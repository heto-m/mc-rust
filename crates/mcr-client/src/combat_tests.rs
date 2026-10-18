use mcr_buf::BlockPos;
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::serverbound::{Facing, PlayerActionKind};
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::game::GameType;
use mcr_world::{BlockState, ChunkColumn, ChunkPos};

use crate::client::Client;
use crate::combat::{CombatKeys, MISS_TICKS};
use crate::inventory::HOTBAR_START;
use crate::use_item::UseTarget;

const STONE: BlockPos = BlockPos { x: 1, y: 64, z: 1 };

fn client() -> Client {
    let mut c = Client::for_test();
    let cfg = *c.state.chunks.config();
    c.state
        .chunks
        .insert(ChunkPos::new(0, 0), ChunkColumn::empty(&cfg));
    let stone = c.state.data.block_by_name("minecraft:stone").unwrap();
    let state = c.state.data.block_info(stone).unwrap().default_state as u16;
    c.state.chunks.set_block(1, 64, 1, BlockState(state));
    c.state.player.physics.on_ground = true;
    c
}

fn hold(c: &mut Client, name: &str) {
    let item = c.state.data.item_by_name(name).unwrap() as i32;
    c.state.inventory.player.slots[HOTBAR_START] = ItemStack::new(item, 1);
}

/// An iron pickaxe whose rule names stone directly; tests have no block tags.
fn hold_pickaxe(c: &mut Client) {
    use mcr_protocol::types::component::{
        DataComponent, DataComponentPatch, HolderSet, Tool, ToolRule,
    };
    hold(c, "minecraft:iron_pickaxe");
    let stone = c.state.data.block_by_name("minecraft:stone").unwrap() as i32;
    c.state.inventory.player.slots[HOTBAR_START].components = DataComponentPatch {
        added: vec![DataComponent::Tool(Tool {
            rules: vec![ToolRule {
                blocks: HolderSet::Ids(vec![stone]),
                speed: Some(6.0),
                correct_for_drops: Some(true),
            }],
            default_mining_speed: 1.0,
            damage_per_block: 1,
            can_destroy_blocks_in_creative: true,
        })],
        removed: vec![],
    };
}

fn charge(c: &mut Client) {
    for _ in 0..40 {
        c.tick_attack_strength();
    }
}

fn on_stone() -> UseTarget {
    UseTarget::Block {
        pos: STONE,
        face: Facing::Up,
        cursor: [0.5, 1.0, 0.5],
        inside: false,
    }
}

fn entity() -> UseTarget {
    UseTarget::Entity {
        id: 7,
        location: [0.0, 1.0, 0.0],
    }
}

fn click(attack: bool) -> CombatKeys {
    CombatKeys {
        attack_clicks: u8::from(attack),
        use_clicks: u8::from(!attack),
        attack_down: attack,
        use_down: !attack,
    }
}

fn actions(sent: &[S<'_>]) -> Vec<PlayerActionKind> {
    sent.iter()
        .filter_map(|p| match p {
            S::PlayerAction(a) => Some(a.action),
            _ => None,
        })
        .collect()
}

fn count(sent: &[S<'_>], name: &str) -> usize {
    sent.iter().filter(|p| p.name() == name).count()
}

fn server_using(c: &mut Client) {
    c.state.player.data.insert(8, EntityDataValue::Byte(1));
    c.update_using_item();
}

#[test]
fn spear_left_click_sends_stab_instead_of_attacking() {
    let mut c = client();
    hold(&mut c, "minecraft:iron_spear");
    charge(&mut c);
    let out = c.handle_keybinds(click(true), entity(), false).unwrap();
    let sent = c.take_sent();
    assert!(out.swing && out.stab);
    assert_eq!(actions(&sent), [PlayerActionKind::Stab]);
    assert_eq!(count(&sent, "minecraft:attack"), 0);
    assert_eq!(count(&sent, "minecraft:punch"), 0);

    charge(&mut c);
    c.handle_keybinds(click(true), on_stone(), false).unwrap();
    for _ in 0..5 {
        c.handle_keybinds(
            CombatKeys {
                attack_down: true,
                ..CombatKeys::default()
            },
            on_stone(),
            false,
        )
        .unwrap();
    }
    let sent = c.take_sent();
    assert_eq!(
        actions(&sent),
        [PlayerActionKind::Stab],
        "a spear never mines"
    );
}

#[test]
fn an_uncharged_spear_does_not_stab() {
    let mut c = client();
    hold(&mut c, "minecraft:iron_spear");
    c.tick_attack_strength();
    c.handle_keybinds(click(true), entity(), false).unwrap();
    assert!(c.take_sent().is_empty());
}

#[test]
fn spear_right_click_charges_until_release() {
    let mut c = client();
    hold(&mut c, "minecraft:iron_spear");
    c.handle_keybinds(click(false), UseTarget::Air, false)
        .unwrap();
    assert!(c.is_using_item());
    let held = CombatKeys {
        use_down: true,
        ..CombatKeys::default()
    };
    c.handle_keybinds(held, UseTarget::Air, false).unwrap();
    let out = c
        .handle_keybinds(CombatKeys::default(), UseTarget::Air, false)
        .unwrap();
    assert!(out.released);
    let sent = c.take_sent();
    assert_eq!(count(&sent, "minecraft:use_item"), 1);
    assert_eq!(actions(&sent), [PlayerActionKind::ReleaseUseItem]);
}

#[test]
fn release_is_sent_for_a_use_only_the_server_started() {
    let mut c = client();
    hold(&mut c, "minecraft:brush");
    server_using(&mut c);
    assert!(c.is_using_item());
    let held = CombatKeys {
        use_down: true,
        ..CombatKeys::default()
    };
    c.handle_keybinds(held, on_stone(), false).unwrap();
    assert!(actions(&c.take_sent()).is_empty());
    c.handle_keybinds(CombatKeys::default(), on_stone(), false)
        .unwrap();
    assert_eq!(actions(&c.take_sent()), [PlayerActionKind::ReleaseUseItem]);
    assert!(!c.is_using_item());
}

#[test]
fn a_bundle_use_is_held_until_release() {
    let mut c = client();
    hold(&mut c, "minecraft:bundle");
    let out = c
        .handle_keybinds(click(false), UseTarget::Air, false)
        .unwrap();
    assert!(out.swing, "BundleItem#use succeeds with a swing");
    assert!(c.is_using_item());
    c.handle_keybinds(CombatKeys::default(), UseTarget::Air, false)
        .unwrap();
    assert_eq!(actions(&c.take_sent()), [PlayerActionKind::ReleaseUseItem]);
}

#[test]
fn no_attack_dig_or_use_repeat_while_using() {
    let mut c = client();
    hold(&mut c, "minecraft:shield");
    c.handle_keybinds(click(false), UseTarget::Air, false)
        .unwrap();
    assert!(c.is_using_item());
    c.take_sent();
    let busy = CombatKeys {
        attack_clicks: 1,
        use_clicks: 1,
        attack_down: true,
        use_down: true,
    };
    for _ in 0..10 {
        c.handle_keybinds(busy, on_stone(), false).unwrap();
        c.handle_keybinds(busy, entity(), false).unwrap();
    }
    let sent = c.take_sent();
    assert!(
        sent.is_empty(),
        "{:?}",
        sent.iter().map(S::name).collect::<Vec<_>>()
    );
}

#[test]
fn holding_use_on_an_entity_repeats_every_four_ticks() {
    let mut c = client();
    let held = CombatKeys {
        use_down: true,
        ..CombatKeys::default()
    };
    c.handle_keybinds(click(false), entity(), false).unwrap();
    for _ in 0..8 {
        c.handle_keybinds(held, entity(), false).unwrap();
    }
    // Ticks 0, 4 and 8, each trying the main hand and then the off hand.
    assert_eq!(count(&c.take_sent(), "minecraft:interact"), 6);
}

#[test]
fn a_miss_locks_attacks_until_the_key_is_released() {
    let mut c = client();
    charge(&mut c);
    c.handle_keybinds(click(true), UseTarget::Air, false)
        .unwrap();
    assert_eq!(count(&c.take_sent(), "minecraft:punch"), 1);
    assert_eq!(c.miss_time(), MISS_TICKS - 1);
    c.handle_keybinds(click(true), entity(), false).unwrap();
    assert!(c.take_sent().is_empty(), "locked out after a miss");
    c.handle_keybinds(CombatKeys::default(), entity(), false)
        .unwrap();
    assert_eq!(c.miss_time(), 0);
    c.handle_keybinds(click(true), entity(), false).unwrap();
    assert_eq!(count(&c.take_sent(), "minecraft:attack"), 1);
}

#[test]
fn creative_misses_have_no_lockout() {
    let mut c = client();
    c.state.player.apply_game_mode(GameType::Creative);
    c.handle_keybinds(click(true), UseTarget::Air, false)
        .unwrap();
    assert_eq!(c.miss_time(), 0);
}

#[test]
fn rowing_keeps_the_hands_busy() {
    let mut c = client();
    c.combat.hands_busy = true;
    c.handle_keybinds(click(true), entity(), false).unwrap();
    c.handle_keybinds(click(false), entity(), false).unwrap();
    assert!(c.take_sent().is_empty());
}

#[test]
fn attack_strength_restarts_on_item_change_and_abort() {
    let mut c = client();
    charge(&mut c);
    assert_eq!(c.attack_strength_scale(0.0), 1.0);
    hold(&mut c, "minecraft:stick");
    c.tick_attack_strength();
    assert!(c.attack_strength_scale(0.0) < 0.5);
    charge(&mut c);
    let mine = CombatKeys {
        attack_clicks: 1,
        attack_down: true,
        ..CombatKeys::default()
    };
    c.handle_keybinds(mine, on_stone(), false).unwrap();
    charge(&mut c);
    c.handle_keybinds(CombatKeys::default(), on_stone(), false)
        .unwrap();
    assert_eq!(c.attack_strength_scale(0.0), 0.0, "stopDestroyBlock resets");
}

fn mine_ticks(c: &mut Client) -> usize {
    let held = CombatKeys {
        attack_down: true,
        ..CombatKeys::default()
    };
    for tick in 1..400 {
        c.handle_keybinds(held, on_stone(), false).unwrap();
        if actions(&c.take_sent()).contains(&PlayerActionKind::StopDestroyBlock) {
            return tick;
        }
    }
    panic!("never broke");
}

#[test]
fn per_tick_progress_restarts_when_the_held_item_changes() {
    let mut c = client();
    let start = CombatKeys {
        attack_clicks: 1,
        attack_down: true,
        ..CombatKeys::default()
    };
    c.handle_keybinds(start, on_stone(), false).unwrap();
    assert_eq!(
        actions(&c.take_sent()),
        [PlayerActionKind::StartDestroyBlock]
    );
    let held = CombatKeys {
        attack_down: true,
        ..CombatKeys::default()
    };
    for _ in 0..3 {
        c.handle_keybinds(held, on_stone(), false).unwrap();
    }
    assert!(c.digging().unwrap().progress > 0.0);
    hold_pickaxe(&mut c);
    c.handle_keybinds(held, on_stone(), false).unwrap();
    assert_eq!(
        actions(&c.take_sent()),
        [
            PlayerActionKind::AbortDestroyBlock,
            PlayerActionKind::StartDestroyBlock
        ]
    );
    assert_eq!(c.digging().unwrap().progress, 0.0);
    let step = c.state.destroy_progress(STONE).unwrap();
    let (mut acc, mut ticks) = (0.0f32, 0);
    while acc < 1.0 {
        acc += step;
        ticks += 1;
    }
    assert_eq!(ticks, 8);
    assert_eq!(mine_ticks(&mut c), ticks);
}

#[test]
fn leaving_the_ground_slows_the_rest_of_the_dig() {
    let mut c = client();
    hold_pickaxe(&mut c);
    let start = CombatKeys {
        attack_clicks: 1,
        attack_down: true,
        ..CombatKeys::default()
    };
    c.handle_keybinds(start, on_stone(), false).unwrap();
    let grounded = c.digging().unwrap().progress;
    c.state.player.physics.on_ground = false;
    let held = CombatKeys {
        attack_down: true,
        ..CombatKeys::default()
    };
    c.handle_keybinds(held, on_stone(), false).unwrap();
    let airborne = c.digging().unwrap().progress - grounded;
    assert!(
        (airborne * 5.0 - grounded).abs() < 1e-6,
        "{grounded} {airborne}"
    );
}

#[test]
fn a_finished_dig_waits_five_ticks_before_the_next() {
    let mut c = client();
    hold_pickaxe(&mut c);
    c.handle_keybinds(click(true), on_stone(), false).unwrap();
    mine_ticks(&mut c);
    let stone = c.state.data.block_by_name("minecraft:stone").unwrap();
    let state = c.state.data.block_info(stone).unwrap().default_state as u16;
    c.state.chunks.set_block(1, 64, 1, BlockState(state));
    let held = CombatKeys {
        attack_down: true,
        ..CombatKeys::default()
    };
    for _ in 0..5 {
        c.handle_keybinds(held, on_stone(), false).unwrap();
        assert!(actions(&c.take_sent()).is_empty());
    }
    c.handle_keybinds(held, on_stone(), false).unwrap();
    assert_eq!(
        actions(&c.take_sent()),
        [PlayerActionKind::StartDestroyBlock]
    );
}

#[test]
fn an_open_screen_stops_digging_and_locks_attacks() {
    let mut c = client();
    c.handle_keybinds(click(true), on_stone(), false).unwrap();
    c.take_sent();
    c.handle_keybinds(click(true), on_stone(), true).unwrap();
    assert_eq!(
        actions(&c.take_sent()),
        [PlayerActionKind::AbortDestroyBlock]
    );
    assert!(c.miss_time() > MISS_TICKS);
}
