//! Section meshing: generated 16^3 sections with their one-block border, meshed by vanilla's
//! `ModelBlockRenderer` and `FluidRenderer` over a level built from the case, and by `mcr-mesh`'s
//! `Mesher`. Quads compare per render layer as multisets: each layer maps a quad's centre (1/8 block)
//! to the quads there, sorted, with positions, sprite-local UVs, the vertex colour (tint times
//! directional shade times AO), packed light and the normal of the winding.
//!
//! Our quads are decoded the way `terrain.wgsl` expands them; merged fluid tops are split back into
//! one quad per block. Vanilla's terrain vertex format has no normal, so the facing compared is the
//! one the winding gives (what back-face culling sees), in eighths.

use std::sync::Arc;

use mcr_assets::ResourceManager;
use mcr_mesh::{
    BiomeClimate, BiomeColors, ColorMaps, DEFAULT_BLEND_RADIUS, MeshInput, MeshOptions, Mesher,
    ModelCache, PAD, TINT_CONST_BASE, pidx,
};
use mcr_render::{
    Face, MODEL_FLAG_OPAQUE, MODEL_FLAG_UP_SHADE, SectionMesh, light_block, light_sky, tint_at,
};
use mcr_world::SectionPos;
use serde_json::{Map, Value, json};

use crate::{Case, Env, Harness, ParityError, Result, Rng, round};

const CASES: usize = 400;
const STEP: f64 = 1.0 / 1024.0;
/// Vanilla rounds down at each `ARGB` step (gray, scale, multiply); the shader multiplies floats, and
/// model quads keep AO in six bits.
const COLOR_TOLERANCE: f64 = 5.0;
/// Sub-pixel offsets on tiny or sloped quads tip one eighth of the winding normal.
const NORMAL_TOLERANCE: f64 = 1.0;
/// Keeps pixel-aligned centroids (1/64 steps) off bucket boundaries, so sub-pixel offsets stay paired.
const KEY_OFFSET: f64 = 0.5625;
/// `CardinalLighting.DEFAULT` by render face (+X, -X, +Y, -Y, +Z, -Z).
const FACE_SHADE: [f32; 6] = [0.6, 0.6, 1.0, 0.5, 0.8, 0.8];
const MISSING_SPRITE: &str = "minecraft:missingno";

/// `(block, weight)`; each palette entry takes one random state of its block.
const BLOCKS: &[(&str, u32)] = &[
    ("stone", 12),
    ("dirt", 6),
    ("grass_block", 8),
    ("oak_planks", 4),
    ("cobblestone", 4),
    ("oak_log", 3),
    ("sand", 3),
    ("bricks", 2),
    ("glowstone", 2),
    ("sea_lantern", 1),
    ("magma_block", 1),
    ("redstone_block", 1),
    ("soul_sand", 1),
    ("farmland", 2),
    ("dirt_path", 2),
    ("oak_slab", 5),
    ("smooth_stone_slab", 4),
    ("oak_stairs", 5),
    ("stone_brick_stairs", 4),
    ("oak_fence", 2),
    ("cobblestone_wall", 2),
    ("glass", 4),
    ("glass_pane", 2),
    ("white_stained_glass", 2),
    ("red_stained_glass_pane", 1),
    ("tinted_glass", 1),
    ("ice", 2),
    ("honey_block", 1),
    ("slime_block", 1),
    ("oak_leaves", 4),
    ("birch_leaves", 2),
    ("spruce_leaves", 2),
    ("mangrove_leaves", 1),
    ("short_grass", 4),
    ("fern", 2),
    ("tall_grass", 2),
    ("dandelion", 2),
    ("poppy", 1),
    ("pink_petals", 1),
    ("leaf_litter", 1),
    ("bush", 1),
    ("sugar_cane", 1),
    ("vine", 1),
    ("melon_stem", 1),
    ("attached_pumpkin_stem", 1),
    ("lily_pad", 1),
    ("cactus", 1),
    ("torch", 1),
    ("wall_torch", 1),
    ("lantern", 1),
    ("snow", 2),
    ("oak_trapdoor", 2),
    ("oak_door", 1),
    ("ladder", 1),
    ("iron_bars", 1),
    ("redstone_wire", 2),
    ("scaffolding", 1),
    ("hopper", 1),
    ("cauldron", 1),
    ("water_cauldron", 1),
    ("mangrove_roots", 1),
    ("powder_snow", 1),
    ("water", 14),
    ("lava", 4),
    ("seagrass", 2),
    ("kelp", 1),
    ("bubble_column", 1),
];

