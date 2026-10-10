//! Chunk meshing: bakes block states into render data and turns padded sections into `SectionMesh`es.

mod biome_zoom;
mod cache;
mod faces;
mod fluid;
mod input;
mod mesher;
mod pool;
mod random;
mod simplex;
pub mod testing;
mod tint;
mod visgraph;

pub use biome_zoom::zoom_biome;
pub use cache::{
    ATTR_AIR, ATTR_EMISSIVE, ATTR_FULL_COLLISION, ATTR_HALF_TRANSPARENT, ATTR_LEAVES,
    ATTR_LIGHT_OPAQUE, ATTR_SOLID, ATTR_SOLID_RENDER, CubeFace, FaceMask, FluidInfo, FluidKind,
    FluidSprites, LAYER_NONE, MASK_EMPTY, MASK_FULL, ModelCache, NO_CULL, QuadDef, RenderLayer,
    Shape, StateModel,
};
pub use input::{MeshInput, PAD, PADDED, UNLIT_LIGHT, pidx, quart_index};
pub use mesher::{MeshOptions, Mesher, Shading};
pub use pool::{MeshPool, MeshResult};
pub use random::{JavaRandom, OffsetSpec, block_offset, mth_get_seed, offset_spec};
pub use tint::{
    BiomeClimate, BiomeColors, ColorMaps, DEFAULT_BLEND_RADIUS, GrassModifier, MAX_BLEND_RADIUS,
    NO_Y, PALETTE_LEN, TINT_CONST_BASE, TINT_DRY_FOLIAGE, TINT_FOLIAGE, TINT_GRASS, TINT_NONE,
    TINT_WATER, constant_palette, tint_kinds,
};
