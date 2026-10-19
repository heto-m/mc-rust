use crate::aabb::Aabb;
use crate::tests::{TestWorld, default_state};
use crate::{PlayerInput, PlayerPhysics};

fn keys(forward: bool, sprint: bool, jump: bool) -> PlayerInput {
    PlayerInput {
        forward,
        sprint,
        jump,
        ..PlayerInput::default()
    }
}

fn settled(w: &TestWorld, pos: [f64; 3]) -> PlayerPhysics {
    let mut p = PlayerPhysics::new(pos);
    for _ in 0..5 {
        p.tick(&PlayerInput::default(), w);
    }
    p
}

fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} vs {b} (tol {tol})");
}

/// A one-block step across the path at z = 2.
fn step_world() -> TestWorld {
    let mut w = TestWorld::flat("minecraft:stone", 64);
    w.fill([-3, 64, 2], [3, 64, 8], default_state("minecraft:stone"));
    w
}

#[test]
fn auto_jump_climbs_a_one_block_step() {
    let w = step_world();
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    p.auto_jump = true;
    let mut sent_jump = false;
    for _ in 0..30 {
        let out = p.tick(&keys(true, false, false), &w);
        sent_jump |= out.input.is_some_and(|i| i.jump);
    }
    assert!(sent_jump, "the queued jump is part of the sent input");
    assert_eq!(p.pos[1], 65.0);
    assert!(p.pos[2] > 2.5, "{}", p.pos[2]);
}

#[test]
fn without_auto_jump_the_step_blocks() {
    let w = step_world();
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    for _ in 0..30 {
        p.tick(&keys(true, false, false), &w);
    }
    assert_eq!(p.pos[1], 64.0);
    assert!(p.pos[2] < 2.0);
}

#[test]
fn auto_jump_ignores_a_two_block_wall_and_sneaking() {
    let mut w = step_world();
    w.fill([-3, 65, 2], [3, 65, 8], default_state("minecraft:stone"));
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    p.auto_jump = true;
    for _ in 0..30 {
        p.tick(&keys(true, false, false), &w);
    }
    assert_eq!(p.pos[1], 64.0);
    let w = step_world();
    let mut q = settled(&w, [0.5, 64.0, 0.5]);
    q.auto_jump = true;
    let sneak = PlayerInput {
        forward: true,
        sneak: true,
        ..PlayerInput::default()
    };
    for _ in 0..60 {
        q.tick(&sneak, &w);
    }
    assert_eq!(q.pos[1], 64.0);
}

#[test]
fn blindness_refuses_and_stops_sprinting() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    p.effects.blindness = true;
    for _ in 0..10 {
        let out = p.tick(&keys(true, true, false), &w);
        assert_eq!(out.sprint_command, None);
    }
    assert!(!p.sprinting);

    let mut q = settled(&w, [0.5, 64.0, 0.5]);
    let out = q.tick(&keys(true, true, false), &w);
    assert_eq!(out.sprint_command, Some(true));
    q.effects.blindness = true;
    let out = q.tick(&keys(true, true, false), &w);
    assert!(!q.sprinting);
    assert_eq!(out.sprint_command, Some(false));
}

fn pool() -> TestWorld {
    let mut w = TestWorld::flat("minecraft:stone", 40);
    w.fill([-6, 40, -6], [6, 63, 6], default_state("minecraft:water"));
    w
}

fn swim_drag(grace: bool) -> f64 {
    let w = pool();
    let mut p = PlayerPhysics::new([0.5, 50.0, 0.5]);
    p.effects.dolphins_grace = grace;
    p.tick(&PlayerInput::default(), &w);
    p.vel = [0.0, 0.0, 0.1];
    p.tick(&PlayerInput::default(), &w);
    p.vel[2] / 0.1
}

#[test]
fn dolphins_grace_lightens_water_drag() {
    near(swim_drag(false), f64::from(0.8f32), 1e-6);
    near(swim_drag(true), f64::from(0.96f32), 1e-6);
}

fn powder_floor() -> TestWorld {
    let mut w = TestWorld::flat("minecraft:stone", 60);
    w.fill(
        [-3, 60, -3],
        [3, 63, 3],
        default_state("minecraft:powder_snow"),
    );
    w
}

