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

fn creative_glider(pos: [f64; 3], flying: bool) -> PlayerPhysics {
    let mut p = glider(pos);
    p.abilities.may_fly = true;
    p.abilities.flying = flying;
    p
}

/// Press and release space: the two output flags of both ticks.
fn tap(p: &mut PlayerPhysics, w: &TestWorld) -> (bool, bool) {
    let jump = PlayerInput {
        jump: true,
        ..PlayerInput::default()
    };
    let down = p.tick(&jump, w);
    let up = p.tick(&PlayerInput::default(), w);
    (
        down.start_fall_flying || up.start_fall_flying,
        down.abilities_changed || up.abilities_changed,
    )
}

#[test]
fn creative_flyer_with_an_elytra_does_not_glide_on_space() {
    let w = TestWorld::flat("minecraft:stone", 0);
    let mut p = creative_glider([0.5, 40.0, 0.5], true);
    p.tick(&PlayerInput::default(), &w);
    let (glide, _) = tap(&mut p, &w);
    assert!(!glide && !p.fall_flying && p.abilities.flying);
}

#[test]
fn double_tap_toggles_creative_flight_with_an_elytra_equipped() {
    let w = TestWorld::flat("minecraft:stone", 0);
    let mut p = creative_glider([0.5, 0.0, 0.5], false);
    for _ in 0..3 {
        p.tick(&PlayerInput::default(), &w);
    }
    assert!(p.on_ground);
    let (_, changed) = tap(&mut p, &w);
    assert!(!changed && !p.abilities.flying);
    let (glide, changed) = tap(&mut p, &w);
    assert!(changed && p.abilities.flying);
    assert!(!glide && !p.fall_flying);
    tap(&mut p, &w);
    let (_, changed) = tap(&mut p, &w);
    assert!(changed && !p.abilities.flying);
}

#[test]
fn landing_ends_creative_flight_and_reports_it() {
    let w = TestWorld::flat("minecraft:stone", 0);
    let mut p = creative_glider([0.5, 0.3, 0.5], true);
    let sneak = PlayerInput {
        sneak: true,
        ..PlayerInput::default()
    };
    let mut changed = false;
    for _ in 0..20 {
        changed |= p.tick(&sneak, &w).abilities_changed;
    }
    assert!(!p.abilities.flying && changed);
}
