//! A small synthetic model cache and input helpers, usable without the client jar.

use mcr_world::SectionPos;

use crate::cache::{
    ATTR_AIR, ATTR_FULL_COLLISION, ATTR_HALF_TRANSPARENT, ATTR_LEAVES, ATTR_SOLID_RENDER, CubeFace,
    FluidInfo, FluidKind, LAYER_NONE, ModelCache, QuadDef, RenderLayer, Shape, StateModel,
};
use crate::input::MeshInput;
use crate::tint::{TINT_GRASS, TINT_NONE};

pub const AIR: u16 = 0;
pub const STONE: u16 = 1;
pub const DIRT: u16 = 2;
pub const GLASS: u16 = 3;
pub const STAINED_GLASS: u16 = 4;
pub const WATER: u16 = 5;
pub const SLAB: u16 = 6;
pub const GRASS_BLOCK: u16 = 7;
pub const LEAVES: u16 = 8;
pub const STONE_LAYER: u16 = 1;
pub const DIRT_LAYER: u16 = 2;
pub const OVERLAY_LAYER: u16 = 9;

fn cube(layer: u16, rl: RenderLayer, block: u16) -> StateModel {
    StateModel {
        shape: Shape::Cube,
        layer: rl,
        ao: true,
        block,
        cube: [CubeFace {
            layer,
            tint: TINT_NONE,
            overlay: LAYER_NONE,
            overlay_tint: TINT_NONE,
        }; 6],
        ..StateModel::EMPTY
    }
}

