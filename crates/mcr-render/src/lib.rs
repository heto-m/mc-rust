mod animation;
mod arena;
mod camera;
mod environment;
mod mesh;
mod overlay;
mod quad;
mod renderer;
mod staging;
pub mod synthetic;
mod texture;

pub use animation::{AnimationTable, SpriteAnimation, SpriteFrame};
pub use arena::Arena;
pub use camera::{Camera, Frustum};
pub use environment::{CARDINAL_DEFAULT, CARDINAL_NETHER, Environment, Rgb, hex, srgb_lerp};
pub use mesh::SectionMesh;
pub use mesh::{TINT_KINDS, TINT_TABLE_WORDS, TintTable};
pub use overlay::{Overlay, OverlayCtx};
pub use quad::{
    CubeQuad, Face, LIGHT_FULL_SKY, ModelQuad, ModelQuadDesc, PackedQuad, QuadError,
    cube_ao_brightness, light_block, light_sky, model_ao, pack_light,
};
pub use renderer::{
    FrameInputs, FrameStats, GpuTimes, MemoryStats, RenderError, Renderer, RendererConfig, Target,
};
pub use texture::{MAX_TEXTURE_GROUPS, TextureArrayData, TextureGroupData};

pub use glam;
pub use wgpu;

pub const QUAD_WGSL: &str = include_str!("shaders/quad.wgsl");
pub const DECODE_TEST_WGSL: &str = include_str!("shaders/decode_test.wgsl");
