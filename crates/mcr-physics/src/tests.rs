use std::collections::HashMap;

use crate::{CollisionWorld, MoveKind, PlayerInput, PlayerPhysics};

pub struct TestWorld {
    floor_y: i32,
    floor: u32,
    blocks: HashMap<[i32; 3], u32>,
}

pub fn default_state(name: &str) -> u32 {
    let v = mcr_data::v26_3();
    let b = v.block_by_name(name).unwrap_or_else(|| panic!("{name}"));
    v.block_info(b).unwrap().default_state
}

pub fn state_with(name: &str, props: &[(&str, &str)]) -> u32 {
    let v = mcr_data::v26_3();
    let b = v.block_by_name(name).unwrap();
    let (first, end) = v.block_info(b).unwrap().states;
    (first..end)
        .find(|&s| {
            props
                .iter()
                .all(|p| v.state_properties(s).any(|kv| kv == *p))
        })
        .unwrap_or_else(|| panic!("{name} {props:?}"))
}

impl TestWorld {
    /// Solid `floor` blocks for every y below `floor_y`, air elsewhere.
    pub fn flat(floor: &str, floor_y: i32) -> Self {
        Self {
            floor_y,
            floor: default_state(floor),
            blocks: HashMap::new(),
        }
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, state: u32) {
        self.blocks.insert([x, y, z], state);
    }

    pub fn fill(&mut self, a: [i32; 3], b: [i32; 3], state: u32) {
        for x in a[0]..=b[0] {
            for y in a[1]..=b[1] {
                for z in a[2]..=b[2] {
                    self.set(x, y, z, state);
                }
            }
        }
    }
}

impl CollisionWorld for TestWorld {
    fn block_state(&self, x: i32, y: i32, z: i32) -> u32 {
        if let Some(&s) = self.blocks.get(&[x, y, z]) {
            s
        } else if y < self.floor_y {
            self.floor
        } else {
            0
        }
    }
}

fn idle() -> PlayerInput {
    PlayerInput::default()
}

fn fwd() -> PlayerInput {
    PlayerInput {
        forward: true,
        ..PlayerInput::default()
    }
}

fn settled(w: &TestWorld, pos: [f64; 3]) -> PlayerPhysics {
    let mut p = PlayerPhysics::new(pos);
    for _ in 0..5 {
        p.tick(&idle(), w);
    }
    p
}

fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} vs {b} (tol {tol})");
}

#[test]
fn stands_on_floor_with_gravity_each_tick() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let p = settled(&w, [0.5, 64.0, 0.5]);
    assert!(p.on_ground);
    assert_eq!(p.pos[1], 64.0);
}

#[test]
fn walking_displacement_matches_hand_trace() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    let z0 = p.pos[2];
    p.tick(&fwd(), &w);
    near(p.pos[2] - z0, 0.098, 1e-6);
    near(p.vel[2], 0.098 * 0.546, 1e-6);
    for _ in 0..80 {
        p.tick(&fwd(), &w);
    }
    let z1 = p.pos[2];
    p.tick(&fwd(), &w);
    near(p.pos[2] - z1, 0.098 / (1.0 - 0.546), 1e-4);
}

#[test]
fn sprinting_is_thirty_percent_faster() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    let input = PlayerInput {
        forward: true,
        sprint: true,
        ..PlayerInput::default()
    };
    for _ in 0..80 {
        p.tick(&input, &w);
    }
    assert!(p.sprinting);
    let z1 = p.pos[2];
    p.tick(&input, &w);
    near(p.pos[2] - z1, 0.098 * 1.3 / (1.0 - 0.546), 1e-4);
}

#[test]
fn jump_apex_and_trajectory() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    let mut vy = f64::from(0.42f32);
    let mut ref_y = 0.0f64;
    let mut ref_max = 0.0f64;
    let jump = PlayerInput {
        jump: true,
        ..PlayerInput::default()
    };
    let mut max = 0.0f64;
    let mut airborne_ticks = 0;
    for t in 0..40 {
        let none = idle();
        p.tick(if t == 0 { &jump } else { &none }, &w);
        if vy.abs() < 0.003 {
            vy = 0.0;
        }
        ref_y += vy;
        vy = (vy - 0.08) * 0.98;
        ref_max = ref_max.max(ref_y);
        if ref_y < 0.0 {
            ref_y = 0.0;
        }
        max = max.max(p.pos[1] - 64.0);
        if !p.on_ground {
            airborne_ticks += 1;
        }
    }
    near(max, 1.2522, 1e-3);
    near(max, ref_max, 1e-6);
    assert_eq!(p.pos[1], 64.0);
    assert!(p.on_ground);
    assert!((11..=13).contains(&airborne_ticks), "{airborne_ticks}");
}

