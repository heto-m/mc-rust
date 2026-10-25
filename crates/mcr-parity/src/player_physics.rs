//! Local player physics: vanilla `LocalPlayer.tick` (keyboard input, `aiStep`, `travel`,
//! `Entity.move`) in a scene served by a fake level, against `PlayerPhysics::tick` over the same
//! blocks and keys. Each tick's position, velocity and ground flag compare.

use std::collections::HashMap;

use mcr_physics::{CollisionWorld, PlayerInput, PlayerPhysics};
use serde_json::{Value, json};

use crate::{Case, Env, Harness, Result, Rng};

const CASES: usize = 1600;
const TICKS: usize = 40;
const START: [f64; 3] = [0.5, 64.0, 0.5];

const SCENES: &[&str] = &[
    "flat",
    "stairs",
    "slabs",
    "step",
    "ladder",
    "vine",
    "water",
    "lava",
    "edge",
    "soul_sand",
    "honey",
    "cobweb",
    "powder_snow",
    "ice",
    "blue_ice",
    "slime",
    "elytra",
];

#[derive(Debug)]
pub struct PlayerPhysicsHarness;

fn state(name: &str, props: &[(&str, &str)]) -> u32 {
    let v = mcr_data::v26_3();
    let Some(b) = v.block_by_name(name) else {
        return 0;
    };
    let Some(info) = v.block_info(b) else {
        return 0;
    };
    if props.is_empty() {
        return info.default_state;
    }
    let (first, end) = info.states;
    (first..end)
        .find(|&s| {
            props
                .iter()
                .all(|p| v.state_properties(s).any(|kv| kv == *p))
        })
        .unwrap_or(info.default_state)
}

/// Scene blocks plus a square floor `(y, state, half extent)` around the origin.
struct Scene {
    floor: (i32, u32, i32),
    blocks: Vec<[i64; 4]>,
    start: [f64; 3],
}

impl Scene {
    fn put(&mut self, x: i32, y: i32, z: i32, s: u32) {
        self.blocks.push([x.into(), y.into(), z.into(), s.into()]);
    }

    fn fill(&mut self, a: [i32; 3], b: [i32; 3], s: u32) {
        for x in a[0]..=b[0] {
            for y in a[1]..=b[1] {
                for z in a[2]..=b[2] {
                    self.put(x, y, z, s);
                }
            }
        }
    }
}

fn scene(kind: &str) -> Scene {
    let stone = state("minecraft:stone", &[]);
    let mut s = Scene {
        floor: (63, stone, 8),
        blocks: Vec::new(),
        start: START,
    };
    match kind {
        "stairs" => {
            let stair = state(
                "minecraft:oak_stairs",
                &[
                    ("facing", "south"),
                    ("half", "bottom"),
                    ("shape", "straight"),
                ],
            );
            for i in 0..4 {
                s.fill([-2, 64, 3 + i], [2, 63 + i, 3 + i], stone);
                s.fill([-2, 64 + i, 3 + i], [2, 64 + i, 3 + i], stair);
            }
        }
        "slabs" => {
            let slab = state("minecraft:stone_slab", &[("type", "bottom")]);
            for i in 0..4 {
                s.fill([-2, 64, 3 + i], [2, 63 + i, 3 + i], stone);
                s.fill([-2, 64 + i, 3 + i], [2, 64 + i, 3 + i], slab);
            }
        }
        "step" => s.fill([-2, 64, 3], [2, 64, 5], stone),
        "ladder" | "vine" => {
            s.fill([-2, 64, 3], [2, 72, 3], stone);
            let climb = if kind == "ladder" {
                state("minecraft:ladder", &[("facing", "north")])
            } else {
                state("minecraft:vine", &[("south", "true"), ("up", "false")])
            };
            s.fill([-1, 64, 2], [1, 71, 2], climb);
        }
        "water" | "lava" => {
            let fluid = state(&format!("minecraft:{kind}"), &[("level", "0")]);
            s.floor.0 = 60;
            s.fill([-8, 61, -8], [8, 62, 8], fluid);
            s.start[1] = 63.0;
        }
        "edge" => s.floor.2 = 1,
        "soul_sand" | "honey" | "ice" | "blue_ice" | "slime" => {
            let name = if kind == "honey" {
                "honey_block"
            } else if kind == "slime" {
                "slime_block"
            } else {
                kind
            };
            s.floor.1 = state(&format!("minecraft:{name}"), &[]);
            if kind == "slime" {
                s.start[1] = 70.0;
            }
        }
        "elytra" => s.start[1] = 90.0,
        "cobweb" => s.fill([-2, 64, 2], [2, 65, 4], state("minecraft:cobweb", &[])),
        "powder_snow" => s.fill([-2, 64, 2], [2, 64, 4], state("minecraft:powder_snow", &[])),
        _ => {}
    }
    s
}

