use mcr_protocol::play::clientbound::world::{LevelParticles, ParticleRandomization};
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

fn packet(particle: Particle, at: [f64; 3]) -> LevelParticles {
    LevelParticles {
        particle,
        override_limiter: false,
        always_show: false,
        x: at[0],
        y: at[1],
        z: at[2],
        x_dist: 0.0,
        y_dist: 0.0,
        z_dist: 0.0,
        x_max_speed: 0.0,
        y_max_speed: 0.0,
        z_max_speed: 0.0,
        count: 0,
        randomization: ParticleRandomization::Default,
    }
}

#[test]
fn override_limiter_reaches_beyond_32_blocks() {
    let mut e = engine();
    e.set_camera(DVec3::ZERO);
    let far = [100.0, 0.0, 0.0];
    e.emit_packet(&packet(simple("flame"), far));
    assert_eq!(e.count(), 0);
    let mut always = packet(simple("flame"), far);
    always.always_show = true;
    e.emit_packet(&always);
    assert_eq!(
        e.count(),
        0,
        "always_show only affects the particle setting"
    );
    let mut forced = packet(simple("flame"), far);
    forced.override_limiter = true;
    e.emit_packet(&forced);
    assert_eq!(e.count(), 1);
    e.emit_packet(&packet(simple("sonic_boom"), far));
    assert_eq!(e.count(), 2, "limiter-overriding types ignore distance");
    e.add_particle(&simple("flame"), [10.0, 0.0, 0.0], [0.0; 3]);
    assert_eq!(e.count(), 3);
}

#[test]
fn minimal_particles_drop_all_but_forced() {
    let mut e = engine();
    e.set_density(Density::Minimal);
    for _ in 0..50 {
        e.add_particle(&simple("flame"), [0.0; 3], [0.0; 3]);
    }
    assert_eq!(e.count(), 0);
    e.add_particle(&simple("explosion"), [0.0; 3], [0.0; 3]);
    assert_eq!(e.count(), 1);
    let mut always = packet(simple("flame"), [0.0; 3]);
    always.always_show = true;
    always.count = 1000;
    e.emit_packet(&always);
    let shown = e.count() - 1;
    assert!(
        shown > 20 && shown < 200,
        "always_show rolls 1 in 10, got {shown}"
    );
}

#[test]
fn alternative_randomization_stays_in_the_box() {
    let mut e = engine();
    let mut p = packet(simple("flame"), [0.0; 3]);
    p.count = 200;
    p.x_dist = 2.0;
    p.y_dist = 1.0;
    p.z_dist = 3.0;
    p.x_max_speed = 0.5;
    p.randomization = ParticleRandomization::Alternative;
    e.emit_packet(&p);
    assert_eq!(e.count(), 200);
    // Flame jitters its spawn by at most 0.05 on each axis.
    assert!(e.pools[0].iter().all(|q| {
        (-0.06..2.06).contains(&q.start.x)
            && (-0.06..1.06).contains(&q.start.y)
            && (-0.06..3.06).contains(&q.start.z)
    }));
}

#[test]
fn every_particle_type_has_its_own_provider() {
    for name in PARTICLE_NAMES {
        let (kind, _) = sim::kind_of(name);
        assert_ne!(
            kind,
            sim::Kind::Generic,
            "{name} falls back to the generic particle"
        );
    }
}

#[test]
fn falling_leaves_live_300_ticks_with_gentle_gravity() {
    let mut e = engine();
    e.add_simple("cherry_leaves", DVec3::new(0.0, 100.0, 0.0), DVec3::ZERO);
    e.add_simple("pale_oak_leaves", DVec3::new(0.0, 100.0, 0.0), DVec3::ZERO);
    let cherry = &e.pools[Pool::Opaque as usize][0];
    assert_eq!(cherry.life, 300);
    assert!((cherry.gravity - 0.25 * 1.2 * 0.0025).abs() < 1e-7);
    let pale = &e.pools[Pool::Opaque as usize][1];
    assert!((pale.gravity - 0.07 * 1.2 * 0.0025).abs() < 1e-7);
    assert!((pale.vel.y + 0.021).abs() < 1e-6);
    let y0 = pale.pos.y;
    for _ in 0..100 {
        e.tick(&OpenWorld);
    }
    let left: Vec<_> = e.pools[Pool::Opaque as usize].iter().collect();
    assert!(
        !left.is_empty(),
        "leaves outlive the default particle lifetime"
    );
    assert!(left.iter().all(|p| p.pos.y < y0 && p.roll != 0.0));
    for _ in 0..250 {
        e.tick(&OpenWorld);
    }
    assert_eq!(e.count(), 0);
}

