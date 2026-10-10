mod common;

use std::sync::Arc;

use common::{Fixture, fixture, state_of};
use mcr_mesh::{BiomeColors, ColorMaps, MeshInput, Mesher, constant_palette};
use mcr_render::glam::DVec3;
use mcr_render::{Camera, FrameInputs, RenderError, Renderer, RendererConfig, SectionMesh, Target};
use mcr_world::SectionPos;

const LEAF_PROPS: [(&str, &str); 3] = [
    ("persistent", "true"),
    ("distance", "7"),
    ("waterlogged", "false"),
];

fn leaf_wall(f: &Fixture, colors: Arc<BiomeColors>, block: &str) -> SectionMesh {
    let state = state_of(block, &LEAF_PROPS);
    let mut input = MeshInput::blank(SectionPos::new(0, 0, 0), 2);
    for y in 4..12 {
        for x in 4..12 {
            input.set_state(x, y, 4, state);
        }
    }
    let mut out = SectionMesh::default();
    Mesher::new(f.cache.clone(), colors).mesh(&input, &mut out);
    out
}

fn unpack(color: u32) -> [u32; 3] {
    [color & 255, color >> 8 & 255, color >> 16 & 255]
}

#[test]
fn constant_tinted_leaves_resolve_to_their_palette_colour() {
    let Some(f) = fixture() else { return };
    let palette = constant_palette();
    for (block, expected) in [
        ("minecraft:spruce_leaves", [0x61, 0x99, 0x61]),
        ("minecraft:birch_leaves", [0x80, 0xa7, 0x55]),
    ] {
        let mesh = leaf_wall(f, f.colors.clone(), block);
        let quads: Vec<_> = mesh.faces.iter().flatten().map(|q| q.decode()).collect();
        assert!(!quads.is_empty(), "{block}");
        for q in quads {
            assert!(q.tint >= 8, "{block}: kind {}", q.tint);
            assert_eq!(
                unpack(palette[usize::from(q.tint - 8)]),
                expected,
                "{block}"
            );
        }
    }
}

#[test]
fn biome_tinted_leaves_get_a_table_before_biomes_are_known() {
    let Some(f) = fixture() else { return };
    let colors = Arc::new(BiomeColors::from_climates(
        &[],
        &ColorMaps::flat(0x00ff00, 0x00aa00, 0x888800),
    ));
    let mesh = leaf_wall(f, colors, "minecraft:oak_leaves");
    let table = mesh.tints.expect("tint table");
    assert!(table[256..512].iter().all(|&c| unpack(c) == [0, 0xaa, 0]));
}

fn renderer() -> Option<Renderer> {
    match Renderer::new(
        Target::Offscreen {
            width: 128,
            height: 128,
        },
        RendererConfig::default(),
    ) {
        Ok(r) => Some(r),
        Err(RenderError::NoAdapter(why)) => {
            eprintln!("skipping GPU test: {why}");
            None
        }
        Err(e) => panic!("renderer: {e}"),
    }
}

fn green_pixels(f: &Fixture, mesh: &SectionMesh) -> Option<usize> {
    let mut r = renderer()?;
    r.set_texture_groups(&f.cache.texture_groups());
    r.set_tint_palette(&constant_palette());
    r.set_section([0, 0, 0], mesh).expect("fits");
    let cam = Camera {
        position: DVec3::new(8.0, 8.0, 14.0),
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
    Some(
        px.as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[1] > p[0].saturating_add(8) && p[1] > p[2].saturating_add(8))
            .count(),
    )
}

#[test]
fn leaves_render_green_on_the_gpu() {
    let Some(f) = fixture() else { return };
    let empty = Arc::new(BiomeColors::from_climates(
        &[],
        &ColorMaps::flat(0x00ff00, 0x00aa00, 0x888800),
    ));
    for (block, colors) in [
        ("minecraft:spruce_leaves", f.colors.clone()),
        ("minecraft:birch_leaves", f.colors.clone()),
        ("minecraft:oak_leaves", empty),
    ] {
        let mesh = leaf_wall(f, colors, block);
        let Some(green) = green_pixels(f, &mesh) else {
            return;
        };
        assert!(green > 500, "{block}: {green} green pixels");
    }
}
