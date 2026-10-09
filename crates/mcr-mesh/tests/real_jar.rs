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
