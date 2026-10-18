//! `MCR_LIVE_ADDR=localhost:25578 MCR_LIVE_CONSOLE=<stdin file> MCR_LIVE_LOG=<server log> \
//!  cargo test -p mcr-client --test live_containers -- --ignored --nocapture`
//!
//! Mount screens, villager offers, stonecutter and loom lists, and the crafter's slot mirroring.

mod common;

use common::Live;
use mcr_buf::BlockPos;
use mcr_client::MountFamily;
use mcr_protocol::play::serverbound::{ContainerInput as I, Facing};
use mcr_protocol::types::game::Hand;

const FLOOR: i32 = -61;

fn find(live: &Live, kind: &str) -> Option<i32> {
    let id = live.client.state.data.entity_type_by_name(kind)? as i32;
    live.client
        .state
        .entities
        .iter()
        .filter(|e| e.kind == id)
        .map(|e| e.id)
        .max()
}

fn start(name: &str) -> Live {
    let mut live = Live::connect(name);
    live.console(&format!("op {name}"));
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console(&format!("gamemode survival {name}"));
    live.console(&format!("clear {name}"));
    live.console("kill @e[type=!player]");
    live.console(&format!("tp {name} 0.5 {} 0.5", FLOOR + 1));
    live.idle(20);
    live
}

fn open_block(live: &mut Live, name: &str, block: &str) {
    live.console(&format!("setblock 2 {} 0 {block}", FLOOR + 1));
    live.console(&format!("tp {name} 0.5 {} 0.5", FLOOR + 1));
    live.idle(10);
    live.client
        .use_item_on(
            Hand::Main,
            BlockPos {
                x: 2,
                y: FLOOR + 1,
                z: 0,
            },
            Facing::Up,
            [0.5, 1.0, 0.5],
            false,
        )
        .unwrap();
    live.until(block, 60, |l| l.client.state.inventory.open.is_some());
    live.idle(8);
}

fn close(live: &mut Live) {
    live.client.close_container().unwrap();
    live.idle(6);
}

