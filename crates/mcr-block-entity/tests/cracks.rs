mod common;

use common::*;
use mcr_block_entity::mcr_entity_model::glam::{DVec3, Vec3};
use mcr_block_entity::{BlockEntityData, BlockEntityKind as K};

#[test]
fn cracking_chest_reports_its_posed_model_quads() {
    let Some(mut h) = Harness::new() else { return };
    let chest = state("chest", &[("facing", "north")]);
    h.scene
        .set([0, 0, 0], chest, K::Chest, BlockEntityData::None);
    h.scene
        .set([3, 0, 0], chest, K::Chest, BlockEntityData::None);
    let frame = h.frame(
        DVec3::new(1.5, 1.5, 4.0),
        Vec3::new(1.5, 0.5, 0.5),
        70.0,
        64,
        64,
    );
    h.scene.set_cracking([[0, 0, 0]]);
    h.scene.update(&frame, None, &|_| [0, 15]);
    let closed = h.scene.crack_quads([0, 0, 0]).to_vec();
    assert!(h.scene.crack_quads([3, 0, 0]).is_empty());
    assert_eq!(closed.len(), 18, "base, lid and lock boxes");
    let corners = closed.iter().flatten();
    assert!(
        corners
            .clone()
            .all(|p| p.iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v)))
    );
    let top = corners.map(|p| p[1]).fold(0.0f32, f32::max);
    assert!((top - 14.0 / 16.0).abs() < 1e-4, "{top}");

    h.scene.block_event([0, 0, 0], 1, 1, 0);
    h.advance(10, &frame);
    h.scene.update(&frame, None, &|_| [0, 15]);
    let open = h.scene.crack_quads([0, 0, 0]);
    let top = open.iter().flatten().map(|p| p[1]).fold(0.0f32, f32::max);
    assert!(top > 1.2, "the lid swings up: {top}");

    h.scene.set_cracking([]);
    h.scene.update(&frame, None, &|_| [0, 15]);
    assert!(h.scene.crack_quads([0, 0, 0]).is_empty());
}
