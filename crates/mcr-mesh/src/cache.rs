//! Per-state render data, baked once from the resource packs.

use mcr_assets::{
    BakedQuad, Identifier, ModelBaker, ResourceManager, SpriteId, TextureArrayBuilder,
    TextureArrays, Transparency,
};
use mcr_data::Version;

use crate::faces::{self, VANILLA_TO_RENDER};
use crate::tint::{self, TINT_NONE, constant_palette, tint_kinds};

pub const LAYER_NONE: u16 = 0xffff;
pub const NO_CULL: u8 = 0xff;

pub const ATTR_SOLID_RENDER: u8 = 1;
pub const ATTR_FULL_COLLISION: u8 = 2;
/// Glass-like blocks hide faces against the same block, and fluids use their overlay sprite beside them.
pub const ATTR_HALF_TRANSPARENT: u8 = 4;
pub const ATTR_LEAVES: u8 = 8;
pub const ATTR_AIR: u8 = 16;

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
}

impl CubeFace {
    pub const NONE: Self = Self {
        layer: LAYER_NONE,
        tint: TINT_NONE,
        overlay: LAYER_NONE,
        overlay_tint: TINT_NONE,
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
    };
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
    pub fluid_sprites: FluidSprites,
    pub textures: TextureArrays,
    pub missing_sprites: Vec<String>,
    pub unbaked_states: usize,
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

    #[inline]
    pub fn attr(&self, id: u16) -> u8 {
        self.attrs.get(usize::from(id)).copied().unwrap_or(0)
    }

    /// Bit `f` is set when the state fully covers its face `f` (render face order).
    #[inline]
    pub fn occlusion(&self, id: u16) -> u8 {
        self.occlusion.get(usize::from(id)).copied().unwrap_or(0)
    }

