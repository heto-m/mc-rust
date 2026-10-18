//! Long-session bounds: server-keyed tables stay capped and world-scoped state goes with its world.

use std::time::Instant;

use mcr_buf::{BlockPos, Identifier};
use mcr_protocol::dispatch::PlayClientbound as P;
use mcr_protocol::play::clientbound::player::GameRuleValues;
use mcr_protocol::play::clientbound::ui::Pairs;
use mcr_protocol::play::clientbound::world::{
    ClockUpdate, SetChunkCacheCenter, SetChunkCacheRadius, SetTime,
};
use mcr_world::{ChunkColumn, ChunkPos};
use uuid::Uuid;

use crate::config::ClientConfig;
use crate::events::ClientEvent;
use crate::state::{ClientState, MAX_CLOCKS, MAX_GAME_RULES, Outbox};

fn state() -> ClientState {
    ClientState::new(&ClientConfig::default(), Uuid::nil())
}

fn run(s: &mut ClientState, packet: P<'_>) -> Outbox {
    let mut out = Outbox::default();
    s.handle_play(packet, &mut out, Instant::now()).unwrap();
    out
}

#[test]
fn server_chosen_clock_ids_stay_bounded() {
    let mut s = state();
    for round in 0..1000 {
        let clock_updates = (0..64)
            .map(|i| ClockUpdate {
                clock: round * 64 + i,
                total_ticks: 0,
                partial_tick: 0.0,
                rate: 1.0,
            })
            .collect();
        run(
            &mut s,
            P::SetTime(SetTime {
                game_time: 0,
                clock_updates,
            }),
        );
    }
    assert_eq!(s.world.clocks.len(), MAX_CLOCKS);
}

#[test]
fn game_rules_stay_bounded_in_count_and_length() {
    let mut s = state();
    for round in 0..64 {
        let values = (0..1024)
            .map(|i| {
                let key = Identifier::parse(&format!("soak:rule_{round}_{i}")).unwrap();
                (key, "v".repeat(10_000))
            })
            .collect();
        run(
            &mut s,
            P::GameRuleValues(GameRuleValues {
                values: Pairs(values),
            }),
        );
    }
    assert_eq!(s.server.game_rules.len(), MAX_GAME_RULES);
    assert!(s.server.game_rules.values().all(|v| v.len() <= 256));
}

fn fill_around(s: &mut ClientState, cx: i32, cz: i32) {
    let cfg = *s.chunks.config();
    let r = s.chunk_storage_radius();
    for x in cx - r..=cx + r {
        for z in cz - r..=cz + r {
            s.chunks
                .insert(ChunkPos::new(x, z), ChunkColumn::empty(&cfg));
        }
    }
}

#[test]
fn far_centre_jumps_and_radius_shrinks_drop_old_columns() {
    let mut s = state();
    s.world.view_distance = 4;
    let per_view = (2 * s.chunk_storage_radius() + 1).pow(2) as usize;
    for hop in 0..50 {
        let (cx, cz) = (hop * 10_000, -hop * 7_000);
        fill_around(&mut s, cx, cz);
        run(
            &mut s,
            P::SetChunkCacheCenter(SetChunkCacheCenter { x: cx, z: cz }),
        );
        assert!(s.chunks.len() <= per_view, "hop {hop}: {}", s.chunks.len());
    }
    run(
        &mut s,
        P::SetChunkCacheRadius(SetChunkCacheRadius { radius: 2 }),
    );
    assert_eq!(
        s.chunks.len(),
        (2 * s.chunk_storage_radius() + 1).pow(2) as usize
    );
    run(
        &mut s,
        P::SetChunkCacheCenter(SetChunkCacheCenter {
            x: i32::MAX,
            z: i32::MIN,
        }),
    );
    assert_eq!(s.chunks.len(), 0);
}

#[test]
fn reconfiguration_drops_world_scoped_state() {
    let mut s = state();
    s.camera_entity = Some(42);
    s.controlled_vehicle = Some(43);
    s.server
        .set_game_rule("minecraft:keep_inventory", "true".into());
    s.world.set_clock(
        0,
        crate::state::ClockState {
            ticks: 6000.0,
            rate: 1.0,
        },
    );
    s.predictions.retain(BlockPos::new(1, 2, 3), 5, 0, [0.0; 3]);
    s.reset_for_reconfiguration();
    assert_eq!(s.camera_entity, None);
    assert_eq!(s.controlled_vehicle, None);
    assert!(s.server.game_rules.is_empty());
    assert!(s.world.clocks.is_empty());
    assert_eq!(s.predictions.len(), 0);
    assert_eq!(s.gameplay, Default::default());
}

#[test]
fn reconfiguring_is_reported_to_the_app() {
    let mut s = state();
    let out = run(
        &mut s,
        P::StartConfiguration(mcr_protocol::play::clientbound::player::StartConfiguration),
    );
    assert!(
        out.events
            .iter()
            .any(|e| matches!(e, ClientEvent::Reconfiguring))
    );
}
