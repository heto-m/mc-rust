mod common;

use common::{fixture, state_of};
use mcr_mesh::testing::{TerrainBlocks, fill_terrain_input};
use mcr_mesh::{MeshInput, Mesher, constant_palette};
use mcr_render::glam::DVec3;
use mcr_render::{Camera, FrameInputs, RenderError, Renderer, RendererConfig, SectionMesh, Target};
use mcr_world::SectionPos;

fn blocks() -> TerrainBlocks {
    TerrainBlocks {
        stone: state_of("minecraft:stone", &[]),
        dirt: state_of("minecraft:dirt", &[]),
        grass_block: state_of("minecraft:grass_block", &[("snowy", "false")]),
        sand: state_of("minecraft:sand", &[]),
        water: state_of("minecraft:water", &[("level", "0")]),
        coal_ore: state_of("minecraft:coal_ore", &[]),
        iron_ore: state_of("minecraft:iron_ore", &[]),
        log: state_of("minecraft:oak_log", &[("axis", "y")]),
        leaves: state_of(
            "minecraft:oak_leaves",
            &[("persistent", "true"), ("waterlogged", "false")],
        ),
        short_grass: state_of("minecraft:short_grass", &[]),
    }
}

fn save_png(name: &str, w: u32, h: u32, rgba: &[u8]) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::create_dir_all(&dir).unwrap();
    let file = std::fs::File::create(dir.join(name)).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

/// Display platform floating above the terrain at x 0..12, y 110, z 0..6.
fn showcase(wx: i32, wy: i32, wz: i32) -> Option<u16> {
    if !(0..13).contains(&wx) || !(0..7).contains(&wz) || !(110..114).contains(&wy) {
        return None;
    }
    let s = |n: &str, p: &[(&str, &str)]| Some(state_of(n, p));
    let platform = s("minecraft:grass_block", &[("snowy", "false")]);
    match (wy, wx, wz) {
        (110, 1..=3, 0..=2) => s("minecraft:water", &[("level", "0")]),
        (110, 5..=6, 0..=2) => s("minecraft:lava", &[("level", "0")]),
        (110, _, _) => platform,
        (111, 1, 4) => s("minecraft:glass", &[]),
        (111, 3, 4) => s("minecraft:red_stained_glass", &[]),
        (111, 4, 4) => s("minecraft:blue_stained_glass", &[]),
        (111, 5, 4) => s(
            "minecraft:oak_leaves",
            &[("persistent", "true"), ("waterlogged", "false")],
        ),
        (111, 7, 4) => s(
            "minecraft:oak_stairs",
            &[
                ("facing", "east"),
                ("half", "bottom"),
                ("shape", "straight"),
                ("waterlogged", "false"),
            ],
        ),
        (111, 9, 4) => s(
            "minecraft:oak_slab",
            &[("type", "bottom"), ("waterlogged", "false")],
        ),
        (111, 11, 4) => s("minecraft:torch", &[]),
        (111, 8, 1) => s("minecraft:short_grass", &[]),
        (111, 10, 1) => s("minecraft:grass_block", &[("snowy", "false")]),
        (111, 12, 1) => s("minecraft:stone", &[]),
        (112, 12, 1) => s("minecraft:glass", &[]),
        (111, 9, 1) => s("minecraft:poppy", &[]),
        _ => None,
    }
}

#[test]
fn real_texture_scene_renders() {
    let Some(f) = fixture() else { return };
    let mut r = match Renderer::new(
        Target::Offscreen {
            width: 1280,
            height: 720,
        },
        RendererConfig::default(),
    ) {
        Ok(r) => r,
        Err(RenderError::NoAdapter(why)) => {
            eprintln!("skipping GPU test: {why}");
            return;
        }
        Err(e) => panic!("renderer: {e}"),
    };
    r.set_texture_groups(&f.cache.texture_groups());
    r.set_tint_palette(&constant_palette());

    let b = blocks();
    let seed = 5;
    let plains = f
        .biome_names
        .iter()
        .position(|n| n == "minecraft:plains")
        .unwrap() as u16;
    let swamp = f
        .biome_names
        .iter()
        .position(|n| n == "minecraft:swamp")
        .unwrap() as u16;
    let biome = move |x: i32, _z: i32| if x < 6 { swamp } else { plains };
    let mut mesher = Mesher::new(f.cache.clone(), f.colors.clone());
    let mut mesh = SectionMesh::default();
    let mut quads = 0;
    for sz in -4..=4 {
        for sx in -4..=4 {
            for sy in 0..=7 {
                let mut input = MeshInput::blank(SectionPos::new(sx, sy, sz), 2);
                fill_terrain_input(&mut input, &b, seed, biome);
                for y in -1..=16 {
                    for z in -1..=16 {
                        for x in -1..=16 {
                            let (wx, wy, wz) = (sx * 16 + x, sy * 16 + y, sz * 16 + z);
                            if let Some(s) = showcase(wx, wy, wz) {
                                input.set_state(x, y, z, s);
                            }
                        }
                    }
                }
                mesher.mesh(&input, &mut mesh);
                quads += mesh.cube_count() + mesh.model.len() + mesh.translucent.len();
                r.set_section([sx, sy, sz], &mesh).expect("fits");
            }
        }
    }
    println!("{quads} quads");
    let views = [
        (
            "chunk-mesher-scene.png",
            DVec3::new(5.5, 114.0, 12.0),
            -0.32,
        ),
        (
            "chunk-mesher-closeup.png",
            DVec3::new(5.0, 112.6, 7.5),
            -0.3,
        ),
    ];
    for (name, position, pitch) in views {
        let cam = Camera {
            position,
            pitch,
            ..Camera::default()
        };
        for _ in 0..3 {
            r.render(&FrameInputs {
                camera: &cam,
                sky_light: 1.0,
            })
            .expect("render");
        }
        let px = r.read_offscreen().unwrap();
        save_png(name, 1280, 720, &px);
        let distinct: std::collections::HashSet<[u8; 4]> =
            px.as_chunks::<4>().0.iter().copied().collect();
        assert!(distinct.len() > 500, "{}", distinct.len());
    }
}
