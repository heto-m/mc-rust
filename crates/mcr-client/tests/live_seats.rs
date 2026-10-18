//! Needs MCR_LIVE_ADDR, MCR_LIVE_CONSOLE (a file the server reads as stdin) and MCR_LIVE_LOG; run with `--ignored`.

mod common;

use common::Live;

fn me() -> String {
    format!("St{}", std::process::id() % 100_000)
}

fn server_y(live: &mut Live, target: &str) -> f64 {
    let before = live.log_text().len();
    live.console(&format!("data get entity {target} Pos[1]"));
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let text = live.log_text();
        if let Some(line) = text[before.min(text.len())..]
            .lines()
            .find(|l| l.contains("entity data:"))
        {
            let value = line.rsplit("entity data: ").next().unwrap();
            return value.trim_end_matches('d').parse().unwrap();
        }
    }
    panic!("no answer for {target}");
}

fn ride(live: &mut Live, summon: &str, kind: &str) -> i32 {
    live.console("kill @e[type=!player]");
    live.idle(10);
    live.console(summon);
    live.idle(10);
    let id = live.client.state.data.entity_type_by_name(kind).unwrap() as i32;
    let vehicle = live
        .client
        .state
        .entities
        .iter()
        .filter(|e| e.kind == id)
        .map(|e| e.id)
        .max()
        .unwrap_or_else(|| panic!("no {kind}"));
    live.client.mount(vehicle).unwrap();
    live.until("mounted", 100, |l| {
        l.client.state.player.vehicle == Some(vehicle)
    });
    live.idle(10);
    vehicle
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_seat_heights() {
    let name = me();
    let mut live = Live::connect(&name);
    live.console(&format!("op {name}"));
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console(&format!("gamemode survival {name}"));
    // The floor is built after the teleport; on low terrain the drop would kill.
    live.console(&format!(
        "effect give {name} minecraft:resistance 30 4 true"
    ));
    live.console(&format!("tp {name} 0.5 100 -16.5"));
    live.until("origin chunks", 400, |l| {
        l.client
            .state
            .chunks
            .column(mcr_world::ChunkPos::from_block(0, -12))
            .is_some()
    });
    live.idle(60);
    live.console("fill -8 99 -24 8 99 -8 minecraft:stone");
    live.console("fill -8 100 -24 8 110 -8 minecraft:air");
    live.console(&format!("tp {name} 0.5 100 -16.5"));
    live.idle(40);

    let saddle = "equipment:{saddle:{id:\"minecraft:saddle\"}}";
    let cases = [
        (
            format!("summon minecraft:horse 0.5 100 -14.5 {{Tame:1b,{saddle}}}"),
            "minecraft:horse",
            1.44375 - 0.6,
        ),
        (
            "summon minecraft:oak_boat 0.5 100 -14.5".into(),
            "minecraft:oak_boat",
            0.1875 - 0.6,
        ),
        (
            format!("summon minecraft:pig 0.5 100 -14.5 {{{saddle}}}"),
            "minecraft:pig",
            0.86875 - 0.6,
        ),
        (
            "summon minecraft:minecart 0.5 100 -14.5".into(),
            "minecraft:minecart",
            0.1875 - 0.6,
        ),
    ];
    for (summon, kind, expected) in cases {
        let vehicle = ride(&mut live, &summon, kind);
        let v = live.client.state.entities.get(vehicle).unwrap().pos;
        let feet = live.pos();
        println!(
            "{kind}: feet - vehicle = {:.4}, expected {expected:.4}",
            feet[1] - v[1]
        );
        let name = me();
        let selector = format!("@e[type={kind},limit=1,sort=nearest]");
        let server = server_y(&mut live, &name) - server_y(&mut live, &selector);
        println!("{kind}: server says {server:.4}");
        assert!(
            (server - expected).abs() < 0.02,
            "{kind} server seat height"
        );
        assert!(
            (feet[1] - v[1] - expected).abs() < 0.02,
            "{kind} seat height"
        );
        live.run(5, |_| mcr_client::PlayerInput {
            sneak: true,
            ..Default::default()
        });
        live.until("dismounted", 60, |l| {
            l.client.state.player.vehicle.is_none()
        });
    }
}
