//! Section mesher: culling, ambient occlusion, greedy merging and generic model quads.

use std::sync::Arc;

use mcr_render::{CubeQuad, Face, ModelQuad, ModelQuadDesc, PackedQuad, SectionMesh};

use crate::cache::{
    ATTR_AIR, ATTR_FULL_COLLISION, ATTR_HALF_TRANSPARENT, ATTR_LEAVES, ATTR_SOLID_RENDER,
    LAYER_NONE, ModelCache, NO_CULL, QuadDef, RenderLayer, Shape, StateModel,
};
use crate::faces::{
    NORMAL, RENDER_TO_VANILLA, TANGENTS, VANILLA_OFFSET, axis, corner_position, corner_uv, opposite,
};
use crate::fluid;
use crate::input::{MeshInput, PADDED, pidx};
#[cfg(test)]
use crate::tint::TINT_GRASS;
use crate::tint::{BiomeColors, TINT_WATER};

#[derive(Debug, Clone, Copy)]
pub struct MeshOptions {
    pub greedy: bool,
}

impl Default for MeshOptions {
    fn default() -> Self {
        Self { greedy: true }
    }
}

const VANILLA_ADJACENT: [[usize; 4]; 6] = [
    [4, 5, 2, 3],
    [5, 4, 2, 3],
    [1, 0, 5, 4],
    [4, 5, 0, 1],
    [1, 0, 2, 3],
    [0, 1, 2, 3],
];

const FACE_TO_PACKED: [Face; 6] = Face::ALL;

/// `[face][corner]` -> vanilla adjacency indices `(i in 0..2, j in 2..4)` for the vertex.
fn corner_table() -> [[[usize; 2]; 4]; 6] {
    let mut table = [[[0; 2]; 4]; 6];
    for (face, row) in table.iter_mut().enumerate() {
        let v = RENDER_TO_VANILLA[face];
        let [ua, va] = TANGENTS[axis(face)];
        for (c, entry) in row.iter_mut().enumerate() {
            let mut off = [0i32; 3];
            off[ua] = if c & 1 == 1 { 1 } else { -1 };
            off[va] = if c >> 1 == 1 { 1 } else { -1 };
            let dot = |dir: usize| (0..3).map(|a| off[a] * VANILLA_OFFSET[dir][a]).sum::<i32>();
            let i = (0..2)
                .find(|&k| dot(VANILLA_ADJACENT[v][k]) > 0)
                .expect("corner along first adjacency axis");
            let j = (2..4)
                .find(|&k| dot(VANILLA_ADJACENT[v][k]) > 0)
                .expect("corner along second adjacency axis");
            *entry = [i, j];
        }
    }
    table
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Shading {
    pub light: u8,
    pub ao: [u8; 4],
}

pub(crate) struct Ctx<'a> {
    pub cache: &'a ModelCache,
    pub states: &'a [u16; PADDED],
    pub light: &'a [u8; PADDED],
    pub attrs: &'a [u8; PADDED],
    pub occlusion: &'a [u8; PADDED],
}

#[inline]
fn in_pad(c: [i32; 3]) -> bool {
    c.iter().all(|&v| (-1..=16).contains(&v))
}

impl Ctx<'_> {
    #[inline]
    pub fn state(&self, c: [i32; 3]) -> u16 {
        if in_pad(c) {
            self.states[pidx(c[0], c[1], c[2])]
        } else {
            0
        }
    }

    #[inline]
    pub fn attr(&self, c: [i32; 3]) -> u8 {
        if in_pad(c) {
            self.attrs[pidx(c[0], c[1], c[2])]
        } else {
            0
        }
    }

    #[inline]
    pub fn occlusion(&self, c: [i32; 3]) -> u8 {
        if in_pad(c) {
            self.occlusion[pidx(c[0], c[1], c[2])]
        } else {
            0
        }
    }

    #[inline]
    pub fn light(&self, c: [i32; 3]) -> (u8, u8) {
        if in_pad(c) {
            let l = self.light[pidx(c[0], c[1], c[2])];
            (l >> 4, l & 15)
        } else {
            (15, 0)
        }
    }

    fn permeable(&self, c: [i32; 3]) -> bool {
        self.attr(c) & ATTR_SOLID_RENDER == 0
    }

    /// Shade in fifths: 1 for full-collision blocks, 5 otherwise.
    fn shade5(&self, c: [i32; 3]) -> i32 {
        if self.attr(c) & ATTR_FULL_COLLISION != 0 {
            1
        } else {
            5
        }
    }
}

fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn blend_neighbor(mut n: (u8, u8), center: (u8, u8)) -> (u8, u8) {
    if center.0 > 2 || center.1 > 2 {
        if n == (0, 0) {
            n = center;
        } else if n.0 == 0 {
            n.0 = center.0;
        }
    }
    n
}

pub struct Mesher {
    cache: Arc<ModelCache>,
    colors: Arc<BiomeColors>,
    options: MeshOptions,
    corners: [[[usize; 2]; 4]; 6],
    keys: Vec<u64>,
    occupied: Vec<[u64; 4]>,
    overlays: [Vec<PackedQuad>; 6],
    attrs: Vec<u8>,
    occlusion: Vec<u8>,
}

impl std::fmt::Debug for Mesher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mesher")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

const VALID: u64 = 1 << 63;

fn pack_key(layer: u16, overlay: u16, tint: u8, otint: u8, light: u8, ao: [u8; 4]) -> u64 {
    let ao =
        u64::from(ao[0]) | u64::from(ao[1]) << 2 | u64::from(ao[2]) << 4 | u64::from(ao[3]) << 6;
    VALID
        | u64::from(layer)
        | u64::from(overlay) << 16
        | u64::from(tint) << 32
        | u64::from(otint) << 38
        | u64::from(light) << 44
        | ao << 52
}

fn key_ao(key: u64) -> [u8; 4] {
    let ao = (key >> 52 & 255) as u8;
    std::array::from_fn(|i| ao >> (2 * i) & 3)
}

fn u_uniform(key: u64) -> bool {
    let ao = key_ao(key);
    ao[0] == ao[1] && ao[2] == ao[3]
}

fn v_uniform(key: u64) -> bool {
    let ao = key_ao(key);
    ao[0] == ao[2] && ao[1] == ao[3]
}

impl Mesher {
    pub fn new(cache: Arc<ModelCache>, colors: Arc<BiomeColors>) -> Self {
        Self::with_options(cache, colors, MeshOptions::default())
    }

    pub fn with_options(
        cache: Arc<ModelCache>,
        colors: Arc<BiomeColors>,
        options: MeshOptions,
    ) -> Self {
        Self {
            cache,
            colors,
            options,
            corners: corner_table(),
            keys: vec![0; 6 * 16 * 256],
            occupied: vec![[0; 4]; 6 * 16],
            overlays: Default::default(),
            attrs: vec![0; PADDED],
            occlusion: vec![0; PADDED],
        }
    }

    pub fn cache(&self) -> &Arc<ModelCache> {
        &self.cache
    }

