mod anim;
mod assets;
mod cube;
mod data;
mod gpu;
mod model;
mod pose;
mod procedural;

pub use anim::{BoundAnimation, catmull_rom, sample};
pub use assets::{ImageRgba, load_jar_texture};
pub use cube::{Vertex, unwrap_cube};
pub use data::{
    AnimationDef, AnimationSet, ChannelDef, CubeDef, Interp, KeyframeDef, LayerDef, LayerSet,
    PartDef, RendererInfo, RendererSet, Target,
};
pub use gpu::{EntityRenderer, Instance, ModelHandle, TextureArrayBuilder, TextureLayer};
pub use model::{Bone, BoneId, Model};
pub use pose::{Pose, evaluate};
pub use procedural::{AnimState, Biped, Quadruped, Rig};

pub use glam;
pub use wgpu;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("png: {0}")]
    Png(#[from] png::DecodingError),
    #[error("unsupported png color type {0:?}")]
    PngFormat(png::ColorType),
}
