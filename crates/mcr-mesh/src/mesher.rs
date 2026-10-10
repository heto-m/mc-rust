//! Section mesher: culling, smooth lighting, greedy merging, variants and generic model quads.

use std::sync::Arc;

use mcr_render::{
    CubeQuad, Face, ModelQuad, ModelQuadDesc, PackedQuad, SectionMesh, cube_ao_brightness, model_ao,
};

use crate::cache::{
    ATTR_AIR, ATTR_EMISSIVE, ATTR_FULL_COLLISION, ATTR_HALF_TRANSPARENT, ATTR_LEAVES,
    ATTR_LIGHT_OPAQUE, ATTR_SOLID_RENDER, Alt, CubeFace, FluidKind, LAYER_NONE, META_ATTR_MASK,
    META_EMISSION_SHIFT, META_FULL_SHIFT, META_PARTIAL_SHIFT, ModelCache, NO_CULL, QuadDef,
    RenderLayer, Shape, StateModel,
};
use crate::faces::{
    NORMAL, RENDER_TO_VANILLA, TANGENTS, VANILLA_OFFSET, axis, corner_position, corner_uv,
};
use crate::fluid;
use crate::input::{MeshInput, PADDED, pidx};
use crate::random::{JavaRandom, OFFSET_NONE, block_offset, mth_get_seed};
use crate::tint::{BiomeColors, NO_Y, TINT_WATER};

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

/// Faces whose vanilla quad diagonal runs between corners 0 and 3; the others use 1-2.
pub(crate) const DIAG03: [bool; 6] = [true, false, true, false, false, true];

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

/// Light and ambient occlusion of the four corners of a face. `ao` counts the unoccluded shade terms
/// (0..=4); light is sky and block in 1/16 levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shading {
    pub light: [u16; 4],
    pub ao: [u8; 4],
}

impl Shading {
    pub fn uniform(sky: u8, block: u8) -> Self {
        Self {
            light: [mcr_render::pack_light(sky.min(15) * 16, block.min(15) * 16); 4],
            ao: [4; 4],
        }
    }
}

pub(crate) struct Ctx<'a> {
    pub cache: &'a ModelCache,
    pub states: &'a [u16; PADDED],
    pub light: &'a [u8; PADDED],
    pub meta: &'a [u32; PADDED],
    pub origin: [i32; 3],
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
    pub fn meta(&self, c: [i32; 3]) -> u32 {
        if in_pad(c) {
            self.meta[pidx(c[0], c[1], c[2])]
        } else {
            0
        }
    }

    #[inline]
    pub fn attr(&self, c: [i32; 3]) -> u8 {
        (self.meta(c) & META_ATTR_MASK) as u8
    }

    /// Sky and block light at `c` as vanilla's `getLightCoords` reads it: emitting blocks raise block
    /// light and emissive ones are fully bright.
    #[inline]
    pub fn light(&self, c: [i32; 3]) -> (u8, u8) {
        if !in_pad(c) {
            return (15, 0);
        }
        let i = pidx(c[0], c[1], c[2]);
        let m = self.meta[i];
        if m & u32::from(ATTR_EMISSIVE) != 0 {
            return (15, 15);
        }
        let l = self.light[i];
        let emission = (m >> META_EMISSION_SHIFT & 15) as u8;
        (l >> 4, (l & 15).max(emission))
    }

    fn permeable(&self, c: [i32; 3]) -> bool {
        self.attr(c) & ATTR_LIGHT_OPAQUE == 0
    }

    /// Shade in fifths: 1 for full-collision blocks, 5 otherwise.
    fn shade5(&self, c: [i32; 3]) -> i32 {
        if self.attr(c) & ATTR_FULL_COLLISION != 0 {
            1
        } else {
            5
        }
    }

    /// Whether the face of `state` towards `neighbor` (the cell at `cell + normal[face]`) is drawn.
    pub fn face_visible(&self, state: u16, neighbor: [i32; 3], face: usize) -> bool {
        let m = self.meta(neighbor);
        let opp = face ^ 1;
        if m >> (META_FULL_SHIFT + opp as u32) & 1 != 0 {
            return false;
        }
        let nid = self.state(neighbor);
        if skips_same(
            self.cache.attr(state),
            self.cache.state(state).block,
            self.cache,
            nid,
        ) {
            return false;
        }
        if m >> (META_PARTIAL_SHIFT + opp as u32) & 1 == 0 {
            return true;
        }
        self.cache.face_visible(state, nid, face)
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
    keys: Vec<u128>,
    occupied: Vec<[u64; 4]>,
    overlays: [Vec<PackedQuad>; 6],
    meta: Vec<u32>,
    tint_y: [u8; 256],
}