    pub fn mesh(&mut self, input: &MeshInput, out: &mut SectionMesh) {
        out.clear();
        let cache = Arc::clone(&self.cache);
        let mut attrs = std::mem::take(&mut self.attrs);
        let mut occlusion = std::mem::take(&mut self.occlusion);
        let mut any = false;
        for (i, &s) in input.states.iter().enumerate() {
            attrs[i] = cache.attr(s);
            occlusion[i] = cache.occlusion(s);
            any |= s != 0;
        }
        if !any {
            self.attrs = attrs;
            self.occlusion = occlusion;
            return;
        }
        let ctx = Ctx {
            cache: &cache,
            states: &input.states,
            light: &input.light,
            attrs: attrs.as_slice().try_into().expect("padded size"),
            occlusion: occlusion.as_slice().try_into().expect("padded size"),
        };
        self.occupied.iter_mut().for_each(|o| *o = [0; 4]);
        let mut needs_tint = false;
        const STEP: [usize; 6] = [1, 1, 18 * 18, 18 * 18, 18, 18];

        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    let i = pidx(x, y, z);
                    let a = attrs[i];
                    if a & ATTR_AIR != 0 {
                        continue;
                    }
                    let id = input.states[i];
                    if id == 0 {
                        continue;
                    }
                    if a & ATTR_SOLID_RENDER != 0 {
                        let hidden = (0..6).all(|f| {
                            let n = if f & 1 == 0 { i + STEP[f] } else { i - STEP[f] };
                            occlusion[n] >> (f ^ 1) & 1 != 0
                        });
                        if hidden {
                            continue;
                        }
                    }
                    let cell = [x, y, z];
                    let sm = ctx.cache.state(id);
                    match sm.shape {
                        Shape::Cube => needs_tint |= self.cube(&ctx, cell, sm, out),
                        Shape::Model => needs_tint |= self.model(&ctx, cell, sm, out),
                        Shape::Empty => {}
                    }
                    if sm.fluid.kind != crate::cache::FluidKind::None {
                        needs_tint |= fluid::mesh_fluid(self, &ctx, cell, sm, out);
                    }
                }
            }
        }
        self.attrs = attrs;
        self.occlusion = occlusion;
        self.flush_cubes(out);

        if needs_tint && !self.colors.is_empty() {
            let mut table = self.colors.new_table();
            self.colors
                .fill_table(&input.biomes, input.blend_radius, &mut table);
            out.tints = Some(table);
        }
    }

    /// Light and corner occlusion for a quad facing `face`, shaded around `base`.
    pub(crate) fn shade(
        &self,
        ctx: &Ctx,
        face: usize,
        base: [i32; 3],
        center_light: [i32; 3],
        want_ao: bool,
    ) -> Shading {
        let center = ctx.light(center_light);
        if !want_ao {
            return Shading {
                light: center.0 << 4 | center.1,
                ao: [3; 4],
            };
        }
        let v = RENDER_TO_VANILLA[face];
        let n = NORMAL[face];
        let adj = |k: usize| VANILLA_OFFSET[VANILLA_ADJACENT[v][k]];
        let open = ctx.shade5(base) == 5
            && (0..4).all(|k| {
                let s = add(base, adj(k));
                ctx.shade5(s) == 5 && ctx.light(s) == center
            })
            && (0..4).all(|c| {
                let [i, j] = self.corners[face][c];
                let d = add(add(base, adj(i)), adj(j));
                ctx.shade5(d) == 5 && ctx.light(d) == center
            });
        if open {
            return Shading {
                light: center.0 << 4 | center.1,
                ao: [3; 4],
            };
        }
        let side: [[i32; 3]; 4] = std::array::from_fn(|k| add(base, adj(k)));
        let side_shade: [i32; 4] = std::array::from_fn(|k| ctx.shade5(side[k]));
        let side_light: [(u8, u8); 4] = std::array::from_fn(|k| ctx.light(side[k]));
        let permeable: [bool; 4] = std::array::from_fn(|k| ctx.permeable(add(side[k], n)));
        let center_shade = ctx.shade5(base);

        let mut ao = [0u8; 4];
        let (mut sky_sum, mut block_sum) = (0u32, 0u32);
        for (c, level) in ao.iter_mut().enumerate() {
            let [i, j] = self.corners[face][c];
            let (corner_shade, corner_light) = if !permeable[i] && !permeable[j] {
                (side_shade[0], side_light[0])
            } else {
                let cell = add(side[i], adj(j));
                (ctx.shade5(cell), ctx.light(cell))
            };
            let total = side_shade[i] + side_shade[j] + corner_shade + center_shade;
            let norm = total as f32 / 4.0 + (5 - center_shade) as f32 / 4.0;
            *level = (norm - 2.0).round().clamp(0.0, 3.0) as u8;
            let a = blend_neighbor(side_light[i], center);
            let b = blend_neighbor(side_light[j], center);
            let d = blend_neighbor(corner_light, center);
            sky_sum += u32::from(a.0 + b.0 + d.0 + center.0);
            block_sum += u32::from(a.1 + b.1 + d.1 + center.1);
        }
        Shading {
            light: (((sky_sum + 8) / 16) as u8) << 4 | ((block_sum + 8) / 16) as u8,
            ao,
        }
    }

    fn cube(&mut self, ctx: &Ctx, cell: [i32; 3], sm: &StateModel, out: &mut SectionMesh) -> bool {
        let mut tinted = false;
        let attr = ctx.attr(cell);
        for (face, cf) in sm.cube.iter().copied().enumerate() {
            if cf.layer == LAYER_NONE {
                continue;
            }
            let neighbor = add(cell, NORMAL[face]);
            if ctx.occlusion(neighbor) >> opposite(face) & 1 != 0 {
                continue;
            }
            if attr & ATTR_HALF_TRANSPARENT != 0
                && skips_same(attr, sm, ctx.cache, ctx.state(neighbor))
            {
                continue;
            }
            let shading = self.shade(ctx, face, neighbor, neighbor, sm.ao);
            tinted |=
                (1..=TINT_WATER).contains(&cf.tint) || (1..=TINT_WATER).contains(&cf.overlay_tint);
            if sm.layer == RenderLayer::Translucent {
                self.push_translucent_face(face, cell, cf.layer, cf.tint, shading, out);
                continue;
            }
            let ax = axis(face);
            let [ua, va] = TANGENTS[ax];
            let (slice, u, v) = (cell[ax] as usize, cell[ua] as usize, cell[va] as usize);
            let key = pack_key(
                cf.layer,
                cf.overlay,
                cf.tint,
                cf.overlay_tint,
                shading.light,
                shading.ao,
            );
            self.keys[(face * 16 + slice) * 256 + v * 16 + u] = key;
            self.occupied[face * 16 + slice][v / 4] |= 1u64 << ((v % 4) * 16 + u);
        }
        tinted
    }

    fn push_translucent_face(
        &self,
        face: usize,
        cell: [i32; 3],
        layer: u16,
        tint: u8,
        shading: Shading,
        out: &mut SectionMesh,
    ) {
        let corners = std::array::from_fn(|c| corner_position(face, cell, c & 1, c >> 1));
        let uvs = std::array::from_fn(|c| corner_uv(face, (c & 1) as f32, (c >> 1) as f32));
        let desc = ModelQuadDesc {
            corners,
            uvs,
            layer,
            light: shading.light,
            shade_face: FACE_TO_PACKED[face],
            flags: 0,
            ao: shading.ao,
            tint,
            cell: [cell[0] as u8, cell[2] as u8],
        };
        if let Ok(q) = ModelQuad::encode(&desc) {
            out.translucent.push(q);
        }
    }

    fn flush_cubes(&mut self, out: &mut SectionMesh) {
        for face in 0..6 {
            self.overlays[face].clear();
            for slice in 0..16 {
                self.greedy_slice(face, slice, out);
            }
            let overlays = std::mem::take(&mut self.overlays[face]);
            out.faces[face].extend_from_slice(&overlays);
            self.overlays[face] = overlays;
        }
    }

    fn greedy_slice(&mut self, face: usize, slice: usize, out: &mut SectionMesh) {
        let slot = face * 16 + slice;
        if self.occupied[slot] == [0; 4] {
            return;
        }
        let keys = &self.keys[slot * 256..slot * 256 + 256];
        let bit = |occ: &[u64; 4], u: usize, v: usize| occ[v / 4] >> ((v % 4) * 16 + u) & 1 != 0;
        let mut occ = self.occupied[slot];
        for word in 0..4 {
            while occ[word] != 0 {
                let bit_index = occ[word].trailing_zeros() as usize;
                let (u, v) = (bit_index % 16, word * 4 + bit_index / 16);
                let key = keys[v * 16 + u];
                let (mut w, mut h) = (1, 1);
                if self.options.greedy {
                    if u_uniform(key) {
                        while u + w < 16 && bit(&occ, u + w, v) && keys[v * 16 + u + w] == key {
                            w += 1;
                        }
                    }
                    if v_uniform(key) {
                        'grow: while v + h < 16 {
                            for du in 0..w {
                                if !bit(&occ, u + du, v + h) || keys[(v + h) * 16 + u + du] != key {
                                    break 'grow;
                                }
                            }
                            h += 1;
                        }
                    }
                }
                for dv in 0..h {
                    for du in 0..w {
                        occ[(v + dv) / 4] &= !(1u64 << (((v + dv) % 4) * 16 + u + du));
                    }
                }
                let ax = axis(face);
                let [ua, va] = TANGENTS[ax];
                let mut pos = [0usize; 3];
                pos[ax] = slice;
                pos[ua] = u;
                pos[va] = v;
                let ao = key_ao(key);
                let tint = (key >> 32 & 63) as u8;
                let light = (key >> 44 & 255) as u8;
                let mut q = CubeQuad {
                    x: pos[0] as u8,
                    y: pos[1] as u8,
                    z: pos[2] as u8,
                    face: FACE_TO_PACKED[face],
                    width: w as u8,
                    height: h as u8,
                    layer: (key & 0xffff) as u16,
                    ao,
                    light,
                    tint,
                };
                if let Ok(p) = PackedQuad::encode(&q) {
                    out.faces[face].push(p);
                }
                let overlay = (key >> 16 & 0xffff) as u16;
                if overlay != LAYER_NONE {
                    q.layer = overlay;
                    q.tint = (key >> 38 & 63) as u8;
                    if let Ok(p) = PackedQuad::encode(&q) {
                        self.overlays[face].push(p);
                    }
                }
            }
        }
    }

    fn model(&mut self, ctx: &Ctx, cell: [i32; 3], sm: &StateModel, out: &mut SectionMesh) -> bool {
        let attr = ctx.attr(cell);
        let mut tinted = false;
        let mut cull_visible = [None::<bool>; 6];
        for qd in ctx.cache.quads(sm) {
            if qd.cull != NO_CULL {
                let f = usize::from(qd.cull);
                let visible = *cull_visible[f].get_or_insert_with(|| {
                    let nid = ctx.state(add(cell, NORMAL[f]));
                    ctx.cache.occlusion(nid) >> opposite(f) & 1 == 0
                        && !skips_same(attr, sm, ctx.cache, nid)
                });
                if !visible {
                    continue;
                }
            }
            tinted |= (1..=TINT_WATER).contains(&qd.tint);
            self.push_model_quad(ctx, cell, sm, qd, out);
        }
        tinted
    }

    fn push_model_quad(
        &self,
        ctx: &Ctx,
        cell: [i32; 3],
        sm: &StateModel,
        qd: &QuadDef,
        out: &mut SectionMesh,
    ) {
        let face = usize::from(qd.normal);
        let front = add(cell, NORMAL[face]);
        let cubic = qd.boundary;
        let base = if cubic { front } else { cell };
        let center_light = if cubic || ctx.attr(front) & ATTR_SOLID_RENDER == 0 {
            front
        } else {
            cell
        };
        let shading = self.shade(ctx, face, base, center_light, sm.ao);
        let [ua, va] = TANGENTS[axis(face)];
        let slot_of = [0usize, 1, 3, 2];
        let mut corners = [[0.0f32; 3]; 4];
        let mut uvs = [[0.0f32; 2]; 4];
        let mut ao = [3u8; 4];
        for (i, &s) in slot_of.iter().enumerate() {
            let p = qd.pos[i];
            corners[s] = [
                cell[0] as f32 + p[0],
                cell[1] as f32 + p[1],
                cell[2] as f32 + p[2],
            ];
            uvs[s] = qd.uv[i];
            let c = usize::from(p[ua] > 0.5) + 2 * usize::from(p[va] > 0.5);
            ao[s] = shading.ao[c];
        }
        let desc = ModelQuadDesc {
            corners,
            uvs,
            layer: qd.layer,
            light: shading.light,
            shade_face: FACE_TO_PACKED[usize::from(qd.shade)],
            flags: 0,
            ao,
            tint: qd.tint,
            cell: [cell[0] as u8, cell[2] as u8],
        };
        if let Ok(q) = ModelQuad::encode(&desc) {
            if qd.translucent {
                out.translucent.push(q);
            } else {
                out.model.push(q);
            }
        }
    }

    pub(crate) fn push_fluid_quad(
        &self,
        desc: &ModelQuadDesc,
        translucent: bool,
        out: &mut SectionMesh,
    ) {
        if let Ok(q) = ModelQuad::encode(desc) {
            if translucent {
                out.translucent.push(q);
            } else {
                out.model.push(q);
            }
        }
    }
}

