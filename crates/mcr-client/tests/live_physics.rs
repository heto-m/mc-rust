//! `MCR_LIVE_ADDR=... MCR_LIVE_CONSOLE=... MCR_LIVE_LOG=... cargo test -p mcr-client --test live_physics -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_client::PlayerInput;

fn keys(forward: bool, sprint: bool, jump: bool, sneak: bool) -> PlayerInput {
    PlayerInput {
        forward,
        sprint,
        jump,
        sneak,
        ..PlayerInput::default()
    }
}

fn goto(live: &mut Live, x: f64, y: f64, z: f64) {
    let before = live.teleports();
    live.console(&format!("tp Walker {x} {y} {z} 270 0"));
    live.until("teleported", 100, |l| l.teleports() > before);
    live.idle(30);
}

fn check(live: &Live, name: &str, baseline: u32) {
    assert_eq!(
        live.teleports(),
        baseline,
        "{name}: server corrected our movement"
    );
    let log = live.log_text();
    assert!(
        !log.contains("Walker moved wrongly"),
        "{name}: moved wrongly"
    );
    assert!(
        !log.contains("Walker moved too quickly"),
        "{name}: moved too quickly"
    );
    println!("{name}: ok at {:?}", live.pos());
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_terrain_physics() {
    let mut live = Live::connect("Walker");
    live.console("op Walker");
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    if live.client.state.player.dead {
        live.client.respawn().unwrap();
        live.until("respawned", 200, |l| !l.client.state.player.dead);
        live.until("loaded again", 400, |l| l.client.state.player.loaded);
    }
    live.console("difficulty peaceful");
    live.console("gamemode survival Walker");
    live.yaw = 270.0;
    live.console("forceload add 90 90 250 110");
    live.idle(60);
    live.console("fill 95 -60 95 140 -40 105 minecraft:air");
    live.console("fill 140 -60 95 190 -40 105 minecraft:air");
    live.console("fill 190 -60 95 240 -40 105 minecraft:air");
    live.console("fill 95 -62 95 140 -61 105 minecraft:dirt");
    live.console("fill 140 -62 95 190 -61 105 minecraft:dirt");
    live.console("fill 190 -62 95 240 -61 105 minecraft:dirt");
    live.idle(40);

    goto(&mut live, 100.5, -60.0, 100.5);
    live.console("fill 100 -61 100 140 -61 100 minecraft:packed_ice");
    live.idle(20);
    let base = live.teleports();
    live.run(40, |_| keys(true, true, false, false));
    live.idle(40);
    check(&live, "ice", base);

    goto(&mut live, 150.5, -60.0, 100.5);
    live.console("setblock 152 -60 100 minecraft:stone_slab");
    live.console("setblock 153 -60 100 minecraft:stone");
    live.console("setblock 155 -60 100 minecraft:stone_stairs[facing=east]");
    live.console("setblock 156 -60 100 minecraft:stone");
    live.console("setblock 156 -59 100 minecraft:stone_slab");
    live.console("setblock 158 -60 100 minecraft:oak_stairs[facing=east,half=top]");
    live.idle(20);
    let base = live.teleports();
    let y0 = live.pos()[1];
    live.run(80, |t| keys(true, true, t % 10 == 0, false));
    live.idle(20);
    assert!(
        live.pos()[0] > 153.0,
        "walked over the slab and stairs: {:?} dead={} tp={}",
        live.pos(),
        live.client.state.player.dead,
        live.teleports()
    );
    assert!(live.pos()[1] >= y0, "stepped up");
    check(&live, "slabs and stairs", base);

    goto(&mut live, 170.5, -60.0, 100.5);
    live.console("fill 175 -63 98 190 -61 102 minecraft:water");
    live.idle(30);
    let base = live.teleports();
    live.run(40, |_| keys(true, false, false, false));
    live.run(60, |t| keys(true, true, t % 20 < 6, false));
    live.idle(20);
    check(&live, "swimming", base);

    goto(&mut live, 200.5, -60.0, 100.5);
    live.console("fill 203 -60 100 203 -40 100 minecraft:stone");
    live.console("fill 202 -60 100 202 -40 100 minecraft:ladder[facing=west]");
    live.idle(30);
    let base = live.teleports();
    live.run(10, |_| keys(true, false, false, false));
    let y0 = live.pos()[1];
    live.run(80, |_| keys(true, false, false, false));
    assert!(
        live.pos()[1] > y0 + 5.0,
        "climbed the ladder to {}",
        live.pos()[1]
    );
    live.run(20, |_| keys(true, false, false, true));
    live.idle(10);
    check(&live, "ladder", base);

    goto(&mut live, 220.5, -60.0, 100.5);
    live.console("fill 222 -64 96 232 -61 104 minecraft:air");
    live.idle(30);
    let base = live.teleports();
    let y0 = live.pos()[1];
    live.run(60, |_| keys(true, false, false, true));
    assert!(
        (live.pos()[1] - y0).abs() < 0.01,
        "sneaking stopped at the edge, y {} -> {}",
        y0,
        live.pos()[1]
    );
    assert!(
        live.pos()[0] < 222.9,
        "stayed on the ledge: {}",
        live.pos()[0]
    );
    check(&live, "sneak edge", base);
}
