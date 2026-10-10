//! `MCR_LIVE_ADDR=localhost:25610 MCR_LIVE_CONSOLE=<stdin file> MCR_LIVE_LOG=<server log> \
//!  cargo test -p mcr-client --test live_elytra -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_client::{PlayerInput, UseTarget};

fn speed(live: &Live) -> f64 {
    let v = live.client.state.player.physics.vel;
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_elytra_glide_firework_and_landing() {
    let mut live = Live::connect("Glider");
    live.console("op Glider");
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console("gamemode survival Glider");
    live.console("clear Glider");
    live.console("kill @e[type=!player]");
    live.console("fill -3 199 -3 3 199 3 minecraft:stone");
    live.console("fill -3 200 -3 3 230 3 minecraft:air");
    live.console("tp Glider 0.5 200 0.5 0 0");
    live.console("item replace entity Glider armor.chest with minecraft:elytra");
    live.console("give Glider minecraft:firework_rocket 8");
    live.idle(30);
    live.client.select_hotbar(0).unwrap();
    live.idle(5);
    let baseline = live.teleports();
    assert!(live.client.glider_equipped(), "elytra equipped");

    live.run(14, |_| PlayerInput {
        forward: true,
        ..PlayerInput::default()
    });
    live.run(6, |_| PlayerInput {
        forward: true,
        ..PlayerInput::default()
    });
    assert!(
        !live.client.state.player.physics.on_ground,
        "off the platform"
    );
    live.run(1, |_| PlayerInput {
        jump: true,
        forward: true,
        ..PlayerInput::default()
    });
    live.run(1, |_| PlayerInput::default());
    assert!(
        live.client.state.player.physics.fall_flying,
        "gliding started"
    );

    let start = live.pos();
    live.run(40, |_| PlayerInput {
        pitch: 10.0,
        ..PlayerInput::default()
    });
    assert!(
        live.client.state.player.physics.fall_flying,
        "still gliding"
    );
    let glide_speed = speed(&live);
    live.client.start_use(UseTarget::Air).expect("use firework");
    live.run(25, |_| PlayerInput {
        pitch: -5.0,
        ..PlayerInput::default()
    });
    println!(
        "start {start:?} now {:?} speed before {glide_speed:.2} after {:.2}",
        live.pos(),
        speed(&live)
    );
    assert!(speed(&live) > glide_speed, "the rocket sped the glide up");

    for _ in 0..30 {
        live.run(20, |_| PlayerInput {
            pitch: 55.0,
            ..PlayerInput::default()
        });
        if live.client.state.player.physics.on_ground {
            break;
        }
    }
    live.idle(10);
    let p = live.client.state.player.physics.clone();
    println!("landed at {:?} on_ground {}", p.pos, p.on_ground);
    assert!(p.on_ground, "landed");
    assert!(!p.fall_flying, "server ended the glide");
    let log = live.log_text();
    assert!(
        !log.contains("Glider moved wrongly"),
        "server rejected glide movement"
    );
    assert!(
        !log.contains("Glider moved too quickly"),
        "server rejected glide speed"
    );
    assert_eq!(live.teleports(), baseline, "no corrective teleports");
}
