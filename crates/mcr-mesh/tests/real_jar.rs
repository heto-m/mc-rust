mod common;

use std::time::Instant;

use common::{fixture, state_of};
use mcr_mesh::{
    FluidKind, MeshInput, Mesher, RenderLayer, Shape, TINT_FOLIAGE, TINT_GRASS, TINT_WATER,
};
use mcr_render::SectionMesh;
use mcr_world::SectionPos;

#[test]
fn classification_of_known_blocks() {
    let Some(f) = fixture() else { return };
    let c = &f.cache;
    let stone = c.state(state_of("minecraft:stone", &[]));
    assert_eq!(
        (stone.shape, stone.layer),
        (Shape::Cube, RenderLayer::Solid)
    );
    assert!(stone.cube.iter().all(|f| f.overlay == mcr_mesh::LAYER_NONE));

    let glass = c.state(state_of("minecraft:glass", &[]));
    assert_eq!(glass.shape, Shape::Cube);
    assert_ne!(glass.layer, RenderLayer::Solid);

    let stained = c.state(state_of("minecraft:red_stained_glass", &[]));
    assert_eq!(stained.layer, RenderLayer::Translucent);

    let leaves = c.state(state_of(
        "minecraft:oak_leaves",
        &[("persistent", "true"), ("waterlogged", "false")],
    ));
    assert_eq!(
        (leaves.shape, leaves.layer),
        (Shape::Cube, RenderLayer::Cutout)
    );
    assert_eq!(leaves.cube[0].tint, TINT_FOLIAGE);

    let grass = c.state(state_of("minecraft:grass_block", &[("snowy", "false")]));
    assert_eq!(grass.shape, Shape::Cube);
    assert_eq!(grass.cube[2].tint, TINT_GRASS);
    assert_ne!(grass.cube[4].overlay, mcr_mesh::LAYER_NONE);
    assert_eq!(grass.cube[4].overlay_tint, TINT_GRASS);
    assert_eq!(grass.cube[3].overlay, mcr_mesh::LAYER_NONE);

    let water = c.state(state_of("minecraft:water", &[("level", "0")]));
    assert_eq!(water.fluid.kind, FluidKind::Water);
    assert_eq!(water.shape, Shape::Empty);
    let lava = c.state(state_of("minecraft:lava", &[("level", "0")]));
    assert_eq!(lava.fluid.kind, FluidKind::Lava);

    let torch = c.state(state_of("minecraft:torch", &[]));
    assert_eq!(torch.shape, Shape::Model);
    let stairs = c.state(state_of(
        "minecraft:oak_stairs",
        &[
            ("facing", "north"),
            ("half", "bottom"),
            ("shape", "straight"),
            ("waterlogged", "false"),
        ],
    ));
    assert_eq!(stairs.shape, Shape::Model);
    let waterlogged = c.state(state_of(
        "minecraft:oak_stairs",
        &[
            ("facing", "north"),
            ("half", "bottom"),
            ("shape", "straight"),
            ("waterlogged", "true"),
        ],
    ));
    assert_eq!(waterlogged.fluid.kind, FluidKind::Water);
    assert_eq!(waterlogged.shape, Shape::Model);

    let fern = c.state(state_of("minecraft:short_grass", &[]));
    assert_eq!(fern.shape, Shape::Model);
    assert!(c.quads(fern).iter().all(|q| q.tint == TINT_GRASS));

    assert!(
        f.cache.missing_sprites.len() < 20,
        "{:?}",
        f.cache.missing_sprites
    );
    println!(
        "texture groups: {:?}",
        f.cache
            .textures
            .groups
            .iter()
            .map(|g| (g.width, g.height, g.layers))
            .collect::<Vec<_>>()
    );
    assert!(f.cache.textures.groups.len() <= 4);
}

#[test]
fn every_block_state_meshes_without_panicking() {
    let Some(f) = fixture() else { return };
    let total = mcr_data::v26_3().state_count();
    let mut mesher = Mesher::new(f.cache.clone(), f.colors.clone());
    let mut out = SectionMesh::default();
    let (mut quads, mut sections) = (0, 0);
    for base in (0..total).step_by(4096) {
        let mut input = MeshInput::blank(SectionPos::new(0, 0, 0), 2);
        for i in 0..4096usize {
            let id = (base + i) as u16;
            if base + i >= total {
                break;
            }
            let (x, y, z) = ((i % 16) as i32, (i / 256) as i32, (i / 16 % 16) as i32);
            input.set_state(x, y, z, id);
        }
        for (i, l) in input.light.iter_mut().enumerate() {
            *l = (i.wrapping_mul(2_654_435_761) >> 7) as u8;
        }
        mesher.mesh(&input, &mut out);
        quads += out.cube_count() + out.model.len() + out.translucent.len();
        sections += 1;
    }
    println!("{sections} sections, {quads} quads");
    assert!(quads > 10_000);
}

#[test]
fn build_time_and_unbaked_report() {
    let Some(f) = fixture() else { return };
    println!(
        "{} states, {} unbaked, {} model quads",
        f.cache.state_count(),
        f.cache.unbaked_states,
        f.cache.quads(&mcr_mesh::StateModel::EMPTY).len()
    );
    let t = Instant::now();
    let _ = mcr_mesh::ModelCache::build(&f.resources, mcr_data::v26_3()).unwrap();
    println!("model cache build {:?}", t.elapsed());
    let _ = TINT_WATER;
}

