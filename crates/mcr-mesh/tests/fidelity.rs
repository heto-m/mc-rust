mod common;

use common::{fixture, state_of};
use mcr_mesh::{ATTR_EMISSIVE, MeshInput, Mesher};
use mcr_render::{Face, SectionMesh, light_block};
use mcr_world::SectionPos;

fn mesh(f: &common::Fixture, set: impl Fn(i32, i32, i32) -> u32) -> SectionMesh {
    let mut input = MeshInput::blank(SectionPos::new(0, 0, 0), 2);
    for y in -1..=16 {
        for z in -1..=16 {
            for x in -1..=16 {
                input.set_state(x, y, z, set(x, y, z));
                input.set_light(x, y, z, 15, 0);
            }
        }
    }
    let mut out = SectionMesh::default();
    Mesher::new(f.cache.clone(), f.colors.clone()).mesh(&input, &mut out);
    out
}

/// AO of the floor top face west of the probe block at (5, 3, 5).
fn floor_ao_beside(f: &common::Fixture, probe: u32) -> [u8; 4] {
    let stone = state_of("minecraft:stone", &[]);
    let m = mesh(f, |x, y, z| match (x, y, z) {
        (5, 3, 5) => probe,
        (_, ..=2, _) => stone,
        _ => 0,
    });
    m.faces[Face::PosY as usize]
        .iter()
        .map(|q| q.decode())
        .find(|q| q.y == 2 && q.x <= 4 && 4 < q.x + q.width && q.z <= 5 && 5 < q.z + q.height)
        .expect("floor quad")
        .ao
}

#[test]
fn ao_follows_each_blocks_shade_brightness() {
    let Some(f) = fixture() else { return };
    let open = [4u8; 4];
    let stone = state_of("minecraft:stone", &[]);
    assert_ne!(floor_ao_beside(f, stone), open, "full blocks cast AO");
    assert_eq!(floor_ao_beside(f, state_of("minecraft:glass", &[])), open);
    let barrier = state_of("minecraft:barrier", &[("waterlogged", "false")]);
    assert_eq!(floor_ao_beside(f, barrier), open);
    let soul_sand = state_of("minecraft:soul_sand", &[]);
    assert_ne!(floor_ao_beside(f, soul_sand), open, "soul sand casts AO");
    assert_ne!(floor_ao_beside(f, state_of("minecraft:mud", &[])), open);
    let snow = |layers: &str| state_of("minecraft:snow", &[("layers", layers)]);
    assert_ne!(floor_ao_beside(f, snow("8")), open);
    assert_eq!(floor_ao_beside(f, snow("7")), open);
}

fn on_plane(m: &SectionMesh, axis: usize, at: f32) -> usize {
    m.model
        .iter()
        .chain(&m.translucent)
        .map(|q| q.decode())
        .filter(|q| q.corners.iter().all(|c| (c[axis] - at).abs() < 1e-3))
        .count()
}

#[test]
fn connected_panes_and_bars_hide_the_faces_between_them() {
    let Some(f) = fixture() else { return };
    let pane = |name: &str, east: &str, west: &str| {
        state_of(
            name,
            &[
                ("east", east),
                ("west", west),
                ("north", "false"),
                ("south", "false"),
                ("waterlogged", "false"),
            ],
        )
    };
    for (a, b) in [
        ("minecraft:glass_pane", "minecraft:glass_pane"),
        ("minecraft:iron_bars", "minecraft:iron_bars"),
        ("minecraft:iron_bars", "minecraft:copper_bars"),
    ] {
        let row = mesh(f, |x, y, z| match (x, y, z) {
            (3, 3, 3) => pane(a, "true", "false"),
            (4, 3, 3) => pane(b, "false", "true"),
            _ => 0,
        });
        assert_eq!(on_plane(&row, 0, 4.0), 0, "{a} beside {b}");
    }
}

#[test]
fn emissive_elements_light_up_in_the_dark() {
    let Some(f) = fixture() else { return };
    let flower = state_of("minecraft:open_eyeblossom", &[]);
    let mut input = MeshInput::blank(SectionPos::new(0, 0, 0), 2);
    input.set_state(3, 3, 3, flower);
    for y in -1..=16 {
        for z in -1..=16 {
            for x in -1..=16 {
                input.set_light(x, y, z, 0, 0);
            }
        }
    }
    let mut out = SectionMesh::default();
    Mesher::new(f.cache.clone(), f.colors.clone()).mesh(&input, &mut out);
    let lit = out
        .model
        .iter()
        .map(|q| q.decode())
        .filter(|q| q.light.iter().all(|&l| light_block(l) == 240))
        .count();
    assert!(lit >= 2, "glow layer quads at full light: {lit}");
    assert!(lit < out.model.len(), "the base layer stays dark");
}

#[test]
fn only_active_sculk_sensors_and_magma_are_emissive() {
    let Some(f) = fixture() else { return };
    let emissive = |name: &str, props: &[(&str, &str)]| {
        f.cache.attr(state_of(name, props)) & ATTR_EMISSIVE != 0
    };
    let sensor = |name: &str, phase: &str| {
        let mut props = vec![
            ("power", "0"),
            ("sculk_sensor_phase", phase),
            ("waterlogged", "false"),
        ];
        if name.contains("calibrated") {
            props.push(("facing", "north"));
        }
        emissive(name, &props)
    };
    assert!(emissive("minecraft:magma_block", &[]));
    assert!(sensor("minecraft:sculk_sensor", "active"));
    assert!(!sensor("minecraft:sculk_sensor", "inactive"));
    assert!(!sensor("minecraft:sculk_sensor", "cooldown"));
    assert!(sensor("minecraft:calibrated_sculk_sensor", "active"));
    assert!(!sensor("minecraft:calibrated_sculk_sensor", "cooldown"));
}

#[test]
fn both_halves_of_a_tall_plant_seed_from_the_lower_one() {
    let Some(f) = fixture() else { return };
    let upper = f
        .cache
        .state(state_of("minecraft:tall_grass", &[("half", "upper")]));
    let lower = f
        .cache
        .state(state_of("minecraft:tall_grass", &[("half", "lower")]));
    assert_eq!((upper.seed_offset, lower.seed_offset), ([0, -1, 0], [0; 3]));
    let foot = f.cache.state(state_of(
        "minecraft:red_bed",
        &[("facing", "east"), ("occupied", "false"), ("part", "foot")],
    ));
    assert_eq!(foot.seed_offset, [1, 0, 0]);
    let mut quads = Vec::new();
    for pos in [[i32::MIN; 3], [i32::MAX; 3]] {
        for state in [
            state_of("minecraft:tall_grass", &[("half", "upper")]),
            state_of("minecraft:stone", &[]),
            state_of("minecraft:poppy", &[]),
        ] {
            f.cache.model_quads(state, pos, &mut quads);
        }
    }
    assert!(!quads.is_empty());
}
