use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::Deserialize;

use crate::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct CubeDef {
    pub origin: [f32; 3],
    pub size: [f32; 3],
    pub uv: [f32; 2],
    pub grow: [f32; 3],
    pub mirror: bool,
    #[serde(default)]
    pub tex_scale: Option<[f32; 2]>,
    #[serde(default)]
    pub faces: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartDef {
    pub pose: [f32; 9],
    pub cubes: Vec<CubeDef>,
    pub children: BTreeMap<String, PartDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LayerDef {
    pub tex_w: u32,
    pub tex_h: u32,
    pub root: PartDef,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct LayerSet(pub HashMap<String, LayerDef>);

impl LayerSet {
    pub fn from_json(json: &str) -> Result<Self, Error> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    pub fn get(&self, layer: &str) -> Option<&LayerDef> {
        self.0.get(layer)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Position,
    Rotation,
    Scale,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Interp {
    Linear,
    Catmullrom,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyframeDef {
    pub t: f32,
    pub pre: [f32; 3],
    pub post: [f32; 3],
    pub interp: Interp,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelDef {
    pub target: Target,
    pub keyframes: Vec<KeyframeDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnimationDef {
    pub length: f32,
    pub looping: bool,
    pub bones: BTreeMap<String, Vec<ChannelDef>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct AnimationSet(pub HashMap<String, AnimationDef>);

impl AnimationSet {
    pub fn from_json(json: &str) -> Result<Self, Error> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    pub fn get(&self, name: &str) -> Option<&AnimationDef> {
        self.0.get(name)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RendererInfo {
    pub renderer: Option<String>,
    pub textures: Vec<String>,
    pub model_layers: Vec<String>,
    pub shadow_radius: Option<f32>,
    pub ctor_floats: Vec<f32>,
    pub dynamic_texture: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct RendererSet(pub HashMap<String, RendererInfo>);

impl RendererSet {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }
}