fn mesh_world(
    f: &common::Fixture,
    section: SectionPos,
    set: impl Fn(i32, i32, i32) -> u16,
) -> SectionMesh {
    let mut input = MeshInput::blank(section, 2);
    for y in -1..=16 {
        for z in -1..=16 {
            for x in -1..=16 {
                input.set_state(x, y, z, set(x, y, z));
            }
        }
    }
    let mut out = SectionMesh::default();
    Mesher::new(f.cache.clone(), f.colors.clone()).mesh(&input, &mut out);
    out
}

#[test]
fn grass_and_stone_rotate_by_position() {
    let Some(f) = fixture() else { return };
    let grass = state_of("minecraft:grass_block", &[("snowy", "false")]);
    let stone = state_of("minecraft:stone", &[]);
    let sm = f.cache.state(grass);
    assert_eq!(sm.shape, Shape::Cube, "rotated variants stay cubes");
    let mesh = mesh_world(f, SectionPos::new(3, 2, -5), |_, y, _| match y {
        ..=3 => stone,
        4 => grass,
        _ => 0,
    });
    let tops: Vec<_> = mesh.faces[mcr_render::Face::PosY as usize]
        .iter()
        .map(|q| q.decode())
        .collect();
    let rotations: std::collections::HashSet<u8> = tops.iter().map(|q| q.rotation).collect();
    assert!(rotations.len() >= 3, "{rotations:?}");
    let again = mesh_world(f, SectionPos::new(3, 2, -5), |_, y, _| match y {
        ..=3 => stone,
        4 => grass,
        _ => 0,
    });
    assert_eq!(mesh.faces[2], again.faces[2]);
    let checker = mesh_world(f, SectionPos::new(3, 2, -5), |x, y, z| match y {
        ..=3 => stone,
        4 if (x + z) % 2 == 0 => grass,
        _ => 0,
    });
    let sides: std::collections::HashSet<u8> = checker.faces[mcr_render::Face::PosX as usize]
        .iter()
        .map(|q| q.decode().rotation)
        .collect();
    assert_eq!(
        sides,
        [0u8].into(),
        "side faces look the same in every rotation"
    );
}

#[test]
fn flowers_are_offset_like_vanilla() {
    let Some(f) = fixture() else { return };
    let poppy = state_of("minecraft:poppy", &[]);
    let section = SectionPos::new(1, 0, 2);
    let mesh = mesh_world(f, section, |_, y, _| if y == 3 { poppy } else { 0 });
    let spec = mcr_mesh::offset_spec("minecraft:poppy");
    let plain = f.cache.quads(f.cache.state(poppy));
    assert!(!plain.is_empty());
    let mut checked = 0;
    for q in mesh.model.iter().map(|q| q.decode()) {
        let (wx, wz) = (
            section.x * 16 + i32::from(q.cell[0]),
            section.z * 16 + i32::from(q.cell[2]),
        );
        let off = mcr_mesh::block_offset(spec, wx, wz);
        let min_x = q.corners.iter().map(|c| c[0]).fold(f32::MAX, f32::min) - f32::from(q.cell[0]);
        let base_min = plain
            .iter()
            .map(|d| d.pos.iter().map(|p| p[0]).fold(f32::MAX, f32::min))
            .fold(f32::MAX, f32::min);
        if (min_x - base_min - off[0]).abs() < 1.0 / 64.0 {
            checked += 1;
        }
    }
    assert!(checked > 100, "offset applied to most quads: {checked}");
}

#[test]
fn animated_sprites_are_exported() {
    let Some(f) = fixture() else { return };
    let anims = f.cache.animations();
    assert!(anims.len() > 20, "{}", anims.len());
    let water = f.cache.fluid_sprites.water_still;
    let a = anims
        .iter()
        .find(|a| a.layer == water)
        .expect("water still animates");
    assert!(a.frames.len() > 1);
    let groups = f.cache.texture_groups();
    assert_eq!(
        groups.iter().map(|g| g.animations.len()).sum::<usize>(),
        anims.len()
    );
}

#[test]
fn partial_shapes_cull_like_vanilla() {
    let Some(f) = fixture() else { return };
    let slab = |ty: &str| {
        state_of(
            "minecraft:oak_slab",
            &[("type", ty), ("waterlogged", "false")],
        )
    };
    let count = |a: u16, b: u16| {
        let m = mesh_world(f, SectionPos::new(0, 0, 0), |x, y, z| match (x, y, z) {
            (3, 3, 3) => a,
            (4, 3, 3) => b,
            _ => 0,
        });
        m.cube_count() + m.model.len() + m.translucent.len()
    };
    let bottom = count(slab("bottom"), 0);
    let two = count(slab("bottom"), slab("bottom"));
    assert_eq!(two, 2 * bottom - 2, "shared sides cull");
    let mixed = count(slab("bottom"), slab("top"));
    assert_eq!(mixed, 2 * bottom, "disjoint halves keep their sides");
}

#[test]
fn flowing_water_uses_the_flow_sprite_on_top() {
    let Some(f) = fixture() else { return };
    let water = |level: &str| state_of("minecraft:water", &[("level", level)]);
    let stone = state_of("minecraft:stone", &[]);
    let mesh = mesh_world(f, SectionPos::new(0, 0, 0), |x, y, z| match (x, y, z) {
        (_, ..=2, _) => stone,
        (3, 3, 3) => water("0"),
        (4, 3, 3) => water("2"),
        _ => 0,
    });
    let layers: std::collections::HashSet<u16> = mesh
        .translucent
        .iter()
        .map(|q| q.decode())
        .filter(|q| q.shade_face == mcr_render::Face::PosY && q.cell[0] == 4)
        .map(|q| q.layer)
        .collect();
    assert!(layers.contains(&f.cache.fluid_sprites.water_flow));
    assert!(!layers.contains(&f.cache.fluid_sprites.water_still));
}
