//! Per-state render data, baked once from the resource packs.

use std::collections::HashMap;
use std::sync::Arc;

use mcr_assets::{
    BakedModel, BakedQuad, Identifier, ModelBaker, ResourceManager, SpriteId, TextureArrayBuilder,
    TextureArrays, Transparency,
};
use mcr_data::Version;
use mcr_render::{SpriteAnimation, SpriteFrame};

use crate::faces::{self, VANILLA_TO_RENDER};
use crate::java::{JavaModels, JavaState, JavaTint};
use crate::random::{JavaRandom, OFFSET_NONE, OffsetSpec, block_offset, mth_get_seed, offset_spec};
use crate::tint::{self, TINT_NONE, constant_palette, tint_kinds};

pub const LAYER_NONE: u16 = 0xffff;
pub const NO_CULL: u8 = 0xff;

pub const ATTR_SOLID_RENDER: u8 = 1;
pub const ATTR_FULL_COLLISION: u8 = 2;
/// Vanilla's `HalfTransparentBlock`s (glass, ice, slime, honey, copper grates): fluids use their overlay
/// sprite beside them and leaves.
pub const ATTR_HALF_TRANSPARENT: u8 = 4;
pub const ATTR_LEAVES: u8 = 8;
pub const ATTR_AIR: u8 = 16;
/// Solid-render blocks that dampen light (vanilla's `!isLightPermeable`).
pub const ATTR_LIGHT_OPAQUE: u8 = 32;
/// Rendered at full brightness regardless of light (magma block, active sculk sensors).
pub const ATTR_EMISSIVE: u8 = 64;
/// Vanilla's `isSolid` (fluid heights and flow treat these as walls).
pub const ATTR_SOLID: u8 = 128;

pub const META_ATTR_MASK: u32 = 0xff;
pub const META_FULL_SHIFT: u32 = 8;
pub const META_PARTIAL_SHIFT: u32 = 16;
pub const META_EMISSION_SHIFT: u32 = 24;
/// Vanilla's `getShadeBrightness` is 0.2 rather than 1.0: smooth lighting darkens corners next to it.
pub const META_SHADE_DARK: u32 = 1 << 28;

pub const MASK_EMPTY: u16 = 0;
pub const MASK_FULL: u16 = 1;

/// A 16x16 coverage grid of one occlusion face, bit `v * 16 + u` over the face's tangent axes.
pub type FaceMask = [u64; 4];

const FULL_MASK: FaceMask = [u64::MAX; 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderLayer {
    Solid,
    Cutout,
    Translucent,
}

impl RenderLayer {
    fn from_transparency(t: Transparency, forced: bool) -> Self {
        if forced {
            return Self::Translucent;
        }
        match t {
            Transparency::Opaque => Self::Solid,
            Transparency::Cutout => Self::Cutout,
            Transparency::Translucent => Self::Translucent,
        }
    }
}

/// Vanilla's per-block `skipRendering`: faces hidden by the neighbouring state alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkipRule {
    #[default]
    None,
    /// `HalfTransparentBlock` and powder snow: hidden against the same block.
    SameBlock,
    /// Mangrove roots: hidden against the same block above and below only.
    SameBlockVertical,
    /// Hidden against any leaves when cutout leaves are off.
    Leaves,
    /// Panes and bars: hidden against the same block (or two bars) vertically, and horizontally where
    /// both connect.
    Bars,
}

/// `StateModel::conn` bit of blocks in vanilla's `#bars` tag; bits 0 to 5 are connections per render face.
pub const CONN_BARS_TAG: u8 = 1 << 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Empty,
    Cube,
    Model,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CubeFace {
    pub layer: u16,
    pub tint: u8,
    pub overlay: u16,
    pub overlay_tint: u8,
    /// Orientation of the base texture relative to the default face mapping: quarter turns in the low
    /// two bits, bit 2 mirrors u first.
    pub rot: u8,
    pub overlay_rot: u8,
}

