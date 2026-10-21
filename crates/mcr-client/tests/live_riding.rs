//! Nautilus dash and a loyal trident against a vanilla server:
//! `MCR_LIVE_ADDR=... MCR_LIVE_CONSOLE=... MCR_LIVE_LOG=... cargo test -p mcr-client --test live_riding -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_buf::IdOr;
use mcr_client::{ClientEvent, PlayerInput, UseTarget};

const NAME: &str = "Rider5";

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

fn sounds(live: &Live) -> Vec<String> {
    let names = live.client.state.data.registry("minecraft:sound_event");
    live.events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::Sound {
                sound: IdOr::Id(id),
                ..
            } => names.and_then(|r| r.name_of(*id as u32)).map(str::to_owned),
            ClientEvent::Sound {
                sound: IdOr::Inline(s),
                ..
            } => Some(s.location.as_str().to_owned()),
            _ => None,
        })
        .collect()
}

fn owner_tag(live: &Live) -> String {
    let b = live.client.state.uuid.as_u128();
    let ints: Vec<String> = (0..4)
        .map(|i| ((b >> (96 - 32 * i)) as u32 as i32).to_string())
        .collect();
    format!("[I;{}]", ints.join(","))
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_nautilus_dash_and_loyal_trident() {
    let mut live = Live::connect(NAME);
    live.console(&format!("op {NAME}"));
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console(&format!("gamemode survival {NAME}"));
    live.console(&format!(
        "effect give {NAME} minecraft:resistance 60 4 true"
    ));
    live.console(&format!(
        "effect give {NAME} minecraft:water_breathing 60 0 true"
    ));
    live.console(&format!("tp {NAME} 0.5 101 0.5"));
    live.idle(60);
    live.console("kill @e[type=!player]");
    live.console(&format!("clear {NAME}"));
    live.console("fill -12 98 -12 12 98 12 minecraft:stone");
    live.console("fill -10 99 -10 10 104 10 minecraft:water");
    live.console("fill -12 105 -12 12 110 12 minecraft:air");
    live.idle(20);
    live.until("old entities gone", 100, |l| {
        find(l, "minecraft:nautilus").is_none() && find(l, "minecraft:trident").is_none()
    });
    let owner = owner_tag(&live);
    live.console(&format!(
        "summon minecraft:nautilus 0.5 101 0.5 {{Owner:{owner},equipment:{{saddle:{{id:\"minecraft:saddle\"}}}}}}"
    ));
    live.until("nautilus", 100, |l| find(l, "minecraft:nautilus").is_some());
    let nautilus = find(&live, "minecraft:nautilus").unwrap();
    live.client.mount(nautilus).unwrap();
    live.until("mounted", 100, |l| {
        l.client.state.player.vehicle == Some(nautilus)
    });
    live.idle(10);
    let before = live.client.state.vehicle_corrections;
    assert_eq!(live.client.jump_bar(), Some((0.0, false)), "dash ready");
    live.events.clear();
    live.run(8, |_| PlayerInput {
        jump: true,
        forward: true,
        ..PlayerInput::default()
    });
    live.run(1, |_| PlayerInput {
        forward: true,
        ..PlayerInput::default()
    });
    assert_eq!(
        live.client.jump_bar().map(|b| b.1),
        Some(true),
        "dash cooling down"
    );
    live.run(50, |_| PlayerInput {
        forward: true,
        ..PlayerInput::default()
    });
    assert_eq!(
        live.client.jump_bar().map(|b| b.1),
        Some(false),
        "ready again"
    );
    let heard = sounds(&live);
    println!("sounds while dashing: {heard:?}");
    assert!(
        heard.iter().any(|s| s.ends_with("nautilus.dash")),
        "dash sound"
    );
    assert!(
        heard.iter().any(|s| s.contains("nautilus.dash_ready")),
        "dash ready sound"
    );
    assert_eq!(
        live.client.state.vehicle_corrections, before,
        "server corrected the nautilus"
    );
    live.run(5, |_| PlayerInput {
        sneak: true,
        ..PlayerInput::default()
    });
    live.until("dismounted", 60, |l| {
        l.client.state.player.vehicle.is_none()
    });

    live.console("fill -12 99 -12 12 110 12 minecraft:air");
    live.console("fill -12 99 -12 12 99 12 minecraft:stone");
    live.console(&format!("tp {NAME} 0.5 100 0.5 0 0"));
    live.console(&format!(
        "give {NAME} minecraft:trident[enchantments={{\"minecraft:loyalty\":3}}]"
    ));
    live.idle(30);
    live.yaw = 0.0;
    live.client.start_use(UseTarget::Air).unwrap();
    live.run(15, |_| PlayerInput {
        pitch: 30.0,
        ..PlayerInput::default()
    });
    live.client.release_use().unwrap();
    live.until("trident thrown", 40, |l| {
        find(l, "minecraft:trident").is_some()
    });
    let trident = find(&live, "minecraft:trident").unwrap();
    live.until("trident landed", 100, |l| {
        l.client
            .state
            .entities
            .get(trident)
            .is_some_and(|e| e.velocity == [0.0; 3])
    });
    let landed = live.client.state.entities.get(trident).unwrap().pos;
    let mut ticks = 0;
    while live
        .client
        .state
        .entities
        .get(trident)
        .is_some_and(|e| e.pos == landed)
        && ticks < 40
    {
        live.idle(1);
        ticks += 1;
    }
    println!("trident left the ground after {ticks} ticks at {landed:?}");
    assert!(ticks >= 5, "stayed in the ground for five ticks");
    live.until("trident back", 200, |l| {
        l.client.state.entities.get(trident).is_none()
    });
    live.idle(10);
    let trident_item = live
        .client
        .state
        .data
        .item_by_name("minecraft:trident")
        .unwrap() as i32;
    assert!(
        live.client
            .state
            .inventory
            .player
            .slots
            .iter()
            .any(|s| s.item == trident_item),
        "the trident came back"
    );
    let log = live.log_text();
    assert!(!log.contains("moved wrongly") && !log.contains("moved too quickly"));
}