#[derive(Debug)]
pub struct SectionMeshHarness;

fn random_state(rng: &mut Rng, name: &str) -> Option<u32> {
    let version = mcr_data::v26_3();
    let block = version.block_by_name(&format!("minecraft:{name}"))?;
    let (first, end) = version.block_info(block)?.states;
    Some(rng.range(i64::from(first), i64::from(end) - 1) as u32)
}

fn palette_entry(state: u32) -> Value {
    let version = mcr_data::v26_3();
    let props: Map<String, Value> = version
        .state_properties(state)
        .map(|(k, v)| (k.to_owned(), json!(v)))
        .collect();
    json!({
        "state": state,
        "block": version.state_block_name(state).unwrap_or("minecraft:air"),
        "props": props,
    })
}

/// Padded index to `(x, y, z)` in `-1..=16`.
fn cell_of(i: usize) -> [i32; 3] {
    let p = PAD as i32;
    let i = i as i32;
    [i % p - 1, i / (p * p) - 1, (i / p) % p - 1]
}

/// Cheap opaque cubes for the ground of terrain scenes.
const GROUND: &[&str] = &[
    "stone",
    "dirt",
    "grass_block",
    "sand",
    "cobblestone",
    "oak_planks",
];

fn generate(rng: &mut Rng) -> Value {
    let weights: Vec<u32> = BLOCKS.iter().map(|b| b.1).collect();
    let mut palette = vec![0u32];
    for _ in 0..rng.range(3, 14) {
        let name = BLOCKS[rng.weighted(&weights)].0;
        if let Some(state) = random_state(rng, name) {
            palette.push(state);
        }
    }
    let floor = palette.len();
    let ground = *rng.pick(GROUND);
    palette.extend(random_state(rng, ground));
    let pick = |rng: &mut Rng| rng.range(1, floor as i64 - 1) as usize;
    let mut cells = vec![0usize; PAD * PAD * PAD];
    let scene = rng.below(3);
    let per_mille = rng.range(3, 25) as u64;
    let height = rng.range(-1, 4) as i32;
    let lo: [i32; 3] = std::array::from_fn(|_| rng.range(-1, 12) as i32);
    let size = rng.range(2, 5) as i32;
    for (i, slot) in cells.iter_mut().enumerate() {
        let c = cell_of(i);
        let inside = (0..3).all(|a| (lo[a]..lo[a] + size).contains(&c[a]));
        let sprinkle = rng.next_u64() % 1000 < per_mille;
        *slot = match scene {
            1 if c[1] < height => floor,
            1 if c[1] == height && rng.chance(8) => pick(rng),
            2 if inside && rng.chance(70) => pick(rng),
            _ if sprinkle => pick(rng),
            _ => 0,
        };
    }
    let uniform = rng.chance(30).then(|| rng.range(0, 255));
    let version = mcr_data::v26_3();
    // The light engine never leaves an emitter's own cell darker than its emission.
    let light: Vec<i64> = cells
        .iter()
        .map(|&c| {
            let l = uniform.unwrap_or_else(|| rng.range(0, 255));
            let emission = i64::from(version.light_emission(palette[c]));
            l & 0xF0 | (l & 15).max(emission)
        })
        .collect();
    let mut color = || rng.range(0, 0xFF_FFFF);
    let tint =
        json!({"grass": color(), "foliage": color(), "dry_foliage": color(), "water": color()});
    json!({
        "section": [rng.range(-3, 3), rng.range(-2, 6), rng.range(-3, 3)],
        "palette": palette.iter().map(|&s| palette_entry(s)).collect::<Vec<_>>(),
        "cells": cells,
        "light": light,
        "tint": tint,
    })
}

