use crate::tests::TestWorld;
use crate::{PlayerInput, PlayerPhysics};

fn glider(pos: [f64; 3]) -> PlayerPhysics {
    let mut p = PlayerPhysics::new(pos);
    p.glider_equipped = true;
    p
}

#[test]
fn jumping_in_the_air_starts_a_glide_once() {
    let w = TestWorld::flat("minecraft:stone", 0);
    let mut p = glider([0.5, 40.0, 0.5]);
    p.tick(&PlayerInput::default(), &w);
    let jump = PlayerInput {
        jump: true,
        ..PlayerInput::default()
    };
    let out = p.tick(&jump, &w);
    assert!(p.fall_flying && out.start_fall_flying);
    let out = p.tick(&jump, &w);
    assert!(!out.start_fall_flying);
}

#[test]
fn no_glide_without_a_glider_or_on_the_ground() {
    let w = TestWorld::flat("minecraft:stone", 0);
    let jump = PlayerInput {
        jump: true,
        ..PlayerInput::default()
    };
    let mut bare = PlayerPhysics::new([0.5, 40.0, 0.5]);
    bare.tick(&PlayerInput::default(), &w);
    bare.tick(&jump, &w);
    assert!(!bare.fall_flying);
    let mut grounded = glider([0.5, 0.0, 0.5]);
    for _ in 0..3 {
        grounded.tick(&PlayerInput::default(), &w);
    }
    grounded.tick(&jump, &w);
    assert!(!grounded.fall_flying);
}

#[test]
fn level_glide_keeps_horizontal_speed_and_sinks_slowly() {
    let p = glider([0.5, 40.0, 0.5]);
    let m = p.fall_flying_movement([0.0, 0.0, 1.0]);
    assert!(m[2] > 0.9 && m[2] < 1.0);
    assert!(m[1] < 0.0 && m[1] > -0.1);
}

#[test]
fn firework_boost_pulls_towards_the_view() {
    let mut p = glider([0.5, 40.0, 0.5]);
    p.fall_flying = true;
    p.apply_firework_boost();
    assert!(p.vel[2] > 0.1, "{:?}", p.vel);
}

#[test]
fn riptide_pushes_along_the_view_and_hops_off_the_ground() {
    let w = TestWorld::flat("minecraft:stone", 0);
    let mut p = PlayerPhysics::new([0.5, 0.0, 0.5]);
    for _ in 0..3 {
        p.tick(&PlayerInput::default(), &w);
    }
    assert!(p.on_ground);
    p.start_riptide(&w, 3.0);
    assert!((p.vel[2] - 3.0).abs() < 1e-3 && p.spin_attack_ticks == 20);
    assert!(p.pos[1] > 1.0);
}

#[test]
fn hostile_velocities_do_not_panic_the_glide() {
    let p = glider([0.5, 40.0, 0.5]);
    for v in [f64::NAN, f64::INFINITY, f64::MAX, f64::MIN] {
        let _ = p.fall_flying_movement([v, v, v]);
    }
}
