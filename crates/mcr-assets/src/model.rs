use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::error::{AssetError, Result};
use crate::geometry::{self, Face, Mat3};
use crate::identifier::Identifier;
use crate::resources::ResourceManager;

const MAX_PARENT_DEPTH: usize = 64;

#[derive(Deserialize)]
#[serde(untagged)]
enum TextureJson {
    Name(String),
    Full {
        sprite: String,
        #[serde(default)]
        force_translucent: bool,
    },
}

#[derive(Deserialize)]
struct FaceJson {
    texture: String,
    uv: Option<[f32; 4]>,
    cullface: Option<String>,
    rotation: Option<i32>,
    tintindex: Option<i32>,
}

#[derive(Deserialize)]
struct RotationJson {
    origin: [f32; 3],
    axis: Option<String>,
    angle: Option<f32>,
    x: Option<f32>,
    y: Option<f32>,
    z: Option<f32>,
    #[serde(default)]
    rescale: bool,
}

#[derive(Deserialize)]
struct ElementJson {
    from: [f32; 3],
    to: [f32; 3],
    rotation: Option<RotationJson>,
    shade_direction_override: Option<String>,
    light_emission: Option<u8>,
    faces: HashMap<String, FaceJson>,
}

#[derive(Deserialize)]
struct TransformJson {
    rotation: Option<[f32; 3]>,
    translation: Option<[f32; 3]>,
    scale: Option<[f32; 3]>,
}

#[derive(Deserialize)]
struct ModelJson {
    parent: Option<String>,
    ambientocclusion: Option<bool>,
    gui_light: Option<String>,
    display: Option<HashMap<String, TransformJson>>,
    textures: Option<HashMap<String, TextureJson>>,
    elements: Option<Vec<ElementJson>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DisplayContext {
    None,
    ThirdPersonLeftHand,
    ThirdPersonRightHand,
    FirstPersonLeftHand,
    FirstPersonRightHand,
    Head,
    Gui,
    Ground,
    Fixed,
    OnShelf,
}

impl DisplayContext {
    pub const ALL: [DisplayContext; 10] = [
        DisplayContext::None,
        DisplayContext::ThirdPersonLeftHand,
        DisplayContext::ThirdPersonRightHand,
        DisplayContext::FirstPersonLeftHand,
        DisplayContext::FirstPersonRightHand,
        DisplayContext::Head,
        DisplayContext::Gui,
        DisplayContext::Ground,
        DisplayContext::Fixed,
        DisplayContext::OnShelf,
    ];

