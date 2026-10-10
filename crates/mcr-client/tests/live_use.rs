//! `MCR_LIVE_ADDR=... MCR_LIVE_CONSOLE=... MCR_LIVE_LOG=... cargo test -p mcr-client --test live_use -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_buf::BlockPos;
use mcr_client::{UseKind, UseResult, UseTarget};
use mcr_protocol::play::serverbound::Facing;
use mcr_protocol::types::game::Hand;

fn count(live: &Live, item: &str) -> i32 {
    let id = live.client.state.data.item_by_name(item).unwrap() as i32;
    live.client
        .state
        .inventory
        .player
        .slots
        .iter()
        .filter(|s| s.item == id)
        .map(|s| s.count)
        .sum()
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_item_use() {
    let mut live = Live::connect("User");
    live.console("op User");
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console("gamemode survival User");
    live.console("clear User");
    for give in [
        "minecraft:golden_apple 2",
        "minecraft:ender_pearl 4",
        "minecraft:shield",
        "minecraft:bow",
        "minecraft:arrow 8",
        "minecraft:stone 16",
    ] {
        live.console(&format!("give User {give}"));
    }
    live.idle(30);
    let baseline = live.teleports();

    live.client.select_hotbar(0).unwrap();
    assert_eq!(
        live.client.start_use(UseTarget::Air).unwrap(),
        UseResult::Consume
    );
    let using = *live.client.using_item().expect("eating");
    assert_eq!(using.kind, UseKind::Consume);
    assert!(live.client.state.player.physics.using_item.is_some());
    live.idle(45);
    assert!(live.client.using_item().is_none(), "eating finished");
    assert_eq!(count(&live, "minecraft:golden_apple"), 1);

    live.client.select_hotbar(1).unwrap();
    live.client.start_use(UseTarget::Air).unwrap();
    live.idle(5);
    let pearl = live.client.state.held_stack().clone();
    assert!(live.client.state.is_on_cooldown(&pearl), "pearl cooldown");
    assert_eq!(count(&live, "minecraft:ender_pearl"), 3);
    assert_eq!(live.client.use_kind(&pearl), None);

    live.client.select_hotbar(2).unwrap();
    live.client.start_use(UseTarget::Air).unwrap();
    assert_eq!(live.client.using_item().unwrap().kind, UseKind::Hold);
    live.idle(10);
    live.client.release_use().unwrap();
    assert!(live.client.using_item().is_none());

    live.client.select_hotbar(3).unwrap();
    live.client.start_use(UseTarget::Air).unwrap();
    live.idle(25);
    live.client.release_use().unwrap();
    live.idle(10);
    assert_eq!(count(&live, "minecraft:arrow"), 7, "bow consumed an arrow");

    live.client.select_hotbar(5).unwrap();
    live.idle(5);
    let p = live.pos();
    let ground = BlockPos::new(
        p[0].floor() as i32 + 2,
        p[1].floor() as i32 - 1,
        p[2].floor() as i32,
    );
    let target = BlockPos::new(ground.x, ground.y + 1, ground.z);
    let r = live
        .client
        .start_use(UseTarget::Block {
            pos: ground,
            face: Facing::Up,
            cursor: [0.5, 1.0, 0.5],
            inside: false,
        })
        .unwrap();
    assert_eq!(r, UseResult::Success);
    assert_ne!(live.block(target), 0, "placement predicted");
    assert!(!live.client.state.predictions.is_empty());
    live.idle(10);
    assert!(live.client.state.predictions.is_empty());
    assert_ne!(live.block(target), 0, "server confirmed the block");
    assert_eq!(count(&live, "minecraft:stone"), 15);
    assert!(
        live.teleports() <= baseline + 1,
        "only the pearl may teleport us"
    );
    let _ = Hand::Main;
}