/// Whether `sm`'s faces against neighbour state `nid` are hidden because both are the same glass-like block.
fn skips_same(attr: u8, sm: &StateModel, cache: &ModelCache, nid: u16) -> bool {
    attr & ATTR_HALF_TRANSPARENT != 0
        && attr & ATTR_LEAVES == 0
        && cache.attr(nid) & ATTR_HALF_TRANSPARENT != 0
        && cache.state(nid).block == sm.block
}

#[cfg(test)]
mod tests {
    use mcr_render::Face as F;

    use super::*;
    use crate::testing::*;

    fn mesher(greedy: bool) -> Mesher {
        Mesher::with_options(
            Arc::new(test_cache()),
            Arc::new(crate::tint::BiomeColors::from_climates(
                &[crate::tint::BiomeClimate::default()],
                &crate::tint::ColorMaps::flat(0x112233, 0x445566, 0x778899),
            )),
            MeshOptions { greedy },
        )
    }

    fn run(greedy: bool, f: impl Fn(i32, i32, i32) -> u16) -> SectionMesh {
        let mut out = SectionMesh::default();
        mesher(greedy).mesh(&input_from(f), &mut out);
        out
    }

    fn inside(x: i32, y: i32, z: i32) -> bool {
        (0..16).contains(&x) && (0..16).contains(&y) && (0..16).contains(&z)
    }

