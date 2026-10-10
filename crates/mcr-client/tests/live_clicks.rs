//! `MCR_LIVE_ADDR=localhost:25580 MCR_LIVE_CONSOLE=<stdin file> MCR_LIVE_LOG=<server log> \
//!  cargo test -p mcr-client --test live_clicks -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_client::{OUTSIDE_SLOT, PlayerInput};
use mcr_protocol::play::serverbound::ContainerInput as I;

fn settle(live: &mut Live) {
    live.idle(8);
}

fn server_updates(live: &Live) -> u64 {
    live.client.state.inventory.server_updates
}

fn click(live: &mut Live, what: &str, slot: i16, button: i8, input: I) {
    let before = server_updates(live);
    live.client.click_container(slot, button, input).unwrap();
    settle(live);
    let after = server_updates(live);
    assert_eq!(
        before, after,
        "server corrected the client after {what} (slot {slot} button {button})"
    );
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_player_menu_clicks() {
    let mut live = Live::connect("Clicker");
    live.console("op Clicker");
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console("gamemode survival Clicker");
    live.console("clear Clicker");
    live.idle(10);
    for give in [
        "minecraft:stone 64",
        "minecraft:stone 20",
        "minecraft:stick 30",
        "minecraft:iron_helmet",
        "minecraft:diamond_sword[custom_name='\"Named\"',enchantments={sharpness:3},damage=5]",
        "minecraft:bundle",
        "minecraft:dirt 10",
    ] {
        live.console(&format!("give Clicker {give}"));
    }
    live.until("items", 100, |l| {
        l.client
            .state
            .inventory
            .player
            .slots
            .iter()
            .filter(|s| !s.is_empty())
            .count()
            >= 6
    });
    settle(&mut live);
    live.client.state.inventory.corrections = 0;
    let slots = live.client.state.inventory.player.slots.clone();
    for (i, s) in slots.iter().enumerate() {
        println!(
            "slot {i}: item {} x{} comps {}",
            s.item,
            s.count,
            s.components.added.len()
        );
    }
    let find = |item: &str, live: &Live| {
        let id = live.client.state.data.item_by_name(item).unwrap() as i32;
        live.client
            .state
            .inventory
            .player
            .slots
            .iter()
            .position(|s| s.item == id && !s.is_empty())
            .unwrap() as i16
    };
    let stone = find("minecraft:stone", &live);
    let stick = find("minecraft:stick", &live);
    let sword = find("minecraft:diamond_sword", &live);
    let helmet = find("minecraft:iron_helmet", &live);
    let dirt = find("minecraft:dirt", &live);

    click(&mut live, "pickup", stick, 0, I::Pickup);
    click(&mut live, "place one", 20, 1, I::Pickup);
    click(&mut live, "place rest", 21, 0, I::Pickup);
    click(&mut live, "pickup named sword", sword, 0, I::Pickup);
    click(&mut live, "place sword", 22, 0, I::Pickup);
    click(&mut live, "pickup half stone", stone, 1, I::Pickup);
    click(&mut live, "merge", stone, 0, I::Pickup);
    click(&mut live, "shift helmet", helmet, 0, I::QuickMove);
    click(&mut live, "shift stone", stone, 0, I::QuickMove);
    click(&mut live, "swap sword", 22, 0, I::Swap);
    click(&mut live, "swap offhand", 21, 40, I::Swap);
    click(&mut live, "throw one", dirt, 0, I::Throw);
    click(&mut live, "throw stack", dirt, 1, I::Throw);
    let drag_from = find("minecraft:stone", &live);
    click(&mut live, "pickup stone for drag", drag_from, 0, I::Pickup);
    click(&mut live, "drag begin", OUTSIDE_SLOT, 0, I::QuickCraft);
    for s in [10, 11, 12] {
        click(&mut live, "drag add", s, 1, I::QuickCraft);
    }
    click(&mut live, "drag end", OUTSIDE_SLOT, 2, I::QuickCraft);
    click(&mut live, "double click", 10, 0, I::PickupAll);
    click(&mut live, "drop cursor", OUTSIDE_SLOT, 0, I::Pickup);
    let _ = PlayerInput::default();
    assert_eq!(live.client.state.inventory.corrections, 0);
}