fn mount_screen_of(live: &mut Live, summon: &str, kind: &str) -> mcr_client::MountScreen {
    live.console(&format!("summon {summon}"));
    live.until(kind, 100, |l| find(l, kind).is_some());
    live.idle(10);
    let id = find(live, kind).unwrap();
    live.client.mount(id).unwrap();
    live.until("mounted", 100, |l| {
        l.client.state.player.vehicle == Some(id)
    });
    assert!(live.client.state.riding_custom_inventory());
    live.client.open_inventory().unwrap();
    live.until("mount screen", 60, |l| {
        l.client.state.inventory.open.is_some()
    });
    live.idle(6);
    let screen = live.client.state.mount_screen().expect("mount screen");
    close(live);
    live.run(5, |_| mcr_client::PlayerInput {
        sneak: true,
        ..Default::default()
    });
    live.until("dismounted", 60, |l| {
        l.client.state.player.vehicle.is_none()
    });
    live.console("kill @e[type=!player]");
    live.idle(5);
    screen
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_mount_screens_open_from_the_inventory_key() {
    let name = "ContMount";
    let mut live = start(name);
    let saddle = "equipment:{saddle:{id:\"minecraft:saddle\"}}";
    let horse = mount_screen_of(
        &mut live,
        &format!("minecraft:horse 0.5 {} 3.5 {{Tame:1b,{saddle}}}", FLOOR + 1),
        "minecraft:horse",
    );
    println!("horse {horse:?}");
    assert_eq!(horse.family, MountFamily::Horse);
    assert_eq!(horse.columns, 0);
    let llama = mount_screen_of(
        &mut live,
        &format!(
            "minecraft:llama 0.5 {} 3.5 {{Tame:1b,ChestedHorse:1b,Strength:3,{saddle}}}",
            FLOOR + 1
        ),
        "minecraft:llama",
    );
    println!("llama {llama:?}");
    assert_eq!(llama.family, MountFamily::Llama);
    assert_eq!(llama.columns, 3);
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_villager_offers_arrive_for_the_open_trade_screen() {
    let name = "ContTrade";
    let mut live = start(name);
    live.console(&format!(
        "summon minecraft:villager 0.5 {} 2.5 {{NoAI:1b,VillagerData:{{profession:\"minecraft:librarian\",level:2,type:\"minecraft:plains\"}},Offers:{{Recipes:[{{buy:{{id:\"minecraft:emerald\",count:5}},sell:{{id:\"minecraft:bread\",count:3}},maxUses:12,xp:4}},{{buy:{{id:\"minecraft:paper\",count:24}},buyB:{{id:\"minecraft:stick\",count:2}},sell:{{id:\"minecraft:emerald\",count:1}},maxUses:16}}]}}}}",
        FLOOR + 1
    ));
    live.until("villager", 100, |l| find(l, "minecraft:villager").is_some());
    live.idle(10);
    let id = find(&live, "minecraft:villager").unwrap();
    live.client.interact(id, Hand::Main).unwrap();
    live.until("offers", 100, |l| {
        l.client.state.gameplay.merchant.is_some()
    });
    let merchant = live.client.state.gameplay.merchant.clone().unwrap();
    println!("{merchant:?}");
    assert_eq!(merchant.offers.len(), 2);
    assert_eq!(
        merchant.container_id,
        live.client.state.inventory.open.as_ref().unwrap().id
    );
    assert_eq!(merchant.offers[0].cost_a.count, 5);
    close(&mut live);
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_stonecutter_and_loom_lists_follow_their_inputs() {
    let name = "ContLists";
    let mut live = start(name);
    live.console(&format!("give {name} minecraft:stone 8"));
    live.console(&format!("give {name} minecraft:white_banner"));
    live.console(&format!("give {name} minecraft:red_dye 4"));
    live.idle(15);

    open_block(&mut live, name, "minecraft:stonecutter");
    assert_eq!(live.client.state.stonecutter_input(), None);
    let n = live
        .client
        .state
        .inventory
        .open
        .as_ref()
        .unwrap()
        .slots
        .len();
    live.client
        .click_container((n - 9) as i16, 0, I::QuickMove)
        .unwrap();
    live.idle(8);
    let input = live.client.state.stonecutter_input().expect("input");
    let options = live.client.state.stonecutter_options(input);
    println!("stonecutter options {}", options.len());
    assert!(options.len() >= 4, "{options:?}");
    close(&mut live);

    open_block(&mut live, name, "minecraft:loom");
    assert_eq!(live.client.state.loom_pattern_count(), 0);
    let n = live
        .client
        .state
        .inventory
        .open
        .as_ref()
        .unwrap()
        .slots
        .len();
    live.client
        .click_container((n - 8) as i16, 0, I::QuickMove)
        .unwrap();
    live.idle(6);
    live.client
        .click_container((n - 7) as i16, 0, I::QuickMove)
        .unwrap();
    live.idle(8);
    let count = live.client.state.loom_pattern_count();
    println!("loom patterns {count}");
    assert!(count > 0);
    close(&mut live);
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_crafter_keeps_the_hotbar_and_clearing_the_cursor_on_close() {
    let name = "ContCraft";
    let mut live = start(name);
    live.console(&format!("give {name} minecraft:stone 8"));
    live.console(&format!("give {name} minecraft:oak_planks 8"));
    live.idle(15);
    let before = live.client.state.inventory.player.slots.clone();
    open_block(&mut live, name, "minecraft:crafter");
    let open = live.client.state.inventory.open.as_ref().unwrap();
    assert_eq!(open.slots.len(), 46);
    assert_eq!(open.player_start, Some(9));
    assert_eq!(
        &live.client.state.inventory.player.slots[9..45],
        &before[9..45]
    );
    assert!(live.client.state.inventory.player.slots[45].is_empty());

    let corrections = live.client.state.inventory.server_updates;
    live.client.click_container(9 + 27, 0, I::Pickup).unwrap();
    live.idle(6);
    assert!(!live.client.state.inventory.cursor.is_empty());
    live.client.click_container(9 + 27, 0, I::Pickup).unwrap();
    live.idle(6);
    live.client.click_container(9, 2, I::Swap).unwrap();
    live.idle(6);
    live.client.click_container(9, 2, I::Swap).unwrap();
    live.idle(6);
    assert_eq!(live.client.state.inventory.server_updates, corrections);
    assert_eq!(
        &live.client.state.inventory.player.slots[9..45],
        &before[9..45]
    );

    live.client.click_container(9 + 27, 0, I::Pickup).unwrap();
    live.idle(4);
    close(&mut live);
    assert!(live.client.state.inventory.cursor.is_empty());
}