    fn area(mesh: &SectionMesh, face: usize) -> u32 {
        mesh.faces[face]
            .iter()
            .map(|q| {
                let q = q.decode();
                u32::from(q.width) * u32::from(q.height)
            })
            .sum()
    }

    #[test]
    fn touching_opaque_cubes_hide_shared_face() {
        let mesh = run(false, |x, y, z| {
            if inside(x, y, z) && y == 5 && z == 5 && (x == 5 || x == 6) {
                STONE
            } else {
                AIR
            }
        });
        assert_eq!(mesh.cube_count(), 10);
        assert_eq!(mesh.faces[F::PosX as usize].len(), 1);
        assert_eq!(mesh.faces[F::PosX as usize][0].decode().x, 6);
        assert_eq!(mesh.faces[F::NegX as usize][0].decode().x, 5);
    }

    #[test]
    fn border_neighbours_cull_faces() {
        let mesh = run(false, |x, y, z| {
            if (inside(x, y, z) && (x, y, z) == (15, 3, 3)) || (x, y, z) == (16, 3, 3) {
                STONE
            } else {
                AIR
            }
        });
        assert!(mesh.faces[F::PosX as usize].is_empty());
        assert_eq!(mesh.cube_count(), 5);
    }

    fn terrain(x: i32, y: i32, z: i32) -> u16 {
        if !inside(x, y, z) && y > 0 {
            return AIR;
        }
        let h = 6 + (x * 7 + z * 3).rem_euclid(5) / 2 + i32::from(x > 8 && z < 6) * 2;
        match y {
            y if y < h - 2 => STONE,
            y if y < h => DIRT,
            y if y == h => GRASS_BLOCK,
            _ if y == h + 1 && x % 5 == 0 => GLASS,
            _ => AIR,
        }
    }

