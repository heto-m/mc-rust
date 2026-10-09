mod bake;
mod blockstate;
mod error;
mod geometry;
mod identifier;
mod model;
mod pack;
mod resources;
mod source;
mod texture;

#[cfg(test)]
mod testutil;

pub use bake::{
    BakedModel, BakedQuad, BakedVariant, ModelBaker, ModelPart, SpriteId, SpriteTable, Vertex,
};
pub use blockstate::{BlockStateDef, Condition, Properties, Selector, Term, Variant};
pub use error::{AssetError, Result};
pub use geometry::Face;
pub use identifier::{DEFAULT_NAMESPACE, Identifier};
pub use model::{Element, FaceDef, Material, ModelLoader, ResolvedModel};
pub use pack::{FormatRange, Overlay, PackFormat, PackMeta};
pub use resources::ResourceManager;
pub use source::{AssetSource, DirSource, ZipSource};
pub use texture::{
    Animation, AnimationFrame, DEFAULT_MIP_LEVELS, Sprite, SpriteSlot, TextureArrayBuilder,
    TextureArrays, TextureGroup, Transparency, decode_sprite, missing_sprite,
};

#[cfg(test)]
mod fixture_tests;
