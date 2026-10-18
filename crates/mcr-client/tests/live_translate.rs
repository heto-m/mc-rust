//! Joins a vanilla server of any supported version and checks what the translator rewrites.
//! `MCR_LIVE_ANY_VERSION=1 MCR_LIVE_ADDR=... MCR_LIVE_CONSOLE=... MCR_LIVE_LOG=... cargo test -p mcr-client --test live_translate -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_buf::BlockPos;
use mcr_client::ClientEvent;
use mcr_protocol::play::serverbound::Facing;

fn entity_names(live: &Live) -> Vec<String> {
    let data = live.client.state.data;
    live.client
        .state
        .entities
        .iter()
        .filter_map(|e| data.entity_type_name(u32::try_from(e.kind).ok()?))
        .map(str::to_owned)
        .collect()
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_translated_session() {
    let mut live = Live::connect("Xlate");
    println!("wire: {}", live.client.wire().version_name());
    live.console("op Xlate");
    live.console("kill @e[type=!player]");
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.until("chunks", 600, |l| l.client.state.chunks.len() >= 100);
    let data = live.client.state.data;
    let p = live.pos();
    let (x, y, z) = (
        p[0].floor() as i32,
        p[1].floor() as i32,
        p[2].floor() as i32,
    );

    let stairs = data.block_by_name("minecraft:oak_stairs").unwrap();
    let stairs = data.block_info(stairs).unwrap().default_state as u16;
    let at = BlockPos::new(x + 2, y, z);
    live.console(&format!("setblock {} {y} {z} minecraft:oak_stairs", at.x));
    live.until("block update", 100, |l| l.block(at) == stairs);
    println!("block update: oak stairs = state {stairs}");

    live.console("gamemode survival Xlate");
    live.console("clear Xlate");
    live.console("give Xlate minecraft:diamond_pickaxe[enchantments={efficiency:5}]");
    let dirt = data.block_by_name("minecraft:dirt").unwrap();
    let dirt = data.block_info(dirt).unwrap().default_state as u16;
    live.console(&format!("setblock {} {y} {z} minecraft:dirt", at.x));
    live.until("dirt", 100, |l| l.block(at) == dirt);
    live.idle(20);
    assert!(live.client.start_destroy(at, Facing::West).unwrap());
    let mut ticks = 0;
    while live.block(at) != 0 && ticks < 200 {
        live.idle(1);
        live.client.continue_destroy(at, Facing::West).unwrap();
        ticks += 1;
    }
    live.idle(15);
    assert_eq!(live.block(at), 0, "dirt broke and stayed broken");
    println!("dig: dirt broke after {ticks} ticks");

    live.console(&format!("summon minecraft:pig {} {y} {z}", x + 3));
    live.console(&format!(
        "summon minecraft:armor_stand {} {y} {}",
        x - 3,
        z + 1
    ));
    live.console(&format!(
        "summon minecraft:zombie {x} {y} {} {{NoAI:1b}}",
        z + 3
    ));
    live.console(&format!(
        "summon minecraft:falling_block {x} {} {z} {{BlockState:{{Name:\"minecraft:sand\"}},NoGravity:1b,Time:-1000}}",
        y + 4
    ));
    live.until("entity spawns", 100, |l| {
        let names = entity_names(l);
        ["pig", "armor_stand", "zombie", "falling_block"]
            .iter()
            .all(|n| names.contains(&format!("minecraft:{n}")))
    });
    live.idle(20);
    let pig = live
        .client
        .state
        .entities
        .iter()
        .filter(|e| data.entity_type_name(e.kind as u32) == Some("minecraft:pig"))
        .min_by(|a, b| {
            let d = |e: &&mcr_client::Entity| {
                (e.pos[0] - f64::from(x + 3)).hypot(e.pos[2] - f64::from(z))
            };
            d(a).total_cmp(&d(b))
        })
        .unwrap();
    println!(
        "{} entities, nearest pig at {:?}",
        entity_names(&live).len(),
        pig.pos
    );
    assert!((pig.pos[0] - f64::from(x + 3)).abs() < 2.0);
    assert!((pig.pos[2] - f64::from(z)).abs() < 2.0);

    live.console("say translated hello");
    live.until("chat", 100, |l| {
        l.events
            .iter()
            .any(|e| matches!(e, ClientEvent::Chat { .. }))
    });

    live.console("gamerule immediate_respawn true");
    live.console("kill Xlate");
    live.until("respawn", 200, |l| {
        l.events.iter().any(|e| matches!(e, ClientEvent::Respawned))
    });
    live.until("chunks after respawn", 600, |l| {
        l.client.state.chunks.len() >= 100
    });
    println!("respawned at {:?}", live.pos());
    live.console("gamerule immediate_respawn false");
    live.idle(5);
}