#[test]
fn leather_boots_walk_on_powder_snow() {
    let w = powder_floor();
    let mut booted = PlayerPhysics::new([0.5, 64.0, 0.5]);
    booted.leather_boots = true;
    let mut bare = PlayerPhysics::new([0.5, 64.0, 0.5]);
    for _ in 0..20 {
        booted.tick(&PlayerInput::default(), &w);
        bare.tick(&PlayerInput::default(), &w);
    }
    assert!(booted.on_ground);
    assert_eq!(booted.pos[1], 64.0);
    assert!(bare.pos[1] < 63.0, "{}", bare.pos[1]);
}

#[test]
fn leather_boots_climb_out_of_powder_snow() {
    let w = powder_floor();
    let mut p = PlayerPhysics::new([0.5, 60.0, 0.5]);
    p.leather_boots = true;
    for _ in 0..3 {
        p.tick(&PlayerInput::default(), &w);
    }
    for _ in 0..40 {
        p.tick(&keys(false, false, true), &w);
    }
    assert!(p.pos[1] >= 64.0, "{}", p.pos[1]);
}

#[test]
fn touching_powder_snow_from_the_side_counts_as_inside() {
    let mut w = TestWorld::flat("minecraft:stone", 64);
    w.set(1, 64, 0, default_state("minecraft:powder_snow"));
    let mut p = settled(&w, [0.75, 64.0, 0.5]);
    p.tick(&PlayerInput::default(), &w);
    assert!(p.in_powder_snow);
    assert_eq!(p.stuck, [0.0; 3]);
}

fn glide_through(block: &str) -> bool {
    let mut w = TestWorld::flat("minecraft:stone", 0);
    w.fill([0, 39, 2], [0, 41, 12], default_state(block));
    let mut p = PlayerPhysics::new([0.5, 40.0, 0.5]);
    p.glider_equipped = true;
    p.fall_flying = true;
    p.vel = [0.0, 0.0, 1.0];
    for _ in 0..6 {
        p.tick(&PlayerInput::default(), &w);
    }
    assert!(p.pos[2] > 2.0, "reached the blocks");
    p.fall_flying
}

#[test]
fn gliding_through_vines_keeps_the_glide() {
    assert!(glide_through("minecraft:vine"));
    assert!(glide_through("minecraft:weeping_vines"));
    assert!(!glide_through("minecraft:ladder"));
}

#[test]
fn weaving_loosens_cobweb() {
    let mut w = TestWorld::flat("minecraft:stone", 64);
    w.set(0, 64, 0, default_state("minecraft:cobweb"));
    let mut p = PlayerPhysics::new([0.5, 64.0, 0.5]);
    p.effects.weaving = true;
    for _ in 0..5 {
        p.tick(&PlayerInput::default(), &w);
    }
    assert_eq!(p.stuck, [0.5, 0.25, 0.5]);
}

#[test]
fn gliding_ignores_block_speed_factor() {
    let mut w = TestWorld::flat("minecraft:soul_sand", 64);
    w.fill([-3, 64, -3], [3, 80, 3], 0);
    let mut p = PlayerPhysics::new([0.5, 64.2, 0.5]);
    p.fall_flying = true;
    p.vel = [0.0, 0.0, 0.5];
    p.do_move(&w, [0.0, -0.1, 0.5]);
    assert_eq!(p.vel[2], 0.5);
    p.fall_flying = false;
    p.do_move(&w, [0.0, 0.0, 0.5]);
    assert!(p.vel[2] < 0.5);
}

#[test]
fn riptide_bounces_off_a_living_entity() {
    let w = TestWorld::flat("minecraft:stone", 0);
    let mut p = PlayerPhysics::new([0.5, 40.0, 0.5]);
    p.spin_attack_ticks = 10;
    p.vel = [0.0, 0.0, 1.0];
    p.spin_targets
        .push((Aabb::new([0.0, 39.0, 1.0], [1.0, 42.0, 2.0]), true));
    p.tick(&PlayerInput::default(), &w);
    assert_eq!(p.spin_attack_ticks, 0);
    assert!(p.vel[2] < 0.0, "{:?}", p.vel);

    let mut q = PlayerPhysics::new([0.5, 40.0, 0.5]);
    q.spin_attack_ticks = 10;
    q.vel = [0.0, 0.0, 1.0];
    q.spin_targets
        .push((Aabb::new([0.0, 39.0, 1.0], [1.0, 42.0, 2.0]), false));
    q.tick(&PlayerInput::default(), &w);
    assert_eq!(q.spin_attack_ticks, 9);
    assert!(q.vel[2] > 0.0);
}
