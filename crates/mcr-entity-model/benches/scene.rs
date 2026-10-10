#[path = "../tests/support/mod.rs"]
mod support;

use criterion::{Criterion, criterion_group, criterion_main};
use mcr_entity_model::EntityView;
use mcr_entity_model::mcr_protocol::types::ItemStack;
use support::*;

const TYPES: [&str; 8] = [
    "minecraft:pig",
    "minecraft:zombie",
    "minecraft:player",
    "minecraft:sheep",
    "minecraft:villager",
    "minecraft:creeper",
    "minecraft:skeleton",
    "minecraft:cow",
];

pub fn scene_update(c: &mut Criterion) {
    let Some(mut h) = Harness::new() else { return };
    let registries = TestRegistries::vanilla();
    let data = Data::default();
    let mut eq: [ItemStack; 8] = Default::default();
    eq[4] = item("minecraft:iron_helmet");
    eq[0] = item("minecraft:iron_sword");
    let views: Vec<EntityView> = (0..2000)
        .map(|i| {
            let mut v = EntityView::named(TYPES[i % TYPES.len()], &data.0);
            v.id = i as i32;
            v.pos = [(i % 50) as f64 * 1.5, 64.0, (i / 50) as f64 * 1.5];
            v.limb_swing = i as f32 * 0.3;
            v.limb_amount = 0.6;
            v.age_ticks = i as f32;
            if i % 2 == 0 {
                v.equipment = Some(&eq);
            }
            v
        })
        .collect();
    let cam = Camera {
        position: mcr_entity_model::glam::DVec3::new(30.0, 70.0, -10.0),
        target: mcr_entity_model::glam::Vec3::new(30.0, 64.0, 30.0),
        fov_deg: 70.0,
    };
    let frame = cam.frame(1280, 720);
    h.scene.update(&registries, views.iter().copied());
    h.scene.prepare(&frame);
    c.bench_function("update_2000_mixed", |b| {
        b.iter(|| h.scene.update(&registries, views.iter().copied()))
    });
    c.bench_function("prepare_2000_mixed", |b| b.iter(|| h.scene.prepare(&frame)));
    let stats = h.scene.stats();
    println!("stats: {stats:?}");
}

criterion_group!(benches, scene_update);
criterion_main!(benches);
