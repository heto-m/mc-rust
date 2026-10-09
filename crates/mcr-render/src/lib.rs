mod arena;
mod camera;
mod mesh;
mod quad;
mod renderer;
mod staging;
pub mod synthetic;
mod texture;

pub use arena::Arena;
pub use camera::{Camera, Frustum};
pub use mesh::SectionMesh;
pub use mesh::{TINT_KINDS, TINT_TABLE_WORDS, TintTable};
pub use quad::{CubeQuad, Face, ModelQuad, ModelQuadDesc, PackedQuad, QuadError};
pub use renderer::{
    FrameInputs, FrameStats, GpuTimes, MemoryStats, RenderError, Renderer, RendererConfig, Target,
};
pub use texture::{MAX_TEXTURE_GROUPS, TextureArrayData, TextureGroupData};

pub use glam;
pub use wgpu;

pub const QUAD_WGSL: &str = include_str!("shaders/quad.wgsl");
pub const DECODE_TEST_WGSL: &str = include_str!("shaders/decode_test.wgsl");