/// One vertex in vanilla's terms: section-local position, sprite-local UV, RGB, block and sky light
/// in 1/16 levels.
#[derive(Clone, Copy, Debug)]
struct Vert {
    p: [f32; 3],
    uv: [f32; 2],
    c: [f32; 3],
    l: [u16; 2],
}

struct Quad {
    layer: &'static str,
    sprite: String,
    v: [Vert; 4],
}

/// `terrain.wgsl`'s `rotate_uv`.
fn rotate_uv(uv: [f32; 2], orientation: u8) -> [f32; 2] {
    let mut uv = uv;
    if orientation & 4 != 0 {
        uv = [-uv[0], uv[1]];
    }
    match orientation & 3 {
        1 => [-uv[1], uv[0]],
        2 => [-uv[0], -uv[1]],
        3 => [uv[1], -uv[0]],
        _ => uv,
    }
}

struct Decoder<'a> {
    cache: &'a ModelCache,
    palette: Vec<u32>,
    tints: Option<&'a [u32]>,
}

impl Decoder<'_> {
    fn sprite(&self, layer: u16) -> String {
        match self.cache.layer_sprites.get(&layer) {
            Some(name) if self.cache.missing_sprites.contains(name) => MISSING_SPRITE.to_owned(),
            Some(name) => name.clone(),
            None => format!("layer {layer}"),
        }
    }

    fn tint(&self, kind: u8, cell: [u8; 3]) -> [f32; 3] {
        let packed = match kind {
            0 => return [255.0; 3],
            k if k >= TINT_CONST_BASE => self
                .palette
                .get(usize::from(k - TINT_CONST_BASE))
                .copied()
                .unwrap_or(0),
            k => match self.tints {
                Some(t) => tint_at(t, k, cell[0], cell[1], cell[2]),
                None => return [255.0; 3],
            },
        };
        [packed & 255, packed >> 8 & 255, packed >> 16 & 255].map(|c| c as f32)
    }
}

fn light_pair(l: u16) -> [u16; 2] {
    [u16::from(light_block(l)), u16::from(light_sky(l))]
}

/// Shader corner order (triangles `0 1 2`, `2 1 3`) to vanilla's quad cycle (diagonal `v0 v2`).
const CYCLE: [usize; 4] = [1, 3, 2, 0];

