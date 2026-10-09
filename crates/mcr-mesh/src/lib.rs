//! Chunk meshing: bakes block states into render data and turns padded sections into `SectionMesh`es.

mod cache;
mod faces;
mod fluid;
mod input;
mod mesher;
mod pool;
pub mod testing;
mod tint;

pub use cache::{
    ATTR_AIR, ATTR_FULL_COLLISION, ATTR_HALF_TRANSPARENT, ATTR_LEAVES, ATTR_SOLID_RENDER, CubeFace,
    FluidInfo, FluidKind, FluidSprites, LAYER_NONE, ModelCache, NO_CULL, QuadDef, RenderLayer,
    Shape, StateModel,
};
pub use input::{MeshInput, PAD, PADDED, UNLIT_LIGHT, pidx};
pub use mesher::{MeshOptions, Mesher, Shading};
pub use pool::{MeshPool, MeshResult};
pub use tint::{
    BiomeClimate, BiomeColors, ColorMaps, DEFAULT_BLEND_RADIUS, GrassModifier, MAX_BLEND_RADIUS,
    PALETTE_LEN, TINT_CONST_BASE, TINT_DRY_FOLIAGE, TINT_FOLIAGE, TINT_GRASS, TINT_NONE,
    TINT_WATER, constant_palette, tint_kinds,
};
