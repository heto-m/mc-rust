mod common;

use common::*;
use mcr_block_entity::mcr_entity_model::glam::{DVec3, Vec3};
use mcr_block_entity::{BlockEntityData, BlockEntityKind as K};

#[test]
fn thousands_of_chests_are_few_draws() {
    let Some(mut h) = Harness::new() else { return };
    let chest = state("chest", &[("facing", "south")]);
    for i in 0..5000i32 {
        h.scene
            .set([i % 70, 0, i / 70], chest, K::Chest, BlockEntityData::None);
    }
    let frame = h.frame(
        DVec3::new(35.0, 40.0, 80.0),
        Vec3::new(35.0, 0.0, 35.0),
        60.0,
        640,
        360,
    );
    let shot = h.render(&frame, 640, 360);
    let stats = h.scene.stats();
    assert!(stats.drawn > 1000, "{stats:?}");
    assert!(stats.draw_calls <= 4, "{stats:?}");
    assert!(shot.covered() > 1000);
}

#[test]
fn far_entries_are_culled_and_far_origins_stay_precise() {
    let Some(mut h) = Harness::new() else { return };
    let chest = state("chest", &[("facing", "south")]);
    h.scene.set(
        [30_000_000, 70, -30_000_000],
        chest,
        K::Chest,
        BlockEntityData::None,
    );
    h.scene
        .set([0, 0, 0], chest, K::Chest, BlockEntityData::None);
    let cam = DVec3::new(30_000_002.0, 72.0, -29_999_998.0);
    let frame = h.frame(
        cam,
        cam.as_vec3() + Vec3::new(-4.0, -4.0, -4.0),
        70.0,
        320,
        200,
    );
    let shot = h.render(&frame, 320, 200);
    let stats = h.scene.stats();
    assert_eq!(stats.drawn, 1, "{stats:?}");
    assert!(shot.covered() > 300);
}