pub fn test_cache() -> ModelCache {
    let mut states = vec![StateModel::EMPTY; 9];
    let mut attrs = vec![0u8; 9];
    let mut occlusion = vec![0u8; 9];
    attrs[0] = ATTR_AIR;

    states[STONE as usize] = cube(STONE_LAYER, RenderLayer::Solid, 1);
    states[DIRT as usize] = cube(DIRT_LAYER, RenderLayer::Solid, 2);
    states[GLASS as usize] = cube(3, RenderLayer::Cutout, 3);
    states[STAINED_GLASS as usize] = cube(4, RenderLayer::Translucent, 4);
    states[LEAVES as usize] = cube(5, RenderLayer::Cutout, 8);
    let mut grass = cube(6, RenderLayer::Solid, 7);
    for f in 0..6 {
        if f != 2 && f != 3 {
            grass.cube[f].overlay = OVERLAY_LAYER;
            grass.cube[f].overlay_tint = TINT_GRASS;
        }
    }
    grass.cube[2].tint = TINT_GRASS;
    states[GRASS_BLOCK as usize] = grass;

    for id in [STONE, DIRT, GRASS_BLOCK] {
        attrs[id as usize] = ATTR_SOLID_RENDER | ATTR_FULL_COLLISION;
        occlusion[id as usize] = 0b11_1111;
    }
    attrs[GLASS as usize] = ATTR_FULL_COLLISION | ATTR_HALF_TRANSPARENT;
    attrs[STAINED_GLASS as usize] = ATTR_FULL_COLLISION | ATTR_HALF_TRANSPARENT;
    attrs[LEAVES as usize] = ATTR_FULL_COLLISION | ATTR_LEAVES | ATTR_HALF_TRANSPARENT;

    states[WATER as usize].fluid = FluidInfo {
        kind: FluidKind::Water,
        amount: 8,
        falling: false,
    };
    states[WATER as usize].block = 5;

    let slab_quad = |face: u8, pos: [[f32; 3]; 4]| QuadDef {
        pos,
        uv: [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
        layer: 7,
        tint: TINT_NONE,
        cull: if face == 3 { 3 } else { crate::cache::NO_CULL },
        normal: face,
        shade: face,
        translucent: false,
        boundary: face == 3,
    };
    states[SLAB as usize] = StateModel {
        shape: Shape::Model,
        ao: true,
        block: 6,
        quad_start: 0,
        quad_count: 2,
        ..StateModel::EMPTY
    };
    attrs[SLAB as usize] = 0;
    occlusion[SLAB as usize] = 1 << 3;
    let quads = vec![
        slab_quad(
            2,
            [
                [0.0, 0.5, 0.0],
                [0.0, 0.5, 1.0],
                [1.0, 0.5, 1.0],
                [1.0, 0.5, 0.0],
            ],
        ),
        slab_quad(
            3,
            [
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
            ],
        ),
    ];
    ModelCache::from_parts(states, quads, attrs, occlusion)
}

pub fn input_from(f: impl Fn(i32, i32, i32) -> u16) -> MeshInput {
    let mut input = MeshInput::blank(SectionPos::new(0, 0, 0), 2);
    for y in -1..=16 {
        for z in -1..=16 {
            for x in -1..=16 {
                input.set_state(x, y, z, f(x, y, z));
                input.set_light(x, y, z, 15, 0);
            }
        }
    }
    input
}

/// Real block state ids used by the terrain generator.
#[derive(Debug, Clone, Copy)]
pub struct TerrainBlocks {
    pub stone: u16,
    pub dirt: u16,
    pub grass_block: u16,
    pub sand: u16,
    pub water: u16,
    pub coal_ore: u16,
    pub iron_ore: u16,
    pub log: u16,
    pub leaves: u16,
    pub short_grass: u16,
}

fn hash3(seed: u32, x: i32, y: i32, z: i32) -> u32 {
    let mut h = seed
        ^ (x as u32).wrapping_mul(0x9e37_79b1)
        ^ (y as u32).wrapping_mul(0x7f4a_7c15)
        ^ (z as u32).wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^ h >> 15
}

pub const SEA_LEVEL: i32 = 62;

/// Rolling terrain with a sea, ores, scattered trees and grass tufts, as a pure function of position.
pub fn terrain_height(seed: u32, x: i32, z: i32) -> i32 {
    mcr_render::synthetic::Terrain { seed }.height(x, z)
}

fn tree_at(seed: u32, tx: i32, tz: i32) -> Option<i32> {
    let h = terrain_height(seed, tx, tz);
    (h > SEA_LEVEL + 1 && hash3(seed ^ 0x77, tx, 0, tz).is_multiple_of(70)).then_some(h)
}

pub fn terrain_state(b: &TerrainBlocks, seed: u32, x: i32, y: i32, z: i32) -> u16 {
    let h = terrain_height(seed, x, z);
    if y > h {
        if y <= SEA_LEVEL {
            return b.water;
        }
        for dz in -2..=2 {
            for dx in -2..=2 {
                if let Some(th) = tree_at(seed, x + dx, z + dz) {
                    let top = th + 5;
                    if dx == 0 && dz == 0 && y > th && y <= top {
                        return b.log;
                    }
                    let dy = y - top;
                    if (-2..=1).contains(&dy)
                        && dx.abs() + dz.abs() + dy.max(0) * 2 <= 3 + i32::from(dy < 0)
                        && !(dx.abs() == 2 && dz.abs() == 2)
                    {
                        return b.leaves;
                    }
                }
            }
        }
        if y == h + 1 && h > SEA_LEVEL && hash3(seed ^ 0x11, x, 0, z).is_multiple_of(5) {
            return b.short_grass;
        }
        return 0;
    }
    if y == h {
        return if h <= SEA_LEVEL + 1 {
            b.sand
        } else {
            b.grass_block
        };
    }
    if y >= h - 3 {
        return if h <= SEA_LEVEL + 1 { b.sand } else { b.dirt };
    }
    match hash3(seed ^ 0x33, x, y, z) % 90 {
        0 => b.coal_ore,
        1 => b.iron_ore,
        _ => b.stone,
    }
}

/// Fills `input` for section `(sx, sy, sz)` from [`terrain_state`], with a crude light gradient under
/// the surface and biome id `biome` for every column.
pub fn fill_terrain_input(
    input: &mut MeshInput,
    b: &TerrainBlocks,
    seed: u32,
    biome: impl Fn(i32, i32) -> u16,
) {
    let SectionPos {
        x: sx,
        y: sy,
        z: sz,
    } = input.pos;
    for y in -1..=16 {
        for z in -1..=16 {
            for x in -1..=16 {
                let (wx, wy, wz) = (sx * 16 + x, sy * 16 + y, sz * 16 + z);
                input.set_state(x, y, z, terrain_state(b, seed, wx, wy, wz));
                let depth = terrain_height(seed, wx, wz) - wy;
                let sky = if depth < 0 {
                    15
                } else {
                    (15 - depth).max(0) as u8
                };
                input.set_light(x, y, z, sky, 0);
            }
        }
    }
    let r = input.blend_radius as i32;
    let side = (16 + 2 * r) as usize;
    for wz in 0..side as i32 {
        for wx in 0..side as i32 {
            input.biomes[wz as usize * side + wx as usize] =
                biome(sx * 16 + wx - r, sz * 16 + wz - r);
        }
    }
}
