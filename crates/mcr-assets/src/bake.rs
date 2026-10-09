use std::collections::HashMap;
use std::sync::Arc;

use crate::blockstate::{BlockStateDef, Properties, Variant};
use crate::error::{AssetError, Result};
use crate::geometry::{self, Face, Rot};
use crate::identifier::Identifier;
use crate::model::{Element, FaceDef, Material, ModelLoader, ResolvedModel};
use crate::resources::ResourceManager;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpriteId(pub u32);

#[derive(Clone, Debug, Default)]
pub struct SpriteTable {
    names: Vec<Identifier>,
    ids: HashMap<Identifier, SpriteId>,
}

impl SpriteTable {
    pub fn intern(&mut self, name: &Identifier) -> SpriteId {
        if let Some(id) = self.ids.get(name) {
            return *id;
        }
        let id = SpriteId(self.names.len() as u32);
        self.names.push(name.clone());
        self.ids.insert(name.clone(), id);
        id
    }

    pub fn get(&self, id: SpriteId) -> &Identifier {
        &self.names[id.0 as usize]
    }

    pub fn lookup(&self, name: &Identifier) -> Option<SpriteId> {
        self.ids.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (SpriteId, &Identifier)> {
        self.names
            .iter()
            .enumerate()
            .map(|(i, n)| (SpriteId(i as u32), n))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakedQuad {
    pub vertices: [Vertex; 4],
    pub sprite: SpriteId,
    pub normal: Face,
    pub cull: Option<Face>,
    pub tint: i32,
    pub shade_dir: Face,
    pub light_emission: u8,
    pub force_translucent: bool,
}

#[derive(Clone, Debug)]
pub struct BakedVariant {
    pub weight: u32,
    pub ambient_occlusion: bool,
    pub particle: Option<SpriteId>,
    pub quads: Arc<[BakedQuad]>,
}

#[derive(Clone, Debug)]
pub struct ModelPart {
    pub variants: Vec<BakedVariant>,
    pub total_weight: u32,
}

impl ModelPart {
    pub fn pick(&self, random: u32) -> &BakedVariant {
        let mut roll = random % self.total_weight.max(1);
        for v in &self.variants {
            if roll < v.weight {
                return v;
            }
            roll -= v.weight;
        }
        &self.variants[0]
    }
}

#[derive(Clone, Debug, Default)]
pub struct BakedModel {
    pub parts: Vec<ModelPart>,
}

impl BakedModel {
    pub fn quad_count(&self) -> usize {
        self.parts
            .iter()
            .map(|p| p.variants.iter().map(|v| v.quads.len()).max().unwrap_or(0))
            .sum()
    }
}

const FACE_VERTICES: [[[u8; 3]; 4]; 6] = [
    [[0, 0, 1], [0, 0, 0], [1, 0, 0], [1, 0, 1]],
    [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]],
    [[1, 1, 0], [1, 0, 0], [0, 0, 0], [0, 1, 0]],
    [[0, 1, 1], [0, 0, 1], [1, 0, 1], [1, 1, 1]],
    [[0, 1, 0], [0, 0, 0], [0, 0, 1], [0, 1, 1]],
    [[1, 1, 1], [1, 0, 1], [1, 0, 0], [1, 1, 0]],
];

fn default_uv(from: [f32; 3], to: [f32; 3], face: Face) -> [f32; 4] {
    match face {
        Face::Down => [from[0], 16.0 - to[2], to[0], 16.0 - from[2]],
        Face::Up => [from[0], from[2], to[0], to[2]],
        Face::North => [16.0 - to[0], 16.0 - to[1], 16.0 - from[0], 16.0 - from[1]],
        Face::South => [from[0], 16.0 - to[1], to[0], 16.0 - from[1]],
        Face::West => [from[2], 16.0 - to[1], to[2], 16.0 - from[1]],
        Face::East => [16.0 - to[2], 16.0 - to[1], 16.0 - from[2], 16.0 - from[1]],
    }
}

fn corner_uv(uv: [f32; 4], index: usize) -> [f32; 2] {
    let u = if index <= 1 { uv[0] } else { uv[2] };
    let v = if index == 0 || index == 3 {
        uv[1]
    } else {
        uv[3]
    };
    [u / 16.0, v / 16.0]
}

fn facing_of(p: &[Vertex; 4]) -> Option<Face> {
    let a = geometry::sub(p[2].pos, p[1].pos);
    let b = geometry::sub(p[0].pos, p[1].pos);
    let n = geometry::cross(a, b);
    if !n.iter().all(|c| c.is_finite()) {
        return None;
    }
    let len = n.iter().map(|c| c * c).sum::<f32>().sqrt();
    if len == 0.0 {
        return None;
    }
    let n = n.map(|c| c / len);
    let mut best = None;
    let mut best_dot = 0.0;
    for face in Face::ALL {
        let fnorm = face.normal();
        let dot: f32 = (0..3).map(|i| n[i] * f32::from(fnorm[i])).sum();
        if dot >= 0.0 && dot > best_dot {
            best_dot = dot;
            best = Some(face);
        }
    }
    best
}

fn fix_winding(vertices: &mut [Vertex; 4], facing: Face) {
    let mut min = [999.0f32; 3];
    let mut max = [-999.0f32; 3];
    for v in vertices.iter() {
        for i in 0..3 {
            min[i] = min[i].min(v.pos[i]);
            max[i] = max[i].max(v.pos[i]);
        }
    }
    for (slot, sel) in FACE_VERTICES[facing as usize].iter().enumerate() {
        let target: [f32; 3] = std::array::from_fn(|i| if sel[i] == 0 { min[i] } else { max[i] });
        if let Some(found) = (slot..4).find(|&j| vertices[j].pos == target) {
            vertices.swap(slot, found);
        }
    }
}

struct BakeCtx<'a> {
    model: &'a ResolvedModel,
    rot: Rot,
    uvlock: bool,
}

fn bake_face(
    ctx: &BakeCtx,
    element: &Element,
    face: Face,
    def: &FaceDef,
    material: &Material,
    sprite: SpriteId,
) -> BakedQuad {
    let uv = def
        .uv
        .unwrap_or_else(|| default_uv(element.from, element.to, face));
    let uv_transform = ctx.uvlock.then(|| ctx.rot.uv_inverse(face));
    let mut vertices = [Vertex {
        pos: [0.0; 3],
        uv: [0.0; 2],
    }; 4];
    for (i, vertex) in vertices.iter_mut().enumerate() {
        let sel = FACE_VERTICES[face as usize][i];
        let mut pos: [f32; 3] = std::array::from_fn(|a| {
            if sel[a] == 0 {
                element.from[a]
            } else {
                element.to[a]
            }
        });
        pos = pos.map(|c| c / 16.0);
        if let Some(r) = &element.rotation {
            let rel = geometry::sub(pos, r.origin);
            let rotated = geometry::mat3_apply(&r.matrix, rel);
            pos = std::array::from_fn(|a| rotated[a] + r.origin[a]);
        }
        if !ctx.rot.is_identity() {
            let rel = pos.map(|c| c - 0.5);
            let rotated = ctx.rot.apply(rel);
            pos = rotated.map(|c| c + 0.5);
        }
        let [u, v] = corner_uv(uv, (i + usize::from(def.rotation)) % 4);
        let uv = match uv_transform {
            Some(m) if !m.is_identity() => m.apply_uv(u, v),
            _ => [u, v],
        };
        *vertex = Vertex { pos, uv };
    }

    let facing = facing_of(&vertices);
    if let (None, Some(f)) = (&element.rotation, facing) {
        fix_winding(&mut vertices, f);
    }
    let normal = facing.unwrap_or(Face::Up);

    BakedQuad {
        vertices,
        sprite,
        normal,
        cull: def.cull.map(|c| ctx.rot.face(c)),
        tint: def.tint,
        shade_dir: element.shade_dir.unwrap_or(normal),
        light_emission: element.light_emission,
        force_translucent: material.force_translucent,
    }
}

fn bake_elements(
    model: &ResolvedModel,
    rot: Rot,
    uvlock: bool,
    sprites: &mut SpriteTable,
) -> Result<Vec<BakedQuad>> {
    let ctx = BakeCtx { model, rot, uvlock };
    let mut quads = Vec::new();
    for element in model.elements.iter() {
        let flat = [0, 1, 2].map(|a| element.from[a] == element.to[a]);
        let draws = |face: Face| match face {
            Face::West | Face::East => !(flat[1] || flat[2]),
            Face::Down | Face::Up => !(flat[0] || flat[2]),
            Face::North | Face::South => !(flat[0] || flat[1]),
        };
        for face in Face::ALL {
            let Some(def) = &element.faces[face as usize] else {
                continue;
            };
            if !draws(face) {
                continue;
            }
            let material =
                ctx.model
                    .material(&def.texture)
                    .ok_or_else(|| AssetError::UnresolvedTexture {
                        model: ctx.model.id.to_string(),
                        variable: def.texture.trim_start_matches('#').to_owned(),
                    })?;
            let sprite = sprites.intern(&material.sprite);
            quads.push(bake_face(&ctx, element, face, def, material, sprite));
        }
    }
    Ok(quads)
}

type VariantKey = (u8, u8, u8, bool);

#[derive(Debug)]
pub struct ModelBaker<'a> {
    resources: &'a ResourceManager,
    loader: ModelLoader,
    states: HashMap<Identifier, Arc<BlockStateDef>>,
    geometry: HashMap<Identifier, Vec<(VariantKey, BakedVariant)>>,
    sprites: SpriteTable,
}

