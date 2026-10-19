//! `MCR_LIVE_ADDR=... MCR_LIVE_CONSOLE=... MCR_LIVE_LOG=... cargo test -p mcr-client --test live_movement -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_client::PlayerInput;

const NAME: &str = "Mover";

fn keys(forward: bool, sprint: bool, jump: bool) -> PlayerInput {
    PlayerInput {
        forward,
        sprint,
        jump,
        ..PlayerInput::default()
    }
}

fn goto(live: &mut Live, x: f64, y: f64, z: f64) {
    let before = live.teleports();
    live.console(&format!("tp {NAME} {x} {y} {z} 270 0"));
    live.until("teleported", 100, |l| l.teleports() > before);
    live.idle(20);
}

fn check(live: &Live, name: &str, baseline: u32) {
    assert_eq!(
        live.teleports(),
        baseline,
        "{name}: server corrected our movement"
    );
    let log = live.log_text();
    for warning in ["moved wrongly", "moved too quickly"] {
        assert!(
            !log.contains(&format!("{NAME} {warning}")),
            "{name}: {warning}"
        );
    }
    println!("{name}: ok at {:?}", live.pos());
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_movement_effects_and_equipment() {
    let mut live = Live::connect(NAME);
    live.console(&format!("op {NAME}"));
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console(&format!("gamemode survival {NAME}"));
    live.console(&format!("clear {NAME}"));
    live.console(&format!("effect clear {NAME}"));
    live.console("forceload add 190 -10 270 90");
    live.yaw = 270.0;
    live.idle(40);

    live.console("fill 195 95 -5 265 180 85 minecraft:air");
    live.console("fill 199 99 -1 240 99 5 minecraft:stone");
    for i in 0..5 {
        let x = 205 + 3 * i;
        live.console(&format!("fill {x} 100 0 240 {} 4 minecraft:stone", 100 + i));
    }
    live.console("fill 199 99 9 260 99 15 minecraft:stone");
    live.console("fill 199 89 19 261 100 31 minecraft:stone");
    live.console("fill 200 90 20 260 99 30 minecraft:water");
    live.console("fill 199 95 39 241 98 45 minecraft:stone");
    live.console("fill 200 99 40 240 99 44 minecraft:powder_snow");
    live.console("fill 199 94 49 205 95 55 minecraft:stone");
    live.console("fill 200 96 50 204 99 54 minecraft:powder_snow");
    live.console("fill 199 150 59 203 150 61 minecraft:stone");
    live.console("fill 214 115 59 222 155 59 minecraft:stone");
    live.console("fill 214 115 60 222 155 60 minecraft:vine[north=true]");
    live.idle(40);

    // Auto-jump up five single steps.
    goto(&mut live, 201.5, 100.0, 2.5);
    let baseline = live.teleports();
    live.client.state.player.physics.auto_jump = true;
    live.run(140, |_| keys(true, false, false));
    live.client.state.player.physics.auto_jump = false;
    let y = live.pos()[1];
    assert!(y >= 104.0, "climbed the steps: {y}");
    check(&live, "auto-jump", baseline);

    // Blind sprinting: refused, and nothing for the server to correct.
    goto(&mut live, 201.5, 100.0, 12.5);
    let baseline = live.teleports();
    live.console(&format!("effect give {NAME} minecraft:blindness 30"));
    live.until("blind", 100, |l| {
        l.client.state.player.physics.effects.blindness
    });
    live.run(40, |_| keys(true, true, false));
    assert!(!live.client.state.player.physics.sprinting, "blind sprint");
    live.console(&format!("effect clear {NAME}"));
    live.until("sighted", 100, |l| {
        !l.client.state.player.physics.effects.blindness
    });
    live.run(20, |_| keys(true, true, false));
    assert!(
        live.client.state.player.physics.sprinting,
        "sprint once sighted"
    );
    live.idle(10);
    check(&live, "blindness", baseline);

    // Swimming with Dolphin's Grace.
    goto(&mut live, 201.5, 95.0, 25.5);
    let baseline = live.teleports();
    live.console(&format!("effect give {NAME} minecraft:dolphins_grace 30"));
    live.until("graced", 100, |l| {
        l.client.state.player.physics.effects.dolphins_grace
    });
    let x0 = live.pos()[0];
    live.run(60, |_| keys(true, true, false));
    let swum = live.pos()[0] - x0;
    println!("dolphin's grace: swam {swum:.2} blocks in 60 ticks");
    live.console(&format!("effect clear {NAME}"));
    live.idle(10);
    check(&live, "dolphin's grace", baseline);

    // Leather boots on powder snow: walk on top, then climb out of a pit.
    live.console(&format!(
        "item replace entity {NAME} armor.feet with minecraft:leather_boots"
    ));
    goto(&mut live, 201.5, 100.0, 42.5);
    assert!(live.client.state.player.physics.leather_boots, "boots seen");
    let baseline = live.teleports();
    live.run(60, |_| keys(true, false, false));
    assert_eq!(live.pos()[1], 100.0, "stays on the snow");
    check(&live, "powder snow walk", baseline);
    goto(&mut live, 202.5, 96.0, 52.5);
    let baseline = live.teleports();
    live.run(60, |_| keys(false, false, true));
    live.idle(20);
    assert!(live.pos()[1] >= 100.0, "climbed out: {:?}", live.pos());
    check(&live, "powder snow climb", baseline);
    live.console(&format!("clear {NAME}"));

    // Gliding along a vine curtain.
    live.console(&format!(
        "item replace entity {NAME} armor.chest with minecraft:elytra"
    ));
    goto(&mut live, 201.5, 151.0, 60.5);
    assert!(live.client.glider_equipped(), "elytra seen");
    let baseline = live.teleports();
    live.run(25, |_| keys(true, false, false));
    assert!(
        !live.client.state.player.physics.on_ground,
        "off the platform"
    );
    live.run(1, |_| keys(true, false, true));
    live.run(1, |_| PlayerInput::default());
    assert!(live.client.state.player.physics.fall_flying, "gliding");
    let mut through_vines = false;
    for _ in 0..60 {
        live.run(1, |_| PlayerInput {
            pitch: 15.0,
            ..PlayerInput::default()
        });
        let p = live.pos();
        if (214.0..223.0).contains(&p[0]) {
            through_vines = true;
            assert!(
                live.client.state.player.physics.fall_flying,
                "glide kept in the vines at {p:?}"
            );
        }
        if p[0] > 226.0 {
            break;
        }
    }
    assert!(
        through_vines,
        "the glide crossed the vines: {:?}",
        live.pos()
    );
    assert!(
        live.client.state.player.physics.fall_flying,
        "still gliding"
    );
    live.idle(5);
    check(&live, "glide through vines", baseline);
    live.console(&format!("clear {NAME}"));
    live.console(&format!("tp {NAME} 201.5 100 12.5"));
    live.idle(20);
}