fn decode(mesh: &SectionMesh, d: &Decoder) -> Vec<Quad> {
    let mut out = Vec::new();
    for list in &mesh.faces {
        for packed in list {
            let q = packed.decode();
            let face = q.face as usize;
            let axis = face >> 1;
            let (ua, va) = match axis {
                0 => (2, 1),
                1 => (0, 2),
                _ => (0, 1),
            };
            let flip = matches!(q.face, Face::PosX | Face::PosY | Face::NegZ);
            let cell = [q.x, q.y, q.z];
            let tint = d.tint(q.tint, cell);
            let corner = |vi: usize| {
                let c = if flip { vi ^ 1 } else { vi };
                let (cu, cv) = ((c & 1) as f32, (c >> 1) as f32);
                let mut p = [f32::from(q.x), f32::from(q.y), f32::from(q.z)];
                if face & 1 == 0 {
                    p[axis] += 1.0;
                }
                p[ua] += cu * f32::from(q.width);
                p[va] += cv * f32::from(q.height);
                let mirror_u = face == 0 || face == 5;
                let mirror_v = axis != 1 || face == 3;
                let u = if mirror_u { 1.0 - cu } else { cu };
                let v = if mirror_v { 1.0 - cv } else { cv };
                let uv = rotate_uv(
                    [u * f32::from(q.width), v * f32::from(q.height)],
                    q.rotation,
                );
                let shade = (0.2 + 0.2 * f32::from(q.ao[c])) * FACE_SHADE[face];
                Vert {
                    p,
                    uv,
                    c: tint.map(|t| t * shade),
                    l: light_pair(q.light[c]),
                }
            };
            out.push(Quad {
                layer: if q.opaque { "solid" } else { "cutout" },
                sprite: d.sprite(q.layer),
                v: CYCLE.map(corner),
            });
        }
    }
    for (list, translucent) in [(&mesh.model, false), (&mesh.translucent, true)] {
        for packed in list {
            let q = packed.decode();
            let tint = d.tint(q.tint, q.cell);
            let mut face_shade = FACE_SHADE[q.shade_face as usize];
            if q.flags & MODEL_FLAG_UP_SHADE != 0 {
                face_shade *= FACE_SHADE[Face::PosY as usize];
            }
            let corner = |s: usize| {
                let shade = f32::from(q.ao[s]) / 63.0 * face_shade;
                Vert {
                    p: q.corners[s],
                    uv: q.uvs[s],
                    c: tint.map(|t| t * shade),
                    l: light_pair(q.light[s]),
                }
            };
            let layer = if translucent {
                "translucent"
            } else if q.flags & MODEL_FLAG_OPAQUE != 0 {
                "solid"
            } else {
                "cutout"
            };
            out.push(Quad {
                layer,
                sprite: d.sprite(q.layer),
                v: CYCLE.map(corner),
            });
        }
    }
    out
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// A quad whose UVs span several tiles (a merged run) as one quad per tile.
fn split(q: Quad) -> Vec<Quad> {
    let min = |a: usize| q.v.iter().map(|v| v.uv[a]).fold(f32::INFINITY, f32::min);
    let max = |a: usize| {
        q.v.iter()
            .map(|v| v.uv[a])
            .fold(f32::NEG_INFINITY, f32::max)
    };
    let span = [0, 1].map(|a| (max(a) - min(a)).round().max(1.0) as usize);
    if span == [1, 1] || span.iter().any(|&s| s > 16) {
        return vec![q];
    }
    let (u0, v0) = (min(0), min(1));
    let st = q.v.map(|v| {
        [
            ((v.uv[0] - u0) / span[0] as f32).round() as usize,
            ((v.uv[1] - v0) / span[1] as f32).round() as usize,
        ]
    });
    let at = |s: usize, t: usize| st.iter().position(|&c| c == [s, t]);
    let (Some(c00), Some(c10), Some(c01), Some(c11)) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1))
    else {
        return vec![q];
    };
    let bilinear = |s: f32, t: f32| {
        let a = lerp3(q.v[c00].p, q.v[c10].p, s);
        let b = lerp3(q.v[c01].p, q.v[c11].p, s);
        lerp3(a, b, t)
    };
    let mut out = Vec::with_capacity(span[0] * span[1]);
    for a in 0..span[0] {
        for b in 0..span[1] {
            let v = std::array::from_fn(|i| {
                let [s, t] = st[i];
                Vert {
                    p: bilinear(
                        (a + s) as f32 / span[0] as f32,
                        (b + t) as f32 / span[1] as f32,
                    ),
                    uv: [s as f32, t as f32],
                    ..q.v[i]
                }
            });
            out.push(Quad {
                layer: q.layer,
                sprite: q.sprite.clone(),
                v,
            });
        }
    }
    out
}

/// The unit normal of the winding in eighths (back-face culling's side), `[0, 0, 0]` when degenerate.
fn facing(p: &[[f64; 3]; 4]) -> [i64; 3] {
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let (a, b) = (sub(p[2], p[1]), sub(p[0], p[1]));
    let n = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len == 0.0 {
        return [0; 3];
    }
    n.map(|c| (c / len * 8.0 + 0.5).floor() as i64)
}

