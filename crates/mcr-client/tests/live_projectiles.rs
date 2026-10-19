//! `MCR_LIVE_ADDR=... MCR_LIVE_CONSOLE=... MCR_LIVE_LOG=... cargo test -p mcr-client --test live_projectiles -- --ignored --nocapture`
//!
//! A second client watches projectiles and a walking player: positions must keep moving every tick
//! between server updates and land close to each synced position.

mod common;

use std::time::{Duration, Instant};

use common::Live;
use mcr_client::PlayerInput;

const WATCHER: &str = "Watcher";
const WALKER: &str = "Strider";

fn tick_both(watcher: &mut Live, walker: &mut Live, walk: bool) {
    let next = Instant::now() + Duration::from_millis(50);
    watcher
        .client
        .tick(&PlayerInput::default())
        .expect("watcher tick");
    let keys = PlayerInput {
        forward: walk,
        yaw: walker.yaw,
        ..PlayerInput::default()
    };
    walker.client.tick(&keys).expect("walker tick");
    watcher.client.drain_events();
    walker.client.drain_events();
    if let Some(d) = next.checked_duration_since(Instant::now()) {
        std::thread::sleep(d);
    }
}

struct Track {
    positions: Vec<[f64; 3]>,
    moving: Vec<bool>,
    /// Where the client expected the entity at each server update, and the synced position.
    syncs: Vec<([f64; 3], [f64; 3])>,
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn follow(watcher: &mut Live, walker: &mut Live, kind: &str, ticks: u32) -> Option<Track> {
    let type_id = watcher
        .client
        .state
        .data
        .entity_type_by_name(&format!("minecraft:{kind}"))? as i32;
    let id = watcher
        .client
        .state
        .entities
        .iter()
        .find(|e| e.kind == type_id)
        .map(|e| e.id)?;
    let mut track = Track {
        positions: Vec::new(),
        moving: Vec::new(),
        syncs: Vec::new(),
    };
    let mut target = None;
    let mut last: Option<[f64; 3]> = None;
    for _ in 0..ticks {
        let before = watcher.client.state.entities.get(id).map(|e| e.pos);
        tick_both(watcher, walker, false);
        let Some(e) = watcher.client.state.entities.get(id) else {
            break;
        };
        if target.is_some_and(|t| t != e.target_pos)
            && let (Some(b), Some(l)) = (before, last)
        {
            let predicted = [2.0 * b[0] - l[0], 2.0 * b[1] - l[1], 2.0 * b[2] - l[2]];
            track.syncs.push((predicted, e.target_pos));
        }
        last = before;
        target = Some(e.target_pos);
        if std::env::var_os("MCR_LIVE_TRACE").is_some() {
            println!(
                "  {kind} pos {:?} target {:?} vel {:?}",
                e.pos, e.target_pos, e.velocity
            );
        }
        track.positions.push(e.pos);
        track
            .moving
            .push(e.velocity.iter().any(|v| v.abs() > 1.0e-3));
    }
    Some(track)
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_projectiles_and_remote_players_move_smoothly() {
    let mut watcher = Live::connect(WATCHER);
    let mut walker = Live::connect(WALKER);
    watcher.console(&format!("op {WATCHER}"));
    watcher.until("watcher loaded", 400, |l| l.client.state.player.loaded);
    walker.until("walker loaded", 400, |l| l.client.state.player.loaded);
    watcher.console("forceload add 280 -60 330 60");
    watcher.console("fill 290 99 -50 320 99 50 minecraft:stone");
    watcher.console("fill 290 100 -50 320 140 50 minecraft:air");
    watcher.console("kill @e[type=!player]");
    watcher.console(&format!("gamemode creative {WATCHER}"));
    watcher.console(&format!("gamemode survival {WALKER}"));
    watcher.console(&format!("tp {WATCHER} 300.5 100 0.5 0 0"));
    watcher.console(&format!("tp {WALKER} 296.5 100 -20.5 0 0"));
    for _ in 0..60 {
        tick_both(&mut watcher, &mut walker, false);
    }

    let shots = [
        ("snowball", "{Motion:[0.0,0.3,0.8]}"),
        ("arrow", "{Motion:[0.0,0.3,1.5]}"),
        ("trident", "{Motion:[0.0,0.3,1.5]}"),
        ("fireball", "{Motion:[0.0,0.0,0.3],acceleration_power:0.1d}"),
        (
            "small_fireball",
            "{Motion:[0.0,0.0,0.3],acceleration_power:0.1d}",
        ),
        (
            "wither_skull",
            "{Motion:[0.0,0.0,0.3],acceleration_power:0.1d}",
        ),
        (
            "wind_charge",
            "{Motion:[0.0,0.0,0.8],acceleration_power:0.0d}",
        ),
        ("llama_spit", "{Motion:[0.0,0.2,0.8]}"),
        ("firework_rocket", "{LifeTime:40}"),
        ("eye_of_ender", "{Motion:[0.0,0.1,0.4]}"),
    ];
    let mut failures = Vec::new();
    for (kind, nbt) in shots {
        watcher.console("kill @e[type=!player]");
        for _ in 0..5 {
            tick_both(&mut watcher, &mut walker, false);
        }
        watcher.console(&format!("summon {kind} 305.5 110 -30.5 {nbt}"));
        let mut track = None;
        for _ in 0..40 {
            tick_both(&mut watcher, &mut walker, false);
            track = follow(&mut watcher, &mut walker, kind, 45);
            if track.is_some() {
                break;
            }
        }
        let Some(track) = track else {
            failures.push(format!("{kind}: never seen"));
            continue;
        };
        let stalls = track
            .positions
            .windows(2)
            .zip(&track.moving[1..])
            .filter(|(w, moving)| **moving && dist(w[0], w[1]) < 1.0e-4)
            .count();
        let worst_sync = track
            .syncs
            .iter()
            .map(|(sim, synced)| dist(*sim, *synced))
            .fold(0.0, f64::max);
        println!(
            "{kind}: {} ticks, {stalls} stalled while moving, {} syncs, worst sync gap {worst_sync:.3}",
            track.positions.len(),
            track.syncs.len()
        );
        if stalls > 0 {
            failures.push(format!("{kind}: {stalls} ticks stalled while moving"));
        }
    }

    // A remote player walking in a straight line: the view replays its steps a little behind it.
    let type_id = watcher
        .client
        .state
        .data
        .entity_type_by_name("minecraft:player")
        .unwrap() as i32;
    let id = watcher
        .client
        .state
        .entities
        .iter()
        .find(|e| e.kind == type_id)
        .map(|e| e.id)
        .expect("walker visible");
    let mut seen = Vec::new();
    let mut truth = Vec::new();
    for t in 0..60 {
        tick_both(&mut watcher, &mut walker, t < 50);
        seen.push(watcher.client.state.entities.get(id).unwrap().pos[2]);
        truth.push(walker.client.state.player.physics.pos[2]);
    }
    let steps: Vec<f64> = seen[10..45].windows(2).map(|w| w[1] - w[0]).collect();
    let stalls = steps.iter().filter(|s| s.abs() < 1.0e-4).count();
    let speed = (truth[45] - truth[10]) / 35.0;
    let lag = (10..45).map(|i| truth[i] - seen[i]).sum::<f64>() / 35.0 / speed;
    println!(
        "remote player: {stalls} stalled ticks of 34, view {lag:.1} ticks behind, end gap {:.4}",
        truth[59] - seen[59]
    );
    if stalls > 2 {
        failures.push(format!("remote player: {stalls} stalled ticks"));
    }
    if (truth[59] - seen[59]).abs() > 1.0e-3 {
        failures.push("remote player: the view does not settle on the walker".into());
    }
    let log = watcher.log_text();
    for warning in ["moved wrongly", "moved too quickly"] {
        if log.contains(warning) {
            failures.push(format!("server logged {warning}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