#[test]
fn sprint_jump_boost_and_flight_distance() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    let run = PlayerInput {
        forward: true,
        sprint: true,
        ..PlayerInput::default()
    };
    for _ in 0..60 {
        p.tick(&run, &w);
    }
    let vz = p.vel[2];
    let z0 = p.pos[2];
    let jump = PlayerInput { jump: true, ..run };
    p.tick(&jump, &w);
    let ground_speed = f64::from(0.1f32 * 1.3) * 0.98;
    near(p.pos[2] - z0, vz + 0.2 + ground_speed, 2e-4);
    let mut ticks = 1;
    while !p.on_ground {
        p.tick(&run, &w);
        ticks += 1;
        assert!(ticks < 40);
    }
    let dist = p.pos[2] - z0;
    assert!(dist > 3.5 && dist < 4.6, "{dist}");
}

#[test]
fn terminal_velocity() {
    let w = TestWorld::flat("minecraft:stone", -100_000);
    let mut p = PlayerPhysics::new([0.5, 50_000.0, 0.5]);
    for _ in 0..500 {
        p.tick(&idle(), &w);
    }
    near(p.vel[1], -3.92, 1e-3);
}

#[test]
fn wall_stops_flush() {
    let mut w = TestWorld::flat("minecraft:stone", 64);
    let stone = default_state("minecraft:stone");
    w.fill([-1, 64, 3], [1, 66, 3], stone);
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    for _ in 0..40 {
        p.tick(&fwd(), &w);
    }
    near(p.pos[2] + f64::from(0.3f32), 3.0, 1e-9);
    assert!(p.horizontal_collision);
    assert!(p.on_ground);
}

#[test]
fn steps_up_onto_slab_without_jumping() {
    let mut w = TestWorld::flat("minecraft:stone", 64);
    let slab = state_with(
        "minecraft:stone_slab",
        &[("type", "bottom"), ("waterlogged", "false")],
    );
    w.fill([0, 64, 2], [0, 64, 12], slab);
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    for _ in 0..30 {
        p.tick(&fwd(), &w);
    }
    assert_eq!(p.pos[1], 64.5);
    assert!(p.pos[2] > 2.5);
    assert!(p.on_ground);
}

#[test]
fn cannot_step_up_full_block() {
    let mut w = TestWorld::flat("minecraft:stone", 64);
    w.set(0, 64, 2, default_state("minecraft:stone"));
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    for _ in 0..30 {
        p.tick(&fwd(), &w);
    }
    assert_eq!(p.pos[1], 64.0);
    assert!(p.pos[2] < 2.0);
}

#[test]
fn ice_keeps_sliding() {
    let w = TestWorld::flat("minecraft:ice", 64);
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    p.vel[2] = 0.3;
    p.tick(&idle(), &w);
    let f = f64::from(0.98f32 * 0.91f32);
    near(p.vel[2], 0.3 * f, 1e-7);
    let v1 = p.vel[2];
    p.tick(&idle(), &w);
    near(p.vel[2], v1 * f, 1e-7);
}

#[test]
fn water_sinks_slowly() {
    let mut w = TestWorld::flat("minecraft:stone", 50);
    w.fill([-3, 50, -3], [3, 70, 3], default_state("minecraft:water"));
    let mut p = PlayerPhysics::new([0.5, 60.0, 0.5]);
    p.tick(&idle(), &w);
    near(p.vel[1], -0.005, 1e-12);
    let y1 = p.pos[1];
    p.tick(&idle(), &w);
    near(p.pos[1] - y1, -0.005, 1e-9);
    near(p.vel[1], -0.009, 1e-7);
    for _ in 0..200 {
        p.tick(&idle(), &w);
    }
    near(p.vel[1], -0.025, 1e-4);
}