/// The layered multiset form both sides print; `quads` yields `(layer, sprite, vertices)` with
/// vertices as `[x, y, z, u, v, r, g, b, block, sky]` in quad order.
pub(crate) fn normalize<'a>(
    quads: impl Iterator<Item = (&'a str, String, [[f64; 10]; 4])>,
) -> Value {
    let mut layers: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, Vec<(String, Value)>>,
    > = Default::default();
    for (layer, sprite, mut v) in quads {
        for a in 3..5 {
            let shift = v.iter().map(|x| x[a]).fold(f64::INFINITY, f64::min);
            let shift = (shift + 1.0 / 2048.0).floor();
            v.iter_mut().for_each(|x| x[a] -= shift);
        }
        for x in &mut v {
            for c in x.iter_mut().take(5) {
                *c = round(*c, STEP);
            }
        }
        let start = if v[2] < v[0] { 2 } else { 0 };
        let v: [[f64; 10]; 4] = std::array::from_fn(|i| v[(start + i) % 4]);
        let pos = v.map(|x| [x[0], x[1], x[2]]);
        let centre: Vec<i64> = (0..3)
            .map(|a| (pos.iter().map(|p| p[a]).sum::<f64>() * 2.0 + KEY_OFFSET).floor() as i64)
            .collect();
        let key = format!("{},{},{}", centre[0], centre[1], centre[2]);
        let n = facing(&pos);
        let sort = format!(
            "{sprite};{},{},{};{}",
            n[0],
            n[1],
            n[2],
            v.iter()
                .flat_map(|x| x[..5].iter().map(|c| (c * 1024.0) as i64))
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        let quad = json!({
            "sprite": sprite,
            "n": n,
            "p": v.iter().map(|x| x[..5].to_vec()).collect::<Vec<_>>(),
            "c": v.iter().map(|x| x[5..8].to_vec()).collect::<Vec<_>>(),
            "l": v.iter().map(|x| x[8..].to_vec()).collect::<Vec<_>>(),
        });
        layers
            .entry(layer.to_owned())
            .or_default()
            .entry(key)
            .or_default()
            .push((sort, quad));
    }
    let mut out = Map::new();
    for (layer, at) in layers {
        let mut m = Map::new();
        for (key, mut list) in at {
            list.sort_by(|a, b| a.0.cmp(&b.0));
            m.insert(key, Value::Array(list.into_iter().map(|q| q.1).collect()));
        }
        out.insert(layer, Value::Object(m));
    }
    Value::Object(out)
}

fn quad_rows(q: &Quad) -> [[f64; 10]; 4] {
    q.v.map(|v| {
        [
            f64::from(v.p[0]),
            f64::from(v.p[1]),
            f64::from(v.p[2]),
            f64::from(v.uv[0]),
            f64::from(v.uv[1]),
            round(f64::from(v.c[0]), 1.0 / 64.0),
            round(f64::from(v.c[1]), 1.0 / 64.0),
            round(f64::from(v.c[2]), 1.0 / 64.0),
            f64::from(v.l[0]),
            f64::from(v.l[1]),
        ]
    })
}

fn field<'a>(case: &'a Case, key: &str) -> Result<&'a Vec<Value>> {
    case.case[key]
        .as_array()
        .ok_or_else(|| ParityError::Invalid(format!("section case {}: no {key}", case.id)))
}

fn input(case: &Case) -> Result<(MeshInput, BiomeColors)> {
    let section = field(case, "section")?;
    let coord = |i: usize| section.get(i).and_then(Value::as_i64).unwrap_or(0) as i32;
    let pos = SectionPos {
        x: coord(0),
        y: coord(1),
        z: coord(2),
    };
    let palette: Vec<u32> = field(case, "palette")?
        .iter()
        .map(|p| p["state"].as_u64().unwrap_or(0) as u32)
        .collect();
    let mut input = MeshInput::blank(pos, DEFAULT_BLEND_RADIUS);
    for (i, (cell, light)) in field(case, "cells")?
        .iter()
        .zip(field(case, "light")?)
        .enumerate()
    {
        let [x, y, z] = cell_of(i);
        let state = palette
            .get(cell.as_u64().unwrap_or(0) as usize)
            .copied()
            .unwrap_or(0);
        let light = light.as_u64().unwrap_or(0) as u8;
        debug_assert_eq!(pidx(x, y, z), i);
        input.set_state(x, y, z, state);
        input.set_light(x, y, z, light >> 4, light & 15);
    }
    let tint = &case.case["tint"];
    let hex = |k: &str| tint[k].as_u64().unwrap_or(0) as u32;
    let climate = BiomeClimate {
        water: hex("water"),
        grass: Some(hex("grass")),
        foliage: Some(hex("foliage")),
        dry_foliage: Some(hex("dry_foliage")),
        ..BiomeClimate::default()
    };
    let colors = BiomeColors::from_climates(&[climate], &ColorMaps::flat(0, 0, 0));
    Ok((input, colors))
}

