mod common;

use common::*;
use mcr_block_entity::mcr_entity_model::glam::{DVec3, Vec3};
use mcr_block_entity::{BlockEntityData, BlockEntityKind as K, SpawnerData};

#[test]
fn extreme_positions_and_game_times_do_not_overflow() {
    let Some(mut h) = Harness::new() else { return };
    let kinds = [
        ("trial_spawner", K::TrialSpawner),
        ("spawner", K::Spawner),
        ("chest", K::Chest),
        ("white_banner", K::Banner),
        ("decorated_pot", K::DecoratedPot),
    ];
    for (i, (name, kind)) in kinds.into_iter().enumerate() {
        for pos in [
            [i32::MIN, i32::MIN, i32::MIN],
            [i32::MAX, i32::MAX, i32::MAX],
            [i as i32, 0, 0],
        ] {
            h.scene.set(
                pos,
                state(name, &[]),
                kind,
                BlockEntityData::Spawner(SpawnerData {
                    entity_id: Some("minecraft:pig".into()),
                    next_mob_spawns_at: i64::MIN,
                    ..Default::default()
                }),
            );
            h.scene.block_event(pos, 1, 1, i64::MAX);
        }
    }
    let base = h.frame(DVec3::new(0.0, 2.0, 5.0), Vec3::ZERO, 70.0, 64, 64);
    for time in [i64::MIN, 0, i64::MAX, i64::MIN, i64::MAX] {
        let mut f = base;
        f.game_time = time;
        h.scene.update(&f, None, &|_| [0, 15]);
    }
}
