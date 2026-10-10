//! `MCR_LIVE_ADDR=... MCR_LIVE_CONSOLE=... MCR_LIVE_LOG=... cargo test -p mcr-client --test live_dig -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_buf::BlockPos;
use mcr_protocol::play::serverbound::Facing;

fn dig(live: &mut Live, pos: BlockPos) -> u32 {
    assert!(live.client.start_destroy(pos, Facing::West).unwrap());
    let mut ticks = 0;
    while live.block(pos) != 0 && ticks < 400 {
        live.idle(1);
        live.client.continue_destroy(pos, Facing::West).unwrap();
        ticks += 1;
    }
    ticks
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_digging_matches_vanilla() {
    let mut live = Live::connect("Digger");
    live.console("op Digger");
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console("gamemode survival Digger");
    live.console("clear Digger");
    live.console("give Digger minecraft:iron_pickaxe");
    live.console("give Digger minecraft:diamond_pickaxe[enchantments={efficiency:5}]");
    live.idle(30);
    let p = live.pos();
    let target = BlockPos::new(
        p[0].floor() as i32 + 2,
        p[1].floor() as i32,
        p[2].floor() as i32,
    );
    let place = |live: &mut Live, block: &str| {
        live.console(&format!(
            "setblock {} {} {} {block}",
            target.x, target.y, target.z
        ));
        live.until("block placed", 60, |l| l.block(target) != 0);
    };

    for (slot, label, expected) in [
        (0u8, "iron pickaxe on stone", 8u32),
        (
            1,
            "efficiency 5 (2 ticks plus the 5 tick delay after a break)",
            7,
        ),
    ] {
        place(&mut live, "minecraft:stone");
        live.client.select_hotbar(slot).unwrap();
        live.idle(20);
        let p = live.client.state.destroy_progress(target).unwrap();
        println!("{label}: progress per tick {p}");
        let updates_before = live.client.state.predictions.len();
        let ticks = dig(&mut live, target);
        println!("{label}: broke after {ticks} ticks (expected {expected})");
        assert_eq!(ticks, expected, "{label}");
        live.idle(15);
        assert_eq!(
            live.block(target),
            0,
            "server restored the block after {label}"
        );
        assert_eq!(live.client.state.predictions.len(), updates_before);
        assert!(!live.log_text().contains("moved wrongly"));
    }

    place(&mut live, "minecraft:stone");
    live.client.select_hotbar(8).unwrap();
    live.idle(10);
    let p = live.client.state.destroy_progress(target).unwrap();
    assert!((p - 1.0 / 1.5 / 100.0).abs() < 1e-5, "bare hand on stone");
}