impl CubeFace {
    pub const NONE: Self = Self {
        layer: LAYER_NONE,
        tint: TINT_NONE,
        overlay: LAYER_NONE,
        overlay_tint: TINT_NONE,
        rot: 0,
        overlay_rot: 0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FluidKind {
    #[default]
    None,
    Water,
    Lava,
    /// A modded fluid: index into [`ModelCache::modded_fluids`].
    Other(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FluidInfo {
    pub kind: FluidKind,
    pub amount: u8,
    pub falling: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct StateModel {
    pub shape: Shape,
    /// Layer of a cube state's base faces.
    pub layer: RenderLayer,
    pub ao: bool,
    pub block: u16,
    pub quad_start: u32,
    pub quad_count: u32,
    pub cube: [CubeFace; 6],
    pub fluid: FluidInfo,
    /// One plus the index into the cache's random models; zero when every part has a single variant.
    pub random: u32,
    pub offset: OffsetSpec,
    pub emission: u8,
    /// Layer of the model's `particle` texture; `LAYER_NONE` when it has none.
    pub particle: u16,
    pub skip: SkipRule,
    /// Pane and bar connections by render face, plus [`CONN_BARS_TAG`].
    pub conn: u8,
    /// From this cell to the one whose position seeds the variant pick (the lower half of doors and
    /// tall plants, the head of beds).
    pub seed_offset: [i8; 3],
}

impl StateModel {
    pub const EMPTY: Self = Self {
        shape: Shape::Empty,
        layer: RenderLayer::Solid,
        ao: false,
        block: 0,
        quad_start: 0,
        quad_count: 0,
        cube: [CubeFace::NONE; 6],
        fluid: FluidInfo {
            kind: FluidKind::None,
            amount: 0,
            falling: false,
        },
        random: 0,
        offset: OffsetSpec::NONE,
        emission: 0,
        particle: LAYER_NONE,
        skip: SkipRule::None,
        conn: 0,
        seed_offset: [0; 3],
    };
}

/// One weighted alternative of a model part.
#[derive(Debug, Clone, Copy)]
pub struct Alt {
    pub weight: u32,
    pub cube: [CubeFace; 6],
    pub quad_start: u32,
    pub quad_count: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct PartSpan {
    pub alt_start: u32,
    pub alt_count: u32,
    pub total_weight: u32,
}

/// Variant selection data of a state with at least one weighted part.
#[derive(Debug, Clone, Copy)]
pub struct RandomModel {
    /// Multipart models reseed from `nextLong` for every part.
    pub multipart: bool,
    pub part_start: u32,
    pub part_count: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct QuadDef {
    pub pos: [[f32; 3]; 4],
    pub uv: [[f32; 2]; 4],
    pub layer: u16,
    pub tint: u8,
    pub cull: u8,
    pub normal: u8,
    pub shade: u8,
    pub translucent: bool,
    /// Flat on a block boundary plane along its normal.
    pub boundary: bool,
    /// Flat along its normal axis.
    pub flat: bool,
    /// The element's `light_emission`.
    pub emission: u8,
}

pub(crate) const MAX_PARTS: usize = 32;

/// The chosen alternative of every part of a random model.
pub(crate) struct Picks {
    pub alts: [u32; MAX_PARTS],
    pub count: usize,
}

/// Sprites, layer and tint of a modded fluid, from Java's fluid model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModdedFluid {
    pub still: u16,
    pub flow: u16,
    /// `LAYER_NONE` when the fluid has no overlay sprite.
    pub overlay: u16,
    pub translucent: bool,
    pub tint: u8,
}

/// Modded fluid families a cache distinguishes.
pub const MAX_MODDED_FLUIDS: usize = 255;
/// Tint kinds past vanilla's constants that modded constant colours take (the quad field holds 6 bits).
const MODDED_TINT_FIRST: u8 = tint::TINT_CONST_BASE + tint::PALETTE_LEN as u8;
const MODDED_TINT_LAST: u8 = 62;

#[derive(Debug, Clone, Copy, Default)]
pub struct FluidSprites {
    pub water_still: u16,
    pub water_flow: u16,
    pub water_overlay: u16,
    pub lava_still: u16,
    pub lava_flow: u16,
}

#[derive(Debug)]
pub struct ModelCache {
    pub(crate) states: Vec<StateModel>,
    pub(crate) quads: Vec<QuadDef>,
    pub(crate) attrs: Vec<u8>,
    pub(crate) occlusion: Vec<u8>,
    pub(crate) meta: Vec<u32>,
    pub(crate) masks: Vec<FaceMask>,
    pub(crate) face_masks: Vec<[u16; 6]>,
    pub(crate) randoms: Vec<RandomModel>,
    pub(crate) parts: Vec<PartSpan>,
    pub(crate) alts: Vec<Alt>,
    pub fluid_sprites: FluidSprites,
    pub modded_fluids: Vec<ModdedFluid>,
    /// Constant tint colours (`r | g << 8 | b << 16`) of modded fluids and Java-baked models, after
    /// vanilla's palette.
    pub modded_tints: Vec<u32>,
    pub textures: TextureArrays,
    /// Bit per packed texture layer whose sprite has no transparent texel in any frame.
    opaque_layers: Vec<u64>,
    pub missing_sprites: Vec<String>,
    pub unbaked_states: usize,
    /// The first few blocks with unbaked states.
    pub unbaked_blocks: Vec<String>,
    /// Every block with states neither Rust nor Java's report could bake, for asking Java.
    pub java_requests: Vec<String>,
    /// States meshed from Java's baked quads.
    pub java_states: usize,
}

fn opaque_layers(textures: &TextureArrays) -> Vec<u64> {
    let mut bits = vec![0u64; (1 << 16) / 64];
    for s in textures.slots() {
        if s.transparency == Transparency::Opaque && s.group < 4 && s.first_layer < 1 << 14 {
            let layer = (s.group << 14 | s.first_layer) as usize;
            bits[layer / 64] |= 1 << (layer % 64);
        }
    }
    bits
}

fn compute_meta(attr: u8, occ: u8, partial: u8, emission: u8, dark: bool) -> u32 {
    u32::from(attr)
        | u32::from(occ) << META_FULL_SHIFT
        | u32::from(partial) << META_PARTIAL_SHIFT
        | u32::from(emission.min(15)) << META_EMISSION_SHIFT
        | if dark { META_SHADE_DARK } else { 0 }
}

impl ModelCache {
    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    /// Approximate heap bytes: per-state tables, quads and the texture arrays.
    pub fn heap_bytes(&self) -> (usize, usize) {
        use std::mem::size_of_val;
        let tables = size_of_val(&self.states[..])
            + size_of_val(&self.quads[..])
            + size_of_val(&self.attrs[..])
            + size_of_val(&self.occlusion[..])
            + size_of_val(&self.meta[..])
            + size_of_val(&self.masks[..])
            + size_of_val(&self.face_masks[..])
            + size_of_val(&self.alts[..]);
        let textures = self
            .textures
            .groups
            .iter()
            .flat_map(|g| g.mips.iter())
            .map(Vec::len)
            .sum();
        (tables, textures)
    }

    pub fn state(&self, id: u32) -> &StateModel {
        self.states.get(id as usize).unwrap_or(&StateModel::EMPTY)
    }

    pub fn quad_count(&self) -> usize {
        self.quads.len()
    }

    pub fn quads(&self, state: &StateModel) -> &[QuadDef] {
        &self.quads[state.quad_start as usize..(state.quad_start + state.quad_count) as usize]
    }

    pub(crate) fn quads_range(&self, start: u32, count: u32) -> &[QuadDef] {
        &self.quads[start as usize..(start + count) as usize]
    }

    pub(crate) fn random_model(&self, state: &StateModel) -> Option<&RandomModel> {
        (state.random != 0).then(|| &self.randoms[state.random as usize - 1])
    }

    pub(crate) fn part_span(&self, index: u32) -> &PartSpan {
        &self.parts[index as usize]
    }

    pub(crate) fn alt(&self, index: u32) -> &Alt {
        &self.alts[index as usize]
    }

    /// The weighted alternative of every part of a random state at world position `pos`, seeded like
    /// vanilla's `BlockState#getSeed`.
    pub(crate) fn picks(&self, sm: &StateModel, pos: [i32; 3]) -> Picks {
        let w: [i32; 3] =
            std::array::from_fn(|a| pos[a].wrapping_add(i32::from(sm.seed_offset[a])));
        let model = self.random_model(sm).expect("random state");
        let mut rng = JavaRandom::new(mth_get_seed(w[0], w[1], w[2]));
        let mut picks = Picks {
            alts: [0; MAX_PARTS],
            count: 0,
        };
        let seed = model.multipart.then(|| rng.next_long());
        for p in 0..(model.part_count as usize).min(MAX_PARTS) {
            if let Some(s) = seed {
                rng.set_seed(s);
            }
            let span = self.part_span(model.part_start + p as u32);
            let alt = if span.alt_count > 1 {
                let mut roll = rng.next_int(span.total_weight as i32) as u32;
                let mut chosen = span.alt_start;
                for a in span.alt_start..span.alt_start + span.alt_count {
                    let weight = self.alt(a).weight;
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

    /// Block-local corners of every quad `state` draws at world position `pos`, with its variant and
    /// offset, for vanilla's crumbling overlay; empty for states without a model.
    pub fn model_quads(&self, state: u32, pos: [i32; 3], out: &mut Vec<[[f32; 3]; 4]>) {
        let sm = self.state(state);
        let offset = if sm.offset.kind == OFFSET_NONE {
            [0.0; 3]
        } else {
            block_offset(sm.offset, pos[0], pos[2])
        };
        let shifted = |q: [[f32; 3]; 4]| q.map(|c| std::array::from_fn(|a| c[a] + offset[a]));
        let mut emit = |cube: &[CubeFace; 6], quads: &[QuadDef]| {
            for (face, f) in cube.iter().enumerate() {
                if f.layer != LAYER_NONE {
                    let [ua, va] = faces::TANGENTS[faces::axis(face)];
                    let ccw = (ua + 1) % 3 == va;
                    let order = if ccw == faces::is_positive(face) {
                        [(0, 0), (1, 0), (1, 1), (0, 1)]
                    } else {
                        [(0, 0), (0, 1), (1, 1), (1, 0)]
                    };
                    out.push(shifted(
                        order.map(|(u, v)| faces::corner_position(face, [0, 0, 0], u, v)),
                    ));
                }
            }
            out.extend(quads.iter().map(|q| shifted(q.pos)));
        };
        match sm.shape {
            Shape::Empty => {}
            _ if sm.random != 0 => {
                let picks = self.picks(sm, pos);
                for &a in &picks.alts[..picks.count] {
                    let alt = self.alt(a);
                    emit(&alt.cube, self.quads_range(alt.quad_start, alt.quad_count));
                }
            }
            Shape::Cube => emit(&sm.cube, &[]),
            Shape::Model => emit(&[CubeFace::NONE; 6], self.quads(sm)),
        }
    }

    #[inline]
    pub fn attr(&self, id: u32) -> u8 {
        self.attrs.get(id as usize).copied().unwrap_or(0)
    }

    /// Bit `f` is set when the state fully covers its face `f` (render face order).
    #[inline]
    pub fn occlusion(&self, id: u32) -> u8 {
        self.occlusion.get(id as usize).copied().unwrap_or(0)
    }

    /// Attributes, full and partial occlusion bits and emission packed for one lookup per cell.
    #[inline]
    pub fn meta(&self, id: u32) -> u32 {
        self.meta.get(id as usize).copied().unwrap_or(0)
    }

    /// Occlusion mask id of face `face` of state `id`: `MASK_EMPTY`, `MASK_FULL` or a partial shape.
    #[inline]
    pub fn face_mask(&self, id: u32, face: usize) -> u16 {
        match self.face_masks.get(id as usize) {
            Some(m) => m[face],
            None => self.fallback_mask(id, face),
        }
    }

    fn fallback_mask(&self, id: u32, face: usize) -> u16 {
        if self.occlusion(id) >> face & 1 != 0 {
            MASK_FULL
        } else {
            MASK_EMPTY
        }
    }

    pub fn mask(&self, id: u16) -> &FaceMask {
        self.masks.get(id as usize).unwrap_or(&FULL_MASK)
    }

    /// Vanilla's `skipRendering` of `state` against neighbour state `nid` across render face `face`.
    #[inline]
    pub fn skips(&self, state: &StateModel, nid: u32, face: usize, cutout_leaves: bool) -> bool {
        match state.skip {
            SkipRule::None => false,
            SkipRule::SameBlock => self.state(nid).block == state.block,
            SkipRule::SameBlockVertical => face >> 1 == 1 && self.state(nid).block == state.block,
            SkipRule::Leaves => !cutout_leaves && self.state(nid).skip == SkipRule::Leaves,
            SkipRule::Bars => {
                let n = self.state(nid);
                let paired = n.skip == SkipRule::Bars
                    && (n.block == state.block || state.conn & n.conn & CONN_BARS_TAG != 0);
                paired && (face >> 1 == 1 || state.conn >> face & n.conn >> (face ^ 1) & 1 != 0)
            }
        }
    }

    /// Vanilla `Block.shouldRenderFace` on occlusion shapes, without the same-block skip rule.
    pub fn face_visible(&self, state: u32, neighbor: u32, face: usize) -> bool {
        let occluder = self.face_mask(neighbor, face ^ 1);
        if occluder == MASK_FULL {
            return false;
        }
        if occluder == MASK_EMPTY {
            return true;
        }
        let shape = self.face_mask(state, face);
        if shape == MASK_EMPTY {
            return true;
        }
        let (a, b) = (self.mask(shape), self.mask(occluder));
        a.iter().zip(b).any(|(a, b)| a & !b != 0)
    }

    /// The block texture groups in the renderer's upload format, with their animated sprites.
    pub fn texture_groups(&self) -> Vec<mcr_render::TextureGroupData> {
        let animations = self.animations();
        self.textures
            .groups
            .iter()
            .enumerate()
            .map(|(i, g)| mcr_render::TextureGroupData {
                width: g.width,
                height: g.height,
                layers: g.layers,
                mips: g.mips.clone(),
                animations: animations
                    .iter()
                    .filter(|a| usize::from(a.layer >> 14) == i)
                    .cloned()
                    .collect(),
            })
            .collect()
    }

    /// Every animated sprite with its packed first layer.
    pub fn animations(&self) -> Vec<SpriteAnimation> {
        let mut out: Vec<SpriteAnimation> = self
            .textures
            .animated()
            .filter_map(|(id, anim)| {
                let slot = self.textures.slot(id)?;
                (slot.group < 4 && slot.first_layer < 1 << 14).then(|| SpriteAnimation {
                    layer: (slot.group << 14 | slot.first_layer) as u16,
                    frames: anim
                        .frames
                        .iter()
                        .map(|f| SpriteFrame {
                            layer_offset: f.layer_offset as u16,
                            ticks: f.ticks,
                        })
                        .collect(),
                    interpolate: anim.interpolate,
                })
            })
            .collect();
        out.sort_by_key(|a| a.layer);
        out
    }

    /// Vanilla's constant tints, then the modded ones.
    pub fn palette(&self) -> Vec<u32> {
        let mut p = constant_palette().to_vec();
        p.extend_from_slice(&self.modded_tints);
        p
    }

    pub fn from_parts(
        states: Vec<StateModel>,
        quads: Vec<QuadDef>,
        attrs: Vec<u8>,
        occlusion: Vec<u8>,
    ) -> Self {
        let meta = (0..states.len())
            .map(|i| {
                let dark =
                    attrs[i] & ATTR_FULL_COLLISION != 0 && attrs[i] & ATTR_HALF_TRANSPARENT == 0;
                compute_meta(attrs[i], occlusion[i], 0, states[i].emission, dark)
            })
            .collect();
        Self {
            states,
            quads,
            attrs,
            occlusion,
            meta,
            masks: vec![[0; 4], FULL_MASK],
            face_masks: Vec::new(),
            randoms: Vec::new(),
            parts: Vec::new(),
            alts: Vec::new(),
            fluid_sprites: FluidSprites::default(),
            modded_fluids: Vec::new(),
            modded_tints: Vec::new(),
            textures: TextureArrays::default(),
            opaque_layers: Vec::new(),
            missing_sprites: Vec::new(),
            unbaked_states: 0,
            unbaked_blocks: Vec::new(),
            java_requests: Vec::new(),
            java_states: 0,
        }
    }

    /// Whether a quad on this packed texture layer can skip the alpha test without changing a pixel.
    pub fn layer_opaque(&self, layer: u16) -> bool {
        self.opaque_layers
            .get(usize::from(layer) / 64)
            .is_some_and(|w| w >> (layer % 64) & 1 != 0)
    }

    /// Marks a hand-built cache's texture layer opaque.
    pub fn with_opaque_layer(mut self, layer: u16) -> Self {
        let word = usize::from(layer) / 64;
        if self.opaque_layers.len() <= word {
            self.opaque_layers.resize(word + 1, 0);
        }
        self.opaque_layers[word] |= 1 << (layer % 64);
        self
    }

    /// Overrides the shade brightness of a hand-built cache's state: `dark` is vanilla's 0.2.
    pub fn with_shade(mut self, id: u32, dark: bool) -> Self {
        if let Some(m) = self.meta.get_mut(id as usize) {
            *m = *m & !META_SHADE_DARK | if dark { META_SHADE_DARK } else { 0 };
        }
        self
    }

    /// Adds partial occlusion masks to a hand-built cache: `shapes[state]` gives the mask of each face.
    pub fn with_face_masks(mut self, masks: Vec<FaceMask>, per_state: Vec<[u16; 6]>) -> Self {
        self.masks.extend(masks);
        for (i, m) in per_state.iter().enumerate() {
            let partial = (0..6).fold(0u8, |acc, f| {
                acc | u8::from(m[f] != MASK_EMPTY && m[f] != MASK_FULL) << f
            });
            self.meta[i] |= u32::from(partial) << META_PARTIAL_SHIFT;
        }
        self.face_masks = per_state;
        self
    }

    /// Bakes every block state of `version` against `resources`. Absent textures use the missing
    /// sprite; states whose model cannot be baked render nothing.
    pub fn build(
        resources: &ResourceManager,
        version: &Version,
    ) -> Result<Self, mcr_assets::AssetError> {
        Self::build_with(resources, version, &JavaModels::default())
    }

    /// [`Self::build`], meshing states Rust cannot bake from Java's quads and modded fluids from
    /// Java's fluid models where `java` has them.
    pub fn build_with(
        resources: &ResourceManager,
        version: &Version,
        java: &JavaModels,
    ) -> Result<Self, mcr_assets::AssetError> {
        let count = version.state_count();
        let mut baker = ModelBaker::new(resources);
        let mut baked = Vec::with_capacity(count);
        let mut multipart_of: HashMap<u16, bool> = HashMap::new();
        let mut unbaked = 0;
        let mut unbaked_blocks: Vec<String> = Vec::new();
        let mut java_requests: Vec<String> = Vec::new();
        let mut java_tints: HashMap<u32, [JavaTint; 2]> = HashMap::new();
        for id in 0..count as u32 {
            let name = version.state_block_name(id).unwrap_or("minecraft:air");
            let props: Vec<(&str, &str)> = version.state_properties(id).collect();
            let rust = baker.bake_block_state(name, &props);
            if rust.is_err()
                && let Some(state) = java.state(name, &props)
            {
                java_tints.insert(id, state.tints);
                baked.push(Some(java_model(&mut baker, state)));
                continue;
            }
            match rust {
                Ok(model) => {
                    if let Some(info) = version.block_state(id)
                        && let std::collections::hash_map::Entry::Vacant(e) =
                            multipart_of.entry(info.block as u16)
                    {
                        let multipart = baker
                            .blockstate(name)
                            .map(|d| !d.multipart.is_empty())
                            .unwrap_or(false);
                        e.insert(multipart);
                    }
                    baked.push(Some(model));
                }
                Err(err) => {
                    tracing::debug!(%name, %err, "state has no model");
                    unbaked += 1;
                    if unbaked_blocks.len() < 16 && unbaked_blocks.last().is_none_or(|b| b != name)
                    {
                        unbaked_blocks.push(name.to_owned());
                    }
                    if !java.has_block(name)
                        && java_requests.len() < MAX_JAVA_REQUESTS
                        && java_requests.last().is_none_or(|b| b != name)
                    {
                        java_requests.push(name.to_owned());
                    }
                    baked.push(None);
                }
            }
        }

        let mut modded_fluid_ids = Vec::new();
        let mut families: Vec<&str> = Vec::new();
        let mut java_fluids: Vec<_> = java.fluids.iter().collect();
        java_fluids.sort_unstable_by(|a, b| a.0.cmp(b.0));
        for (fluid, info) in java_fluids {
            if is_vanilla_fluid(fluid) || families.len() >= MAX_MODDED_FLUIDS {
                continue;
            }
            if !families.contains(&info.family.as_str()) {
                families.push(info.family.as_str());
                modded_fluid_ids.push(info);
            }
        }
        let mut sprites = baker.into_sprites();
        let modded_fluid_sprites: Vec<[SpriteId; 3]> = modded_fluid_ids
            .iter()
            .map(|f| {
                let overlay = f.overlay.as_deref().unwrap_or("missingno");
                [&f.still, &f.flow, &Arc::from(overlay)]
                    .map(|n| sprites.intern(&Identifier::parse(n)))
            })
            .collect();
        let fluid_ids = [
            "block/water_still",
            "block/water_flow",
            "block/water_overlay",
            "block/lava_still",
            "block/lava_flow",
        ]
        .map(|n| sprites.intern(&Identifier::parse(n)));
        let missing_id = sprites.intern(&Identifier::parse("missingno"));
        let mut builder = TextureArrayBuilder::default();
        let missing = builder.add_from_resources(resources, &sprites)?;
        let textures = builder.build();
        let missing_sprites = missing
            .iter()
            .map(|id| sprites.get(*id).to_string())
            .collect::<Vec<_>>();

        let layer_of = |id: SpriteId| -> u16 {
            let pack = |id: SpriteId| {
                textures
                    .slot(id)
                    .filter(|s| s.group < 4 && s.first_layer < 1 << 14)
                    .map(|s| (s.group << 14 | s.first_layer) as u16)
            };
            pack(id).or_else(|| pack(missing_id)).unwrap_or(0)
        };
        let transparency_of = |id: SpriteId| {
            textures
                .slot(id)
                .map_or(Transparency::Opaque, |s| s.transparency)
        };
        let fluid_sprites = FluidSprites {
            water_still: layer_of(fluid_ids[0]),
            water_flow: layer_of(fluid_ids[1]),
            water_overlay: layer_of(fluid_ids[2]),
            lava_still: layer_of(fluid_ids[3]),
            lava_flow: layer_of(fluid_ids[4]),
        };

        let mut cache = Self::from_parts(Vec::new(), Vec::new(), Vec::new(), Vec::new());
        cache.masks = vec![[0; 4], FULL_MASK];
        for (info, ids) in modded_fluid_ids.iter().zip(&modded_fluid_sprites) {
            let tint = cache.java_tint_kind(info.tint);
            cache.modded_fluids.push(ModdedFluid {
                still: layer_of(ids[0]),
                flow: layer_of(ids[1]),
                overlay: if info.overlay.is_some() {
                    layer_of(ids[2])
                } else {
                    LAYER_NONE
                },
                translucent: info.translucent,
                tint,
            });
        }
        let mut mask_ids: HashMap<FaceMask, u16> = HashMap::new();
        let mut runs = QuadRuns::default();
        let mut run: Vec<QuadDef> = Vec::new();
        mask_ids.insert([0; 4], MASK_EMPTY);
        mask_ids.insert(FULL_MASK, MASK_FULL);

        for (id, model) in baked.iter().enumerate() {
            let id = id as u32;
            let info = version.block_state(id);
            let name = version.state_block_name(id).unwrap_or("minecraft:air");
            let short = name.strip_prefix("minecraft:").unwrap_or(name);
            let mut attr = 0;
            let mut occ = 0;
            let mut fluid = FluidInfo::default();
            let mut block = 0;
            let mut emission = 0;
            let mut face_ids = [MASK_EMPTY; 6];
            let mut full_collision = false;
            if let Some(info) = info {
                block = info.block as u16;
                emission = info.light_emission;
                if info.is_solid_render() {
                    attr |= ATTR_SOLID_RENDER;
                    if info.light_block > 0 {
                        attr |= ATTR_LIGHT_OPAQUE;
                    }
                }
                if info.is_full_collision() {
                    attr |= ATTR_FULL_COLLISION;
                    full_collision = true;
                }
                if is_legacy_solid(version.collision_shape(id)) {
                    attr |= ATTR_SOLID;
                }
                if info.is_air() {
                    attr |= ATTR_AIR;
                }
                for (v, render) in VANILLA_TO_RENDER.iter().enumerate() {
                    if info.is_face_fully_occluding(v) {
                        occ |= 1 << render;
                    }
                }
                if info.can_occlude() && occ != 0b11_1111 {
                    let boxes = version.occlusion_shape(id);
                    for (face, slot) in face_ids.iter_mut().enumerate() {
                        let mask = face_mask_of(boxes, face);
                        *slot = *mask_ids.entry(mask).or_insert_with(|| {
                            cache.masks.push(mask);
                            (cache.masks.len() - 1) as u16
                        });
                    }
                }
                for (face, slot) in face_ids.iter_mut().enumerate() {
                    if occ >> face & 1 != 0 {
                        *slot = MASK_FULL;
                    }
                }
                if let Some(f) = info.fluid {
                    let fname = version.fluid_name(f.fluid).unwrap_or("");
                    let modded = java
                        .fluids
                        .get(fname)
                        .filter(|_| !is_vanilla_fluid(fname))
                        .and_then(|j| families.iter().position(|&fam| fam == j.family));
                    let kind = if let Some(i) = modded {
                        FluidKind::Other(i as u8)
                    } else if fname.contains("water") {
                        FluidKind::Water
                    } else if fname.contains("lava") {
                        FluidKind::Lava
                    } else {
                        FluidKind::None
                    };
                    fluid = FluidInfo {
                        kind,
                        amount: f.amount,
                        falling: f.falling,
                    };
                }
            }
            let props: Vec<(&str, &str)> = version.state_properties(id).collect();
            let class = BlockClass::of(short, &props);
            if class.half_transparent {
                attr |= ATTR_HALF_TRANSPARENT;
            }
            if class.skip == SkipRule::Leaves {
                attr |= ATTR_LEAVES;
            }
            if class.emissive {
                attr |= ATTR_EMISSIVE;
            }
            let dark = class.shade_dark.unwrap_or(full_collision);
            let partial = (0..6).fold(0u8, |acc, f| {
                acc | u8::from(face_ids[f] != MASK_EMPTY && face_ids[f] != MASK_FULL) << f
            });
            cache.attrs.push(attr);
            cache.occlusion.push(occ);
            cache
                .meta
                .push(compute_meta(attr, occ, partial, emission, dark));
            cache.face_masks.push(face_ids);

            let mut sm = StateModel::EMPTY;
            sm.skip = class.skip;
            sm.conn = class.conn;
            sm.seed_offset = class.seed_offset;
            sm.block = block;
            sm.fluid = fluid;
            sm.emission = emission;
            sm.offset = offset_spec(name);
            if let Some(model) = model {
                let kinds = match java_tints.get(&id) {
                    Some(t) => [cache.java_tint_kind(t[0]), cache.java_tint_kind(t[1])],
                    None => tint_kinds(name, &props),
                };
                let tint_of = |q: &BakedQuad| match q.tint {
                    0 => kinds[0],
                    1 => kinds[1],
                    _ => TINT_NONE,
                };
                let quad_layer = |q: &BakedQuad| {
                    RenderLayer::from_transparency(transparency_of(q.sprite), q.force_translucent)
                };
                let first: Vec<_> = model.parts.iter().map(|p| &p.variants[0]).collect();
                sm.particle = first
                    .first()
                    .and_then(|v| v.particle)
                    .map_or(LAYER_NONE, layer_of);
                let ao = first.iter().all(|v| v.ambient_occlusion)
                    && info.is_none_or(|i| i.light_emission == 0);
                let random = model.parts.iter().any(|p| p.variants.len() > 1);
                let multipart = multipart_of.get(&block).copied().unwrap_or(false);
                sm.ao = ao;

                let cube_of = |list: &[&BakedQuad]| -> Option<([CubeFace; 6], RenderLayer)> {
                    let cube = as_cube(list)?;
                    let mut faces = [CubeFace::NONE; 6];
                    let mut layer = RenderLayer::Solid;
                    for (face, slot) in cube.iter().enumerate() {
                        if let Some((base, rot)) = slot.0 {
                            faces[face] = CubeFace {
                                layer: layer_of(base.sprite),
                                tint: tint_of(base),
                                overlay: slot.1.map_or(LAYER_NONE, |(q, _)| layer_of(q.sprite)),
                                overlay_tint: slot.1.map_or(TINT_NONE, |(q, _)| tint_of(q)),
                                rot,
                                overlay_rot: slot.1.map_or(0, |(_, r)| r),
                            };
                            layer = layer.max(quad_layer(base));
                        }
                    }
                    let overlay_translucent = cube.iter().any(|s| {
                        s.1.is_some_and(|(q, _)| quad_layer(q) == RenderLayer::Translucent)
                    });
                    let layered_translucent =
                        layer == RenderLayer::Translucent && cube.iter().any(|s| s.1.is_some());
                    (!(overlay_translucent || layered_translucent)).then_some((faces, layer))
                };

                let lists: Vec<Vec<Vec<&BakedQuad>>> = model
                    .parts
                    .iter()
                    .map(|p| {
                        p.variants
                            .iter()
                            .map(|v| v.quads.iter().collect::<Vec<_>>())
                            .collect()
                    })
                    .collect();
                if lists.iter().all(|p| p.iter().all(|v| v.is_empty())) {
                    cache.states.push(sm);
                    continue;
                }

                let as_cubes: Option<Vec<([CubeFace; 6], RenderLayer)>> = (model.parts.len() == 1)
                    .then(|| lists[0].iter().map(|l| cube_of(l)).collect())
                    .flatten();
                if let Some(alts) = &as_cubes {
                    sm.shape = Shape::Cube;
                    sm.cube = alts[0].0;
                    sm.layer = alts.iter().map(|a| a.1).max().unwrap_or(RenderLayer::Solid);
                } else {
                    sm.shape = Shape::Model;
                }

                if random {
                    let part_start = cache.parts.len() as u32;
                    for (p, part) in model.parts.iter().enumerate() {
                        let alt_start = cache.alts.len() as u32;
                        for (v, variant) in part.variants.iter().enumerate() {
                            let (cube, quad_start, quad_count) = match &as_cubes {
                                Some(a) => (a[v].0, 0, 0),
                                None => {
                                    run.clear();
                                    run.extend(lists[p][v].iter().map(|q| {
                                        compile_quad(
                                            q,
                                            layer_of(q.sprite),
                                            tint_of(q),
                                            quad_layer(q),
                                        )
                                    }));
                                    let start = runs.push(&mut cache.quads, &run);
                                    ([CubeFace::NONE; 6], start, run.len() as u32)
                                }
                            };
                            cache.alts.push(Alt {
                                weight: variant.weight.max(1),
                                cube,
                                quad_start,
                                quad_count,
                            });
                        }
                        cache.parts.push(PartSpan {
                            alt_start,
                            alt_count: part.variants.len() as u32,
                            total_weight: part.variants.iter().map(|v| v.weight.max(1)).sum(),
                        });
                    }
                    cache.randoms.push(RandomModel {
                        multipart,
                        part_start,
                        part_count: model.parts.len() as u32,
                    });
                    sm.random = cache.randoms.len() as u32;
                } else if sm.shape == Shape::Model {
                    run.clear();
                    for part in &lists {
                        run.extend(part[0].iter().map(|q| {
                            compile_quad(q, layer_of(q.sprite), tint_of(q), quad_layer(q))
                        }));
                    }
                    sm.quad_start = runs.push(&mut cache.quads, &run);
                    sm.quad_count = run.len() as u32;
                }
            }
            cache.states.push(sm);
        }

        cache.fluid_sprites = fluid_sprites;
        cache.opaque_layers = opaque_layers(&textures);
        cache.textures = textures;
        cache.missing_sprites = missing_sprites;
        cache.unbaked_states = unbaked;
        cache.unbaked_blocks = unbaked_blocks;
        cache.java_requests = java_requests;
        cache.java_states = java_tints.len();
        Ok(cache)
    }

    /// This cache, baked for `from`, re-indexed for `to`: the same blocks with the same states in
    /// another id order (a loader's registry sync renumbers modded blocks); `None` when `to` differs in
    /// anything the bake read.
    pub fn remap(mut self, from: &Version, to: &Version) -> Option<Self> {
        let n = to.state_count();
        if from.state_count() != n || self.states.len() != n || self.face_masks.len() != n {
            return None;
        }
        let mut old_of = Vec::with_capacity(n);
        let mut block_of = Vec::with_capacity(n);
        for block in 0..to.block_count() as u32 {
            let (name, new) = (to.block_name(block)?, to.block_info(block)?);
            let old = from.block_by_name(name).and_then(|b| from.block_info(b))?;
            if new.states.1 - new.states.0 != old.states.1 - old.states.0
                || new.states.0 as usize != old_of.len()
            {
                return None;
            }
            for i in 0..new.states.1 - new.states.0 {
                let (o, t) = (old.states.0 + i, new.states.0 + i);
                if !same_render_inputs(from, o, to, t) {
                    return None;
                }
                old_of.push(o as usize);
                block_of.push(block as u16);
            }
        }
        if old_of.len() != n {
            return None;
        }
        fn pick<T: Copy>(v: &[T], old_of: &[usize]) -> Vec<T> {
            old_of.iter().map(|&o| v[o]).collect()
        }
        let mut states = pick(&self.states, &old_of);
        for (s, &b) in states.iter_mut().zip(&block_of) {
            s.block = b;
        }
        self.states = states;
        self.attrs = pick(&self.attrs, &old_of);
        self.occlusion = pick(&self.occlusion, &old_of);
        self.meta = pick(&self.meta, &old_of);
        self.face_masks = pick(&self.face_masks, &old_of);
        Some(self)
    }

    /// The tint kind of a tint Java reported; constant colours take modded palette slots, the nearest
    /// existing slot once they run out.
    fn java_tint_kind(&mut self, tint: JavaTint) -> u8 {
        match tint {
            JavaTint::None => TINT_NONE,
            JavaTint::Grass => tint::TINT_GRASS,
            JavaTint::Foliage => tint::TINT_FOLIAGE,
            JavaTint::DryFoliage => tint::TINT_DRY_FOLIAGE,
            JavaTint::Water => tint::TINT_WATER,
            JavaTint::Rgb(rgb) => {
                let packed = (rgb >> 16 & 255) | (rgb & 0xff00) | (rgb & 255) << 16;
                if let Some(i) = self.modded_tints.iter().position(|&c| c == packed) {
                    return MODDED_TINT_FIRST + i as u8;
                }
                if usize::from(MODDED_TINT_FIRST) + self.modded_tints.len()
                    <= usize::from(MODDED_TINT_LAST)
                {
                    self.modded_tints.push(packed);
                    return MODDED_TINT_FIRST + (self.modded_tints.len() - 1) as u8;
                }
                let distance = |c: u32| {
                    (0..3)
                        .map(|k| {
                            (i32::from((c >> (8 * k)) as u8) - i32::from((packed >> (8 * k)) as u8))
                                .abs()
                        })
                        .sum::<i32>()
                };
                self.modded_tints
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, c)| distance(**c))
                    .map_or(TINT_NONE, |(i, _)| MODDED_TINT_FIRST + i as u8)
            }
        }
    }
}

/// Whether state `a` of `from` and state `b` of `to` bake alike: same properties, flags, light,
/// fluid and shapes.
fn same_render_inputs(from: &Version, a: u32, to: &Version, b: u32) -> bool {
    let (Some(mut x), Some(y)) = (from.block_state(a), to.block_state(b)) else {
        return false;
    };
    x.block = y.block;
    let fluid = |v: &Version, info: &mcr_data::BlockStateInfo| {
        info.fluid.map(|f| v.fluid_name(f.fluid).map(str::to_owned))
    };
    x == y
        && fluid(from, &x) == fluid(to, &y)
        && from.state_properties(a).eq(to.state_properties(b))
        && from.collision_shape(a) == to.collision_shape(b)
        && from.occlusion_shape(a) == to.occlusion_shape(b)
}

/// Blocks with unbaked states a cache asks Java about at most.
const MAX_JAVA_REQUESTS: usize = 4096;

fn is_vanilla_fluid(name: &str) -> bool {
    matches!(
        name,
        "minecraft:water"
            | "minecraft:flowing_water"
            | "minecraft:lava"
            | "minecraft:flowing_lava"
            | "minecraft:empty"
    )
}

/// One part, one variant: Java already picked the weighted variants.
fn java_model(baker: &mut ModelBaker, state: &JavaState) -> BakedModel {
    let sprites = baker.sprites_mut();
    let quads: Vec<BakedQuad> = state
        .quads
        .iter()
        .map(|q| BakedQuad {
            vertices: std::array::from_fn(|v| mcr_assets::Vertex {
                pos: q.pos[v],
                uv: q.uv[v],
            }),
            sprite: sprites.intern(&Identifier::parse(&q.sprite)),
            normal: q.normal,
            cull: q.cull,
            tint: q.tint,
            shade_dir: q.shade,
            light_emission: q.emission,
            force_translucent: q.translucent,
        })
        .collect();
    let particle = state
        .particle
        .as_deref()
        .map(|p| sprites.intern(&Identifier::parse(p)));
    BakedModel {
        parts: vec![mcr_assets::ModelPart {
            variants: vec![mcr_assets::BakedVariant {
                weight: 1,
                ambient_occlusion: state.ao,
                particle,
                quads: quads.into(),
            }],
            total_weight: 1,
        }],
    }
}

fn is_legacy_solid(boxes: &[mcr_data::Aabb]) -> bool {
    if boxes.is_empty() {
        return false;
    }
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for b in boxes {
        for a in 0..3 {
            lo[a] = lo[a].min(b.min[a]);
            hi[a] = hi[a].max(b.max[a]);
        }
    }
    let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    (size[0] + size[1] + size[2]) / 3.0 >= 0.729_166_7 || size[1] >= 1.0
}

/// Coverage of one occlusion face by `boxes` (vanilla `VoxelShape.getFaceShape`) on a 16x16 grid.
fn face_mask_of(boxes: &[mcr_data::Aabb], face: usize) -> FaceMask {
    let axis = faces::axis(face);
    let [ua, va] = faces::TANGENTS[axis];
    let positive = faces::is_positive(face);
    let eps = 1e-4;
    let mut occupied = [0u64; 4];
    let mut full = [0u64; 4];
    let mut touching = [0u64; 4];
    for v in 0..16usize {
        for u in 0..16usize {
            let (cu, cv) = ((u as f32 + 0.5) / 16.0, (v as f32 + 0.5) / 16.0);
            let mut spans: Vec<(f32, f32)> = Vec::new();
            let mut touches = false;
            for b in boxes {
                if b.min[ua] <= cu && cu < b.max[ua] && b.min[va] <= cv && cv < b.max[va] {
                    spans.push((b.min[axis], b.max[axis]));
                    touches |= if positive {
                        b.max[axis] > 1.0 - eps
                    } else {
                        b.min[axis] < eps
                    };
                }
            }
            if spans.is_empty() {
                continue;
            }
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut reach = 0.0f32;
            for (lo, hi) in &spans {
                if *lo <= reach + eps {
                    reach = reach.max(*hi);
                }
            }
            let bit = 1u64 << ((v % 4) * 16 + u);
            occupied[v / 4] |= bit;
            if spans[0].0 <= eps && reach >= 1.0 - eps {
                full[v / 4] |= bit;
            }
            if touches {
                touching[v / 4] |= bit;
            }
        }
    }
    if occupied == full { occupied } else { touching }
}

type FaceQuads<'a> = (Option<(&'a BakedQuad, u8)>, Option<(&'a BakedQuad, u8)>);

/// Per render face: the base quad and an optional overlay quad with their texture rotations, when every
/// quad is a standard (possibly quarter-turned) full face.
fn as_cube<'a>(quads: &[&'a BakedQuad]) -> Option<[FaceQuads<'a>; 6]> {
    let mut out: [FaceQuads; 6] = [(None, None); 6];
    for q in quads {
        let cull = q.cull?;
        let face = faces::render_face_from_vanilla(cull);
        if faces::render_face_from_vanilla(q.normal) != face {
            return None;
        }
        let rot = face_rotation(q, face)?;
        let slot = &mut out[face];
        if slot.0.is_none() {
            slot.0 = Some((*q, rot));
        } else if slot.1.is_none() {
            slot.1 = Some((*q, rot));
        } else {
            return None;
        }
    }
    Some(out)
}

/// The default UV `(su, sv)` of a face corner under orientation `k` (bit 2 mirrors u, then quarter turns).
fn orient(k: u8, su: f32, sv: f32) -> [f32; 2] {
    let (mu, mv) = if k & 4 != 0 { (1.0 - su, sv) } else { (su, sv) };
    match k & 3 {
        0 => [mu, mv],
        1 => [1.0 - mv, mu],
        2 => [1.0 - mu, 1.0 - mv],
        _ => [mv, 1.0 - mu],
    }
}

/// Orientation of the texture on a full-face quad, or `None` when it is not a standard face.
fn face_rotation(q: &BakedQuad, face: usize) -> Option<u8> {
    let ax = faces::axis(face);
    let [ua, va] = faces::TANGENTS[ax];
    let plane = if faces::is_positive(face) { 1.0 } else { 0.0 };
    let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
    let mut seen = 0u8;
    let mut candidates = [true; 8];
    for v in &q.vertices {
        let (cu, cv) = (v.pos[ua], v.pos[va]);
        if !near(v.pos[ax], plane)
            || !(near(cu, 0.0) || near(cu, 1.0))
            || !(near(cv, 0.0) || near(cv, 1.0))
        {
            return None;
        }
        let [su, sv] = faces::corner_uv(face, cu, cv);
        for (k, c) in candidates.iter_mut().enumerate() {
            let turned = orient(k as u8, su, sv);
            *c &= near(v.uv[0], turned[0]) && near(v.uv[1], turned[1]);
        }
        seen |= 1 << (usize::from(cu > 0.5) + 2 * usize::from(cv > 0.5));
    }
    if seen != 0b1111 {
        return None;
    }
    candidates.iter().position(|&c| c).map(|k| k as u8)
}

fn quad_bits(q: &QuadDef) -> [u32; 22] {
    let mut bits = [0; 22];
    for (i, v) in q
        .pos
        .iter()
        .flatten()
        .chain(q.uv.iter().flatten())
        .enumerate()
    {
        bits[i] = v.to_bits();
    }
    bits[20] = u32::from(q.layer) << 16 | u32::from(q.tint) << 8 | u32::from(q.cull);
    bits[21] = u32::from(q.normal) << 16
        | u32::from(q.shade) << 8
        | u32::from(q.emission) << 3
        | u32::from(q.translucent) << 2
        | u32::from(q.boundary) << 1
        | u32::from(q.flat);
    bits
}

/// Shares one copy of identical compiled quad runs between states and variants.
#[derive(Default)]
struct QuadRuns {
    starts: HashMap<u64, Vec<u32>>,
}

impl QuadRuns {
    fn push(&mut self, quads: &mut Vec<QuadDef>, run: &[QuadDef]) -> u32 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        run.len().hash(&mut h);
        for q in run {
            quad_bits(q).hash(&mut h);
        }
        let starts = self.starts.entry(h.finish()).or_default();
        let same = |&start: &u32| {
            let s = start as usize;
            quads.get(s..s + run.len()).is_some_and(|stored| {
                stored
                    .iter()
                    .zip(run)
                    .all(|(a, b)| quad_bits(a) == quad_bits(b))
            })
        };
        if let Some(&start) = starts.iter().find(|s| same(s)) {
            return start;
        }
        let start = quads.len() as u32;
        quads.extend_from_slice(run);
        starts.push(start);
        start
    }
}

fn compile_quad(q: &BakedQuad, layer: u16, tint: u8, rl: RenderLayer) -> QuadDef {
    let normal = faces::render_face_from_vanilla(q.normal);
    let ax = faces::axis(normal);
    let coord = q.vertices[0].pos[ax];
    let flat = q.vertices.iter().all(|v| (v.pos[ax] - coord).abs() < 1e-5);
    let boundary = flat
        && if faces::is_positive(normal) {
            coord > 1.0 - 1e-4
        } else {
            coord < 1e-4
        };
    QuadDef {
        pos: std::array::from_fn(|i| q.vertices[i].pos),
        uv: std::array::from_fn(|i| q.vertices[i].uv),
        layer,
        tint,
        cull: q
            .cull
            .map_or(NO_CULL, |c| faces::render_face_from_vanilla(c) as u8),
        normal: normal as u8,
        shade: faces::render_face_from_vanilla(q.shade_dir) as u8,
        translucent: rl == RenderLayer::Translucent,
        boundary,
        flat,
        emission: q.light_emission.min(15),
    }
}

/// What a block's vanilla class changes about rendering, read from its name and properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct BlockClass {
    half_transparent: bool,
    skip: SkipRule,
    conn: u8,
    emissive: bool,
    /// `Some` where the block overrides `getShadeBrightness`; otherwise full collision decides.
    shade_dark: Option<bool>,
    seed_offset: [i8; 3],
}

impl BlockClass {
    fn of(short: &str, props: &[(&str, &str)]) -> Self {
        let prop = |key: &str| props.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
        let mut c = Self::default();
        let transparent = short.ends_with("glass") || short.ends_with("copper_grate");
        c.half_transparent =
            transparent || matches!(short, "ice" | "frosted_ice" | "slime_block" | "honey_block");
        let bars_tag = short == "iron_bars" || short.ends_with("copper_bars");
        if c.half_transparent || short == "powder_snow" {
            c.skip = SkipRule::SameBlock;
        } else if short.ends_with("_leaves") {
            c.skip = SkipRule::Leaves;
        } else if short == "mangrove_roots" {
            c.skip = SkipRule::SameBlockVertical;
        } else if bars_tag || short.ends_with("glass_pane") {
            c.skip = SkipRule::Bars;
            for (face, key) in [(0, "east"), (1, "west"), (4, "south"), (5, "north")] {
                if prop(key) == Some("true") {
                    c.conn |= 1 << face;
                }
            }
            if bars_tag {
                c.conn |= CONN_BARS_TAG;
            }
        }
        c.emissive = short == "magma_block"
            || (matches!(short, "sculk_sensor" | "calibrated_sculk_sensor")
                && prop("sculk_sensor_phase") == Some("active"));
        c.shade_dark = if transparent || matches!(short, "barrier" | "light" | "structure_void") {
            Some(false)
        } else if matches!(short, "soul_sand" | "mud") {
            Some(true)
        } else if short == "snow" {
            Some(prop("layers") == Some("8"))
        } else {
            None
        };
        if prop("half") == Some("upper") {
            c.seed_offset = [0, -1, 0];
        } else if prop("part") == Some("foot") {
            c.seed_offset = match prop("facing") {
                Some("north") => [0, 0, -1],
                Some("south") => [0, 0, 1],
                Some("west") => [-1, 0, 0],
                Some("east") => [1, 0, 0],
                _ => [0; 3],
            };
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use mcr_data::Aabb;

    use super::*;

    fn bx(min: [f32; 3], max: [f32; 3]) -> Aabb {
        Aabb { min, max }
    }

    fn count(m: &FaceMask) -> u32 {
        m.iter().map(|w| w.count_ones()).sum()
    }

    #[test]
    fn bottom_slab_masks() {
        let slab = [bx([0.0; 3], [1.0, 0.5, 1.0])];
        assert_eq!(face_mask_of(&slab, 3), FULL_MASK, "bottom face is covered");
        assert_eq!(
            count(&face_mask_of(&slab, 2)),
            0,
            "top face is not at the boundary"
        );
        assert_eq!(count(&face_mask_of(&slab, 0)), 16 * 8, "side covers half");
    }

    #[test]
    fn stair_top_covers_the_back_half() {
        let stair = [
            bx([0.0; 3], [1.0, 0.5, 1.0]),
            bx([0.5, 0.5, 0.0], [1.0, 1.0, 1.0]),
        ];
        assert_eq!(count(&face_mask_of(&stair, 2)), 8 * 16);
        assert_eq!(face_mask_of(&stair, 3), FULL_MASK);
    }

    #[test]
    fn cube_like_shapes_use_their_projection() {
        let post = [bx([0.25, 0.0, 0.25], [0.75, 1.0, 0.75])];
        assert_eq!(count(&face_mask_of(&post, 2)), 8 * 8);
        let stacked = [
            bx([0.25, 0.0, 0.25], [0.75, 0.5, 0.75]),
            bx([0.25, 0.5, 0.25], [0.75, 1.0, 0.75]),
        ];
        assert_eq!(face_mask_of(&stacked, 2), face_mask_of(&post, 2));
    }

    #[test]
    fn rotation_detection() {
        use mcr_assets::{Face, Vertex};
        let mk = |face: usize, k: usize| {
            let ax = faces::axis(face);
            let [ua, va] = faces::TANGENTS[ax];
            let plane = if faces::is_positive(face) { 1.0 } else { 0.0 };
            let vertex = |cu: f32, cv: f32| {
                let mut pos = [0.0; 3];
                pos[ax] = plane;
                pos[ua] = cu;
                pos[va] = cv;
                let [su, sv] = faces::corner_uv(face, cu, cv);
                Vertex {
                    pos,
                    uv: orient(k as u8, su, sv),
                }
            };
            BakedQuad {
                vertices: [
                    vertex(0.0, 0.0),
                    vertex(1.0, 0.0),
                    vertex(1.0, 1.0),
                    vertex(0.0, 1.0),
                ],
                sprite: SpriteId(0),
                normal: Face::Up,
                cull: Some(Face::Up),
                tint: -1,
                shade_dir: Face::Up,
                light_emission: 0,
                force_translucent: false,
            }
        };
        for k in 0..8 {
            assert_eq!(face_rotation(&mk(2, k), 2), Some(k as u8));
        }
    }
}