impl std::fmt::Debug for Mesher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mesher")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

const VALID: u128 = 1 << 127;

struct Key {
    layer: u16,
    overlay: u16,
    tint: u8,
    otint: u8,
    rot: u8,
    orot: u8,
    shading: Shading,
}

fn pack_key(k: &Key) -> u128 {
    let mut ao = 0u128;
    for (i, a) in k.shading.ao.iter().enumerate() {
        ao |= u128::from(*a) << (3 * i);
    }
    let mut light = 0u128;
    for (i, l) in k.shading.light.iter().enumerate() {
        light |= u128::from(*l) << (16 * i);
    }
    VALID
        | u128::from(k.layer)
        | u128::from(k.overlay) << 16
        | u128::from(k.tint) << 32
        | u128::from(k.otint) << 38
        | u128::from(k.rot) << 44
        | u128::from(k.orot) << 47
        | ao << 50
        | light << 62
}

fn key_ao(key: u128) -> [u8; 4] {
    std::array::from_fn(|i| (key >> (50 + 3 * i) & 7) as u8)
}

fn key_light(key: u128) -> [u16; 4] {
    std::array::from_fn(|i| (key >> (62 + 16 * i)) as u16)
}

fn u_uniform(key: u128) -> bool {
    let (ao, l) = (key_ao(key), key_light(key));
    ao[0] == ao[1] && ao[2] == ao[3] && l[0] == l[1] && l[2] == l[3]
}

fn v_uniform(key: u128) -> bool {
    let (ao, l) = (key_ao(key), key_light(key));
    ao[0] == ao[2] && ao[1] == ao[3] && l[0] == l[2] && l[1] == l[3]
}

/// Which model slots of a cube-derived quad hold which cube corners, matching vanilla's diagonal.
fn cube_slots(face: usize) -> [usize; 4] {
    if DIAG03[face] {
        [1, 0, 3, 2]
    } else {
        [0, 1, 2, 3]
    }
}

const MAX_PARTS: usize = 32;