    #[test]
    fn greedy_preserves_area_and_reduces_quads() {
        let merged = run(true, terrain);
        let plain = run(false, terrain);
        for face in 0..6 {
            assert_eq!(area(&merged, face), area(&plain, face), "face {face}");
        }
        assert!(merged.cube_count() < plain.cube_count());
        assert_eq!(merged.translucent.len(), plain.translucent.len());
    }

    #[test]
    fn flat_floor_merges_into_one_quad_and_is_fully_lit() {
        let mesh = run(true, |x, y, z| {
            if y <= 4 && inside(x, y, z) || y < 0 {
                STONE
            } else {
                AIR
            }
        });
        let tops = &mesh.faces[F::PosY as usize];
        assert_eq!(tops.len(), 1);
        let q = tops[0].decode();
        assert_eq!((q.width, q.height), (16, 16));
        assert_eq!(q.ao, [3; 4]);
        assert_eq!(q.light, 0xf0);
    }

    fn top_quad_at(mesh: &SectionMesh, x: u8, y: u8, z: u8) -> mcr_render::CubeQuad {
        mesh.faces[F::PosY as usize]
            .iter()
            .map(|q| q.decode())
            .find(|q| (q.x, q.y, q.z) == (x, y, z))
            .expect("top quad")
    }

