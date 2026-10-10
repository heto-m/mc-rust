//! Per-state render data, baked once from the resource packs.

use std::collections::HashMap;

use mcr_assets::{
    BakedQuad, Identifier, ModelBaker, ResourceManager, SpriteId, TextureArrayBuilder,
    TextureArrays, Transparency,
};
use mcr_data::Version;
use mcr_render::{SpriteAnimation, SpriteFrame};

use crate::faces::{self, VANILLA_TO_RENDER};
use crate::random::{OffsetSpec, offset_spec};
use crate::tint::{self, TINT_NONE, constant_palette, tint_kinds};

pub const LAYER_NONE: u16 = 0xffff;
pub const NO_CULL: u8 = 0xff;

pub const ATTR_SOLID_RENDER: u8 = 1;
pub const ATTR_FULL_COLLISION: u8 = 2;
/// Glass-like blocks hide faces against the same block, and fluids use their overlay sprite beside them.
pub const ATTR_HALF_TRANSPARENT: u8 = 4;
pub const ATTR_LEAVES: u8 = 8;
pub const ATTR_AIR: u8 = 16;
/// Solid-render blocks that dampen light (vanilla's `!isLightPermeable`).
pub const ATTR_LIGHT_OPAQUE: u8 = 32;
/// Rendered at full brightness regardless of light (magma block, sculk sensor).
pub const ATTR_EMISSIVE: u8 = 64;
/// Vanilla's `isSolid` (fluid heights and flow treat these as walls).
pub const ATTR_SOLID: u8 = 128;

pub const META_ATTR_MASK: u32 = 0xff;
pub const META_FULL_SHIFT: u32 = 8;
pub const META_PARTIAL_SHIFT: u32 = 16;
pub const META_EMISSION_SHIFT: u32 = 24;

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
}

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
    pub textures: TextureArrays,
    pub missing_sprites: Vec<String>,
    pub unbaked_states: usize,
}

fn compute_meta(attr: u8, occ: u8, partial: u8, emission: u8) -> u32 {
    u32::from(attr)
        | u32::from(occ) << META_FULL_SHIFT
        | u32::from(partial) << META_PARTIAL_SHIFT
        | u32::from(emission.min(15)) << META_EMISSION_SHIFT
}