fn keys(rng: &mut Rng) -> String {
    let mut k = String::new();
    for (c, p) in [
        ('F', 70),
        ('B', 8),
        ('L', 15),
        ('R', 15),
        ('J', 25),
        ('S', 15),
        ('P', 25),
    ] {
        if rng.chance(p) {
            k.push(c);
        }
    }
    k
}

struct SceneWorld {
    floor: (i32, u32, i32),
    blocks: HashMap<[i32; 3], u32>,
}

impl CollisionWorld for SceneWorld {
    fn block_state(&self, x: i32, y: i32, z: i32) -> u32 {
        if let Some(&s) = self.blocks.get(&[x, y, z]) {
            return s;
        }
        let (fy, fs, ext) = self.floor;
        if y == fy && x.abs() <= ext && z.abs() <= ext {
            fs
        } else {
            0
        }
    }
}

fn input(tick: &Value) -> PlayerInput {
    let k = tick["keys"].as_str().unwrap_or("");
    PlayerInput {
        forward: k.contains('F'),
        backward: k.contains('B'),
        left: k.contains('L'),
        right: k.contains('R'),
        jump: k.contains('J'),
        sneak: k.contains('S'),
        sprint: k.contains('P'),
        yaw: tick["yaw"].as_f64().unwrap_or(0.0) as f32,
        pitch: tick["pitch"].as_f64().unwrap_or(0.0) as f32,
    }
}

fn probe_case(case: &Case) -> Value {
    let c = &case.case;
    let floor = &c["floor"];
    let mut world = SceneWorld {
        floor: (
            floor[0].as_i64().unwrap_or(0) as i32,
            floor[1].as_u64().unwrap_or(0) as u32,
            floor[2].as_i64().unwrap_or(0) as i32,
        ),
        blocks: HashMap::new(),
    };
    for b in c["blocks"].as_array().into_iter().flatten() {
        let n = |i: usize| b[i].as_i64().unwrap_or(0) as i32;
        world.blocks.insert([n(0), n(1), n(2)], n(3) as u32);
    }
    let start = &c["start"];
    let mut player = PlayerPhysics::new([
        start[0].as_f64().unwrap_or(0.0),
        start[1].as_f64().unwrap_or(0.0),
        start[2].as_f64().unwrap_or(0.0),
    ]);
    player.glider_equipped = c["elytra"].as_bool().unwrap_or(false);
    let (mut pos, mut vel, mut ground) = (Vec::new(), Vec::new(), Vec::new());
    for tick in c["ticks"].as_array().into_iter().flatten() {
        player.tick(&input(tick), &world);
        pos.push(json!(player.pos));
        vel.push(json!(player.vel));
        ground.push(json!(player.on_ground));
    }
    json!({"pos": pos, "vel": vel, "ground": ground})
}

impl Harness for PlayerPhysicsHarness {
    fn name(&self) -> &'static str {
        "player_physics"
    }

    fn scope(&self) -> &'static [&'static str] {
        &["crates/mcr-physics/src"]
    }

    fn count(&self) -> usize {
        CASES
    }

    fn tolerance(&self, field: &str) -> f64 {
        if matches!(field, "pos" | "vel") {
            1e-6
        } else {
            0.0
        }
    }

    fn case(&self, id: usize, rng: &mut Rng) -> Value {
        let kind = SCENES[id % SCENES.len()];
        let s = scene(kind);
        let mut ticks = Vec::with_capacity(TICKS);
        let mut held = keys(rng);
        let mut yaw = if rng.chance(70) {
            0.0
        } else {
            rng.range(-180, 180) as f32
        };
        let mut pitch = rng.range(-30, 30) as f32;
        for _ in 0..TICKS {
            if rng.chance(10) {
                held = keys(rng);
            }
            if rng.chance(10) {
                yaw += rng.range(-45, 45) as f32;
                pitch = rng.range(-60, 60) as f32;
            }
            ticks.push(json!({"keys": held, "yaw": yaw, "pitch": pitch}));
        }
        json!({
            "scene": kind,
            "floor": [s.floor.0, s.floor.1, s.floor.2],
            "blocks": s.blocks,
            "start": s.start,
            "elytra": kind == "elytra",
            "ticks": ticks,
        })
    }

    fn probe(&self, _env: &Env, cases: &[Case]) -> Result<Vec<Value>> {
        Ok(cases
            .iter()
            .map(|c| crate::guarded(|| probe_case(c)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_resolves_its_blocks() {
        for kind in SCENES {
            let s = scene(kind);
            assert_ne!(s.floor.1, 0, "{kind}");
            assert!(s.blocks.iter().all(|b| b[3] != 0), "{kind}");
        }
        let cases = crate::generate(&PlayerPhysicsHarness);
        assert!(
            cases
                .iter()
                .all(|c| c.case["ticks"].as_array().unwrap().len() == TICKS)
        );
    }
}
