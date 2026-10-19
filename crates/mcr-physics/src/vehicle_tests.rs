use crate::tests::{TestWorld, default_state, state_with};
use crate::types::Attributes;
use crate::{Boat, Mount, MountMode, MoveKind, PlayerInput, PlayerPhysics};

fn horse(pos: [f64; 3]) -> Mount {
    let attrs = Attributes {
        step_height: 1.0,
        ..Attributes::default()
    };
    Mount::new(pos, 0.0, 1.3964844, 1.6, attrs).with_mode(MountMode::Horse)
}

fn water_world() -> TestWorld {
    let mut w = TestWorld::flat("minecraft:stone", 50);
    w.fill([-8, 50, -8], [8, 59, 8], default_state("minecraft:water"));
    w
}

#[test]
fn a_floating_horse_rises_in_deep_water() {
    let w = water_world();
    let mut sinking = horse([0.5, 54.0, 0.5]);
    let mut floating = horse([0.5, 54.0, 0.5]);
    floating.body.floats_while_ridden = true;
    for _ in 0..40 {
        sinking.tick(&w, &PlayerInput::default(), 0.2, 0.2);
        floating.tick(&w, &PlayerInput::default(), 0.2, 0.2);
    }
    assert!(sinking.body.pos[1] < 53.5, "{}", sinking.body.pos[1]);
    assert!(floating.body.pos[1] > 56.0, "{}", floating.body.pos[1]);
}

#[test]
fn ridden_mobs_steer_in_the_air_with_a_tenth_of_their_speed() {
    let w = TestWorld::flat("minecraft:stone", 0);
    let mut m = horse([0.5, 100.0, 0.5]);
    let forward = PlayerInput {
        forward: true,
        ..PlayerInput::default()
    };
    m.tick(&w, &forward, 0.3, 0.3);
    let dz = m.body.vel[2] / f64::from(0.91f32);
    assert!((dz - 0.03 * 0.98).abs() < 1e-4, "{dz}");
}

#[test]
fn a_boat_takes_the_normalised_current() {
    let mut w = TestWorld::flat("minecraft:stone", 60);
    let flowing = state_with("minecraft:water", &[("level", "2")]);
    w.fill([-8, 60, -8], [8, 60, 8], default_state("minecraft:water"));
    w.fill([1, 60, -8], [8, 60, 8], flowing);
    let mut boat = Boat::new([1.5, 60.4, 0.5], 0.0, 1.375, 0.5625);
    boat.body.update_fluid_interaction(&w);
    let speed = boat.body.vel[0].hypot(boat.body.vel[2]);
    assert!((speed - 0.014).abs() < 1e-3, "{speed}");
}

#[test]
fn boats_are_slowed_by_cobwebs() {
    let mut w = TestWorld::flat("minecraft:stone", 60);
    w.set(0, 60, 1, default_state("minecraft:cobweb"));
    let mut boat = Boat::new([0.5, 60.0, 0.5], 0.0, 1.375, 0.5625);
    boat.body.vel = [0.0, 0.0, 0.3];
    boat.tick(&w, &PlayerInput::default());
    let before = boat.body.pos[2];
    boat.tick(&w, &PlayerInput::default());
    assert!(
        boat.body.pos[2] - before < 0.1,
        "{}",
        boat.body.pos[2] - before
    );
}

#[test]
fn a_detached_camera_sends_no_movement() {
    let w = TestWorld::flat("minecraft:stone", 60);
    let mut p = PlayerPhysics::new([0.5, 60.0, 0.5]);
    p.detached_camera = true;
    for _ in 0..30 {
        let out = p.tick(&PlayerInput::default(), &w);
        assert_eq!(out.kind, MoveKind::None);
    }
    p.pos[0] += 1.0;
    p.detached_camera = false;
    assert_eq!(p.tick(&PlayerInput::default(), &w).kind, MoveKind::Pos);
}