impl ModelCache {
    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    pub fn state(&self, id: u16) -> &StateModel {
        self.states
            .get(usize::from(id))
            .unwrap_or(&StateModel::EMPTY)
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

    #[inline]
    pub fn attr(&self, id: u16) -> u8 {
        self.attrs.get(usize::from(id)).copied().unwrap_or(0)
    }

    /// Bit `f` is set when the state fully covers its face `f` (render face order).
    #[inline]
    pub fn occlusion(&self, id: u16) -> u8 {
        self.occlusion.get(usize::from(id)).copied().unwrap_or(0)
    }

    /// Attributes, full and partial occlusion bits and emission packed for one lookup per cell.
    #[inline]
    pub fn meta(&self, id: u16) -> u32 {
        self.meta.get(usize::from(id)).copied().unwrap_or(0)
    }

    /// Occlusion mask id of face `face` of state `id`: `MASK_EMPTY`, `MASK_FULL` or a partial shape.
    #[inline]
    pub fn face_mask(&self, id: u16, face: usize) -> u16 {
        match self.face_masks.get(usize::from(id)) {
            Some(m) => m[face],
            None => self.fallback_mask(id, face),
        }
    }

    fn fallback_mask(&self, id: u16, face: usize) -> u16 {
        if self.occlusion(id) >> face & 1 != 0 {
            MASK_FULL
        } else {
            MASK_EMPTY
        }
    }

    pub fn mask(&self, id: u16) -> &FaceMask {
        self.masks.get(usize::from(id)).unwrap_or(&FULL_MASK)
    }

    /// Vanilla `Block.shouldRenderFace` on occlusion shapes, without the same-block skip rule.
    pub fn face_visible(&self, state: u16, neighbor: u16, face: usize) -> bool {
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

    pub fn palette(&self) -> [u32; tint::PALETTE_LEN] {
        constant_palette()
    }

    pub fn from_parts(
        states: Vec<StateModel>,
        quads: Vec<QuadDef>,
        attrs: Vec<u8>,
        occlusion: Vec<u8>,
    ) -> Self {
        let meta = (0..states.len())
            .map(|i| compute_meta(attrs[i], occlusion[i], 0, states[i].emission))
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
            textures: TextureArrays::default(),
            missing_sprites: Vec::new(),
            unbaked_states: 0,
        }
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
        let count = version.state_count();
        let mut baker = ModelBaker::new(resources);
        let mut baked = Vec::with_capacity(count);
        let mut multipart_of: HashMap<u16, bool> = HashMap::new();
        let mut unbaked = 0;
        for id in 0..count as u32 {
            let name = version.state_block_name(id).unwrap_or("minecraft:air");
            let props: Vec<(&str, &str)> = version.state_properties(id).collect();
            match baker.bake_block_state(name, &props) {
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
                    baked.push(None);
                }
            }
        }

        let mut sprites = baker.into_sprites();
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
        let mut mask_ids: HashMap<FaceMask, u16> = HashMap::new();
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
                    let kind = if fname.contains("water") {
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
            if short.ends_with("glass")
                || short.ends_with("stained_glass")
                || matches!(
                    short,
                    "ice"
                        | "frosted_ice"
                        | "slime_block"
                        | "honey_block"
                        | "powder_snow"
                        | "mangrove_roots"
                )
            {
                attr |= ATTR_HALF_TRANSPARENT;
            }
            if short.ends_with("_leaves") {
                attr |= ATTR_LEAVES | ATTR_HALF_TRANSPARENT;
            }
            if matches!(short, "magma_block" | "sculk_sensor") {
                attr |= ATTR_EMISSIVE;
            }
            let partial = (0..6).fold(0u8, |acc, f| {
                acc | u8::from(face_ids[f] != MASK_EMPTY && face_ids[f] != MASK_FULL) << f
            });
            cache.attrs.push(attr);
            cache.occlusion.push(occ);
            cache.meta.push(compute_meta(attr, occ, partial, emission));
            cache.face_masks.push(face_ids);

            let mut sm = StateModel::EMPTY;
            sm.block = block;
            sm.fluid = fluid;
            sm.emission = emission;
            sm.offset = offset_spec(name);
            if let Some(model) = model {
                let props: Vec<(&str, &str)> = version.state_properties(id).collect();
                let kinds = tint_kinds(name, &props);
                let tint_of = |q: &BakedQuad| match q.tint {
                    0 => kinds[0],
                    1 => kinds[1],
                    _ => TINT_NONE,
                };
                let quad_layer = |q: &BakedQuad| {
                    RenderLayer::from_transparency(transparency_of(q.sprite), q.force_translucent)
                };
                let first: Vec<_> = model.parts.iter().map(|p| &p.variants[0]).collect();
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
                                    let start = cache.quads.len() as u32;
                                    for q in &lists[p][v] {
                                        cache.quads.push(compile_quad(
                                            q,
                                            layer_of(q.sprite),
                                            tint_of(q),
                                            quad_layer(q),
                                        ));
                                    }
                                    ([CubeFace::NONE; 6], start, lists[p][v].len() as u32)
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
                    sm.quad_start = cache.quads.len() as u32;
                    for part in &lists {
                        for q in &part[0] {
                            cache.quads.push(compile_quad(
                                q,
                                layer_of(q.sprite),
                                tint_of(q),
                                quad_layer(q),
                            ));
                        }
                    }
                    sm.quad_count = cache.quads.len() as u32 - sm.quad_start;
                }
            }
            cache.states.push(sm);
        }

        cache.fluid_sprites = fluid_sprites;
        cache.textures = textures;
        cache.missing_sprites = missing_sprites;
        cache.unbaked_states = unbaked;
        Ok(cache)
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