    #[test]
    fn ao_darkens_corners_next_to_a_block() {
        let mesh = run(false, |x, y, z| {
            if y <= 0 || (x, y, z) == (5, 1, 5) {
                STONE
            } else {
                AIR
            }
        });
        let west = top_quad_at(&mesh, 4, 0, 5);
        assert_eq!(west.ao, [3, 2, 3, 2]);
        let open = top_quad_at(&mesh, 9, 0, 9);
        assert_eq!(open.ao, [3; 4]);
    }

    #[test]
    fn ao_concave_corner_is_darkest() {
        let walls = [(5, 1, 5), (5, 2, 5), (4, 1, 6), (4, 2, 6)];
        let mesh = run(false, |x, y, z| {
            if y <= 0 || walls.contains(&(x, y, z)) {
                STONE
            } else {
                AIR
            }
        });
        let q = top_quad_at(&mesh, 4, 0, 5);
        assert_eq!(q.ao[3], 0);
        assert_eq!(q.ao[0], 3);
    }

    #[test]
    fn ao_blocks_merging_across_gradients() {
        let mesh = run(true, |x, y, z| {
            if y <= 0 || (x, y, z) == (5, 1, 5) {
                STONE
            } else {
                AIR
            }
        });
        let covering = mesh.faces[F::PosY as usize]
            .iter()
            .map(|q| q.decode())
            .find(|q| q.x <= 4 && 4 < q.x + q.width && q.z <= 5 && 5 < q.z + q.height)
            .expect("quad over the shaded cell");
        assert_eq!((covering.width, covering.height), (1, 1));
    }

    #[test]
    fn glass_is_cutout_stained_glass_translucent() {
        let mesh = run(false, |x, y, z| match (x, y, z) {
            (3, 3, 3) => GLASS,
            (8, 3, 3) => STAINED_GLASS,
            _ => AIR,
        });
        assert_eq!(mesh.cube_count(), 6);
        assert_eq!(mesh.translucent.len(), 6);
        assert!(mesh.model.is_empty());
    }

    #[test]
    fn same_glass_hides_inner_faces_but_different_does_not() {
        let pair = |a, b| {
            run(false, move |x, y, z| match (x, y, z) {
                (3, 3, 3) => a,
                (4, 3, 3) => b,
                _ => AIR,
            })
        };
        assert_eq!(pair(GLASS, GLASS).cube_count(), 10);
        assert_eq!(pair(GLASS, STONE).cube_count(), 6 + 5);
        assert_eq!(pair(STAINED_GLASS, STAINED_GLASS).translucent.len(), 10);
        assert_eq!(pair(STAINED_GLASS, GLASS).translucent.len(), 6);
    }