#[test]
fn falling_leaves_stop_on_the_ground() {
    let mut e = engine();
    e.add_simple("pale_oak_leaves", DVec3::new(0.5, 0.05, 0.5), DVec3::ZERO);
    for _ in 0..20 {
        e.tick(&Floor);
    }
    assert_eq!(e.count(), 0);
}

#[test]
fn tinted_leaves_and_falling_dust_take_their_colour() {
    let mut e = engine();
    e.emit(
        &Particle::Color {
            kind: mcr_protocol::types::particle::ColorParticleKind::TintedLeaves,
            argb: 0xFF20_4080u32 as i32,
        },
        [0.0; 3],
        [0.0; 3],
        0.0,
        0,
    );
    let p = &e.pools[0][0];
    assert!((p.color[0] - 0x20 as f32 / 255.0).abs() < 1e-6);
    assert!((p.color[2] - 0x80 as f32 / 255.0).abs() < 1e-6);
    e.emit(
        &Particle::BlockState {
            kind: BlockParticleKind::FallingDust,
            state: 5,
        },
        [0.0; 3],
        [0.0; 3],
        0.0,
        0,
    );
    assert_eq!(e.pools[0][1].kind, sim::Kind::FallingDust);
}

#[test]
fn dust_pillar_uses_the_block_texture() {
    let mut e = engine();
    e.emit(
        &Particle::BlockState {
            kind: BlockParticleKind::DustPillar,
            state: 3,
        },
        [0.0; 3],
        [0.0, 1.0, 0.0],
        1.0,
        0,
    );
    let p = &e.pools[Pool::Terrain as usize][0];
    assert!((20..40).contains(&p.life));
}

#[test]
fn elder_guardian_particles_raise_the_ghost_instead_of_drawing() {
    let mut e = engine();
    e.emit(&simple("elder_guardian"), [0.0; 3], [0.0; 3], 0.0, 0);
    assert_eq!(e.count(), 0);
    assert!(e.take_elder_guardian());
    assert!(!e.take_elder_guardian());
}

#[test]
fn vibrations_and_trails_arrive_at_their_target() {
    use mcr_protocol::types::math::Vec3d;
    use mcr_protocol::types::particle::PositionSource;
    let mut e = engine();
    e.emit(
        &Particle::Vibration {
            source: PositionSource::Block(mcr_buf::BlockPos { x: 10, y: 0, z: 0 }),
            arrival_ticks: 10,
        },
        [0.5, 0.5, 0.5],
        [0.0; 3],
        0.0,
        0,
    );
    e.emit(
        &Particle::Trail {
            target: Vec3d {
                x: 0.0,
                y: 5.0,
                z: 0.0,
            },
            color: 0xFF0000,
            duration: 5,
        },
        [0.0; 3],
        [0.0; 3],
        0.0,
        0,
    );
    e.emit(
        &Particle::Vibration {
            source: PositionSource::Block(mcr_buf::BlockPos { x: 0, y: 0, z: 0 }),
            arrival_ticks: i32::MAX,
        },
        [0.5, 0.5, 0.5],
        [0.0; 3],
        0.0,
        0,
    );
    assert_eq!(e.count(), 3);
    for _ in 0..5 {
        e.tick(&OpenWorld);
    }
    let trail = e.pools[Pool::Opaque as usize]
        .iter()
        .find(|p| p.kind == sim::Kind::Trail)
        .unwrap();
    assert!((trail.pos.y - 5.0).abs() < 1e-9);
    for _ in 0..5 {
        e.tick(&OpenWorld);
    }
    let v = &e.pools[Pool::Translucent as usize][0];
    assert!((v.pos.x - 10.5).abs() < 1e-9);
    assert!(v.life <= 1200, "arrival ticks are bounded");
    assert!(e.instance_count(DVec3::ZERO, 0.5, &Environment::default()) >= 1);
}