impl Harness for SectionMeshHarness {
    fn name(&self) -> &'static str {
        "section_mesh"
    }

    fn scope(&self) -> &'static [&'static str] {
        &[
            "crates/mcr-mesh/src",
            "crates/mcr-assets/src",
            "crates/mcr-render/src/quad.rs",
            "crates/mcr-render/src/mesh.rs",
            "crates/mcr-render/src/shaders/terrain.wgsl",
            "crates/mcr-data/src",
            "crates/mcr-parity/src/section_mesh.rs",
        ]
    }

    fn count(&self) -> usize {
        CASES
    }

    fn case(&self, _id: usize, rng: &mut Rng) -> Value {
        generate(rng)
    }

    fn tolerance(&self, field: &str) -> f64 {
        match field {
            "c" => COLOR_TOLERANCE,
            "n" => NORMAL_TOLERANCE,
            _ => 0.0,
        }
    }

    fn probe(&self, env: &Env, cases: &[Case]) -> Result<Vec<Value>> {
        let resources = ResourceManager::vanilla(&env.client_jar)?;
        let cache = Arc::new(ModelCache::build(&resources, mcr_data::v26_3())?);
        let options = MeshOptions {
            greedy: false,
            cutout_leaves: true,
            ambient_occlusion: true,
        };
        let palette = cache.palette();
        let mut out = Vec::with_capacity(cases.len());
        for case in cases {
            let (input, colors) = input(case)?;
            out.push(crate::guarded(|| {
                let mut mesher = Mesher::with_options(cache.clone(), Arc::new(colors), options);
                let mut mesh = SectionMesh::default();
                mesher.mesh(&input, &mut mesh);
                let decoder = Decoder {
                    cache: &cache,
                    palette: palette.clone(),
                    tints: mesh.tints.as_deref(),
                };
                let quads: Vec<Quad> = decode(&mesh, &decoder)
                    .into_iter()
                    .flat_map(split)
                    .collect();
                normalize(
                    quads
                        .iter()
                        .map(|q| (q.layer, q.sprite.clone(), quad_rows(q))),
                )
            }));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_block_exists() {
        let version = mcr_data::v26_3();
        for (name, _) in BLOCKS {
            assert!(
                version
                    .block_by_name(&format!("minecraft:{name}"))
                    .is_some(),
                "{name}"
            );
        }
    }

    #[test]
    fn cells_follow_the_padded_index() {
        for (x, y, z) in [(-1, -1, -1), (16, 16, 16), (3, 7, -1), (0, 0, 0)] {
            assert_eq!(cell_of(pidx(x, y, z)), [x, y, z]);
        }
    }

    #[test]
    fn merged_runs_split_per_tile() {
        let vert = |p: [f32; 3], uv: [f32; 2]| Vert {
            p,
            uv,
            c: [1.0; 3],
            l: [0, 240],
        };
        let q = Quad {
            layer: "translucent",
            sprite: "s".into(),
            v: [
                vert([0.0, 0.9, 0.0], [0.0, 0.0]),
                vert([0.0, 0.9, 2.0], [0.0, 2.0]),
                vert([3.0, 0.9, 2.0], [3.0, 2.0]),
                vert([3.0, 0.9, 0.0], [3.0, 0.0]),
            ],
        };
        let parts = split(q);
        assert_eq!(parts.len(), 6);
        assert!(parts.iter().all(|p| p.v.iter().all(|v| v.uv[0] <= 1.0)));
        assert_eq!(parts[5].v[2].p, [3.0, 0.9, 2.0]);
    }

    #[test]
    fn normal_form_keeps_the_diagonal_and_winding() {
        let row = |x: f64, z: f64| [x, 1.0, z, x, z, 255.0, 255.0, 255.0, 0.0, 240.0];
        let a = [row(0.0, 0.0), row(0.0, 1.0), row(1.0, 1.0), row(1.0, 0.0)];
        let b = [a[2], a[3], a[0], a[1]];
        let c = [a[1], a[2], a[3], a[0]];
        let n = |v| normalize(std::iter::once(("solid", "s".to_owned(), v)));
        assert_eq!(n(a), n(b));
        assert_ne!(n(a), n(c));
        assert_eq!(n(a)["solid"]["4,8,4"][0]["n"], json!([0, 8, 0]));
    }
}