    /// The block texture groups in the renderer's upload format.
    pub fn texture_groups(&self) -> Vec<mcr_render::TextureGroupData> {
        self.textures
            .groups
            .iter()
            .map(|g| mcr_render::TextureGroupData {
                width: g.width,
                height: g.height,
                layers: g.layers,
                mips: g.mips.clone(),
            })
            .collect()
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
        Self {
            states,
            quads,
            attrs,
            occlusion,
            fluid_sprites: FluidSprites::default(),
            textures: TextureArrays::default(),
            missing_sprites: Vec::new(),
            unbaked_states: 0,
        }
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
        let mut unbaked = 0;
        for id in 0..count as u32 {
            let name = version.state_block_name(id).unwrap_or("minecraft:air");
            let props: Vec<(&str, &str)> = version.state_properties(id).collect();
            match baker.bake_block_state(name, &props) {
                Ok(model) => baked.push(Some(model)),
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

        let mut states = Vec::with_capacity(count);
        let mut quads: Vec<QuadDef> = Vec::new();
        let mut attrs = Vec::with_capacity(count);
        let mut occlusion = Vec::with_capacity(count);
        for (id, model) in baked.iter().enumerate() {
            let id = id as u32;
            let info = version.block_state(id);
            let name = version.state_block_name(id).unwrap_or("minecraft:air");
            let short = name.strip_prefix("minecraft:").unwrap_or(name);
            let mut attr = 0;
            let mut occ = 0;
            let mut fluid = FluidInfo::default();
            let mut block = 0;
            if let Some(info) = info {
                block = info.block as u16;
                if info.is_solid_render() {
                    attr |= ATTR_SOLID_RENDER;
                }
                if info.is_full_collision() {
                    attr |= ATTR_FULL_COLLISION;
                }
                if info.is_air() {
                    attr |= ATTR_AIR;
                }
                for (v, render) in VANILLA_TO_RENDER.iter().enumerate() {
                    if info.is_face_fully_occluding(v) {
                        occ |= 1 << render;
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
            attrs.push(attr);
            occlusion.push(occ);

            let mut sm = StateModel::EMPTY;
            sm.block = block;
            sm.fluid = fluid;
            if let Some(model) = model {
                let props: Vec<(&str, &str)> = version.state_properties(id).collect();
                let kinds = tint_kinds(name, &props);
                let variants: Vec<_> = model.parts.iter().map(|p| &p.variants[0]).collect();
                let ao = variants.iter().all(|v| v.ambient_occlusion)
                    && info.is_none_or(|i| i.light_emission == 0);
                let list: Vec<&BakedQuad> = variants.iter().flat_map(|v| v.quads.iter()).collect();
                if !list.is_empty() {
                    sm.ao = ao;
                    let quad_layer = |q: &BakedQuad| {
                        RenderLayer::from_transparency(
                            transparency_of(q.sprite),
                            q.force_translucent,
                        )
                    };
                    let tint_of = |q: &BakedQuad| match q.tint {
                        0 => kinds[0],
                        1 => kinds[1],
                        _ => TINT_NONE,
                    };
                    if variants.len() == 1
                        && let Some(cube) = as_cube(&list)
                    {
                        sm.shape = Shape::Cube;
                        for (face, slot) in cube.iter().enumerate() {
                            if let Some(base) = slot.0 {
                                sm.cube[face] = CubeFace {
                                    layer: layer_of(base.sprite),
                                    tint: tint_of(base),
                                    overlay: slot.1.map_or(LAYER_NONE, |q| layer_of(q.sprite)),
                                    overlay_tint: slot.1.map_or(TINT_NONE, tint_of),
                                };
                                sm.layer = sm.layer.max(quad_layer(base));
                            }
                        }
                        let overlay_translucent = cube.iter().any(|s| {
                            s.1.is_some_and(|q| quad_layer(q) == RenderLayer::Translucent)
                        });
                        if overlay_translucent
                            || (sm.layer == RenderLayer::Translucent
                                && cube.iter().any(|s| s.1.is_some()))
                        {
                            sm.shape = Shape::Model;
                        }
                    }
                    if sm.shape != Shape::Cube {
                        sm.shape = Shape::Model;
                        sm.quad_start = quads.len() as u32;
                        for q in &list {
                            quads.push(compile_quad(
                                q,
                                layer_of(q.sprite),
                                tint_of(q),
                                quad_layer(q),
                            ));
                        }
                        sm.quad_count = list.len() as u32;
                    }
                }
            }
            states.push(sm);
        }

        Ok(Self {
            states,
            quads,
            attrs,
            occlusion,
            fluid_sprites,
            textures,
            missing_sprites,
            unbaked_states: unbaked,
        })
    }
}

type FaceQuads<'a> = (Option<&'a BakedQuad>, Option<&'a BakedQuad>);

/// Per render face: the base quad and an optional overlay quad, when every quad is a standard full face.
fn as_cube<'a>(quads: &[&'a BakedQuad]) -> Option<[FaceQuads<'a>; 6]> {
    let mut out: [FaceQuads; 6] = [(None, None); 6];
    for q in quads {
        let cull = q.cull?;
        let face = faces::render_face_from_vanilla(cull);
        if faces::render_face_from_vanilla(q.normal) != face || !is_standard_face(q, face) {
            return None;
        }
        let slot = &mut out[face];
        if slot.0.is_none() {
            slot.0 = Some(q);
        } else if slot.1.is_none() {
            slot.1 = Some(q);
        } else {
            return None;
        }
    }
    Some(out)
}

fn is_standard_face(q: &BakedQuad, face: usize) -> bool {
    let ax = faces::axis(face);
    let [ua, va] = faces::TANGENTS[ax];
    let plane = if faces::is_positive(face) { 1.0 } else { 0.0 };
    let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
    let mut seen = 0u8;
    for v in &q.vertices {
        let (cu, cv) = (v.pos[ua], v.pos[va]);
        if !near(v.pos[ax], plane)
            || !(near(cu, 0.0) || near(cu, 1.0))
            || !(near(cv, 0.0) || near(cv, 1.0))
        {
            return false;
        }
        let expected = faces::corner_uv(face, cu, cv);
        if !near(v.uv[0], expected[0]) || !near(v.uv[1], expected[1]) {
            return false;
        }
        seen |= 1 << (usize::from(cu > 0.5) + 2 * usize::from(cv > 0.5));
    }
    seen == 0b1111
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
    }
}