    pub fn name(self) -> &'static str {
        match self {
            DisplayContext::None => "none",
            DisplayContext::ThirdPersonLeftHand => "thirdperson_lefthand",
            DisplayContext::ThirdPersonRightHand => "thirdperson_righthand",
            DisplayContext::FirstPersonLeftHand => "firstperson_lefthand",
            DisplayContext::FirstPersonRightHand => "firstperson_righthand",
            DisplayContext::Head => "head",
            DisplayContext::Gui => "gui",
            DisplayContext::Ground => "ground",
            DisplayContext::Fixed => "fixed",
            DisplayContext::OnShelf => "on_shelf",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }
}

/// Rotation in degrees (JOML `rotationXYZ` order), translation in blocks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemTransform {
    pub rotation: [f32; 3],
    pub translation: [f32; 3],
    pub scale: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuiLight {
    Front,
    Side,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Generated,
    Entity,
    Other,
}

impl Builtin {
    fn from_id(id: &Identifier) -> Self {
        match id.path() {
            "builtin/generated" => Builtin::Generated,
            "builtin/entity" => Builtin::Entity,
            _ => Builtin::Other,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FaceDef {
    pub texture: Box<str>,
    pub uv: Option<[f32; 4]>,
    pub cull: Option<Face>,
    pub tint: i32,
    pub rotation: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ElementRotation {
    pub origin: [f32; 3],
    pub matrix: Mat3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub rotation: Option<ElementRotation>,
    pub shade_dir: Option<Face>,
    pub light_emission: u8,
    pub faces: [Option<FaceDef>; 6],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Material {
    pub sprite: Identifier,
    pub force_translucent: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum Slot {
    Reference(Box<str>),
    Value(Material),
}

#[derive(Debug)]
struct RawModel {
    parent: Option<Identifier>,
    ambient_occlusion: Option<bool>,
    gui_light: Option<GuiLight>,
    display: Vec<(DisplayContext, ItemTransform)>,
    textures: Vec<(Box<str>, Slot)>,
    elements: Option<Arc<[Element]>>,
}

#[derive(Debug)]
pub struct ResolvedModel {
    pub id: Identifier,
    pub elements: Arc<[Element]>,
    pub ambient_occlusion: bool,
    pub textures: HashMap<Box<str>, Material>,
    pub gui_light: GuiLight,
    pub builtin: Option<Builtin>,
    display: [Option<ItemTransform>; 10],
}

impl ResolvedModel {
    pub fn display(&self, context: DisplayContext) -> Option<&ItemTransform> {
        self.display[context as usize].as_ref()
    }

    pub(crate) fn display_table(&self) -> [Option<ItemTransform>; 10] {
        self.display
    }

    pub fn material(&self, texture: &str) -> Option<&Material> {
        self.textures
            .get(texture.strip_prefix('#').unwrap_or(texture))
    }
}

fn quadrant_count(degrees: i32) -> Option<u8> {
    match degrees.rem_euclid(360) {
        0 => Some(0),
        90 => Some(1),
        180 => Some(2),
        270 => Some(3),
        _ => None,
    }
}

fn parse_element(e: ElementJson, path: &str) -> Result<Element> {
    let bad = |m: &str| AssetError::invalid(path, m);
    for v in e.from.iter().chain(&e.to) {
        if !(-16.0..=32.0).contains(v) {
            return Err(bad("element bounds outside -16..32"));
        }
    }
    let rotation = e
        .rotation
        .map(|r| -> Result<ElementRotation> {
            let mut matrix = match (&r.axis, r.angle) {
                (Some(axis), Some(angle)) => {
                    let axis = match axis.to_ascii_lowercase().as_str() {
                        "x" => 0,
                        "y" => 1,
                        "z" => 2,
                        _ => return Err(bad("invalid rotation axis")),
                    };
                    geometry::rotation_axis_angle(axis, angle)
                }
                (None, None) if r.x.is_some() || r.y.is_some() || r.z.is_some() => {
                    let rx = geometry::rotation_axis_angle(0, r.x.unwrap_or(0.0));
                    let ry = geometry::rotation_axis_angle(1, r.y.unwrap_or(0.0));
                    let rz = geometry::rotation_axis_angle(2, r.z.unwrap_or(0.0));
                    geometry::mat3_mul(&rx, &geometry::mat3_mul(&ry, &rz))
                }
                _ => return Err(bad("rotation needs axis+angle or x/y/z")),
            };
            if r.rescale && !geometry::is_identity_f(&matrix) {
                for col in 0..3 {
                    let max = (0..3).map(|row| matrix[row][col].abs()).fold(0.0, f32::max);
                    for row in &mut matrix {
                        row[col] /= max;
                    }
                }
            }
            Ok(ElementRotation {
                origin: r.origin.map(|o| o / 16.0),
                matrix,
            })
        })
        .transpose()?;
    let shade_dir = match &e.shade_direction_override {
        Some(name) => Some(Face::from_name(name).ok_or_else(|| bad("unknown shade direction"))?),
        None => None,
    };
    let light_emission = e.light_emission.unwrap_or(0);
    if light_emission > 15 {
        return Err(bad("light_emission above 15"));
    }
    let mut faces: [Option<FaceDef>; 6] = Default::default();
    for (name, f) in e.faces {
        let face = Face::from_name(&name).ok_or_else(|| bad("unknown face"))?;
        faces[face as usize] = Some(FaceDef {
            texture: f.texture.into(),
            uv: f.uv,
            cull: f.cullface.as_deref().and_then(Face::from_name),
            tint: f.tintindex.unwrap_or(-1),
            rotation: quadrant_count(f.rotation.unwrap_or(0))
                .ok_or_else(|| bad("face rotation must be a multiple of 90"))?,
        });
    }
    if faces.iter().all(Option::is_none) {
        return Err(bad("element has no faces"));
    }
    Ok(Element {
        from: e.from,
        to: e.to,
        rotation,
        shade_dir,
        light_emission,
        faces,
    })
}

fn parse_raw(json: &[u8], path: &str) -> Result<RawModel> {
    let model: ModelJson = serde_json::from_slice(json).map_err(|source| AssetError::Json {
        path: path.to_owned(),
        source,
    })?;
    let textures = model
        .textures
        .unwrap_or_default()
        .into_iter()
        .map(|(slot, tex)| {
            let value = match tex {
                TextureJson::Name(n) => match n.strip_prefix('#') {
                    Some(var) => Slot::Reference(var.into()),
                    None => Slot::Value(Material {
                        sprite: Identifier::parse(&n),
                        force_translucent: false,
                    }),
                },
                TextureJson::Full {
                    sprite,
                    force_translucent,
                } => Slot::Value(Material {
                    sprite: Identifier::parse(&sprite),
                    force_translucent,
                }),
            };
            (slot.into_boxed_str(), value)
        })
        .collect();
    let elements = model
        .elements
        .map(|els| {
            els.into_iter()
                .map(|e| parse_element(e, path))
                .collect::<Result<Arc<[Element]>>>()
        })
        .transpose()?;
    let gui_light = match model.gui_light.as_deref() {
        Some("front") => Some(GuiLight::Front),
        Some("side") => Some(GuiLight::Side),
        Some(_) => return Err(AssetError::invalid(path, "unknown gui_light")),
        None => None,
    };
    let clamp = |v: [f32; 3], limit: f32| v.map(|c| c.clamp(-limit, limit));
    let display = model
        .display
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(name, t)| {
            let context = DisplayContext::from_name(&name)?;
            Some((
                context,
                ItemTransform {
                    rotation: t.rotation.unwrap_or([0.0; 3]),
                    translation: clamp(t.translation.unwrap_or([0.0; 3]).map(|c| c / 16.0), 5.0),
                    scale: clamp(t.scale.unwrap_or([1.0; 3]), 4.0),
                },
            ))
        })
        .collect();
    Ok(RawModel {
        parent: model.parent.as_deref().map(Identifier::parse),
        ambient_occlusion: model.ambientocclusion,
        gui_light,
        display,
        textures,
        elements,
    })
}

fn is_builtin(id: &Identifier) -> bool {
    id.path().starts_with("builtin/")
}

#[derive(Debug, Default)]
pub struct ModelLoader {
    raw: HashMap<Identifier, Arc<RawModel>>,
    resolved: HashMap<Identifier, Arc<ResolvedModel>>,
}

impl ModelLoader {
    fn raw(&mut self, rm: &ResourceManager, id: &Identifier) -> Result<Arc<RawModel>> {
        if let Some(m) = self.raw.get(id) {
            return Ok(m.clone());
        }
        let path = id.resource_path("models", ".json");
        let bytes = rm
            .read(&path)?
            .ok_or_else(|| AssetError::Missing(path.clone()))?;
        let model = Arc::new(parse_raw(&bytes, &path)?);
        self.raw.insert(id.clone(), model.clone());
        Ok(model)
    }

    pub fn resolve(&mut self, rm: &ResourceManager, id: &Identifier) -> Result<Arc<ResolvedModel>> {
        if let Some(m) = self.resolved.get(id) {
            return Ok(m.clone());
        }
        let mut chain = Vec::new();
        let mut current = Some(id.clone());
        let mut builtin = None;
        while let Some(cur) = current {
            if chain.len() >= MAX_PARENT_DEPTH {
                return Err(AssetError::ParentCycle(id.to_string()));
            }
            if is_builtin(&cur) {
                builtin = Some(Builtin::from_id(&cur));
                break;
            }
            let raw = self.raw(rm, &cur)?;
            current = raw.parent.clone();
            chain.push(raw);
        }

        let mut elements: Option<Arc<[Element]>> = None;
        let mut ambient_occlusion = None;
        let mut slots: HashMap<&str, &Slot> = HashMap::new();
        for raw in chain.iter() {
            if elements.is_none() {
                elements.clone_from(&raw.elements);
            }
            if ambient_occlusion.is_none() {
                ambient_occlusion = raw.ambient_occlusion;
            }
        }
        for raw in chain.iter().rev() {
            for (name, slot) in &raw.textures {
                slots.insert(name, slot);
            }
        }

        let mut textures: HashMap<Box<str>, Material> = HashMap::new();
        let mut pending: Vec<(&str, &str)> = Vec::new();
        for (name, slot) in &slots {
            match slot {
                Slot::Value(m) => {
                    textures.insert((*name).into(), m.clone());
                }
                Slot::Reference(target) => pending.push((name, target)),
            }
        }
        loop {
            let before = pending.len();
            pending.retain(|(name, target)| match textures.get(*target) {
                Some(m) => {
                    let m = m.clone();
                    textures.insert((*name).into(), m);
                    false
                }
                None => true,
            });
            if pending.is_empty() || pending.len() == before {
                break;
            }
        }

        let mut gui_light = None;
        let mut display = [None; 10];
        for raw in chain.iter() {
            gui_light = gui_light.or(raw.gui_light);
            for (context, transform) in &raw.display {
                display[*context as usize].get_or_insert(*transform);
            }
        }
        let default_light = if builtin == Some(Builtin::Generated) {
            GuiLight::Front
        } else {
            GuiLight::Side
        };
        let resolved = Arc::new(ResolvedModel {
            id: id.clone(),
            elements: elements.unwrap_or_else(|| Arc::from([])),
            ambient_occlusion: ambient_occlusion.unwrap_or(true),
            textures,
            gui_light: gui_light.unwrap_or(default_light),
            builtin,
            display,
        });
        self.resolved.insert(id.clone(), resolved.clone());
        Ok(resolved)
    }
}
