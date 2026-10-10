use mcr_protocol::types::particle::{BlockParticleKind, Particle};
use mcr_render::glam::DVec3;
use mcr_render::{EffectImage, Environment};

use super::*;

struct Tex;

impl ParticleAssets for Tex {
    fn block_texture(&self, _: i32) -> Option<(EffectImage, [f32; 3])> {
        Some((img(), [1.0; 3]))
    }
    fn item_texture(&self, _: i32) -> Option<EffectImage> {
        Some(img())
    }
    fn named_texture(&self, _: &str) -> Option<EffectImage> {
        Some(img())
    }
}

fn img() -> EffectImage {
    EffectImage {
        width: 16,
        height: 16,
        rgba: vec![255; 16 * 16 * 4],
    }
}

fn engine() -> ParticleEngine {
    let images = vec![img(), img(), img()];
    let sets = (0..PARTICLE_NAMES.len()).map(|_| vec![0, 1, 2]).collect();
    ParticleEngine::new(ParticleAtlas::pack(images, sets), Box::new(Tex))
}

struct Floor;

impl ParticleWorld for Floor {
    fn collision_boxes(&self, _x: i32, y: i32, _z: i32, out: &mut Vec<[f32; 6]>) {
        if y < 0 {
            out.push([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        }
    }
}

fn simple(name: &str) -> Particle {
    Particle::Simple(sim::type_id(name) as i32)
}

#[test]
fn zero_count_is_directional() {
    let mut e = engine();
    e.emit(&simple("crit"), [0.0; 3], [1.0, 2.0, 3.0], 0.5, 0);
    assert_eq!(e.count(), 1);
    let p = &e.pools[Pool::Opaque as usize][0];
    assert_eq!(p.pos, DVec3::ZERO);
    assert!(p.vel.x > 0.0 && p.vel.y > p.vel.x && p.vel.z > p.vel.y);
}

#[test]
fn positive_count_spreads_around_position() {
    let mut e = engine();
    e.emit(
        &simple("flame"),
        [10.0, 64.0, 10.0],
        [2.0, 2.0, 2.0],
        0.1,
        100,
    );
    assert_eq!(e.count(), 100);
    let xs: Vec<f64> = e.pools[0].iter().map(|p| p.pos.x).collect();
    let spread =
        xs.iter().cloned().fold(f64::MIN, f64::max) - xs.iter().cloned().fold(f64::MAX, f64::min);
    assert!(spread > 1.0);
}

#[test]
fn hostile_emits_are_bounded() {
    let mut e = engine();
    e.emit(&simple("flame"), [0.0; 3], [1.0; 3], 1.0, i32::MAX);
    assert!(e.count() <= 1024);
    e.emit(&simple("flame"), [f64::NAN, 0.0, 0.0], [1.0; 3], 1.0, 5);
    e.emit(&simple("flame"), [0.0; 3], [f32::INFINITY; 3], 1.0, 5);
    e.emit(&simple("flame"), [0.0; 3], [1.0; 3], f32::NAN, 5);
    for _ in 0..100 {
        e.emit(&simple("smoke"), [0.0; 3], [1e30; 3], 1e30, 1024);
    }
    assert!(e.pools.iter().all(|p| p.len() <= POOL_CAP));
    for _ in 0..50 {
        e.tick(&OpenWorld);
    }
    assert!(e.pools.iter().flatten().all(|p| p.pos.is_finite()));
}

#[test]
fn every_type_spawns_and_stays_finite() {
    let mut e = engine();
    for id in 0..PARTICLE_NAMES.len() {
        let p = Particle::Simple(id as i32);
        e.emit(&p, [0.5, 1.0, 0.5], [0.1; 3], 0.1, 3);
    }
    e.emit(
        &Particle::Dust {
            color: 0xff8800,
            scale: 1.0,
        },
        [0.0; 3],
        [0.1; 3],
        0.1,
        5,
    );
    assert!(e.count() > 100);
    for _ in 0..400 {
        e.tick(&Floor);
        assert!(e.pools.iter().flatten().all(|p| {
            p.pos.is_finite() && p.size.is_finite() && p.color.iter().all(|c| c.is_finite())
        }));
    }
    assert!(e.count() < 200, "particles must expire");
}

#[test]
fn falling_particles_rest_on_floor() {
    let mut e = engine();
    e.add_simple("lava", DVec3::new(0.5, 3.0, 0.5), DVec3::ZERO);
    for _ in 0..60 {
        e.tick(&Floor);
    }
    assert!(e.pools.iter().flatten().all(|p| p.pos.y >= -1e-6));
}

#[test]
fn block_break_spawns_grid_and_crack_spawns_one() {
    let mut e = engine();
    e.block_break([0, 0, 0], 1, &OpenWorld);
    assert_eq!(e.pools[Pool::Terrain as usize].len(), 64);
    let before = e.count();
    e.block_crack([0, 0, 0], 1, 1, &OpenWorld);
    assert_eq!(e.count(), before + 1);
    let p = e.pools[Pool::Terrain as usize].last().unwrap();
    assert!(p.pos.y > 1.0);
}

#[test]
fn block_particles_need_a_texture() {
    let mut e = ParticleEngine::new(ParticleAtlas::pack(vec![], vec![]), Box::new(NoAssets));
    e.block_break([0, 0, 0], 1, &OpenWorld);
    assert_eq!(e.count(), 0);
}

#[test]
fn terrain_particles_use_distinct_subuvs() {
    let mut e = engine();
    e.emit(
        &Particle::BlockState {
            kind: BlockParticleKind::Block,
            state: 3,
        },
        [0.0; 3],
        [0.0; 3],
        0.0,
        0,
    );
    let p = &e.pools[Pool::Terrain as usize][0];
    assert!(p.sub[0] < 3.0 && p.sub[1] < 3.0);
}

#[test]
fn pool_cap_is_hard() {
    let mut e = engine();
    for _ in 0..40 {
        e.emit(&simple("poof"), [0.0; 3], [1.0; 3], 1.0, 1024);
    }
    assert_eq!(e.pools[0].len(), POOL_CAP);
}

#[test]
fn explosion_emitter_spawns_explosions_and_is_invisible() {
    let mut e = engine();
    e.emit(&simple("explosion_emitter"), [0.0; 3], [0.0; 3], 0.0, 0);
    for _ in 0..3 {
        e.tick(&OpenWorld);
    }
    assert!(e.count() > 6);
    let drawn = e.instance_count(DVec3::ZERO, 0.0, &Environment::default());
    assert!(drawn < e.count());
}

#[test]
fn far_particles_are_skipped_unless_forced() {
    let mut e = engine();
    e.set_camera(DVec3::ZERO);
    let far = [100.0, 0.0, 0.0];
    e.emit_inner(&simple("flame"), far, [0.0; 3], [0.0; 3], 0, false);
    assert_eq!(e.count(), 0);
    e.emit_inner(&simple("flame"), far, [0.0; 3], [0.0; 3], 0, true);
    assert_eq!(e.count(), 1);
}

#[test]
fn explosion_emitter_works_without_sprites_like_the_real_jar() {
    let images = vec![img(), img(), img()];
    let emitter = sim::type_id("explosion_emitter") as usize;
    let sets = (0..PARTICLE_NAMES.len())
        .map(|i| if i == emitter { vec![] } else { vec![0, 1, 2] })
        .collect();
    let mut e = ParticleEngine::new(ParticleAtlas::pack(images, sets), Box::new(Tex));
    e.emit(&simple("explosion_emitter"), [0.0; 3], [0.0; 3], 0.0, 0);
    assert_eq!(e.count(), 1);
    e.tick(&OpenWorld);
    assert!(e.count() > 6);
}
