mod bake;
mod blockstate;
mod error;
mod geometry;
mod identifier;
mod item_bake;
mod item_def;
mod item_eval;
mod item_special;
mod model;
mod pack;
mod resources;
mod source;
mod texture;

#[cfg(test)]
mod testutil;

pub use bake::{
    BakedModel, BakedQuad, BakedVariant, FACE_VERTICES, ModelBaker, ModelPart, SpriteId,
    SpriteTable, Vertex,
};
pub use blockstate::{BlockStateDef, Condition, Properties, Selector, Term, Variant};
pub use error::{AssetError, Result};
pub use geometry::Face;
pub use identifier::{DEFAULT_NAMESPACE, Identifier};
pub use item_bake::{BakedItemModel, ItemBaker};
pub use item_def::{
    CompassTarget, ConditionProp, IDENTITY4, ItemModelDef, ItemNode, Mat4, NodeKind, RangeProp,
    SelectProp, SpecialKind, TimeSource, TintSource, mat4_apply, mat4_mul,
};
pub use item_eval::{
    ChargeType, CompassInputs, ItemContext, ItemLayer, ItemModels, ItemProps, LayerKind,
    LocalDateTime, PlainItem,
};
pub use model::{
    Builtin, DisplayContext, Element, FaceDef, GuiLight, ItemTransform, Material, ModelLoader,
    ResolvedModel,
};
pub use pack::{FormatRange, Overlay, PackFormat, PackMeta};
pub use resources::ResourceManager;
pub use source::{AssetSource, DirSource, ZipLimits, ZipSource};
pub use texture::{
    Animation, AnimationFrame, DEFAULT_MIP_LEVELS, Sprite, SpriteSlot, TextureArrayBuilder,
    TextureArrays, TextureGroup, Transparency, decode_sprite, missing_sprite,
};

#[cfg(test)]
mod fixture_tests;

#[cfg(test)]
mod item_tests;
