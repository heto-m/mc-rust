mod common;

use common::*;
use mcr_block_entity::mcr_entity_model::glam::{DVec3, Vec3};
use mcr_block_entity::{
    BlockEntityData, BlockEntityKind as K, BrushableData, ItemRef, PistonData, ShelfData,
    SpawnerData, VaultData,
};

#[test]
fn extras() {
    let Some(mut h) = Harness::new() else { return };
    let s = &mut h.scene;
    s.set(
        [0, 0, 0],
        state("trial_spawner", &[]),
        K::TrialSpawner,
        BlockEntityData::Spawner(SpawnerData {
            entity_id: Some("minecraft:zombie".into()),
            delay: 20,
            ..Default::default()
        }),
    );
    s.set(
        [2, 0, 0],
        state("vault", &[]),
        K::Vault,
        BlockEntityData::Vault(VaultData {
            display_item: Some(ItemRef {
                id: "minecraft:emerald".into(),
                count: 1,
            }),
            ..Default::default()
        }),
    );
    let mut shelf = ShelfData::default();
    for (i, id) in ["apple", "diamond_sword", "bread"].iter().enumerate() {
        shelf.items[i] = Some(ItemRef {
            id: format!("minecraft:{id}"),
            count: 1,
        });
    }
    s.set(
        [4, 0, 0],
        state("oak_shelf", &[("facing", "south")]),
        K::Shelf,
        BlockEntityData::Shelf(shelf),
    );
    s.set(
        [6, 0, 0],
        state("suspicious_sand", &[("dusted", "2")]),
        K::Brushable,
        BlockEntityData::Brushable(BrushableData {
            item: Some(ItemRef {
                id: "minecraft:gold_nugget".into(),
                count: 1,
            }),
            hit_direction: Some(1),
        }),
    );
    s.set(
        [8, 0, 0],
        state("moving_piston", &[("facing", "up")]),
        K::Piston,
        BlockEntityData::Piston(PistonData {
            moved_state: state("stone", &[]),
            progress: 0.5,
            extending: true,
            facing: 1,
        }),
    );
    s.set(
        [10, 0, 0],
        state("copper_golem_statue", &[("facing", "south")]),
        K::CopperGolemStatue,
        BlockEntityData::None,
    );
    s.set(
        [12, 0, 0],
        state(
            "oxidized_copper_golem_statue",
            &[("facing", "south"), ("copper_golem_pose", "running")],
        ),
        K::CopperGolemStatue,
        BlockEntityData::None,
    );
    h.decor_blocks(&[
        ([0, 0, 0], state("trial_spawner", &[])),
        ([2, 0, 0], state("vault", &[])),
        ([4, 0, 0], state("oak_shelf", &[("facing", "south")])),
        ([6, 0, 0], state("suspicious_sand", &[("dusted", "2")])),
    ]);
    let frame = h.frame(
        DVec3::new(6.0, 3.0, 11.0),
        Vec3::new(6.0, 0.6, 0.5),
        50.0,
        1600,
        800,
    );
    h.advance(3, &frame);
    let shot = h.render(&frame, 1600, 800);
    println!("{}", shot.save("extras").display());
    assert!(shot.covered() > 2000);
}