    #[test]
    fn leaves_do_not_cull_each_other() {
        let mesh = run(false, |x, y, z| match (x, y, z) {
            (3, 3, 3) | (4, 3, 3) => LEAVES,
            _ => AIR,
        });
        assert_eq!(mesh.cube_count(), 12);
    }

    #[test]
    fn grass_overlay_follows_base_with_same_geometry() {
        let mesh = run(true, |x, y, z| {
            if (x, y, z) == (3, 3, 3) {
                GRASS_BLOCK
            } else {
                AIR
            }
        });
        assert_eq!(mesh.cube_count(), 6 + 4);
        for face in [F::PosX, F::NegX, F::PosZ, F::NegZ] {
            let quads = &mesh.faces[face as usize];
            assert_eq!(quads.len(), 2);
            let (base, overlay) = (quads[0].decode(), quads[1].decode());
            assert_eq!(overlay.layer, OVERLAY_LAYER);
            assert_eq!(base.layer, 6);
            assert_eq!(
                (base.x, base.y, base.z, base.width),
                (overlay.x, overlay.y, overlay.z, overlay.width)
            );
        }
        assert!(mesh.tints.is_some());
        assert_eq!(mesh.faces[F::PosY as usize][0].decode().tint, TINT_GRASS);
    }

    #[test]
    fn water_source_surface_is_below_full_height() {
        let mesh = run(
            false,
            |x, y, z| if (x, y, z) == (3, 3, 3) { WATER } else { AIR },
        );
        assert_eq!(mesh.translucent.len(), 6);
        let top = mesh
            .translucent
            .iter()
            .map(|q| q.decode())
            .find(|q| q.shade_face == F::PosY)
            .expect("top face");
        let max_y = top.corners.iter().map(|c| c[1]).fold(f32::MIN, f32::max);
        assert!(max_y < 4.0 && max_y > 3.5, "{max_y}");
        assert!(mesh.tints.is_some());
    }

    #[test]
    fn water_between_water_has_no_internal_faces() {
        let mesh = run(false, |x, y, z| {
            if y == 3 && (3..=4).contains(&x) && z == 3 {
                WATER
            } else {
                AIR
            }
        });
        assert_eq!(mesh.translucent.len(), 10);
        let stacked = run(false, |x, y, z| {
            if x == 3 && z == 3 && (3..=4).contains(&y) {
                WATER
            } else {
                AIR
            }
        });
        let tops = stacked
            .translucent
            .iter()
            .map(|q| q.decode())
            .filter(|q| q.shade_face == F::PosY)
            .count();
        assert_eq!(tops, 1);
    }

    #[test]
    fn slab_model_culls_bottom_against_stone() {
        let on_stone = run(false, |x, y, z| match (x, y, z) {
            (3, 3, 3) => SLAB,
            (3, 2, 3) => STONE,
            _ => AIR,
        });
        assert_eq!(on_stone.model.len(), 1);
        let floating = run(
            false,
            |x, y, z| if (x, y, z) == (3, 3, 3) { SLAB } else { AIR },
        );
        assert_eq!(floating.model.len(), 2);
    }

    #[test]
    fn empty_input_gives_empty_mesh() {
        assert!(run(true, |_, _, _| AIR).is_empty());
    }

    #[test]
    fn light_is_taken_from_the_cell_in_front() {
        let mut input = input_from(|x, y, z| if (x, y, z) == (3, 3, 3) { STONE } else { AIR });
        for (x, z) in [
            (2, 2),
            (2, 3),
            (2, 4),
            (3, 2),
            (3, 3),
            (3, 4),
            (4, 2),
            (4, 3),
            (4, 4),
        ] {
            input.set_light(x, 4, z, 7, 5);
        }
        let mut out = SectionMesh::default();
        mesher(false).mesh(&input, &mut out);
        let top = out.faces[F::PosY as usize][0].decode();
        assert_eq!(top.light, 0x75);
        let side = out.faces[F::PosX as usize][0].decode();
        assert!((8..15).contains(&(side.light >> 4)));
    }
}
