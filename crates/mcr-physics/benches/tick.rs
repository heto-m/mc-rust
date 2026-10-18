use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use mcr_physics::{Aabb, BorderBounds, CollisionWorld, PlayerInput, PlayerPhysics};

struct DenseWorld {
    stone: u32,
    slab: u32,
    stairs: u32,
    fence: u32,
}

impl CollisionWorld for DenseWorld {
    fn block_state(&self, x: i32, y: i32, z: i32) -> u32 {
        if y < 64 {
            return self.stone;
        }
        let h = (x.wrapping_mul(73856093) ^ z.wrapping_mul(19349663)).rem_euclid(7);
        match (y, h) {
            (64, 0) => self.slab,
            (64, 1) => self.stairs,
            (64, 2) => self.fence,
            (64, 3) | (65, 3) => self.stone,
            _ => 0,
        }
    }
}

fn state(name: &str) -> u32 {
    let v = mcr_data::v26_3();
    v.block_info(v.block_by_name(name).unwrap())
        .unwrap()
        .default_state
}

fn bench(c: &mut Criterion) {
    let world = DenseWorld {
        stone: state("minecraft:stone"),
        slab: state("minecraft:stone_slab"),
        stairs: state("minecraft:stone_stairs"),
        fence: state("minecraft:oak_fence"),
    };
    let input = PlayerInput {
        forward: true,
        sprint: true,
        jump: true,
        yaw: 37.0,
        ..PlayerInput::default()
    };
    c.bench_function("tick_dense_sprint_jump", |b| {
        let mut p = PlayerPhysics::new([0.5, 64.0, 0.5]);
        let mut n = 0u32;
        b.iter(|| {
            n += 1;
            if n.is_multiple_of(200) {
                p.snap_to([0.5, 64.0, 0.5], [0.0; 3], 37.0, 0.0);
            }
            black_box(p.tick(black_box(&input), &world));
        });
    });
    let sneak = PlayerInput {
        forward: true,
        sneak: true,
        ..PlayerInput::default()
    };
    c.bench_function("tick_dense_sneak_walk", |b| {
        let mut p = PlayerPhysics::new([0.5, 64.0, 0.5]);
        let mut n = 0u32;
        b.iter(|| {
            n += 1;
            if n.is_multiple_of(200) {
                p.snap_to([0.5, 64.0, 0.5], [0.0; 3], 0.0, 0.0);
            }
            black_box(p.tick(black_box(&sneak), &world));
        });
    });
    c.bench_function("tick_entity_boxes_and_border", |b| {
        let mut p = PlayerPhysics::new([0.5, 64.0, 0.5]);
        p.border = BorderBounds::new(0.0, 0.0, 9.0, 2.9999984e7);
        for i in 0..64 {
            let (x, z) = (f64::from(i % 8) - 4.0, f64::from(i / 8) - 4.0);
            p.entity_colliders.push(Aabb::new(
                [x * 1.5, 64.0, z * 1.5],
                [x * 1.5 + 1.375, 64.5625, z * 1.5 + 1.375],
            ));
        }
        let mut n = 0u32;
        b.iter(|| {
            n += 1;
            if n.is_multiple_of(200) {
                p.snap_to([0.5, 65.0, 0.5], [0.0; 3], 37.0, 0.0);
            }
            black_box(p.tick(black_box(&input), &world));
        });
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