#[test]
fn spore_blossom_air_is_limited() {
    let mut e = engine();
    for _ in 0..1200 {
        e.add_simple("spore_blossom_air", DVec3::ZERO, DVec3::ZERO);
    }
    assert_eq!(e.count(), sim::SPORE_LIMIT);
    e.clear();
    e.add_simple("spore_blossom_air", DVec3::ZERO, DVec3::ZERO);
    assert_eq!(e.count(), 1);
}

#[test]
fn hostile_particle_options_are_bounded() {
    let mut e = engine();
    e.emit(
        &Particle::Geyser {
            kind: mcr_protocol::types::particle::GeyserParticleKind::Geyser,
            water_blocks: i32::MAX,
        },
        [0.0; 3],
        [0.0; 3],
        0.0,
        0,
    );
    e.emit(
        &Particle::Shriek { delay: i32::MAX },
        [0.0; 3],
        [0.0; 3],
        0.0,
        0,
    );
    e.emit(
        &Particle::DragonBreath { power: f32::NAN },
        [0.0; 3],
        [0.0; 3],
        0.0,
        0,
    );
    e.emit(
        &Particle::SculkCharge {
            roll: f32::INFINITY,
        },
        [0.0; 3],
        [0.0; 3],
        0.0,
        0,
    );
    for _ in 0..40 {
        e.tick(&OpenWorld);
        assert!(e.count() < 4000);
        assert!(
            e.pools
                .iter()
                .flatten()
                .all(|p| p.pos.is_finite() && p.roll.is_finite())
        );
    }
}

