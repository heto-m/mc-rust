//! `MCR_LIVE_ADDR=localhost:25610 MCR_LIVE_CONSOLE=<stdin file> MCR_LIVE_LOG=<server log> \
//!  cargo test -p mcr-client --test live_mounts -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_client::{PlayerInput, UseTarget};

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

fn pos_of(live: &Live, id: i32) -> [f64; 3] {
    live.client.state.entities.get(id).unwrap().pos
}

fn horizontal(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn me() -> String {
    format!("Rd{}", std::process::id() % 100_000)
}

fn arena(live: &mut Live, floor: &str) {
    let name = me();
    live.console(&format!("tp {name} 0.5 101 -16.5"));
    live.until("origin chunks", 400, |l| {
        [(0, 0), (12, -12), (-12, 12)].iter().all(|&(x, z)| {
            l.client
                .state
                .chunks
                .column(mcr_world::ChunkPos::from_block(x, z))
                .is_some()
        })
    });
    live.idle(60);
    live.console("kill @e[type=!player]");
    live.console(&format!("clear {name}"));
    live.console("fill -20 99 -20 20 99 20 minecraft:stone");
    live.console(&format!("fill -12 100 -12 12 100 12 {floor}"));
    live.console("fill -20 101 -20 20 115 20 minecraft:air");
    live.console(&format!("tp {name} 0.5 101 -16.5"));
    live.idle(50);
}

fn mount(live: &mut Live, kind: &str) -> i32 {
    for _ in 0..100 {
        if find(live, kind).is_some() {
            break;
        }
        live.idle(1);
    }
    assert!(find(live, kind).is_some(), "no {kind}");
    let id = find(live, kind).unwrap();
    live.client.mount(id).unwrap();
    live.until("mounted", 100, |l| {
        l.client.state.player.vehicle == Some(id)
    });
    live.idle(10);
    id
}

fn dismount(live: &mut Live) {
    live.run(5, |_| PlayerInput {
        sneak: true,
        ..PlayerInput::default()
    });
    live.until("dismounted", 60, |l| {
        l.client.state.player.vehicle.is_none()
    });
}

fn fwd() -> PlayerInput {
    PlayerInput {
        forward: true,
        ..PlayerInput::default()
    }
}

fn check(live: &Live, what: &str, before: u32) {
    let log = live.log_text();
    assert!(
        !log.contains("moved wrongly"),
        "{what}: server rejected the vehicle movement"
    );
    assert_eq!(
        live.client.state.vehicle_corrections, before,
        "{what}: server corrected the vehicle"
    );
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_pig_boost_strider_camel_ghast_minecart() {
    let name = me();
    let mut live = Live::connect(&name);
    live.console(&format!("op {name}"));
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console(&format!("gamemode survival {name}"));
    live.console(&format!("clear {name}"));
    let only = std::env::var("MCR_MOUNTS").unwrap_or_default();
    let want = |n: &str| only.is_empty() || only.split(',').any(|m| m == n);

    if want("pig") {
        arena(&mut live, "minecraft:stone");
        live.console(&format!("give {name} minecraft:carrot_on_a_stick"));
        live.console(
            "summon minecraft:pig 0.5 101 -14.5 {equipment:{saddle:{id:\"minecraft:saddle\"}}}",
        );
        live.idle(10);
        live.client.select_hotbar(0).unwrap();
        let pig = mount(&mut live, "minecraft:pig");
        let before = live.client.state.vehicle_corrections;
        let a = pos_of(&live, pig);
        live.run(20, |_| fwd());
        let b = pos_of(&live, pig);
        live.client.start_use(UseTarget::Air).unwrap();
        live.run(40, |_| fwd());
        let c = pos_of(&live, pig);
        let slow = horizontal(a, b);
        println!(
            "pig 20 ticks {slow:.2}, then 40 ticks {:.2}",
            horizontal(b, c)
        );
        check(&live, "pig", before);
        assert!(
            horizontal(b, c) / 2.0 > slow,
            "carrot boost sped the pig up"
        );
        dismount(&mut live);
    }

    if want("strider") {
        arena(&mut live, "minecraft:lava");
        live.console(&format!("give {name} minecraft:warped_fungus_on_a_stick"));
        live.console(
            "summon minecraft:strider 0.5 101 -10.5 {equipment:{saddle:{id:\"minecraft:saddle\"}}}",
        );
        live.idle(10);
        live.client.select_hotbar(0).unwrap();
        let strider = mount(&mut live, "minecraft:strider");
        let before = live.client.state.vehicle_corrections;
        let a = pos_of(&live, strider);
        for _ in 0..6 {
            live.run(1, |_| fwd());
            if let Some(mcr_client::Sim::Mount(m)) = live.client.riding().map(|r| &r.sim) {
                println!(
                    "m pos {:?} vel {:?} ground {} hc {}",
                    m.body.pos, m.body.vel, m.body.on_ground, m.body.horizontal_collision
                );
            }
        }
        live.run(54, |_| fwd());
        let b = pos_of(&live, strider);
        println!("strider {a:?} -> {b:?}");
        check(&live, "strider", before);
        assert!(horizontal(a, b) > 2.0, "strider walked on lava");
        assert!(b[1] > 100.3, "strider floats on the lava surface");
        dismount(&mut live);
    }

    if want("camel") {
        arena(&mut live, "minecraft:stone");
        live.console(
            "summon minecraft:camel 0.5 101 -14.5 {equipment:{saddle:{id:\"minecraft:saddle\"}}}",
        );
        let camel = mount(&mut live, "minecraft:camel");
        live.idle(60);
        let before = live.client.state.vehicle_corrections;
        let a = pos_of(&live, camel);
        live.run(30, |_| fwd());
        for _ in 0..2 {
            live.run(14, |_| PlayerInput {
                forward: true,
                jump: true,
                ..PlayerInput::default()
            });
            live.run(70, |_| PlayerInput {
                forward: true,
                sprint: true,
                ..PlayerInput::default()
            });
        }
        let b = pos_of(&live, camel);
        println!("camel {a:?} -> {b:?}");
        check(&live, "camel", before);
        assert!(horizontal(a, b) > 5.0, "camel moved");
        dismount(&mut live);
    }

    if want("ghast") {
        arena(&mut live, "minecraft:stone");
        live.console("summon minecraft:happy_ghast 0.5 101 -14.5 {equipment:{body:{id:\"minecraft:blue_harness\"}}}");
        let ghast = mount(&mut live, "minecraft:happy_ghast");
        let before = live.client.state.vehicle_corrections;
        let a = pos_of(&live, ghast);
        live.run(30, |_| PlayerInput {
            jump: true,
            ..PlayerInput::default()
        });
        live.run(60, |_| fwd());
        let b = pos_of(&live, ghast);
        println!("ghast {a:?} -> {b:?}");
        check(&live, "happy ghast", before);
        assert!(b[1] > a[1] + 1.0 && horizontal(a, b) > 3.0, "ghast flew");
        dismount(&mut live);
    }

    if want("minecart") {
        arena(&mut live, "minecraft:stone");
        live.console("fill -10 101 -10 10 101 -10 minecraft:powered_rail[powered=true]");
        live.console("setblock -11 101 -10 minecraft:redstone_block");
        live.console("setblock 11 101 -10 minecraft:stone");
        live.console(&format!("tp {name} -8.5 102 -10.5"));
        live.console("summon minecraft:minecart -9.5 101.2 -9.5");
        live.idle(20);
        let cart = mount(&mut live, "minecraft:minecart");
        let a = pos_of(&live, cart);
        live.idle(60);
        let b = pos_of(&live, cart);
        let rider = live.pos();
        println!("minecart {a:?} -> {b:?} rider {rider:?}");
        assert!(horizontal(rider, b) < 1.5, "rider follows the cart");
        assert!(!live.log_text().contains(" moved wrongly"));
        dismount(&mut live);
    }
}
