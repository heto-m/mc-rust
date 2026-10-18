use crate::tests::{TestWorld, default_state, state_with};
use crate::{Aabb, BorderBounds, PlayerInput, PlayerPhysics, Pose};

fn idle() -> PlayerInput {
    PlayerInput::default()
}

fn walk(yaw: f32) -> PlayerInput {
    PlayerInput {
        forward: true,
        yaw,
        ..PlayerInput::default()
    }
}

fn boat_at(x: f64, y: f64, z: f64) -> Aabb {
    Aabb::new(
        [x - 0.6875, y, z - 0.6875],
        [x + 0.6875, y + 0.5625, z + 0.6875],
    )
}

#[test]
fn lands_on_a_boat_box() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = PlayerPhysics::new([0.5, 66.0, 0.5]);
    p.entity_colliders.push(boat_at(0.5, 64.0, 0.5));
    for _ in 0..30 {
        p.tick(&idle(), &w);
    }
    assert!(p.on_ground);
    assert!((p.pos[1] - 64.5625).abs() < 1e-9, "{}", p.pos[1]);
}

#[test]
fn walks_into_a_shulker_box_side() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = PlayerPhysics::new([0.5, 64.0, 0.5]);
    p.entity_colliders
        .push(Aabb::new([0.0, 64.0, 2.0], [1.0, 65.0, 3.0]));
    for _ in 0..40 {
        p.tick(&walk(0.0), &w);
    }
    assert!(p.pos[2] <= 2.0 - 0.3 + 1e-9, "{}", p.pos[2]);
    assert!(p.horizontal_collision);
}

#[test]
fn world_border_stops_the_player() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = PlayerPhysics::new([0.5, 64.0, 0.5]);
    p.border = BorderBounds::new(0.0, 0.0, 6.0, 2.9999984e7);
    for _ in 0..60 {
        p.tick(&walk(0.0), &w);
    }
    assert!((p.pos[2] - 2.7).abs() < 1e-6, "{}", p.pos[2]);
    assert!(p.horizontal_collision);
}

#[test]
fn far_border_is_ignored() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = PlayerPhysics::new([0.5, 64.0, 0.5]);
    p.border = BorderBounds::new(0.0, 0.0, 200.0, 2.9999984e7);
    for _ in 0..20 {
        p.tick(&walk(0.0), &w);
    }
    assert!(p.pos[2] > 2.0);
    assert!(BorderBounds::new(f64::NAN, 0.0, 1.0, 1.0).is_none());
}

#[test]
fn push_velocity_matches_vanilla() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = PlayerPhysics::new([0.0, 64.0, 0.0]);
    p.push_from(&w, [0.25, 64.0, 0.0]);
    let dd = 0.25f64.sqrt();
    let expect = -(0.25 / dd) * (1.0 / dd).min(1.0) * f64::from(0.05f32);
    assert!((p.vel[0] - expect).abs() < 1e-12, "{}", p.vel[0]);
    assert_eq!(p.vel[2], 0.0);
    let before = p.vel;
    p.push_from(&w, [0.001, 64.0, 0.001]);
    assert_eq!(p.vel, before, "too close to push");
    p.spectator = true;
    p.push_from(&w, [1.0, 64.0, 0.0]);
    assert_eq!(p.vel, before);
}

#[test]
fn pushed_out_of_a_suffocating_block() {
    let mut w = TestWorld::flat("minecraft:stone", 64);
    let stone = default_state("minecraft:stone");
    w.fill([0, 64, -1], [0, 65, 1], stone);
    let mut p = PlayerPhysics::new([0.8, 64.0, 0.5]);
    p.on_ground = true;
    p.tick(&idle(), &w);
    assert!(p.pos[0] > 0.8 && p.vel[0] > 0.0, "nudged east: {:?}", p.vel);
    assert_eq!(p.vel[2], 0.0);
}

#[test]
fn glass_does_not_push_out() {
    let mut w = TestWorld::flat("minecraft:stone", 64);
    w.fill([0, 64, 0], [0, 65, 0], default_state("minecraft:glass"));
    let mut p = PlayerPhysics::new([0.8, 64.0, 0.5]);
    p.on_ground = true;
    p.tick(&idle(), &w);
    assert_eq!(p.pos[0], 0.8);
}

#[test]
fn scale_attribute_scales_box_and_eye() {
    let mut p = PlayerPhysics::new([0.0, 64.0, 0.0]);
    p.attributes.scale = 2.0;
    let bb = p.bounding_box();
    assert!((bb.max[1] - bb.min[1] - 3.6).abs() < 1e-6);
    assert!((bb.max[0] - bb.min[0] - 1.2).abs() < 1e-6);
    assert!((p.eye_height() - 3.24).abs() < 1e-5);
    p.attributes.scale = f64::NAN;
    assert!((p.eye_height() - 1.62).abs() < 1e-6);
    p.attributes.scale = 1.0e9;
    assert!((p.bounding_box().max[1] - 64.0 - 1.8 * 16.0).abs() < 1e-3);
    p.attributes.scale = 2.0;
    p.pose = Pose::Sleeping;
    assert!((p.eye_height() - 0.2).abs() < 1e-6);
}

#[test]
fn hostile_step_height_is_bounded() {
    let w = TestWorld::flat("minecraft:stone", 64);
    for step in [f64::NAN, f64::INFINITY, 1.0e12, -5.0] {
        let mut p = PlayerPhysics::new([0.5, 64.0, 0.5]);
        p.attributes.step_height = step;
        for _ in 0..10 {
            p.tick(&walk(0.0), &w);
        }
        assert!(p.pos.iter().all(|v| v.is_finite()), "{step}");
        assert!(p.pos[1] < 64.0 + 1e-9, "{step}");
    }
}

#[test]
fn unstable_scaffolding_bottom_holds_the_player() {
    let mut w = TestWorld::flat("minecraft:stone", 60);
    let floating = state_with(
        "minecraft:scaffolding",
        &[("bottom", "true"), ("distance", "3")],
    );
    w.set(0, 64, 0, floating);
    let mut p = PlayerPhysics::new([0.5, 64.5, 0.5]);
    for _ in 0..20 {
        p.tick(&idle(), &w);
    }
    assert!(p.on_ground);
    assert!((p.pos[1] - 64.125).abs() < 1e-9, "{}", p.pos[1]);
}

#[test]
fn stable_bottom_scaffolding_has_no_floor_inside() {
    let mut w = TestWorld::flat("minecraft:stone", 60);
    let grounded = state_with(
        "minecraft:scaffolding",
        &[("bottom", "false"), ("distance", "0")],
    );
    w.set(0, 64, 0, grounded);
    let mut p = PlayerPhysics::new([0.5, 64.5, 0.5]);
    for _ in 0..40 {
        p.tick(&idle(), &w);
    }
    assert!(p.pos[1] < 64.0, "{}", p.pos[1]);
}
