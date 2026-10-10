use std::collections::HashMap;
use std::sync::Arc;

use crate::bake::{BakeCtx, BakedQuad, SpriteId, SpriteTable, bake_elements, bake_face};
use crate::error::Result;
use crate::geometry::{Face, Rot};
use crate::identifier::Identifier;
use crate::model::{
    Builtin, DisplayContext, Element, FaceDef, GuiLight, ItemTransform, Material, ModelLoader,
    ResolvedModel,
};
use crate::resources::ResourceManager;
use crate::texture::{Sprite, decode_sprite, missing_sprite};

const LAYERS: usize = 5;
const MIN_Z: f32 = 7.5;
const MAX_Z: f32 = 8.5;
const UV_SHRINK: f32 = 0.1;

#[derive(Clone, Debug)]
pub struct BakedItemModel {
    pub id: Identifier,
    pub quads: Arc<[BakedQuad]>,
    pub gui_light: GuiLight,
    pub builtin: Option<Builtin>,
    pub particle: Option<SpriteId>,
    /// Tints baked into the model itself, used when the layer supplies none.
    pub tints: Vec<u32>,
    display: [Option<ItemTransform>; 10],
}

impl BakedItemModel {
    pub fn display(&self, context: DisplayContext) -> Option<ItemTransform> {
        self.display[context as usize]
    }

