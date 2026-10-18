mod anim;
mod assets;
mod catalog;
mod cube;
mod data;
mod display;
mod effects;
mod equipment;
mod extras;
mod first_person;
mod gpu;
mod hands;
mod humanoids;
pub mod idx;
mod items;
mod kf;
mod maps;
mod misc;
mod mobs;
mod model;
mod plan;
mod pose;
mod rig;
mod rig_humanoid;
mod rig_mobs;
mod scene;
mod skin;
mod special;
mod texcache;
mod view;

pub use anim::{BoundAnimation, catmull_rom, sample};
pub use assets::{
    EquipmentInfo, EquipmentLayer, Equippable, ImageRgba, MAX_EQUIPMENT_MISSES, SceneAssets,
    TrimOverride, load_jar_texture, missing_image,
};
pub use catalog::{Catalog, PlanCx, Simple, is_baby};
pub use cube::{Vertex, unwrap_cube};
pub use data::{
    AnimationDef, AnimationSet, ChannelDef, CubeDef, Interp, KeyframeDef, LayerDef, LayerSet,
    PartDef, RendererInfo, RendererSet, Target,
};
pub use display::{DisplaySample, DisplayTrack, PoseLerp, teleport_duration};
pub use gpu::{
    Bone34, CLASS_SIZES, EntityRenderer, LightInfo, MAX_BONES, MAX_INSTANCES, ModelId,
    OUTLINE_COLOR_FORMAT, OUTLINE_DEPTH_FORMAT, PartInstance, Pipeline, TargetFormats, TexId,
};
pub use humanoids::default_skin;
pub use items::clone_count;
pub use maps::{MapDecoration, stack_map_id};
pub use misc::{cluster_offset, display_local, hidden_while_fresh};
pub use model::{Bone, BoneId, Model};
pub use plan::{Attach, AttachKind, FreeGeo, ItemContext, ItemSource, Part, Plan};
pub use pose::{Pose, evaluate};
pub use rig::{
    AnimIn, ArmPose, Binding, HumanoidStyle, IllagerPose, PiglinPose, Rig, Slot, animate,
};
pub use scene::{EntityScene, FrameParams, NameTag, SceneStats, ShadowGround};
pub use texcache::TextureCache;
pub use view::{
    BedFacing, BodyVisibility, EntityData, EntityView, EventAnim, Glide, NoData, PlayerView,
    Registries, Skin, slot,
};

pub use glam;
pub use mcr_protocol;
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
    #[error("assets: {0}")]
    Assets(String),
    #[error("unsupported png color type {0:?}")]
    PngFormat(png::ColorType),
}