/// The chosen alternative of every part of a random model.
struct Picks {
    alts: [u32; MAX_PARTS],
    count: usize,
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
            meta: vec![0; PADDED],
            tint_y: [NO_Y; 256],
        }
    }

    pub fn cache(&self) -> &Arc<ModelCache> {
        &self.cache
    }

    pub fn mesh(&mut self, input: &MeshInput, out: &mut SectionMesh) {
        out.clear();
        let cache = Arc::clone(&self.cache);
        let mut meta = std::mem::take(&mut self.meta);
        let mut any = false;
        for (m, &s) in meta.iter_mut().zip(input.states.iter()) {
            *m = cache.meta(s);
            any |= s != 0;
        }
        if !any {
            self.meta = meta;
            return;
        }
        let ctx = Ctx {
            cache: &cache,
            states: &input.states,
            light: &input.light,
            meta: meta.as_slice().try_into().expect("padded size"),
            origin: [input.pos.x * 16, input.pos.y * 16, input.pos.z * 16],
        };
        self.occupied.iter_mut().for_each(|o| *o = [0; 4]);
        self.tint_y = [NO_Y; 256];
        let mut needs_tint = false;
        const STEP: [usize; 6] = [1, 1, 18 * 18, 18 * 18, 18, 18];

        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    let i = pidx(x, y, z);
                    let m = meta[i];
                    if m & u32::from(ATTR_AIR) != 0 {
                        continue;
                    }
                    let id = input.states[i];
                    if id == 0 {
                        continue;
                    }
                    if m & u32::from(ATTR_SOLID_RENDER) != 0 {
                        let hidden = (0..6).all(|f| {
                            let n = if f & 1 == 0 { i + STEP[f] } else { i - STEP[f] };
                            meta[n] >> (META_FULL_SHIFT + (f ^ 1) as u32) & 1 != 0
                        });
                        if hidden {
                            continue;
                        }
                    }
                    let cell = [x, y, z];
                    let sm = ctx.cache.state(id);
                    match sm.shape {
                        Shape::Cube => needs_tint |= self.cube(&ctx, cell, id, sm, out),
                        Shape::Model => needs_tint |= self.model(&ctx, cell, id, sm, out),
                        Shape::Empty => {}
                    }
                    if sm.fluid.kind != FluidKind::None {
                        needs_tint |= fluid::mesh_fluid(self, &ctx, cell, id, sm, out);
                    }
                }
            }
        }
        self.meta = meta;
        self.flush_cubes(out);

        if needs_tint {
            let mut table = self.colors.new_table();
            self.colors
                .fill_table_zoomed(input, &self.tint_y, &mut table);
            out.tints = Some(table);
        }
    }

    pub(crate) fn note_tint(&mut self, cell: [i32; 3]) {
        let col = cell[2] as usize * 16 + cell[0] as usize;
        let y = cell[1] as u8;
        if self.tint_y[col] == NO_Y || self.tint_y[col] < y {
            self.tint_y[col] = y;
        }
    }

    /// Corner light and occlusion for a quad facing `face`, shaded around `base` with `center` the cell
    /// whose light stands for the quad.
    pub(crate) fn shade(
        &self,
        ctx: &Ctx,
        face: usize,
        base: [i32; 3],
        center_cell: [i32; 3],
    ) -> Shading {
        let center = ctx.light(center_cell);
        let v = RENDER_TO_VANILLA[face];
        let n = NORMAL[face];
        let adj = |k: usize| VANILLA_OFFSET[VANILLA_ADJACENT[v][k]];
        let uniform = Shading::uniform(center.0, center.1);
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
            return uniform;
        }
        let side: [[i32; 3]; 4] = std::array::from_fn(|k| add(base, adj(k)));
        let side_shade: [i32; 4] = std::array::from_fn(|k| ctx.shade5(side[k]));
        let side_light: [(u8, u8); 4] = std::array::from_fn(|k| ctx.light(side[k]));
        let permeable: [bool; 4] = std::array::from_fn(|k| ctx.permeable(add(side[k], n)));
        let center_shade = ctx.shade5(base);

        let mut out = Shading {
            light: [0; 4],
            ao: [0; 4],
        };
        for c in 0..4 {
            let [i, j] = self.corners[face][c];
            let (corner_shade, corner_light) = if !permeable[i] && !permeable[j] {
                (side_shade[0], side_light[0])
            } else {
                let cell = add(side[i], adj(j));
                (ctx.shade5(cell), ctx.light(cell))
            };
            let total = side_shade[i] + side_shade[j] + corner_shade + center_shade;
            out.ao[c] = ((total - 4) / 4) as u8;
            let a = blend_neighbor(side_light[i], center);
            let b = blend_neighbor(side_light[j], center);
            let d = blend_neighbor(corner_light, center);
            let sky = (a.0 + b.0 + d.0 + center.0) * 4;
            let block = (a.1 + b.1 + d.1 + center.1) * 4;
            out.light[c] = mcr_render::pack_light(sky, block);
        }
        out
    }

    /// Flat lighting: one light value, no corner occlusion.
    fn flat(ctx: &Ctx, light_cell: [i32; 3], own: &StateModel, own_attr: u8) -> Shading {
        if own_attr & ATTR_EMISSIVE != 0 {
            return Shading::uniform(15, 15);
        }
        let (sky, block) = ctx.light(light_cell);
        Shading::uniform(sky, block.max(own.emission))
    }

    fn world_cell(ctx: &Ctx, cell: [i32; 3]) -> [i32; 3] {
        add(ctx.origin, cell)
    }

    fn picks(&self, ctx: &Ctx, cell: [i32; 3], sm: &StateModel) -> Picks {
        let w = Self::world_cell(ctx, cell);
        let model = self.cache.random_model(sm).expect("random state");
        let mut rng = JavaRandom::new(mth_get_seed(w[0], w[1], w[2]));
        let mut picks = Picks {
            alts: [0; MAX_PARTS],
            count: 0,
        };
        let seed = if model.multipart {
            Some(rng.next_long())
        } else {
            None
        };
        for p in 0..(model.part_count as usize).min(MAX_PARTS) {
            if let Some(s) = seed {
                rng.set_seed(s);
            }
            let span = self.cache.part_span(model.part_start + p as u32);
            let alt = if span.alt_count > 1 {
                let mut roll = rng.next_int(span.total_weight as i32) as u32;
                let mut chosen = span.alt_start;
                for a in span.alt_start..span.alt_start + span.alt_count {
                    let weight = self.cache.alt(a).weight;
                    if roll < weight {
                        chosen = a;
                        break;
                    }
                    roll -= weight;
                }
                chosen
            } else {
                span.alt_start
            };
            picks.alts[p] = alt;
            picks.count = p + 1;
        }
        picks
    }

    fn cube(
        &mut self,
        ctx: &Ctx,
        cell: [i32; 3],
        id: u16,
        sm: &StateModel,
        out: &mut SectionMesh,
    ) -> bool {
        let faces = if sm.random != 0 {
            let picks = self.picks(ctx, cell, sm);
            self.cache.alt(picks.alts[0]).cube
        } else {
            sm.cube
        };
        let mut tinted = false;
        let attr = ctx.attr(cell);
        for (face, cf) in faces.iter().copied().enumerate() {
            if cf.layer == LAYER_NONE {
                continue;
            }
            let neighbor = add(cell, NORMAL[face]);
            if !ctx.face_visible(id, neighbor, face) {
                continue;
            }
            let shading = if sm.ao {
                self.shade(ctx, face, neighbor, neighbor)
            } else {
                Self::flat(ctx, neighbor, sm, attr)
            };
            let is_tinted =
                (1..=TINT_WATER).contains(&cf.tint) || (1..=TINT_WATER).contains(&cf.overlay_tint);
            if is_tinted {
                self.note_tint(cell);
            }
            tinted |= is_tinted;
            if sm.layer == RenderLayer::Translucent {
                self.push_translucent_face(face, cell, &cf, shading, out);
                continue;
            }
            let ax = axis(face);
            let [ua, va] = TANGENTS[ax];
            let (slice, u, v) = (cell[ax] as usize, cell[ua] as usize, cell[va] as usize);
            let key = pack_key(&Key {
                layer: cf.layer,
                overlay: cf.overlay,
                tint: cf.tint,
                otint: cf.overlay_tint,
                rot: cf.rot,
                orot: cf.overlay_rot,
                shading,
            });
            self.keys[(face * 16 + slice) * 256 + v * 16 + u] = key;
            self.occupied[face * 16 + slice][v / 4] |= 1u64 << ((v % 4) * 16 + u);
        }
        tinted
    }

    fn push_translucent_face(
        &self,
        face: usize,
        cell: [i32; 3],
        cf: &CubeFace,
        shading: Shading,
        out: &mut SectionMesh,
    ) {
        let slots = cube_slots(face);
        let mut corners = [[0.0; 3]; 4];
        let mut uvs = [[0.0; 2]; 4];
        let mut light = [0u16; 4];
        let mut ao = [0u8; 4];
        for (s, &c) in slots.iter().enumerate() {
            corners[s] = corner_position(face, cell, c & 1, c >> 1);
            uvs[s] = corner_uv(face, (c & 1) as f32, (c >> 1) as f32);
            light[s] = shading.light[c];
            ao[s] = model_ao(cube_ao_brightness(shading.ao[c]));
        }
        let desc = ModelQuadDesc {
            corners,
            uvs,
            layer: cf.layer,
            light,
            shade_face: FACE_TO_PACKED[face],
            flags: 0,
            ao,
            tint: cf.tint,
            cell: [cell[0] as u8, cell[1] as u8, cell[2] as u8],
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
                let mut q = CubeQuad {
                    x: pos[0] as u8,
                    y: pos[1] as u8,
                    z: pos[2] as u8,
                    face: FACE_TO_PACKED[face],
                    width: w as u8,
                    height: h as u8,
                    layer: (key & 0xffff) as u16,
                    ao: key_ao(key),
                    light: key_light(key),
                    tint: (key >> 32 & 63) as u8,
                    rotation: (key >> 44 & 7) as u8,
                };
                if let Ok(p) = PackedQuad::encode(&q) {
                    out.faces[face].push(p);
                }
                let overlay = (key >> 16 & 0xffff) as u16;
                if overlay != LAYER_NONE {
                    q.layer = overlay;
                    q.tint = (key >> 38 & 63) as u8;
                    q.rotation = (key >> 47 & 7) as u8;
                    if let Ok(p) = PackedQuad::encode(&q) {
                        self.overlays[face].push(p);
                    }
                }
            }
        }
    }

    fn model(
        &mut self,
        ctx: &Ctx,
        cell: [i32; 3],
        id: u16,
        sm: &StateModel,
        out: &mut SectionMesh,
    ) -> bool {
        let cache = Arc::clone(&self.cache);
        let spec = sm.offset;
        let offset = if spec.kind == OFFSET_NONE {
            [0.0; 3]
        } else {
            let w = Self::world_cell(ctx, cell);
            block_offset(spec, w[0], w[2])
        };
        let mut state = ModelState {
            ctx,
            cell,
            id,
            sm,
            offset,
            visible: [None; 6],
            shaded: [None; 12],
            tinted: false,
        };
        if sm.random != 0 {
            let picks = self.picks(ctx, cell, sm);
            for &a in &picks.alts[..picks.count] {
                let Alt {
                    quad_start,
                    quad_count,
                    ..
                } = *cache.alt(a);
                for qd in cache.quads_range(quad_start, quad_count) {
                    self.model_quad(&mut state, qd, out);
                }
            }
        } else {
            for qd in cache.quads(sm) {
                self.model_quad(&mut state, qd, out);
            }
        }
        state.tinted
    }

    fn model_quad(&mut self, st: &mut ModelState, qd: &QuadDef, out: &mut SectionMesh) {
        let (ctx, cell) = (st.ctx, st.cell);
        if qd.cull != NO_CULL {
            let f = usize::from(qd.cull);
            let visible = *st.visible[f]
                .get_or_insert_with(|| ctx.face_visible(st.id, add(cell, NORMAL[f]), f));
            if !visible {
                return;
            }
        }
        if (1..=TINT_WATER).contains(&qd.tint) {
            st.tinted = true;
            self.note_tint(cell);
        }
        self.push_model_quad(st, qd, out);
    }

    fn push_model_quad(&self, st: &mut ModelState, qd: &QuadDef, out: &mut SectionMesh) {
        let (ctx, cell, sm) = (st.ctx, st.cell, st.sm);
        let face = usize::from(qd.normal);
        let front = add(cell, NORMAL[face]);
        let cubic = qd.boundary || (qd.flat && ctx.attr(cell) & ATTR_FULL_COLLISION != 0);
        let shading = if sm.ao {
            let memo = face * 2 + usize::from(cubic);
            *st.shaded[memo].get_or_insert_with(|| {
                let base = if cubic { front } else { cell };
                let center = if cubic || ctx.attr(front) & ATTR_SOLID_RENDER == 0 {
                    front
                } else {
                    cell
                };
                self.shade(ctx, face, base, center)
            })
        } else {
            let own = ctx.attr(cell);
            let light_cell = if qd.cull != NO_CULL {
                add(cell, NORMAL[usize::from(qd.cull)])
            } else if cubic {
                front
            } else {
                cell
            };
            Self::flat(ctx, light_cell, sm, own)
        };
        let [ua, va] = TANGENTS[axis(face)];
        let slot_of = [1usize, 3, 2, 0];
        let mut corners = [[0.0f32; 3]; 4];
        let mut uvs = [[0.0f32; 2]; 4];
        let mut light = [0u16; 4];
        let mut ao = [0u8; 4];
        for (i, &s) in slot_of.iter().enumerate() {
            let p = qd.pos[i];
            corners[s] = [
                cell[0] as f32 + p[0] + st.offset[0],
                cell[1] as f32 + p[1] + st.offset[1],
                cell[2] as f32 + p[2] + st.offset[2],
            ];
            uvs[s] = qd.uv[i];
            let (u, v) = (p[ua].clamp(0.0, 1.0), p[va].clamp(0.0, 1.0));
            let w = [(1.0 - u) * (1.0 - v), u * (1.0 - v), (1.0 - u) * v, u * v];
            let sky: f32 = (0..4)
                .map(|c| w[c] * f32::from(mcr_render::light_sky(shading.light[c])))
                .sum();
            let block: f32 = (0..4)
                .map(|c| w[c] * f32::from(mcr_render::light_block(shading.light[c])))
                .sum();
            light[s] = mcr_render::pack_light(sky as u8, block as u8);
            let b: f32 = (0..4)
                .map(|c| w[c] * cube_ao_brightness(shading.ao[c]))
                .sum();
            ao[s] = model_ao(b);
        }
        let desc = ModelQuadDesc {
            corners,
            uvs,
            layer: qd.layer,
            light,
            shade_face: FACE_TO_PACKED[usize::from(qd.shade)],
            flags: 0,
            ao,
            tint: qd.tint,
            cell: [cell[0] as u8, cell[1] as u8, cell[2] as u8],
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

struct ModelState<'a, 'b> {
    ctx: &'a Ctx<'b>,
    cell: [i32; 3],
    id: u16,
    sm: &'a StateModel,
    offset: [f32; 3],
    visible: [Option<bool>; 6],
    shaded: [Option<Shading>; 12],
    tinted: bool,
}

/// Whether faces of a glass-like block against neighbour state `nid` are hidden because both are the
/// same block.
fn skips_same(attr: u8, block: u16, cache: &ModelCache, nid: u16) -> bool {
    attr & ATTR_HALF_TRANSPARENT != 0
        && attr & ATTR_LEAVES == 0
        && cache.attr(nid) & ATTR_HALF_TRANSPARENT != 0
        && cache.state(nid).block == block
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
        assert_eq!(q.ao, [4; 4]);
        assert_eq!(q.light, [mcr_render::LIGHT_FULL_SKY; 4]);
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
        assert_eq!(west.ao, [4, 3, 4, 3]);
        let open = top_quad_at(&mesh, 9, 0, 9);
        assert_eq!(open.ao, [4; 4]);
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
        assert_eq!(q.ao[3], 1);
        assert_eq!(q.ao[0], 4);
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
        assert_eq!(
            mesh.faces[F::PosY as usize][0].decode().tint,
            crate::tint::TINT_GRASS
        );
    }

    #[test]
    fn water_source_surface_is_below_full_height() {
        let mesh = run(
            false,
            |x, y, z| if (x, y, z) == (3, 3, 3) { WATER } else { AIR },
        );
        assert_eq!(mesh.translucent.len(), 11, "top and sides carry back faces");
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
        assert_eq!(mesh.translucent.len(), 18);
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
        assert_eq!(tops, 2, "one top surface, front and back");
    }

    #[test]
    fn slab_model_culls_bottom_against_stone() {
        let on_stone = run(false, |x, y, z| match (x, y, z) {
            (3, 3, 3) => SLAB,
            (3, 2, 3) => STONE,
            _ => AIR,
        });
        assert_eq!(on_stone.model.len(), 5);
        let floating = run(
            false,
            |x, y, z| if (x, y, z) == (3, 3, 3) { SLAB } else { AIR },
        );
        assert_eq!(floating.model.len(), 6);
    }

    #[test]
    fn empty_input_gives_empty_mesh() {
        assert!(run(true, |_, _, _| AIR).is_empty());
    }

    fn lit_input(light: impl Fn(i32, i32, i32) -> (u8, u8)) -> MeshInput {
        let mut input = input_from(|x, y, z| if (x, y, z) == (3, 3, 3) { STONE } else { AIR });
        for y in -1..=16 {
            for z in -1..=16 {
                for x in -1..=16 {
                    let (s, b) = light(x, y, z);
                    input.set_light(x, y, z, s, b);
                }
            }
        }
        input
    }

    fn mesh_of(input: &MeshInput) -> SectionMesh {
        let mut out = SectionMesh::default();
        mesher(false).mesh(input, &mut out);
        out
    }

    #[test]
    fn light_comes_from_the_cell_in_front_per_corner() {
        let input = lit_input(|_, _, _| (7, 5));
        let top = mesh_of(&input).faces[F::PosY as usize][0].decode();
        let want = mcr_render::pack_light(7 * 16, 5 * 16);
        assert_eq!(top.light, [want; 4]);
    }

    #[test]
    fn corner_light_averages_the_four_cells_around_each_corner() {
        let input = lit_input(|x, y, z| {
            if y == 4 && x == 4 && z == 2 {
                (3, 3)
            } else if y == 4 {
                (11, 7)
            } else {
                (0, 0)
            }
        });
        let top = mesh_of(&input).faces[F::PosY as usize][0].decode();
        let corner = |l: u16| (mcr_render::light_sky(l), mcr_render::light_block(l));
        // Corner (1, 0): east side (4,4,3) and north side (3,4,2) are bright, the diagonal (4,4,2) is dim.
        assert_eq!(corner(top.light[1]), ((11 * 3 + 3) * 4, (7 * 3 + 3) * 4));
        assert_eq!(corner(top.light[0]), (11 * 16, 7 * 16));
        assert_eq!(corner(top.light[3]), (11 * 16, 7 * 16));
    }

    #[test]
    fn dark_neighbours_take_the_centre_light() {
        let input = lit_input(|x, y, _z| {
            if y == 4 && x == 2 {
                (0, 0)
            } else if y == 4 {
                (10, 0)
            } else {
                (0, 0)
            }
        });
        let top = mesh_of(&input).faces[F::PosY as usize][0].decode();
        assert_eq!(mcr_render::light_sky(top.light[0]), 10 * 16);
    }

    #[test]
    fn merge_requires_equal_corner_light() {
        let mut input = input_from(|_x, y, _z| if y <= 0 { STONE } else { AIR });
        input.set_light(5, 1, 5, 4, 0);
        let mut out = SectionMesh::default();
        mesher(true).mesh(&input, &mut out);
        let tops = &out.faces[F::PosY as usize];
        assert!(tops.len() > 1);
        for q in tops {
            let q = q.decode();
            if q.width * q.height > 1 {
                assert_eq!(q.light[0], q.light[1]);
            }
        }
        let covering = tops
            .iter()
            .map(|q| q.decode())
            .find(|q| q.x <= 5 && 5 < q.x + q.width && q.z <= 5 && 5 < q.z + q.height)
            .unwrap();
        assert_eq!((covering.width, covering.height), (1, 1));
    }

    fn pair(a: u16, b: u16) -> SectionMesh {
        run(false, move |x, y, z| match (x, y, z) {
            (3, 3, 3) => a,
            (4, 3, 3) => b,
            _ => AIR,
        })
    }

    #[test]
    fn slab_against_slab_culls_the_shared_sides() {
        assert_eq!(pair(SLAB, SLAB).model.len(), 12 - 2);
    }

    #[test]
    fn disjoint_half_slabs_keep_their_sides() {
        assert_eq!(pair(SLAB, SLAB_TOP).model.len(), 12);
    }

    #[test]
    fn full_block_over_a_slab_keeps_its_bottom_face() {
        let mesh = run(false, |x, y, z| match (x, y, z) {
            (3, 3, 3) => SLAB,
            (3, 4, 3) => STONE,
            _ => AIR,
        });
        assert_eq!(mesh.faces[F::NegY as usize].len(), 1);
        let beside = run(false, |x, y, z| match (x, y, z) {
            (3, 3, 3) => SLAB,
            (4, 3, 3) => STONE,
            _ => AIR,
        });
        assert_eq!(
            beside.model.len(),
            6 - 1,
            "the slab side behind stone is covered"
        );
        assert_eq!(
            beside.faces[F::NegX as usize].len(),
            1,
            "stone shows over the slab's open half"
        );
    }

    #[test]
    fn variants_follow_the_position_seed() {
        let mesh = run(false, |_, y, _| if y == 3 { RANDOM } else { AIR });
        let mut seen = [0u32; 4];
        for q in mesh.faces[F::PosY as usize].iter().map(|q| q.decode()) {
            let (x, z) = (i32::from(q.x), i32::from(q.z));
            let seed = crate::random::mth_get_seed(x, 3, z);
            let want = crate::random::JavaRandom::new(seed).next_int(4);
            assert_eq!(i32::from(q.rotation), want, "({x}, {z})");
            seen[q.rotation as usize] += 1;
        }
        assert!(seen.iter().all(|&n| n > 20), "{seen:?}");
        let again = run(false, |_, y, _| if y == 3 { RANDOM } else { AIR });
        assert_eq!(mesh.faces[F::PosY as usize], again.faces[F::PosY as usize]);
    }

    #[test]
    fn lone_water_source_has_vanilla_corner_heights() {
        let mesh = run(
            false,
            |x, y, z| if (x, y, z) == (3, 3, 3) { WATER } else { AIR },
        );
        let top = mesh
            .translucent
            .iter()
            .map(|q| q.decode())
            .find(|q| q.shade_face == F::PosY)
            .unwrap();
        let expect = 3.0 + (8.0 / 9.0 * 10.0) / 12.0;
        for c in top.corners {
            assert!((c[1] - expect).abs() < 1.0 / 128.0, "{} vs {expect}", c[1]);
        }
        let joined = run(false, |x, y, z| match (x, y, z) {
            (3, 3, 3) | (4, 3, 3) => WATER,
            _ => AIR,
        });
        let tops: Vec<_> = joined
            .translucent
            .iter()
            .map(|q| q.decode())
            .filter(|q| q.shade_face == F::PosY && q.cell[0] == 3)
            .collect();
        let shared = tops[0]
            .corners
            .iter()
            .filter(|c| c[0] == 4.0)
            .map(|c| c[1])
            .fold(f32::MIN, f32::max);
        let outer = tops[0]
            .corners
            .iter()
            .filter(|c| c[0] == 3.0)
            .map(|c| c[1])
            .fold(f32::MIN, f32::max);
        assert!(shared > outer, "joined edge is higher: {shared} vs {outer}");
    }
}