    pub(crate) fn derive(
        &self,
        quads: Vec<BakedQuad>,
        tints: Vec<u32>,
        gui_light: GuiLight,
        keep_display: bool,
    ) -> Self {
        Self {
            id: self.id.clone(),
            quads: quads.into(),
            gui_light,
            builtin: self.builtin,
            particle: self.particle,
            tints,
            display: if keep_display {
                self.display
            } else {
                [None; 10]
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Up,
    Down,
    Left,
    Right,
}

impl Side {
    const ALL: [Side; 4] = [Side::Up, Side::Down, Side::Left, Side::Right];

    fn face(self) -> Face {
        match self {
            Side::Up => Face::Up,
            Side::Down => Face::Down,
            Side::Left => Face::East,
            Side::Right => Face::West,
        }
    }

    fn step(self) -> (i32, i32) {
        match self {
            Side::Up => (0, -1),
            Side::Down => (0, 1),
            Side::Left => (-1, 0),
            Side::Right => (1, 0),
        }
    }

    fn horizontal(self) -> bool {
        matches!(self, Side::Up | Side::Down)
    }
}

fn single_face(face: Face, uv: [f32; 4], tint: i32) -> [Option<FaceDef>; 6] {
    let mut faces: [Option<FaceDef>; 6] = Default::default();
    faces[face as usize] = Some(FaceDef {
        texture: "".into(),
        uv: Some(uv),
        cull: None,
        tint,
        rotation: 0,
    });
    faces
}

fn element(from: [f32; 3], to: [f32; 3], faces: [Option<FaceDef>; 6]) -> Element {
    Element {
        from,
        to,
        rotation: None,
        shade_dir: None,
        light_emission: 0,
        faces,
    }
}

/// Front and back faces plus one edge face per opaque pixel border, like vanilla's extruded sprites.
fn extrude_layer(
    ctx: &BakeCtx,
    material: &Material,
    sprite_id: SpriteId,
    sprite: &Sprite,
    tint: i32,
    out: &mut Vec<BakedQuad>,
) {
    let from = [0.0, 0.0, MIN_Z];
    let to = [16.0, 16.0, MAX_Z];
    for (face, uv) in [
        (Face::South, [0.0, 0.0, 16.0, 16.0]),
        (Face::North, [16.0, 0.0, 0.0, 16.0]),
    ] {
        let el = element(from, to, single_face(face, uv, tint));
        let def = el.faces[face as usize].clone().expect("face just set");
        out.push(bake_face(ctx, &el, face, &def, material, sprite_id));
    }

    let (w, h) = (sprite.width as usize, sprite.height as usize);
    let (xs, ys) = (16.0 / w as f32, 16.0 / h as f32);
    let mut edges = vec![[false; 4]; w * h];
    for frame in &sprite.frames {
        let opaque = |x: i32, y: i32| {
            x >= 0
                && y >= 0
                && (x as usize) < w
                && (y as usize) < h
                && frame[(y as usize * w + x as usize) * 4 + 3] != 0
        };
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                if !opaque(x, y) {
                    continue;
                }
                for (i, side) in Side::ALL.into_iter().enumerate() {
                    let (dx, dy) = side.step();
                    if !opaque(x + dx, y + dy) {
                        edges[y as usize * w + x as usize][i] = true;
                    }
                }
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            for (i, side) in Side::ALL.into_iter().enumerate() {
                if !edges[y * w + x][i] {
                    continue;
                }
                let (fx, fy) = (x as f32, y as f32);
                let (u0, u1) = (fx + UV_SHRINK, fx + 1.0 - UV_SHRINK);
                let (v0, v1) = if side.horizontal() {
                    (fy + UV_SHRINK, fy + 1.0 - UV_SHRINK)
                } else {
                    (fy + 1.0 - UV_SHRINK, fy + UV_SHRINK)
                };
                let (mut sx, mut sy, mut ex, mut ey) = (fx, fy, fx, fy);
                match side {
                    Side::Up => ex += 1.0,
                    Side::Down => {
                        ex += 1.0;
                        sy += 1.0;
                        ey += 1.0;
                    }
                    Side::Left => ey += 1.0,
                    Side::Right => {
                        sx += 1.0;
                        ex += 1.0;
                        ey += 1.0;
                    }
                }
                let (sx, ex) = (sx * xs, ex * xs);
                let (sy, ey) = (16.0 - sy * ys, 16.0 - ey * ys);
                let (from, to) = match side {
                    Side::Up => ([sx, sy, MIN_Z], [ex, sy, MAX_Z]),
                    Side::Down => ([sx, ey, MIN_Z], [ex, ey, MAX_Z]),
                    Side::Left => ([sx, sy, MIN_Z], [sx, ey, MAX_Z]),
                    Side::Right => ([ex, sy, MIN_Z], [ex, ey, MAX_Z]),
                };
                let uv = [u0 * xs, v0 * ys, u1 * xs, v1 * ys];
                let face = side.face();
                let el = element(from, to, single_face(face, uv, tint));
                let def = el.faces[face as usize].clone().expect("face just set");
                out.push(bake_face(ctx, &el, face, &def, material, sprite_id));
            }
        }
    }
}

/// Bakes item models and decodes their textures; independent of the resource manager's lifetime.
#[derive(Debug, Default)]
pub struct ItemBaker {
    loader: ModelLoader,
    sprites: SpriteTable,
    baked: HashMap<Identifier, Arc<BakedItemModel>>,
    images: HashMap<SpriteId, Arc<Sprite>>,
}

impl ItemBaker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sprites(&self) -> &SpriteTable {
        &self.sprites
    }

    pub fn intern(&mut self, name: &Identifier) -> SpriteId {
        self.sprites.intern(name)
    }

    /// Decoded texture, the checkerboard when absent.
    pub fn image(&mut self, resources: &ResourceManager, id: SpriteId) -> Result<Arc<Sprite>> {
        if let Some(img) = self.images.get(&id) {
            return Ok(img.clone());
        }
        let name = self.sprites.get(id).clone();
        let path = name.resource_path("textures", ".png");
        let sprite = match resources.read(&path)? {
            Some(png) => {
                let meta = resources.read(&format!("{path}.mcmeta"))?;
                decode_sprite(&path, &png, meta.as_deref())?
            }
            None => missing_sprite(),
        };
        let sprite = Arc::new(sprite);
        self.images.insert(id, sprite.clone());
        Ok(sprite)
    }

    pub fn bake(
        &mut self,
        resources: &ResourceManager,
        id: &Identifier,
    ) -> Result<Arc<BakedItemModel>> {
        if let Some(hit) = self.baked.get(id) {
            return Ok(hit.clone());
        }
        let model = self.loader.resolve(resources, id)?;
        let quads = if model.builtin == Some(Builtin::Generated) {
            self.bake_generated(resources, &model)?
        } else {
            bake_elements(&model, Rot::IDENTITY, false, &mut self.sprites)?
        };
        let particle = model
            .textures
            .get("particle")
            .or_else(|| model.textures.get("layer0"))
            .map(|m| self.sprites.intern(&m.sprite));
        let baked = Arc::new(BakedItemModel {
            id: id.clone(),
            quads: quads.into(),
            gui_light: model.gui_light,
            builtin: model.builtin,
            particle,
            display: model.display_table(),
            tints: Vec::new(),
        });
        self.baked.insert(id.clone(), baked.clone());
        Ok(baked)
    }

    fn bake_generated(
        &mut self,
        resources: &ResourceManager,
        model: &ResolvedModel,
    ) -> Result<Vec<BakedQuad>> {
        let ctx = BakeCtx {
            model,
            rot: Rot::IDENTITY,
            uvlock: false,
        };
        let mut quads = Vec::new();
        for layer in 0..LAYERS {
            let Some(material) = model.material(&format!("layer{layer}")) else {
                break;
            };
            let sprite_id = self.sprites.intern(&material.sprite);
            let image = self.image(resources, sprite_id)?;
            extrude_layer(&ctx, material, sprite_id, &image, layer as i32, &mut quads);
        }
        Ok(quads)
    }
}