impl<'a> ModelBaker<'a> {
    pub fn new(resources: &'a ResourceManager) -> Self {
        Self {
            resources,
            loader: ModelLoader::default(),
            states: HashMap::new(),
            geometry: HashMap::new(),
            sprites: SpriteTable::default(),
        }
    }

    pub fn sprites(&self) -> &SpriteTable {
        &self.sprites
    }

    pub fn into_sprites(self) -> SpriteTable {
        self.sprites
    }

    pub fn blockstate(&mut self, block: &str) -> Result<Arc<BlockStateDef>> {
        let id = Identifier::parse(block);
        if let Some(def) = self.states.get(&id) {
            return Ok(def.clone());
        }
        let path = id.resource_path("blockstates", ".json");
        let bytes = self
            .resources
            .read(&path)?
            .ok_or_else(|| AssetError::Missing(path.clone()))?;
        let def = Arc::new(BlockStateDef::parse(&bytes, &path)?);
        self.states.insert(id, def.clone());
        Ok(def)
    }

    pub fn bake_block_state(&mut self, block: &str, props: Properties) -> Result<BakedModel> {
        let def = self.blockstate(block)?;
        let groups = def.select(props);
        if groups.is_empty() && def.multipart.is_empty() {
            return Err(AssetError::NoVariant {
                block: block.to_owned(),
                properties: props
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(","),
            });
        }
        let mut parts = Vec::with_capacity(groups.len());
        for group in groups {
            let mut variants = Vec::with_capacity(group.len());
            let mut total_weight = 0;
            for v in group {
                let mut baked = self.bake_variant(v)?;
                baked.weight = v.weight;
                total_weight += v.weight;
                variants.push(baked);
            }
            parts.push(ModelPart {
                variants,
                total_weight,
            });
        }
        Ok(BakedModel { parts })
    }

    pub fn bake_variant(&mut self, variant: &Variant) -> Result<BakedVariant> {
        let key = (variant.x, variant.y, variant.z, variant.uvlock);
        if let Some(hit) = self
            .geometry
            .get(&variant.model)
            .and_then(|list| list.iter().find(|(k, _)| *k == key))
        {
            return Ok(hit.1.clone());
        }
        let model = self.loader.resolve(self.resources, &variant.model)?;
        let rot = Rot::from_variant(variant.x, variant.y, variant.z);
        let quads = bake_elements(&model, rot, variant.uvlock, &mut self.sprites)?;
        let particle = model
            .textures
            .get("particle")
            .map(|m| self.sprites.intern(&m.sprite));
        let baked = BakedVariant {
            weight: 1,
            ambient_occlusion: model.ambient_occlusion,
            particle,
            quads: quads.into(),
        };
        self.geometry
            .entry(variant.model.clone())
            .or_default()
            .push((key, baked.clone()));
        Ok(baked)
    }
}
