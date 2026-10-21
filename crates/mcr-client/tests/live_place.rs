//! `MCR_LIVE_ADDR=... MCR_LIVE_CONSOLE=... MCR_LIVE_LOG=... cargo test -p mcr-client --test live_place -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_buf::BlockPos;
use mcr_client::{UseResult, UseTarget};
use mcr_protocol::play::serverbound::Facing;

const ITEMS: [&str; 9] = [
    "minecraft:oak_stairs",
    "minecraft:oak_slab",
    "minecraft:oak_log",
    "minecraft:furnace",
    "minecraft:piston",
    "minecraft:oak_leaves",
    "minecraft:iron_chain",
    "minecraft:stone",
    "minecraft:white_shulker_box",
];

fn describe(live: &Live, state: u32) -> String {
    let data = live.client.state.data;
    let props: Vec<String> = data
        .state_properties(state)
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    format!(
        "{}[{}]",
        data.state_block_name(state).unwrap_or("?"),
        props.join(",")
    )
}

fn rel(pos: BlockPos, face: Facing) -> BlockPos {
    let s = match face {
        Facing::Down => [0, -1, 0],
        Facing::Up => [0, 1, 0],
        Facing::North => [0, 0, -1],
        Facing::South => [0, 0, 1],
        Facing::West => [-1, 0, 0],
        Facing::East => [1, 0, 0],
    };
    BlockPos::new(pos.x + s[0], pos.y + s[1], pos.z + s[2])
}

/// Places with the given hotbar slot and returns the predicted state, which must survive the ack.
fn place(
    live: &mut Live,
    slot: u8,
    (pos, face, cursor): (BlockPos, Facing, [f32; 3]),
    at: BlockPos,
) -> String {
    live.client.select_hotbar(slot).unwrap();
    live.idle(2);
    let r = live
        .client
        .start_use(UseTarget::Block {
            pos,
            face,
            cursor,
            inside: false,
        })
        .unwrap();
    assert_eq!(r, UseResult::Success, "{}", ITEMS[slot as usize]);
    let predicted = live.block(at);
    assert!(
        !live.client.state.predictions.is_empty(),
        "{} was predicted",
        ITEMS[slot as usize]
    );
    live.idle(10);
    assert!(live.client.state.predictions.is_empty());
    let server = live.block(at);
    let (p, s) = (describe(live, predicted), describe(live, server));
    println!("{p} -> server {s}");
    assert_eq!(p, s, "prediction matches the server");
    s
}

fn on(pos: BlockPos, face: Facing, cursor: [f32; 3]) -> ((BlockPos, Facing, [f32; 3]), BlockPos) {
    ((pos, face, cursor), rel(pos, face))
}

const TOP: [f32; 3] = [0.5, 1.0, 0.5];

#[test]
#[ignore = "needs a running vanilla server"]
fn live_placement_prediction() {
    let mut live = Live::connect("Placer");
    live.console("op Placer");
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console("gamemode survival Placer");
    live.console("clear Placer");
    for (i, item) in ITEMS.iter().enumerate() {
        let n = if item.ends_with("shulker_box") { 1 } else { 16 };
        live.console(&format!(
            "item replace entity Placer hotbar.{i} with {item} {n}"
        ));
    }
    live.idle(20);
    let start = live.log_text().len();
    let p = live.pos();
    let (x, y, z) = (
        p[0].floor() as i32 + 2,
        p[1].floor() as i32 - 1,
        p[2].floor() as i32,
    );
    let ground = |dx: i32, dz: i32| BlockPos::new(x + dx, y, z + dz);
    live.console(&format!(
        "fill {} {} {} {} {} {} air",
        x - 1,
        y + 1,
        z - 4,
        x + 4,
        y + 4,
        z + 4
    ));
    live.idle(10);

    live.yaw = 180.0;
    let (c, at) = on(ground(1, 0), Facing::Up, TOP);
    let s = place(&mut live, 0, c, at);
    assert!(
        s.contains("facing=north") && s.contains("half=bottom"),
        "{s}"
    );
    live.yaw = -90.0;
    let (c, at) = on(ground(0, 0), Facing::Up, TOP);
    let s = place(&mut live, 0, c, at);
    assert!(s.contains("shape=outer_left"), "{s}");

    let (c, pillar) = on(ground(2, 2), Facing::Up, TOP);
    place(&mut live, 7, c, pillar);
    live.yaw = 0.0;
    let (c, at) = on(pillar, Facing::North, [0.5, 0.8, 0.0]);
    let s = place(&mut live, 0, c, at);
    assert!(s.contains("facing=south") && s.contains("half=top"), "{s}");
    let (c, slab) = on(pillar, Facing::West, [0.0, 0.8, 0.5]);
    let s = place(&mut live, 1, c, slab);
    assert!(s.contains("type=top"), "{s}");
    let s = place(&mut live, 1, (slab, Facing::Down, [0.5, 0.5, 0.5]), slab);
    assert!(s.contains("type=double"), "{s}");
    let (c, at) = on(pillar, Facing::East, [1.0, 0.5, 0.5]);
    let s = place(&mut live, 2, c, at);
    assert!(s.contains("axis=x"), "{s}");
    let (c, at) = on(ground(2, -2), Facing::Up, TOP);
    let s = place(&mut live, 3, c, at);
    assert!(s.contains("facing=north"), "{s}");
    let (c, at) = on(ground(0, -2), Facing::Up, TOP);
    let s = place(&mut live, 4, c, at);
    assert!(s.contains("facing=north"), "{s}");
    let (c, at) = on(pillar, Facing::Up, TOP);
    let s = place(&mut live, 5, c, at);
    assert!(s.contains("persistent=true"), "{s}");
    let (c, at) = on(ground(1, -3), Facing::Up, TOP);
    let s = place(&mut live, 6, c, at);
    assert!(s.contains("axis=y"), "{s}");
    let (c, at) = on(ground(0, 3), Facing::Up, TOP);
    let s = place(&mut live, 8, c, at);
    assert!(s.contains("facing=up"), "{s}");

    let log = &live.log_text()[start..];
    for warn in ["moved wrongly", "moved too quickly", "WARN"] {
        assert!(!log.contains(warn), "server warned: {log}");
    }
}
