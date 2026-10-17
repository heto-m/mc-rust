mod animation;
mod arena;
mod camera;
mod counters;
mod effects;
mod environment;
mod external;
mod glow;
mod graph;
mod mesh;
mod occlusion;
mod overlay;
mod post;
mod quad;
mod renderer;
mod staging;
pub mod synthetic;
mod texture;

pub use animation::{AnimationTable, SpriteAnimation, SpriteFrame};
pub use arena::Arena;
pub use camera::{Camera, Frustum, beyond_render_fog};
pub use counters::GpuCounters;
pub use effects::{
    CloudMode, CloudSettings, DestroyDecal, EffectAssets, EffectImage, MAX_BOXES_PER_DECAL,
    MAX_DECALS, MAX_WEATHER_RADIUS, MIN_WEATHER_RADIUS, Precipitation, WeatherSource,
    WorldBorderVisual,
};
pub use environment::{
    CARDINAL_DEFAULT, CARDINAL_NETHER, Environment, OverworldBase, Rgb, Skybox, hex, srgb_lerp,
};
pub use external::{ExternalCtx, ExternalSubmitter};
pub use glow::GlowOutline;
pub use graph::{ReachedSections, SectionVisibility};
pub use mesh::SectionMesh;
pub use mesh::{TINT_KINDS, TINT_TABLE_WORDS, TintTable};
pub use overlay::{Overlay, OverlayCtx};
pub use post::{MAX_POST_CHAINS, PostChain, PostOverrides, PostStack};
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