#[test]
fn emitters_spawn_children_and_stay_invisible() {
    let mut e = engine();
    e.emit(&simple("gust_emitter_large"), [0.0; 3], [0.0; 3], 0.0, 0);
    e.tick(&OpenWorld);
    let gusts = e.pools[0]
        .iter()
        .filter(|p| p.kind == sim::Kind::Gust)
        .count();
    assert_eq!(gusts, 3);
    for _ in 0..10 {
        e.tick(&OpenWorld);
    }
    assert!(
        !e.pools[0]
            .iter()
            .any(|p| matches!(p.kind, sim::Kind::Emitter(_)))
    );
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

fn spark(trail: bool, twinkle: bool) -> Spark {
    Spark {
        color: 0xFF0000,
        fade: Some(0x0000FF),
        trail,
        twinkle,
        trailed: false,
    }
}

#[test]
fn firework_sparks_carry_colour_and_fall() {
    let mut e = engine();
    e.firework_spark([0.0; 3], [0.0; 3], spark(false, false));
    let p = &e.pools[Pool::Translucent as usize][0];
    assert_eq!(p.color, [1.0, 0.0, 0.0, 0.99]);
    assert!((48..60).contains(&p.life));
    e.tick(&OpenWorld);
    assert!(e.pools[Pool::Translucent as usize][0].vel.y < 0.0);
}

#[test]
fn trailing_sparks_leave_half_aged_copies() {
    let mut e = engine();
    e.firework_spark([0.0; 3], [0.0; 3], spark(true, false));
    for _ in 0..4 {
        e.tick(&OpenWorld);
    }
    let pool = &e.pools[Pool::Translucent as usize];
    assert!(pool.len() > 1);
    assert!(
        pool[1..]
            .iter()
            .all(|c| c.age >= c.life / 2 && c.aux[5] == 0.0)
    );
}

#[test]
fn twinkling_sparks_blink_after_the_first_third() {
    let mut e = engine();
    e.firework_spark([0.0; 3], [0.0; 3], spark(false, true));
    let mut p = e.pools[Pool::Translucent as usize][0].clone();
    p.age = 0;
    assert!(sim::quad_size(&p, 0.0) > 0.0);
    let hidden = (p.life / 3..p.life)
        .filter(|&age| {
            p.age = age;
            sim::quad_size(&p, 0.0) == 0.0
        })
        .count();
    assert!(hidden > 0 && hidden < (p.life - p.life / 3) as usize);
}

#[test]
fn firework_flash_grows_and_fades_over_four_ticks() {
    let mut e = engine();
    e.firework_flash([0.0; 3], 0x00FF00);
    let p = e.pools[Pool::Translucent as usize][0].clone();
    assert_eq!(p.life, 4);
    assert_eq!(&p.color[..3], &[0.0, 1.0, 0.0]);
    let mut q = p.clone();
    q.age = 2;
    assert!((sim::quad_size(&q, 0.0) - 7.1 * (0.25f32 * std::f32::consts::PI).sin()).abs() < 1e-4);
    assert!(sim::alpha(&q, 0.0) < sim::alpha(&p, 1.0));
    for _ in 0..5 {
        e.tick(&OpenWorld);
    }
    assert_eq!(e.count(), 0);
}

#[test]
fn block_markers_are_still_half_block_quads_that_last_80_ticks() {
    let mut e = engine();
    e.emit(
        &Particle::BlockState {
            kind: BlockParticleKind::BlockMarker,
            state: 3,
        },
        [1.0, 2.0, 3.0],
        [0.0; 3],
        0.0,
        0,
    );
    let p = &e.pools[Pool::Terrain as usize][0];
    assert_eq!((p.size, p.life, p.vel), (0.5, 80, DVec3::ZERO));
    for _ in 0..80 {
        e.tick(&Floor);
    }
    assert_eq!(e.count(), 1);
    e.tick(&Floor);
    assert_eq!(e.count(), 0);
}

#[test]
fn block_markers_need_a_texture() {
    let mut e = ParticleEngine::new(ParticleAtlas::pack(vec![], vec![]), Box::new(NoAssets));
    e.emit(
        &Particle::BlockState {
            kind: BlockParticleKind::BlockMarker,
            state: 3,
        },
        [0.0; 3],
        [0.0; 3],
        0.0,
        0,
    );
    assert_eq!(e.count(), 0);
}

#[test]
fn enchant_particles_fly_from_the_offset_back_to_the_spawn_point() {
    let mut e = engine();
    e.add_particle(
        &Particle::Simple(i32::from(sim::type_id("enchant"))),
        [10.0, 20.0, 30.0],
        [2.0, 0.0, -1.0],
    );
    let p = &e.pools[Pool::Opaque as usize][0];
    assert_eq!(p.pos, DVec3::new(12.0, 20.0, 29.0));
    let life = p.life;
    assert!((30..40).contains(&life));
    let mut last = f64::MAX;
    for _ in 0..life - 1 {
        e.tick(&Floor);
        let p = &e.pools[Pool::Opaque as usize][0];
        assert!(p.pos.x < last && p.pos.x > 10.0);
        last = p.pos.x;
    }
    e.tick(&Floor);
    let p = &e.pools[Pool::Opaque as usize][0];
    assert!((p.pos.x - 10.0).abs() < 1e-9 && (p.pos.y - 18.8).abs() < 1e-6);
    e.tick(&Floor);
    assert_eq!(e.count(), 0);
}

#[test]
fn campfire_smoke_starts_translucent() {
    let mut e = engine();
    e.emit(&simple("campfire_cosy_smoke"), [0.0; 3], [0.0; 3], 0.0, 0);
    e.emit(&simple("campfire_signal_smoke"), [0.0; 3], [0.0; 3], 0.0, 0);
    let alphas: Vec<f32> = e.pools[Pool::Translucent as usize]
        .iter()
        .map(|p| p.color[3])
        .collect();
    assert_eq!(alphas, [0.9, 0.95]);
}
