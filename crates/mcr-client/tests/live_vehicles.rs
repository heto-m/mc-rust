//! `MCR_LIVE_ADDR=... MCR_LIVE_CONSOLE=... MCR_LIVE_LOG=... cargo test -p mcr-client --test live_vehicles -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_client::PlayerInput;

fn find(live: &Live, kind: &str) -> Option<i32> {
    let id = live.client.state.data.entity_type_by_name(kind)? as i32;
    live.client
        .state
        .entities
        .iter()
        .find(|e| e.kind == id)
        .map(|e| e.id)
}

fn mount(live: &mut Live, kind: &str) -> i32 {
    live.until("vehicle entity", 100, |l| find(l, kind).is_some());
    let id = find(live, kind).unwrap();
    live.client.mount(id).unwrap();
    live.until("mounted", 100, |l| {
        l.client.state.player.vehicle == Some(id)
    });
    live.idle(5);
    id
}

fn drive(live: &mut Live, ticks: u32, f: impl Fn(u32) -> PlayerInput) {
    live.run(ticks, f);
}

fn vehicle_pos(live: &Live, id: i32) -> [f64; 3] {
    live.client.state.entities.get(id).unwrap().pos
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_boat_and_horse() {
    let mut live = Live::connect("Rider");
    live.console("op Rider");
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console("gamemode survival Rider");
    live.console("tp Rider ~ -60 ~");
    live.console("kill @e[type=!player]");
    live.idle(20);
    let p = live.pos();
    let (x, z) = (p[0].floor() as i32, p[2].floor() as i32);
    live.console(&format!(
        "fill {} -62 {} {} -61 {} minecraft:water",
        x - 12,
        z - 12,
        x + 12,
        z + 12
    ));
    live.idle(40);
    live.console(&format!(
        "summon minecraft:oak_boat {} -60.6 {}",
        x + 4,
        z + 4
    ));
    let boat = mount(&mut live, "minecraft:oak_boat");
    let start = vehicle_pos(&live, boat);
    println!("boat start {start:?}");

    drive(&mut live, 40, |_| PlayerInput {
        forward: true,
        ..PlayerInput::default()
    });
    drive(&mut live, 30, |_| PlayerInput {
        forward: true,
        right: true,
        ..PlayerInput::default()
    });
    drive(&mut live, 30, |_| PlayerInput {
        forward: true,
        left: true,
        ..PlayerInput::default()
    });
    live.idle(20);
    let end = vehicle_pos(&live, boat);
    println!(
        "boat end {end:?}, corrections {}",
        live.client.state.vehicle_corrections
    );
    let moved = ((end[0] - start[0]).powi(2) + (end[2] - start[2]).powi(2)).sqrt();
    assert!(moved > 2.0, "boat moved {moved}");
    assert_eq!(
        live.client.state.vehicle_corrections, 0,
        "server corrected the boat"
    );
    let log = live.log_text();
    assert!(
        !log.contains("Vehicle of Rider moved wrongly"),
        "server rejected boat movement"
    );
    assert!(!log.contains("moved too quickly"));

    live.run(5, |_| PlayerInput {
        sneak: true,
        ..PlayerInput::default()
    });
    live.until("dismounted", 60, |l| {
        l.client.state.player.vehicle.is_none()
    });

    let p = live.pos();
    live.console(&format!(
        "summon minecraft:horse {} -60 {} {{Tame:1b,equipment:{{saddle:{{id:\"minecraft:saddle\"}}}}}}",
        p[0].floor() as i32 + 2,
        p[2].floor() as i32 + 2
    ));
    let horse = mount(&mut live, "minecraft:horse");
    let start = vehicle_pos(&live, horse);
    let before = live.client.state.vehicle_corrections;
    drive(&mut live, 40, |_| PlayerInput {
        forward: true,
        ..PlayerInput::default()
    });
    for _ in 0..3 {
        drive(&mut live, 12, |_| PlayerInput {
            forward: true,
            jump: true,
            ..PlayerInput::default()
        });
        drive(&mut live, 25, |_| PlayerInput {
            forward: true,
            ..PlayerInput::default()
        });
    }
    live.idle(20);
    let end = vehicle_pos(&live, horse);
    println!(
        "horse {start:?} -> {end:?}, corrections {}",
        live.client.state.vehicle_corrections - before
    );
    let moved = ((end[0] - start[0]).powi(2) + (end[2] - start[2]).powi(2)).sqrt();
    assert!(moved > 5.0, "horse moved {moved}");
    assert_eq!(
        live.client.state.vehicle_corrections, before,
        "server corrected the horse"
    );
    assert!(!live.log_text().contains("Vehicle of Rider moved wrongly"));
    for (y, block) in [(-62, "dirt"), (-61, "grass_block")] {
        live.console(&format!(
            "fill {} {y} {} {} {y} {} minecraft:{block} replace minecraft:water",
            x - 12,
            z - 12,
            x + 12,
            z + 12
        ));
    }
    live.idle(5);
}