#[test]
fn sneaking_stays_on_edge() {
    let mut w = TestWorld::flat("minecraft:stone", 64);
    w.fill([-5, 0, 1], [5, 63, 8], 0);
    let sneak = PlayerInput {
        forward: true,
        sneak: true,
        ..PlayerInput::default()
    };
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    for _ in 0..60 {
        p.tick(&sneak, &w);
    }
    assert!(p.on_ground);
    assert!(p.pos[2] > 1.2 && p.pos[2] < 1.3001, "{}", p.pos[2]);
    let mut q = settled(&w, [0.5, 64.0, 0.5]);
    for _ in 0..30 {
        q.tick(&fwd(), &w);
    }
    assert!(q.pos[1] < 60.0);
}

#[test]
fn cobweb_scales_next_move() {
    let mut w = TestWorld::flat("minecraft:stone", 64);
    w.set(0, 64, 0, default_state("minecraft:cobweb"));
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    assert_eq!(p.stuck[0], 0.25);
    assert_eq!(p.stuck[1], f64::from(0.05f32));
    let z0 = p.pos[2];
    p.tick(&fwd(), &w);
    near(p.pos[2] - z0, 0.098 * 0.25, 1e-6);
}

#[test]
fn flying_up_adds_three_times_flying_speed() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    p.abilities.may_fly = true;
    p.abilities.flying = true;
    p.vel[1] = 0.0;
    let up = PlayerInput {
        jump: true,
        ..PlayerInput::default()
    };
    p.tick(&up, &w);
    near(p.pos[1], 64.15, 1e-6);
    near(p.vel[1], 0.15 * 0.6, 1e-6);
    assert!(p.abilities.flying);
}

#[test]
fn slime_bounces() {
    let mut w = TestWorld::flat("minecraft:stone", 60);
    w.fill(
        [-2, 60, -2],
        [2, 63, 2],
        default_state("minecraft:slime_block"),
    );
    let mut p = PlayerPhysics::new([0.5, 80.0, 0.5]);
    let mut bounced = false;
    for _ in 0..60 {
        p.tick(&idle(), &w);
        if p.vel[1] > 0.5 {
            bounced = true;
        }
    }
    assert!(bounced);
}

#[test]
fn climbing_ladder_limits_fall_speed() {
    let mut w = TestWorld::flat("minecraft:stone", 64);
    w.fill(
        [0, 64, 0],
        [0, 80, 0],
        state_with("minecraft:ladder", &[("facing", "north")]),
    );
    let mut p = PlayerPhysics::new([0.5, 70.0, 0.5]);
    for _ in 0..20 {
        p.tick(&idle(), &w);
    }
    assert!(p.vel[1] >= -0.15 * 0.98 - 1e-9);
    let up = fwd();
    for _ in 0..10 {
        p.tick(&up, &w);
    }
    assert!(p.pos[1] > 70.0 - 4.0);
}

#[test]
fn idle_player_resends_position_every_twenty_ticks() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = PlayerPhysics::new([0.5, 64.0, 0.5]);
    let kinds: Vec<MoveKind> = (0..21).map(|_| p.tick(&idle(), &w).kind).collect();
    assert_eq!(kinds[0], MoveKind::None);
    assert_eq!(kinds[1], MoveKind::StatusOnly);
    assert!(kinds[2..19].iter().all(|&k| k == MoveKind::None));
    assert_eq!(kinds[19], MoveKind::Pos);
    assert_eq!(kinds[20], MoveKind::None);
}

#[test]
fn rotation_only_sends_rot_and_moving_sends_pos_rot() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    let turn = PlayerInput {
        yaw: 10.0,
        ..PlayerInput::default()
    };
    assert_eq!(p.tick(&turn, &w).kind, MoveKind::Rot);
    let walk_turn = PlayerInput {
        yaw: 20.0,
        forward: true,
        ..PlayerInput::default()
    };
    let out = p.tick(&walk_turn, &w);
    assert_eq!(out.kind, MoveKind::PosRot);
    assert!(out.input.is_some());
    assert!(p.tick(&walk_turn, &w).input.is_none());
}

#[test]
fn sprint_command_follows_state() {
    let w = TestWorld::flat("minecraft:stone", 64);
    let mut p = settled(&w, [0.5, 64.0, 0.5]);
    let run = PlayerInput {
        forward: true,
        sprint: true,
        ..PlayerInput::default()
    };
    assert_eq!(p.tick(&run, &w).sprint_command, Some(true));
    assert_eq!(p.tick(&run, &w).sprint_command, None);
    assert_eq!(p.tick(&idle(), &w).sprint_command, Some(false));
}
