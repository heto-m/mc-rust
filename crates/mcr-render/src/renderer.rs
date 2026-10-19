use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::animation::{ANIM_TABLE_LEN, AnimationTable, SpriteAnimation};
use crate::arena::Arena;
use crate::camera::Camera;
use crate::dynlight::{self, DynamicLight, GRID_CELLS, GpuLight, GpuLights};
use crate::effects::{
    CloudSettings, DestroyDecal, EffectAssets, Effects, WeatherSource, WorldBorderVisual,
};
use crate::environment::{Environment, Skybox};
use crate::external::{ExternalCtx, ExternalSubmitter, FrameBatch, PresentBlit};
use crate::graph::VisibilityGraph;
use crate::mesh::SectionMesh;
use crate::mesh::TINT_TABLE_WORDS;
use crate::occlusion::HiZ;
use crate::overlay::{Overlay, OverlayCtx};
use crate::post::{MAX_POST_CHAINS, PostChain, PostOverrides, PostStack};
use crate::quad::{ModelQuad, PackedQuad};
use crate::staging::{Dest, Uploader};
use crate::texture::{MAX_TEXTURE_GROUPS, TextureArrayData, TextureGroupData};

const QUAD_SRC: &str = include_str!("shaders/quad.wgsl");
const GLOBALS_SRC: &str = include_str!("shaders/globals.wgsl");
const TERRAIN_SRC: &str = include_str!("shaders/terrain.wgsl");
const DYNLIGHT_SRC: &str = include_str!("shaders/dynlight.wgsl");
const CULL_SRC: &str = include_str!("shaders/cull.wgsl");
const TRANS_CULL_SRC: &str = include_str!("shaders/trans_cull.wgsl");

pub(crate) const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const TICKS_PER_SECOND: f64 = 20.0;
const LIGHTMAP_BYTES: u64 = 256 * 16;
const READBACK_SLOTS: usize = 3;
const STATS_COUNTER_OFFSET: u64 = 256;
const COUNTERS_SIZE: u64 = 16;
const TIMESTAMPS: u32 = 8;
const STATS_SIZE: u64 = 512;
/// `DrawIndexedIndirect` arguments.
const DRAW_CMD_SIZE: u64 = 20;
/// Greedy meshing leaves at most one face plus one overlay per cell and direction.
const MAX_CUBE_QUADS_PER_DRAW: u32 = 2 * 16 * 16 * 16;
/// Bounds the shared quad index buffer; model or translucent quads past it in one section are dropped.
const MAX_QUADS_PER_DRAW: u32 = 1 << 17;
const DRAW_DATA_SIZE: u64 = 32;
const TINT_PALETTE_WORDS: usize = 64;
const NO_TINT_TABLE: u32 = u32::MAX;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("no suitable GPU adapter: {0}")]
    NoAdapter(String),
    #[error("device request failed: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("surface creation failed: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("adapter lacks required feature {0}")]
    MissingFeature(&'static str),
    #[error("{0} arena is full")]
    ArenaFull(&'static str),
    #[error("section table is full")]
    TooManySections,
    #[error("frame skipped: surface unavailable")]
    SurfaceUnavailable,
    #[error("offscreen readback requires an offscreen target")]
    NotOffscreen,
}

#[derive(Debug)]
pub enum Target {
    Offscreen {
        width: u32,
        height: u32,
    },
    Surface {
        target: wgpu::SurfaceTarget<'static>,
        width: u32,
        height: u32,
    },
}

#[derive(Debug, Clone)]
pub struct RendererConfig {
    pub cube_arena_quads: u32,
    pub model_arena_quads: u32,
    pub max_sections: u32,
    pub vsync: bool,
    pub upload_budget_bytes: usize,
    pub allow_draw_count: bool,
    pub tint_tables: u32,
    pub max_translucent_sections: u32,
    pub occlusion_culling: bool,
    pub visibility_graph: bool,
    /// Requested when the adapter has them (the mod bridge shares the device).
    pub optional_features: wgpu::Features,
    /// Render into an offscreen `Rgba8Unorm` scene that external work can sample, then blit it to
    /// the surface; also raises texture and uniform limits to the adapter's.
    pub scene_texture: bool,
    /// Builds the terrain variants that add [`DynamicLight`]s (mod mode); without it they are ignored.
    pub dynamic_lights: bool,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            cube_arena_quads: 16 << 20,
            model_arena_quads: 512 << 10,
            max_sections: 1 << 17,
            vsync: true,
            upload_budget_bytes: 64 << 20,
            allow_draw_count: true,
            tint_tables: 8192,
            max_translucent_sections: 16384,
            occlusion_culling: true,
            visibility_graph: true,
            optional_features: wgpu::Features::empty(),
            scene_texture: false,
            dynamic_lights: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FrameInputs<'a> {
    pub camera: &'a Camera,
    pub sky_light: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GpuTimes {
    pub cull_ms: f32,
    pub draw_ms: f32,
    /// From the start of the first culling pass to the end of the last world pass.
    pub frame_ms: f32,
    pub cube_draws: u32,
    pub model_draws: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FrameStats {
    pub cpu_ms: f32,
    /// Waiting for the swapchain image and handing it back, within `cpu_ms`.
    pub acquire_ms: f32,
    pub present_ms: f32,
    pub uploaded_bytes: usize,
    pub uploaded_writes: usize,
    pub pending_upload_bytes: usize,
    /// CPU time of the visibility graph search this frame; zero when it was reused.
    pub graph_ms: f32,
    pub gpu: Option<GpuTimes>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryStats {
    pub cube_arena_bytes: u64,
    pub model_arena_bytes: u64,
    pub cube_used_bytes: u64,
    pub model_used_bytes: u64,
    pub culling_bytes: u64,
    pub texture_bytes: u64,
    pub target_bytes: u64,
}

impl MemoryStats {
    pub fn total_bytes(&self) -> u64 {
        self.cube_arena_bytes
            + self.model_arena_bytes
            + self.culling_bytes
            + self.texture_bytes
            + self.target_bytes
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlobalsGpu {
    view_proj: [f32; 16],
    planes: [[f32; 4]; 5],
    cam_int: [i32; 4],
    cam_frac: [f32; 4],
    params: [u32; 4],
    sun: [f32; 4],
    inv_view_proj: [f32; 16],
    fog_color: [f32; 4],
    fog: [f32; 4],
    sky_color: [f32; 4],
    shade_a: [f32; 4],
    shade_b: [f32; 4],
    cull: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SectionMetaGpu {
    origin: [i32; 4],
    bases: [u32; 4],
    counts_a: [u32; 4],
    counts_b: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DrawDataGpu {
    origin: [f32; 3],
    first_quad: u32,
    tint_base: u32,
    pad: [u32; 3],
}

/// The translucent quads a section draws: `count` model quads from `base`, with its tint table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TranslucentDraw {
    base: u32,
    count: u32,
    tint_base: u32,
}

/// Switches a section's translucent draw once the upload batch holding its quads is written.
#[derive(Debug, Clone, Copy)]
struct TranslucentCommit {
    key: [i32; 3],
    serial: u64,
    draw: Option<TranslucentDraw>,
}

#[derive(Debug)]
struct SectionSlot {
    index: u32,
    /// Unique per slot lifetime, so commits queued before a removal never reach a later slot.
    serial: u64,
    cube: Option<(u32, u32)>,
    model: Option<(u32, u32)>,
    tint: Option<u32>,
    /// What the translucent pass draws: the last mesh whose upload was written.
    drawn: Option<TranslucentDraw>,
}

#[derive(Debug)]
struct Readback {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
    two_phase: bool,
    frame: u64,
}

const RB_IDLE: u8 = 0;
const RB_PENDING: u8 = 1;
const RB_READY: u8 = 2;

struct Timestamps {
    set: wgpu::QuerySet,
    period_ns: f32,
}

struct Surface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    view_format: wgpu::TextureFormat,
}

/// Shaders write gamma-encoded colour like vanilla, so the target must not re-encode it as sRGB.
fn surface_format(formats: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    use wgpu::TextureFormat as F;
    [
        F::Rgba8Unorm,
        F::Bgra8Unorm,
        F::Rgba8UnormSrgb,
        F::Bgra8UnormSrgb,
    ]
    .into_iter()
    .find(|f| formats.contains(f))
}

struct Geometry {
    sections: wgpu::Buffer,
    cube_arena: wgpu::Buffer,
    model_arena: wgpu::Buffer,
    cube_cmds: wgpu::Buffer,
    cube_draws: wgpu::Buffer,
    quad_indices: wgpu::Buffer,
    model_cmds: wgpu::Buffer,
    model_draws: wgpu::Buffer,
    counters: wgpu::Buffer,
    visibility: wgpu::Buffer,
    reachable: wgpu::Buffer,
    tints: wgpu::Buffer,
    trans_cmds: wgpu::Buffer,
    trans_draws: wgpu::Buffer,
    trans_count: wgpu::Buffer,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_info: wgpu::AdapterInfo,
    draw_count_supported: bool,
    config: RendererConfig,

    surface: Option<Surface>,
    offscreen: Option<wgpu::Texture>,
    /// Mod mode with a window: the frame renders here and is blitted to the surface.
    scene: Option<(wgpu::Texture, PresentBlit)>,
    external: Option<Box<dyn ExternalSubmitter + Send>>,
    batch: FrameBatch,
    post: PostStack,
    post_chains: Vec<PostChain>,
    menu_blur: u32,
    /// A window without a scene texture renders here while post chains run, then blits.
    post_window: Option<(wgpu::Texture, PresentBlit)>,
    size: (u32, u32),
    depth_view: wgpu::TextureView,
    hiz: HiZ,
    occlusion: bool,
    graph: VisibilityGraph,
    use_graph: bool,
    camera_in_opaque: bool,

    globals: wgpu::Buffer,
    geo: Geometry,
    cull_early: wgpu::ComputePipeline,
    cull_late: wgpu::ComputePipeline,
    trans_cull: wgpu::ComputePipeline,
    trans_cull_bg: wgpu::BindGroup,
    cull_bgl: wgpu::BindGroupLayout,
    cull_bg: wgpu::BindGroup,
    cube_pipeline: wgpu::RenderPipeline,
    model_pipeline: wgpu::RenderPipeline,
    trans_pipeline: wgpu::RenderPipeline,
    trans_geom_bg: wgpu::BindGroup,
    globals_bgl: wgpu::BindGroupLayout,
    globals_bg: wgpu::BindGroup,
    geom_bg: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    textures: Vec<wgpu::Texture>,
    dummy_texture: wgpu::Texture,
    tint_arena: Arena,
    lightmap: wgpu::Buffer,
    anim_buffer: wgpu::Buffer,
    animations: AnimationTable,
    anim_tick: Option<u64>,
    anim_applied_tick: Option<u64>,
    anim_start: Instant,
    environment: Environment,
    lightmap_key: Option<(Environment, f32)>,
    sky_pipeline: wgpu::RenderPipeline,
    effects: Effects,
    dyn_lights: Option<DynamicLights>,

    cube_arena: Arena,
    model_arena: Arena,
    sections: HashMap<[i32; 3], SectionSlot>,
    free_slots: Vec<u32>,
    slot_count: u32,
    uploader: Uploader<TranslucentCommit>,
    commits: Vec<TranslucentCommit>,
    next_serial: u64,

    timestamps: Option<Timestamps>,
    stats_gpu: wgpu::Buffer,
    readbacks: Vec<Readback>,
    readback_cursor: usize,
    latest_gpu: Option<GpuTimes>,
    latest_gpu_frame: u64,
    frame_index: u64,
    capture_requested: bool,
    capture_pending: Option<PendingCapture>,
    captured: Option<Capture>,
    screenshot_requested: bool,
    screenshot: Option<Capture>,
}

/// Terrain pipelines with the dynamic light term, its uniform and this frame's sources.
struct DynamicLights {
    cube: wgpu::RenderPipeline,
    model: wgpu::RenderPipeline,
    trans: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    grid_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    sources: Vec<DynamicLight>,
    scratch: Vec<(f32, GpuLight)>,
    gpu: Box<GpuLights>,
    grid: Vec<[u32; 2]>,
    /// This frame draws terrain with the variants.
    active: bool,
}

/// The world image of one frame, before UI: tightly packed 8-bit pixels in display encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    pub width: u32,
    pub height: u32,
    /// Channel order is blue, green, red, alpha instead of RGBA.
    pub bgra: bool,
    pub pixels: Vec<u8>,
}

struct PendingCapture {
    buffer: wgpu::Buffer,
    row: u32,
    width: u32,
    height: u32,
    bgra: bool,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("adapter", &self.adapter_info.name)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

fn storage_buffer(
    device: &wgpu::Device,
    label: &str,
    size: u64,
    extra: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | extra,
        mapped_at_creation: false,
    })
}

fn storage_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    read_only: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn depth_view(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_group_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    data: &TextureGroupData,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("block textures"),
        size: wgpu::Extent3d {
            width: data.width,
            height: data.height,
            depth_or_array_layers: data.layers,
        },
        mip_level_count: data.mips.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (mip, bytes) in data.mips.iter().enumerate() {
        let (w, h) = data.mip_size(mip);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: data.layers,
            },
        );
    }
    texture
}

fn group_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    groups: &[TextureGroupData],
) -> Vec<wgpu::Texture> {
    groups
        .iter()
        .take(MAX_TEXTURE_GROUPS)
        .map(|g| create_group_texture(device, queue, g))
        .collect()
}

impl Renderer {
    pub fn new(target: Target, config: RendererConfig) -> Result<Self, RenderError> {
        pollster::block_on(Self::new_async(target, config))
    }

    async fn new_async(target: Target, config: RendererConfig) -> Result<Self, RenderError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let (surface_raw, width, height) = match target {
            Target::Offscreen { width, height } => (None, width, height),
            Target::Surface {
                target,
                width,
                height,
            } => (Some(instance.create_surface(target)?), width, height),
        };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface_raw.as_ref(),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| RenderError::NoAdapter(e.to_string()))?;

        let have = adapter.features();
        if !have.contains(wgpu::Features::INDIRECT_FIRST_INSTANCE) {
            return Err(RenderError::MissingFeature("INDIRECT_FIRST_INSTANCE"));
        }
        let optional = wgpu::Features::MULTI_DRAW_INDIRECT_COUNT | wgpu::Features::TIMESTAMP_QUERY;
        let features = wgpu::Features::INDIRECT_FIRST_INSTANCE
            | (have & optional)
            | (have & config.optional_features);
        let al = adapter.limits();
        let mut limits = wgpu::Limits {
            max_storage_buffer_binding_size: al.max_storage_buffer_binding_size,
            max_buffer_size: al.max_buffer_size,
            max_texture_array_layers: al.max_texture_array_layers,
            ..wgpu::Limits::default()
        };
        if config.scene_texture {
            limits.max_uniform_buffer_binding_size = al.max_uniform_buffer_binding_size;
            limits.max_texture_dimension_2d = al.max_texture_dimension_2d;
            limits.max_color_attachments = al.max_color_attachments.min(8);
        }
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("mcr-render"),
                required_features: features,
                required_limits: limits,
                ..Default::default()
            })
            .await?;
        let adapter_info = adapter.get_info();

        let max_binding = al.max_storage_buffer_binding_size.min(al.max_buffer_size);
        let cube_quads = config
            .cube_arena_quads
            .min((max_binding / PackedQuad::SIZE as u64) as u32);
        let model_quads = config
            .model_arena_quads
            .min((max_binding / ModelQuad::SIZE as u64) as u32);

        let surface = surface_raw.map(|surface| {
            let mut cfg = surface
                .get_default_config(&adapter, width, height)
                .expect("adapter supports surface");
            let formats = surface.get_capabilities(&adapter).formats;
            cfg.format = surface_format(&formats).unwrap_or(cfg.format);
            let view_format = cfg.format.remove_srgb_suffix();
            cfg.view_formats = if view_format == cfg.format {
                Vec::new()
            } else {
                vec![view_format]
            };
            cfg.present_mode = if config.vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            };
            cfg.desired_maximum_frame_latency = 2;
            surface.configure(&device, &cfg);
            Surface {
                surface,
                config: cfg,
                view_format,
            }
        });
        let scene = surface.as_ref().filter(|_| config.scene_texture).map(|s| {
            (
                create_offscreen(&device, width, height),
                PresentBlit::new(&device, s.view_format),
            )
        });
        let color_format = match &surface {
            Some(s) if scene.is_none() => s.view_format,
            _ => COLOR_FORMAT,
        };
        let offscreen = surface
            .is_none()
            .then(|| create_offscreen(&device, width, height));
        let post = PostStack::new(&device, color_format);

        let max_sections = u64::from(config.max_sections);
        let geo = Geometry {
            sections: storage_buffer(
                &device,
                "sections",
                max_sections * 64,
                wgpu::BufferUsages::empty(),
            ),
            cube_arena: storage_buffer(
                &device,
                "cube arena",
                u64::from(cube_quads) * PackedQuad::SIZE as u64,
                wgpu::BufferUsages::empty(),
            ),
            model_arena: storage_buffer(
                &device,
                "model arena",
                u64::from(model_quads) * ModelQuad::SIZE as u64,
                wgpu::BufferUsages::empty(),
            ),
            cube_cmds: storage_buffer(
                &device,
                "cube cmds",
                2 * max_sections * 6 * DRAW_CMD_SIZE,
                wgpu::BufferUsages::INDIRECT,
            ),
            quad_indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("quad indices"),
                contents: bytemuck::cast_slice(&quad_indices(MAX_QUADS_PER_DRAW)),
                usage: wgpu::BufferUsages::INDEX,
            }),
            cube_draws: storage_buffer(
                &device,
                "cube draws",
                max_sections * 6 * DRAW_DATA_SIZE,
                wgpu::BufferUsages::empty(),
            ),
            model_cmds: storage_buffer(
                &device,
                "model cmds",
                2 * max_sections * DRAW_CMD_SIZE,
                wgpu::BufferUsages::INDIRECT,
            ),
            model_draws: storage_buffer(
                &device,
                "model draws",
                max_sections * DRAW_DATA_SIZE,
                wgpu::BufferUsages::empty(),
            ),
            counters: storage_buffer(
                &device,
                "counters",
                COUNTERS_SIZE,
                wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_SRC,
            ),
            reachable: storage_buffer(
                &device,
                "graph reachable",
                u64::from(config.max_sections.div_ceil(32)) * 4,
                wgpu::BufferUsages::empty(),
            ),
            visibility: storage_buffer(
                &device,
                "section visibility",
                max_sections * 4,
                wgpu::BufferUsages::empty(),
            ),
            tints: storage_buffer(
                &device,
                "tints",
                (TINT_PALETTE_WORDS as u64
                    + u64::from(config.tint_tables) * TINT_TABLE_WORDS as u64)
                    * 4,
                wgpu::BufferUsages::empty(),
            ),
            trans_cmds: storage_buffer(
                &device,
                "translucent cmds",
                u64::from(config.max_translucent_sections) * DRAW_CMD_SIZE,
                wgpu::BufferUsages::INDIRECT,
            ),
            trans_draws: storage_buffer(
                &device,
                "translucent draws",
                u64::from(config.max_translucent_sections) * DRAW_DATA_SIZE,
                wgpu::BufferUsages::empty(),
            ),
            trans_count: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("translucent count"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        };

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: size_of::<GlobalsGpu>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let cs = wgpu::ShaderStages::COMPUTE;
        let cull_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cull"),
            entries: &[
                uniform_entry(0, cs),
                storage_entry(1, cs, true),
                storage_entry(2, cs, false),
                storage_entry(3, cs, false),
                storage_entry(4, cs, false),
                storage_entry(5, cs, false),
                storage_entry(6, cs, false),
                storage_entry(7, cs, false),
                storage_entry(9, cs, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: cs,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let depth_view = depth_view(&device, width, height);
        let hiz = HiZ::new(&device, &depth_view, width, height);
        let cull_bg = make_cull_bg(&device, &cull_bgl, &globals, &geo, &hiz);
        let cull_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cull"),
            source: wgpu::ShaderSource::Wgsl([GLOBALS_SRC, CULL_SRC].concat().into()),
        });
        let cull_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cull"),
            bind_group_layouts: &[Some(&cull_bgl)],
            immediate_size: 0,
        });
        let cull_pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&cull_layout),
                module: &cull_module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let cull_early = cull_pipeline("cull_early");
        let cull_late = cull_pipeline("cull_late");
        let trans_cull = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("cull translucent"),
            layout: None,
            module: &device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("cull translucent"),
                source: wgpu::ShaderSource::Wgsl([GLOBALS_SRC, TRANS_CULL_SRC].concat().into()),
            }),
            entry_point: Some("cull_translucent"),
            compilation_options: Default::default(),
            cache: None,
        });
        let trans_cull_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cull translucent"),
            layout: &trans_cull.get_bind_group_layout(0),
            entries: &[
                bind(0, &geo.trans_count),
                bind(1, &geo.visibility),
                bind(2, &geo.trans_cmds),
                bind(3, &geo.trans_draws),
            ],
        });

        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        };
        let globals_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture_entry(2),
                texture_entry(3),
                texture_entry(4),
                texture_entry(5),
                uniform_entry(6, wgpu::ShaderStages::VERTEX_FRAGMENT),
                storage_entry(7, wgpu::ShaderStages::FRAGMENT, true),
            ],
        });
        let vs = wgpu::ShaderStages::VERTEX;
        let geom_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("geometry"),
            entries: &[
                storage_entry(0, vs, true),
                storage_entry(1, vs, true),
                storage_entry(2, vs, true),
                storage_entry(3, vs, true),
                storage_entry(4, vs | wgpu::ShaderStages::FRAGMENT, true),
            ],
        });
        let geom_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("geometry"),
            layout: &geom_bgl,
            entries: &[
                bind(0, &geo.cube_arena),
                bind(1, &geo.model_arena),
                bind(2, &geo.cube_draws),
                bind(3, &geo.model_draws),
                bind(4, &geo.tints),
            ],
        });
        let trans_geom_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("translucent geometry"),
            layout: &geom_bgl,
            entries: &[
                bind(0, &geo.cube_arena),
                bind(1, &geo.model_arena),
                bind(2, &geo.trans_draws),
                bind(3, &geo.trans_draws),
                bind(4, &geo.tints),
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blocks"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let dummy_texture =
            create_group_texture(&device, &queue, &TextureGroupData::single_pixel());
        let textures = group_textures(
            &device,
            &queue,
            &[TextureArrayData::placeholder(16).into_group()],
        );
        let lightmap = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lightmap"),
            size: LIGHTMAP_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let anim_buffer = storage_buffer(
            &device,
            "animation table",
            ANIM_TABLE_LEN as u64 * 8,
            wgpu::BufferUsages::empty(),
        );
        queue.write_buffer(
            &anim_buffer,
            0,
            bytemuck::cast_slice(&AnimationTable::identity_table()),
        );
        let globals_bg = make_globals_bg(
            &device,
            &globals_bgl,
            &Bindings {
                globals: &globals,
                lightmap: &lightmap,
                anim: &anim_buffer,
            },
            &textures,
            &dummy_texture,
            &sampler,
        );

        let terrain_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain"),
            source: wgpu::ShaderSource::Wgsl([QUAD_SRC, GLOBALS_SRC, TERRAIN_SRC].concat().into()),
        });
        let draw_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[Some(&globals_bgl), Some(&geom_bgl)],
            immediate_size: 0,
        });
        let terrain_pipeline = |module: &wgpu::ShaderModule,
                                layout: &wgpu::PipelineLayout,
                                label: &str,
                                vs_entry: &str,
                                fs_entry: &str,
                                cull: Option<wgpu::Face>,
                                blend: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some(vs_entry),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(!blend),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(fs_entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend: blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let make_pipeline =
            |label: &str, vs_entry: &str, fs_entry: &str, cull: Option<wgpu::Face>, blend: bool| {
                terrain_pipeline(
                    &terrain_module,
                    &draw_layout,
                    label,
                    vs_entry,
                    fs_entry,
                    cull,
                    blend,
                )
            };
        let cube_pipeline =
            make_pipeline("cube", "vs_cube", "fs_main", Some(wgpu::Face::Back), false);
        let model_pipeline = make_pipeline(
            "model",
            "vs_model",
            "fs_main",
            Some(wgpu::Face::Back),
            false,
        );
        let trans_pipeline = make_pipeline(
            "translucent",
            "vs_model",
            "fs_translucent",
            Some(wgpu::Face::Back),
            true,
        );
        let dyn_lights = config.dynamic_lights.then(|| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("terrain dynamic lights"),
                source: wgpu::ShaderSource::Wgsl(
                    [QUAD_SRC, GLOBALS_SRC, TERRAIN_SRC, DYNLIGHT_SRC]
                        .concat()
                        .into(),
                ),
            });
            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("dynamic lights"),
                entries: &[
                    uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
                    storage_entry(1, wgpu::ShaderStages::FRAGMENT, true),
                ],
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("terrain dynamic lights"),
                bind_group_layouts: &[Some(&globals_bgl), Some(&geom_bgl), Some(&bgl)],
                immediate_size: 0,
            });
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("dynamic lights"),
                size: std::mem::size_of::<GpuLights>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let grid_buffer = storage_buffer(
                &device,
                "dynamic light grid",
                (GRID_CELLS * 8) as u64,
                wgpu::BufferUsages::empty(),
            );
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("dynamic lights"),
                layout: &bgl,
                entries: &[bind(0, &buffer), bind(1, &grid_buffer)],
            });
            let back = Some(wgpu::Face::Back);
            let pipeline = |label, vs, fs, blend| {
                terrain_pipeline(&module, &layout, label, vs, fs, back, blend)
            };
            DynamicLights {
                cube: pipeline("cube dynamic lights", "vs_cube_dyn", "fs_main_dyn", false),
                model: pipeline("model dynamic lights", "vs_model_dyn", "fs_main_dyn", false),
                trans: pipeline(
                    "translucent dynamic lights",
                    "vs_model_dyn",
                    "fs_translucent_dyn",
                    true,
                ),
                buffer,
                grid_buffer,
                bind_group,
                sources: Vec::new(),
                scratch: Vec::new(),
                gpu: Box::default(),
                grid: vec![[0; 2]; GRID_CELLS],
                active: false,
            }
        });
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(&draw_layout),
            vertex: wgpu::VertexState {
                module: &terrain_module,
                entry_point: Some("vs_sky"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &terrain_module,
                entry_point: Some("fs_sky"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let effects = Effects::new(
            &device,
            &queue,
            color_format,
            DEPTH_FORMAT,
            &globals_bgl,
            GLOBALS_SRC,
        );

        let timestamps = features
            .contains(wgpu::Features::TIMESTAMP_QUERY)
            .then(|| Timestamps {
                set: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("frame timestamps"),
                    ty: wgpu::QueryType::Timestamp,
                    count: TIMESTAMPS,
                }),
                period_ns: queue.get_timestamp_period(),
            });
        let stats_gpu = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stats"),
            size: STATS_SIZE,
            usage: wgpu::BufferUsages::QUERY_RESOLVE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let readbacks = (0..READBACK_SLOTS)
            .map(|_| Readback {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("stats readback"),
                    size: STATS_SIZE,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                state: Arc::new(AtomicU8::new(RB_IDLE)),
                two_phase: false,
                frame: 0,
            })
            .collect();

        tracing::info!(adapter = %adapter_info.name, backend = ?adapter_info.backend, ?features, "renderer ready");
        Ok(Self {
            draw_count_supported: config.allow_draw_count
                && features.contains(wgpu::Features::MULTI_DRAW_INDIRECT_COUNT),
            tint_arena: Arena::new(config.tint_tables),
            lightmap,
            anim_buffer,
            animations: AnimationTable::new(),
            anim_tick: None,
            anim_applied_tick: None,
            anim_start: Instant::now(),
            environment: Environment::default(),
            lightmap_key: None,
            sky_pipeline,
            effects,
            dyn_lights,
            cube_arena: Arena::new(cube_quads),
            model_arena: Arena::new(model_quads),
            occlusion: config.occlusion_culling,
            graph: VisibilityGraph::default(),
            use_graph: config.visibility_graph,
            camera_in_opaque: false,
            depth_view,
            hiz,
            size: (width, height),
            device,
            queue,
            adapter_info,
            config,
            surface,
            offscreen,
            scene,
            external: None,
            batch: FrameBatch::default(),
            post,
            post_chains: Vec::new(),
            menu_blur: 5,
            post_window: None,
            globals,
            geo,
            cull_early,
            cull_late,
            trans_cull,
            trans_cull_bg,
            cull_bgl,
            cull_bg,
            cube_pipeline,
            model_pipeline,
            trans_pipeline,
            trans_geom_bg,
            globals_bgl,
            globals_bg,
            geom_bg,
            sampler,
            textures,
            dummy_texture,
            sections: HashMap::new(),
            free_slots: Vec::new(),
            slot_count: 0,
            uploader: Uploader::default(),
            commits: Vec::new(),
            next_serial: 0,
            timestamps,
            stats_gpu,
            readbacks,
            readback_cursor: 0,
            latest_gpu: None,
            latest_gpu_frame: 0,
            frame_index: 0,
            capture_requested: false,
            capture_pending: None,
            captured: None,
            screenshot_requested: false,
            screenshot: None,
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn gpu_counters(&self) -> crate::GpuCounters {
        crate::GpuCounters::read(&self.device)
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    pub fn uses_draw_count(&self) -> bool {
        self.draw_count_supported
    }

    pub fn has_timestamps(&self) -> bool {
        self.timestamps.is_some()
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    pub fn cube_quads_used(&self) -> u32 {
        self.cube_arena.used()
    }

    pub fn model_quads_used(&self) -> u32 {
        self.model_arena.used()
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        if let Some(s) = &mut self.surface {
            s.config.present_mode = if vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            };
            s.surface.configure(&self.device, &s.config);
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        self.size = (width, height);
        if let Some(s) = &mut self.surface {
            s.config.width = width;
            s.config.height = height;
            s.surface.configure(&self.device, &s.config);
        } else {
            self.offscreen = Some(create_offscreen(&self.device, width, height));
        }
        if let Some((scene, _)) = &mut self.scene {
            *scene = create_offscreen(&self.device, width, height);
        }
        self.post.release();
        self.post_window = None;
        self.depth_view = depth_view(&self.device, width, height);
        self.hiz
            .resize(&self.device, &self.depth_view, width, height);
        self.cull_bg = make_cull_bg(
            &self.device,
            &self.cull_bgl,
            &self.globals,
            &self.geo,
            &self.hiz,
        );
    }

    /// Two-phase Hi-Z occlusion culling of sections; off draws every frustum-visible section.
    pub fn set_occlusion_culling(&mut self, enabled: bool) {
        self.occlusion = enabled;
    }

    pub fn occlusion_culling(&self) -> bool {
        self.occlusion
    }

    /// Section connectivity search from the camera; sections it cannot reach are not drawn.
    pub fn set_visibility_graph(&mut self, enabled: bool) {
        self.use_graph = enabled;
        self.graph.invalidate();
    }

    pub fn visibility_graph(&self) -> bool {
        self.use_graph
    }

    /// The sections the visibility graph reached from the last frame's camera; `None` while the
    /// graph is off or skipped (camera inside an opaque block).
    pub fn reached_sections(&self) -> Option<crate::graph::ReachedSections<'_>> {
        (self.use_graph && !self.camera_in_opaque)
            .then(|| self.graph.reached())
            .flatten()
    }

    /// A camera inside an opaque block (spectator) sees through walls the graph would treat as
    /// closed, so the graph search is skipped like vanilla does.
    pub fn set_camera_in_opaque_block(&mut self, inside: bool) {
        self.camera_in_opaque = inside;
    }

    pub fn set_block_textures(&mut self, data: &TextureArrayData) {
        self.set_texture_groups(&[data.clone().into_group()]);
    }

    /// Up to four size groups; a quad's layer field is `group << 14 | layer`.
    pub fn set_texture_groups(&mut self, groups: &[TextureGroupData]) {
        self.textures = group_textures(&self.device, &self.queue, groups);
        let animations = groups
            .iter()
            .flat_map(|g| g.animations.iter().cloned())
            .collect();
        self.set_animations(animations);
        self.globals_bg = make_globals_bg(
            &self.device,
            &self.globals_bgl,
            &Bindings {
                globals: &self.globals,
                lightmap: &self.lightmap,
                anim: &self.anim_buffer,
            },
            &self.textures,
            &self.dummy_texture,
            &self.sampler,
        );
    }

    /// Sky, fog and lighting inputs; takes effect on the next frame without remeshing.
    pub fn set_environment(&mut self, environment: &Environment) {
        if self.environment != *environment {
            self.environment = environment.clone();
        }
    }

    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    /// Uploads sun, moon, end sky, cloud, rain, snow and destroy-stage textures; effects stay hidden until
    /// this is called.
    pub fn set_effect_assets(&mut self, assets: &EffectAssets) {
        self.effects.set_assets(assets);
    }

    pub fn set_cloud_settings(&mut self, settings: CloudSettings) {
        self.effects.set_cloud_settings(settings);
    }

    /// Game time in ticks plus the partial tick; drives cloud scroll and precipitation animation.
    pub fn set_effect_time(&mut self, game_time: u64, partial: f32) {
        self.effects.set_time(game_time, partial);
    }

    /// Rebuilds the precipitation columns around `camera`; call every frame while
    /// `Environment::rain_level` is positive. `radius` is the weather radius option (3 to 10).
    pub fn update_weather(&mut self, camera: &Camera, radius: u32, source: &dyn WeatherSource) {
        let env = self.environment.clone();
        self.effects
            .update_weather(&env, camera.position.to_array(), radius, source);
    }

    /// Blocks currently being broken (own or other players'); replaces the previous list.
    pub fn set_destroy_decals(&mut self, decals: &[DestroyDecal]) {
        self.effects.set_decals(decals);
    }

    /// Hides the rain and snow columns while the rain level still darkens sky and fog.
    pub fn set_weather_hidden(&mut self, hidden: bool) {
        self.effects.set_weather_hidden(hidden);
    }

    /// The world border walls, drawn when the camera is within `distance` blocks of one.
    pub fn set_world_border(&mut self, border: Option<WorldBorderVisual>, distance: f64) {
        self.effects.set_border(border, distance);
    }

    /// Animated sprites (packed layer plus frame list); the table is evaluated at the current tick.
    pub fn set_animations(&mut self, animations: Vec<SpriteAnimation>) {
        let tick = self.current_tick();
        let table = self.animations.set(animations, tick);
        self.queue
            .write_buffer(&self.anim_buffer, 0, bytemuck::cast_slice(table));
        self.anim_applied_tick = Some(tick);
    }

    /// Pins the animation clock to a client tick count; without it a wall clock at 20 ticks per second
    /// is used.
    pub fn set_animation_tick(&mut self, tick: u64) {
        self.anim_tick = Some(tick);
    }

    fn current_tick(&self) -> u64 {
        self.anim_tick
            .unwrap_or_else(|| (self.anim_start.elapsed().as_secs_f64() * TICKS_PER_SECOND) as u64)
    }

    fn update_animations(&mut self) {
        let tick = self.current_tick();
        if self.anim_applied_tick == Some(tick) {
            return;
        }
        self.anim_applied_tick = Some(tick);
        for (index, entry) in self.animations.update(tick) {
            self.queue.write_buffer(
                &self.anim_buffer,
                index as u64 * 8,
                bytemuck::bytes_of(&entry),
            );
        }
    }

    fn update_lightmap(&mut self, sky_scale: f32) {
        let key = (self.environment.clone(), sky_scale);
        if self.lightmap_key.as_ref() == Some(&key) {
            return;
        }
        let map = self.environment.lightmap(sky_scale);
        self.queue
            .write_buffer(&self.lightmap, 0, bytemuck::cast_slice(&map));
        self.lightmap_key = Some(key);
    }

    /// Colours (`r | g << 8 | b << 16`) for constant tint kinds 8 and up, indexed `kind - 8`.
    /// This frame's mod light sources in world space; ignored unless built with
    /// [`RendererConfig::dynamic_lights`].
    pub fn set_dynamic_lights(&mut self, lights: &[DynamicLight]) {
        if let Some(d) = &mut self.dyn_lights {
            d.sources.clear();
            d.sources.extend_from_slice(lights);
        }
    }

    /// Dynamic light sources the last frame's terrain evaluated.
    pub fn dynamic_lights_drawn(&self) -> usize {
        self.dyn_lights
            .as_ref()
            .filter(|d| d.active)
            .map_or(0, |d| d.gpu.count())
    }

    pub fn set_tint_palette(&mut self, colors: &[u32]) {
        let mut words = [0u32; TINT_PALETTE_WORDS];
        let n = colors.len().min(TINT_PALETTE_WORDS);
        words[..n].copy_from_slice(&colors[..n]);
        self.queue
            .write_buffer(&self.geo.tints, 0, bytemuck::cast_slice(&words));
    }

    pub fn memory_stats(&self) -> MemoryStats {
        let texture_bytes: u64 = self
            .textures
            .iter()
            .map(|tex| {
                let texels: u64 = (0..tex.mip_level_count())
                    .map(|m| {
                        u64::from((tex.width() >> m).max(1)) * u64::from((tex.height() >> m).max(1))
                    })
                    .sum();
                texels * 4 * u64::from(tex.depth_or_array_layers())
            })
            .sum();
        let (w, h) = self.size;
        let g = &self.geo;
        MemoryStats {
            cube_arena_bytes: g.cube_arena.size(),
            model_arena_bytes: g.model_arena.size(),
            cube_used_bytes: u64::from(self.cube_arena.used()) * PackedQuad::SIZE as u64,
            model_used_bytes: u64::from(self.model_arena.used()) * ModelQuad::SIZE as u64,
            culling_bytes: [
                &g.sections,
                &g.cube_cmds,
                &g.cube_draws,
                &g.model_cmds,
                &g.model_draws,
                &g.visibility,
                &g.reachable,
                &g.tints,
                &g.trans_cmds,
                &g.trans_draws,
            ]
            .iter()
            .map(|b| b.size())
            .sum::<u64>()
                + self.hiz.bytes(),
            texture_bytes,
            target_bytes: u64::from(w) * u64::from(h) * 8,
        }
    }

    pub fn allocator_report(&self) -> Option<wgpu::AllocatorReport> {
        self.device.generate_allocator_report()
    }

    /// Replaces (or inserts) the geometry of the section at `key` (section coordinates).
    pub fn set_section(&mut self, key: [i32; 3], mesh: &SectionMesh) -> Result<(), RenderError> {
        self.graph.set(key, mesh.visibility);
        if mesh.is_empty() {
            self.remove_geometry(key);
            return Ok(());
        }
        let face_quads = mesh
            .faces
            .each_ref()
            .map(|q| &q[..q.len().min(MAX_CUBE_QUADS_PER_DRAW as usize)]);
        let cube_total = face_quads.iter().map(|q| q.len() as u32).sum::<u32>();
        let bounded = |q: &[ModelQuad]| q.len().min(MAX_QUADS_PER_DRAW as usize);
        let model_quads = &mesh.model[..bounded(&mesh.model)];
        let translucent_quads = &mesh.translucent[..bounded(&mesh.translucent)];
        let model_main = model_quads.len() as u32;
        let translucent_total = translucent_quads.len() as u32;
        let model_total = model_main + translucent_total;

        let cube = if cube_total > 0 {
            Some((
                self.cube_arena
                    .alloc(cube_total)
                    .ok_or(RenderError::ArenaFull("cube"))?,
                cube_total,
            ))
        } else {
            None
        };
        let free_cube = |this: &mut Self| {
            if let Some((o, n)) = cube {
                this.cube_arena.free(o, n);
            }
        };
        let model = if model_total > 0 {
            match self.model_arena.alloc(model_total) {
                Some(o) => Some((o, model_total)),
                None => {
                    free_cube(self);
                    return Err(RenderError::ArenaFull("model"));
                }
            }
        } else {
            None
        };
        let tint = match &mesh.tints {
            Some(_) => match self.tint_arena.alloc(1) {
                Some(i) => Some(i),
                None => {
                    free_cube(self);
                    if let Some((o, n)) = model {
                        self.model_arena.free(o, n);
                    }
                    return Err(RenderError::ArenaFull("tint"));
                }
            },
            None => None,
        };

        let old = self.sections.remove(&key);
        let index = match &old {
            Some(slot) => slot.index,
            None => match self.free_slots.pop() {
                Some(i) => i,
                None if self.slot_count < self.config.max_sections => {
                    self.slot_count += 1;
                    self.slot_count - 1
                }
                None => {
                    free_cube(self);
                    if let Some((o, n)) = model {
                        self.model_arena.free(o, n);
                    }
                    if let Some(t) = tint {
                        self.tint_arena.free(t, 1);
                    }
                    return Err(RenderError::TooManySections);
                }
            },
        };
        let (serial, drawn) = match &old {
            Some(old) => (old.serial, old.drawn),
            None => {
                self.next_serial += 1;
                (self.next_serial, None)
            }
        };
        if let Some(old) = old {
            self.release(&old);
        }

        let mut counts = [0u32; 6];
        if let Some((base, _)) = cube {
            let mut dst = u64::from(base) * PackedQuad::SIZE as u64;
            for (face, quads) in face_quads.iter().enumerate() {
                counts[face] = quads.len() as u32;
                let bytes: &[u8] = bytemuck::cast_slice(quads);
                self.uploader.push(Dest::Cube, dst, bytes);
                dst += bytes.len() as u64;
            }
        }
        if let Some((base, _)) = model {
            let at = u64::from(base) * ModelQuad::SIZE as u64;
            self.uploader
                .push(Dest::Model, at, bytemuck::cast_slice(model_quads));
            self.uploader.push(
                Dest::Model,
                at + u64::from(model_main) * ModelQuad::SIZE as u64,
                bytemuck::cast_slice(translucent_quads),
            );
        }
        let tint_base = tint.map_or(NO_TINT_TABLE, |t| {
            (TINT_PALETTE_WORDS + t as usize * TINT_TABLE_WORDS) as u32
        });
        if let (Some(t), Some(table)) = (tint, &mesh.tints) {
            self.uploader.push(
                Dest::Tints,
                (TINT_PALETTE_WORDS + t as usize * TINT_TABLE_WORDS) as u64 * 4,
                bytemuck::cast_slice(table.as_slice()),
            );
        }
        let meta = SectionMetaGpu {
            origin: [key[0] * 16, key[1] * 16, key[2] * 16, 1],
            bases: [
                cube.map_or(0, |c| c.0),
                model.map_or(0, |m| m.0),
                model_main,
                tint_base,
            ],
            counts_a: [counts[0], counts[1], counts[2], counts[3]],
            counts_b: [counts[4], counts[5], 0, 0],
        };
        self.uploader.push(
            Dest::Sections,
            u64::from(index) * 64,
            bytemuck::bytes_of(&meta),
        );
        let draw = model
            .filter(|_| translucent_total > 0)
            .map(|(base, _)| TranslucentDraw {
                base: base + model_main,
                count: translucent_total,
                tint_base,
            });
        self.uploader
            .end_batch_with(TranslucentCommit { key, serial, draw });
        self.graph.set_slot(key, Some(index));
        self.sections.insert(
            key,
            SectionSlot {
                index,
                serial,
                cube,
                model,
                tint,
                drawn,
            },
        );
        Ok(())
    }

    fn release(&mut self, slot: &SectionSlot) {
        if let Some((o, n)) = slot.cube {
            self.cube_arena.free(o, n);
        }
        if let Some((o, n)) = slot.model {
            self.model_arena.free(o, n);
        }
        if let Some(t) = slot.tint {
            self.tint_arena.free(t, 1);
        }
    }

    /// Drops every section and visibility node, whatever world or height range they came from.
    pub fn clear_sections(&mut self) {
        let keys: Vec<[i32; 3]> = self.sections.keys().copied().collect();
        for key in keys {
            self.remove_geometry(key);
        }
        self.graph.clear();
    }

    pub fn graph_nodes(&self) -> usize {
        self.graph.len()
    }

    pub fn remove_section(&mut self, key: [i32; 3]) {
        self.graph.remove(key);
        self.remove_geometry(key);
    }

    fn remove_geometry(&mut self, key: [i32; 3]) {
        let Some(slot) = self.sections.remove(&key) else {
            return;
        };
        self.graph.set_slot(key, None);
        self.release(&slot);
        self.uploader.push(
            Dest::Sections,
            u64::from(slot.index) * 64,
            bytemuck::bytes_of(&SectionMetaGpu::zeroed()),
        );
        self.uploader.end_batch();
        self.free_slots.push(slot.index);
    }

    fn flush_uploads(&mut self) -> (usize, usize) {
        let (queue, geo) = (&self.queue, &self.geo);
        let f = self.uploader.flush(
            self.config.upload_budget_bytes,
            |dest, offset, bytes| {
                let buffer = match dest {
                    Dest::Cube => &geo.cube_arena,
                    Dest::Model => &geo.model_arena,
                    Dest::Sections => &geo.sections,
                    Dest::Tints => &geo.tints,
                };
                queue.write_buffer(buffer, offset, bytes);
            },
            &mut self.commits,
        );
        for c in self.commits.drain(..) {
            if let Some(slot) = self.sections.get_mut(&c.key)
                && slot.serial == c.serial
            {
                slot.drawn = c.draw;
            }
        }
        (f.bytes, f.writes)
    }

    /// Writes back-to-front draws for sections with translucent quads and returns their count.
    fn build_translucent(
        &mut self,
        frustum: &crate::Frustum,
        cam_int: [i32; 3],
        cam_frac: glam::Vec3,
        graph: bool,
        fog_end: f32,
    ) -> u32 {
        let mut visible: Vec<(f32, glam::Vec3, u32, u32, u32, u32)> = Vec::new();
        for (key, slot) in &self.sections {
            let Some(TranslucentDraw {
                base,
                count,
                tint_base,
            }) = slot.drawn
            else {
                continue;
            };
            if graph && !self.graph.reachable(slot.index) {
                continue;
            }
            let lo = glam::Vec3::new(
                (key[0] * 16 - cam_int[0]) as f32,
                (key[1] * 16 - cam_int[1]) as f32,
                (key[2] * 16 - cam_int[2]) as f32,
            ) - cam_frac;
            if !frustum.intersects_aabb(lo, lo + glam::Vec3::splat(16.0))
                || crate::beyond_render_fog(lo, lo + glam::Vec3::splat(16.0), fog_end)
            {
                continue;
            }
            visible.push((
                (lo + glam::Vec3::splat(8.0)).length_squared(),
                lo,
                base,
                count,
                tint_base,
                slot.index,
            ));
        }
        visible.sort_by(|a, b| b.0.total_cmp(&a.0));
        visible.truncate(self.config.max_translucent_sections as usize);
        let cmds: Vec<[u32; 5]> = visible
            .iter()
            .enumerate()
            .map(|(i, v)| [v.3 * 6, 1, 0, 0, i as u32])
            .collect();
        let draws: Vec<DrawDataGpu> = visible
            .iter()
            .map(|v| DrawDataGpu {
                origin: v.1.to_array(),
                first_quad: v.2,
                tint_base: v.4,
                pad: [v.5, 0, 0],
            })
            .collect();
        if !cmds.is_empty() {
            self.queue
                .write_buffer(&self.geo.trans_cmds, 0, bytemuck::cast_slice(&cmds));
            self.queue
                .write_buffer(&self.geo.trans_draws, 0, bytemuck::cast_slice(&draws));
        }
        self.queue.write_buffer(
            &self.geo.trans_count,
            0,
            bytemuck::cast_slice(&[cmds.len() as u32, 0, 0, 0]),
        );
        cmds.len() as u32
    }

    pub fn pending_upload_bytes(&self) -> usize {
        self.uploader.pending_bytes()
    }

    fn collect_gpu_stats(&mut self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
        for rb in &self.readbacks {
            if rb.state.load(Ordering::Acquire) != RB_READY {
                continue;
            }
            {
                let view = rb
                    .buffer
                    .slice(..)
                    .get_mapped_range()
                    .expect("mapped readback");
                let ts: &[u64] = bytemuck::cast_slice(&view[..8 * TIMESTAMPS as usize]);
                let c: &[u32] = bytemuck::cast_slice(
                    &view[STATS_COUNTER_OFFSET as usize..][..COUNTERS_SIZE as usize],
                );
                let period = self.timestamps.as_ref().map_or(0.0, |t| t.period_ns);
                let ms = |a: u64, b: u64| b.saturating_sub(a) as f32 * period / 1.0e6;
                let times = if rb.two_phase {
                    GpuTimes {
                        cull_ms: ms(ts[0], ts[1]) + ms(ts[4], ts[5]),
                        draw_ms: ms(ts[2], ts[3]) + ms(ts[6], ts[7]),
                        frame_ms: ms(ts[0], ts[7]),
                        cube_draws: c[0] + c[2],
                        model_draws: c[1] + c[3],
                    }
                } else {
                    GpuTimes {
                        cull_ms: ms(ts[0], ts[1]),
                        draw_ms: ms(ts[2], ts[3]),
                        frame_ms: ms(ts[0], ts[3]),
                        cube_draws: c[0],
                        model_draws: c[1],
                    }
                };
                if rb.frame >= self.latest_gpu_frame {
                    self.latest_gpu = Some(times);
                    self.latest_gpu_frame = rb.frame;
                }
            }
            rb.buffer.unmap();
            rb.state.store(RB_IDLE, Ordering::Release);
        }
    }

    /// Blocks until in-flight GPU work and stat readbacks finish, then returns the newest GPU stats.
    pub fn wait_gpu_stats(&mut self) -> Option<GpuTimes> {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        self.collect_gpu_stats();
        self.latest_gpu
    }

    pub fn render(&mut self, frame: &FrameInputs) -> Result<FrameStats, RenderError> {
        self.render_with_overlay(frame, None)
    }

    pub fn target_format(&self) -> wgpu::TextureFormat {
        match &self.surface {
            Some(s) if self.scene.is_none() => s.view_format,
            _ => COLOR_FORMAT,
        }
    }

    /// Post chains to run on the world image before UI overlays, in order; at most
    /// [`MAX_POST_CHAINS`] are kept.
    pub fn set_post_effects(&mut self, chains: &[PostChain]) {
        if self.post_chains != chains {
            self.post_chains.clear();
            self.post_chains
                .extend_from_slice(&chains[..chains.len().min(MAX_POST_CHAINS)]);
        }
    }

    /// Resource-pack tuning of the built-in post chains; see [`PostOverrides`].
    pub fn set_post_overrides(&mut self, overrides: PostOverrides) {
        self.post.set_overrides(overrides);
    }

    pub fn post_effects(&self) -> &[PostChain] {
        &self.post_chains
    }

    /// The menu background blurriness (0..=10) the `blur` chain uses.
    pub fn set_menu_blur(&mut self, radius: u32) {
        self.menu_blur = radius.min(10);
    }

    /// Hooks external GPU work into every frame; see [`ExternalSubmitter`].
    pub fn set_external(&mut self, external: Option<Box<dyn ExternalSubmitter + Send>>) {
        self.external = external;
    }

    /// The image external work composes onto: the scene texture, or the offscreen target.
    pub fn scene_texture(&self) -> Option<&wgpu::Texture> {
        self.scene
            .as_ref()
            .map(|(t, _)| t)
            .or(self.offscreen.as_ref())
    }

    /// `overlay` records into the frame's encoder after the world pass; depth is kept for it.
    pub fn render_with_overlay(
        &mut self,
        frame: &FrameInputs,
        mut overlay: Option<&mut dyn Overlay>,
    ) -> Result<FrameStats, RenderError> {
        let start = Instant::now();
        let (uploaded_bytes, uploaded_writes) = self.flush_uploads();
        self.collect_gpu_stats();

        let (w, h) = self.size;
        let cam = frame.camera;
        let frustum = cam.frustum(w as f32 / h as f32);
        let ci = cam.block_pos();
        let cf = cam.frac();
        self.update_animations();
        self.update_lightmap(frame.sky_light);
        let env = self.environment.clone();
        let (rd_start, rd_end) = env.render_fog_range();
        let view_proj = cam.view_proj(w as f32 / h as f32);
        let graph = self.use_graph && !self.camera_in_opaque;
        let globals = GlobalsGpu {
            view_proj: view_proj.to_cols_array(),
            planes: frustum.0.map(|p| p.to_array()),
            cam_int: [ci.x, ci.y, ci.z, 0],
            cam_frac: [cf.x, cf.y, cf.z, 0.0],
            params: [
                self.slot_count,
                u32::from(self.occlusion) | u32::from(graph) << 1,
                self.late_cube_base(),
                self.late_model_base(),
            ],
            sun: [
                frame.sky_light,
                f32::from(u8::from(
                    env.skybox == Skybox::Overworld
                        && cam.position.y < f64::from(env.horizon_height),
                )),
                0.0,
                0.0,
            ],
            inv_view_proj: view_proj.inverse().to_cols_array(),
            fog_color: [env.fog_color[0], env.fog_color[1], env.fog_color[2], 1.0],
            fog: [env.fog_start, env.fog_end, rd_start, rd_end],
            sky_color: [
                env.sky_color[0],
                env.sky_color[1],
                env.sky_color[2],
                env.sky_fog_end,
            ],
            shade_a: [
                env.cardinal[0],
                env.cardinal[1],
                env.cardinal[2],
                env.cardinal[3],
            ],
            shade_b: [env.cardinal[4], env.cardinal[5], 0.0, 0.0],
            cull: [w as f32, h as f32, self.hiz.mip_count() as f32, cam.near],
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        if let Some(d) = &mut self.dyn_lights {
            dynlight::select(
                &d.sources,
                cam.position,
                &frustum,
                &mut d.scratch,
                &mut d.gpu,
                &mut d.grid,
            );
            d.active = d.gpu.count() > 0;
            if d.active {
                self.queue
                    .write_buffer(&d.buffer, 0, bytemuck::bytes_of(&*d.gpu));
                self.queue
                    .write_buffer(&d.grid_buffer, 0, bytemuck::cast_slice(&d.grid));
            }
        }
        let mut graph_ms = 0.0;
        if graph {
            let origin = [ci.x >> 4, ci.y >> 4, ci.z >> 4];
            if self.graph.needs_update(origin) {
                let started = Instant::now();
                let bits = self.graph.update(origin, self.slot_count);
                self.queue
                    .write_buffer(&self.geo.reachable, 0, bytemuck::cast_slice(bits));
                graph_ms = started.elapsed().as_secs_f32() * 1000.0;
            }
        }
        let trans_n = self.build_translucent(&frustum, [ci.x, ci.y, ci.z], cf, graph, rd_end);
        self.effects.prepare(&env, cam.position.to_array());

        let mut surface_tex = None;
        let mut surface_view = None;
        let acquire_start = Instant::now();
        let color_view = match (&self.surface, &self.offscreen) {
            (Some(s), _) if self.scene.is_some() => {
                let tex = match s.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                        s.surface.configure(&self.device, &s.config);
                        return Err(RenderError::SurfaceUnavailable);
                    }
                    _ => return Err(RenderError::SurfaceUnavailable),
                };
                surface_view = Some(tex.texture.create_view(&wgpu::TextureViewDescriptor {
                    format: Some(s.view_format),
                    ..Default::default()
                }));
                surface_tex = Some(tex);
                let (scene, _) = self.scene.as_ref().expect("checked by the guard");
                scene.create_view(&wgpu::TextureViewDescriptor::default())
            }
            (Some(s), _) => {
                let tex = match s.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                        s.surface.configure(&self.device, &s.config);
                        return Err(RenderError::SurfaceUnavailable);
                    }
                    _ => return Err(RenderError::SurfaceUnavailable),
                };
                let view = tex.texture.create_view(&wgpu::TextureViewDescriptor {
                    format: Some(s.view_format),
                    ..Default::default()
                });
                surface_tex = Some(tex);
                if self.post_chains.is_empty()
                    && !self.capture_requested
                    && !self.screenshot_requested
                {
                    self.post_window = None;
                    view
                } else {
                    let (main, _) = self.post_window.get_or_insert_with(|| {
                        let t = self.device.create_texture(&wgpu::TextureDescriptor {
                            label: Some("post window"),
                            size: wgpu::Extent3d {
                                width: w,
                                height: h,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: s.view_format,
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                | wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_SRC,
                            view_formats: &[],
                        });
                        (t, PresentBlit::new(&self.device, s.view_format))
                    });
                    surface_view = Some(view);
                    main.create_view(&wgpu::TextureViewDescriptor::default())
                }
            }
            (None, Some(t)) => t.create_view(&wgpu::TextureViewDescriptor::default()),
            (None, None) => unreachable!("renderer always has a target"),
        };
        let acquire_ms = acquire_start.elapsed().as_secs_f32() * 1000.0;

        let aspect = w as f32 / h as f32;
        let mut external = self.external.take();
        let split = external.is_some() && self.scene_texture().is_some();
        if let (Some(ext), Some(scene)) = (external.as_mut(), self.scene_texture()) {
            ext.before_frame(&ExternalCtx {
                device: &self.device,
                queue: &self.queue,
                scene,
                depth: &self.depth_view,
                size: self.size,
                camera: cam,
                view: cam.view_rotation(),
                proj: cam.projection(aspect),
                sections: self.reached_sections(),
                batch: &self.batch,
            });
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let slots = u64::from(self.slot_count);
        let two_phase = self.occlusion;
        self.frame_index += 1;
        let late_cube = u64::from(self.late_cube_base()) * DRAW_CMD_SIZE;
        let late_model = u64::from(self.late_model_base()) * DRAW_CMD_SIZE;
        encoder.clear_buffer(&self.geo.counters, 0, None);
        if !self.draw_count_supported {
            let (cube_len, model_len) = (slots * 6 * DRAW_CMD_SIZE, slots * DRAW_CMD_SIZE);
            encoder.clear_buffer(&self.geo.cube_cmds, 0, Some(cube_len));
            encoder.clear_buffer(&self.geo.model_cmds, 0, Some(model_len));
            if two_phase {
                encoder.clear_buffer(&self.geo.cube_cmds, late_cube, Some(cube_len));
                encoder.clear_buffer(&self.geo.model_cmds, late_model, Some(model_len));
            }
        }
        let groups = self.slot_count.div_ceil(64).max(1);
        let compute_ts = |begin: u32| {
            self.timestamps
                .as_ref()
                .map(|t| wgpu::ComputePassTimestampWrites {
                    query_set: &t.set,
                    beginning_of_pass_write_index: Some(begin),
                    end_of_pass_write_index: Some(begin + 1),
                })
        };
        let render_ts = |begin: u32| {
            self.timestamps
                .as_ref()
                .map(|t| wgpu::RenderPassTimestampWrites {
                    query_set: &t.set,
                    beginning_of_pass_write_index: Some(begin),
                    end_of_pass_write_index: Some(begin + 1),
                })
        };

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("cull early"),
                timestamp_writes: compute_ts(0),
            });
            pass.set_pipeline(&self.cull_early);
            pass.set_bind_group(0, &self.cull_bg, &[]);
            pass.dispatch_workgroups(groups, 1, 1);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terrain"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(env.fog_color[0]),
                            g: f64::from(env.fog_color[1]),
                            b: f64::from(env.fog_color[2]),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: render_ts(2),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_bind_group(1, &self.geom_bg, &[]);
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);
            self.effects.draw_sky(&mut pass);
            pass.set_bind_group(1, &self.geom_bg, &[]);
            self.draw_sections(&mut pass, 0, 0, 0);
            if !two_phase && !split {
                self.draw_late_layers(&mut pass, overlay.as_deref_mut(), trans_n);
            }
        }

        if two_phase {
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("occlusion"),
                    timestamp_writes: compute_ts(4),
                });
                self.hiz.record(&mut pass);
                pass.set_pipeline(&self.cull_late);
                pass.set_bind_group(0, &self.cull_bg, &[]);
                pass.dispatch_workgroups(groups, 1, 1);
                if trans_n > 0 {
                    pass.set_pipeline(&self.trans_cull);
                    pass.set_bind_group(0, &self.trans_cull_bg, &[]);
                    pass.dispatch_workgroups(trans_n.div_ceil(64), 1, 1);
                }
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terrain late"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: render_ts(6),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_bind_group(1, &self.geom_bg, &[]);
            self.draw_sections(&mut pass, late_cube, late_model, 8);
            if !split {
                self.draw_late_layers(&mut pass, overlay.as_deref_mut(), trans_n);
            }
        }

        if let (true, Some(ext), Some(scene)) = (split, external.as_mut(), self.scene_texture()) {
            self.batch.hold(encoder);
            ext.after_opaque(&ExternalCtx {
                device: &self.device,
                queue: &self.queue,
                scene,
                depth: &self.depth_view,
                size: self.size,
                camera: cam,
                view: cam.view_rotation(),
                proj: cam.projection(aspect),
                sections: self.reached_sections(),
                batch: &self.batch,
            });
            encoder = self.batch.resume(&self.device, "frame after opaque");
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terrain translucent"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_bind_group(1, &self.geom_bg, &[]);
            self.draw_late_layers(&mut pass, overlay.as_deref_mut(), trans_n);
        }

        let format = self.target_format();
        if let Some(overlay) = overlay.as_deref_mut() {
            overlay.record(&mut OverlayCtx {
                device: &self.device,
                queue: &self.queue,
                encoder: &mut encoder,
                color: &color_view,
                depth: &self.depth_view,
                format,
                size: self.size,
                camera: frame.camera,
            });
        }
        if let (Some(ext), Some(scene)) = (external.as_mut(), self.scene_texture()) {
            self.batch.hold(encoder);
            ext.after_world(&ExternalCtx {
                device: &self.device,
                queue: &self.queue,
                scene,
                depth: &self.depth_view,
                size: self.size,
                camera: cam,
                view: cam.view_rotation(),
                proj: cam.projection(aspect),
                sections: self.reached_sections(),
                batch: &self.batch,
            });
            encoder = self.batch.resume(&self.device, "frame after external");
        }
        let post_main = self
            .post_window
            .as_ref()
            .map(|(t, _)| t)
            .or(self.scene_texture())
            .filter(|_| !self.post_chains.is_empty())
            .cloned();
        if let Some(main) = &post_main {
            self.post.record(
                &self.device,
                &self.queue,
                &mut encoder,
                main,
                &self.post_chains,
                self.menu_blur,
            );
        }
        if std::mem::take(&mut self.capture_requested) {
            self.capture_pending = self.record_capture(&mut encoder);
        }
        if let (Some(ext), Some(scene)) = (external.as_mut(), self.scene_texture()) {
            self.batch.hold(encoder);
            ext.before_ui(&ExternalCtx {
                device: &self.device,
                queue: &self.queue,
                scene,
                depth: &self.depth_view,
                size: self.size,
                camera: cam,
                view: cam.view_rotation(),
                proj: cam.projection(aspect),
                sections: self.reached_sections(),
                batch: &self.batch,
            });
            encoder = self.batch.resume(&self.device, "frame after ui hook");
        }
        if let Some(overlay) = overlay {
            overlay.record_ui(&mut OverlayCtx {
                device: &self.device,
                queue: &self.queue,
                encoder: &mut encoder,
                color: &color_view,
                depth: &self.depth_view,
                format,
                size: self.size,
                camera: frame.camera,
            });
        }
        let screenshot = if std::mem::take(&mut self.screenshot_requested) {
            self.record_capture(&mut encoder)
        } else {
            None
        };
        if let (Some((_, blit)), Some(dst)) = (
            self.scene.as_ref().or(self.post_window.as_ref()),
            &surface_view,
        ) {
            blit.record(&self.device, &mut encoder, &color_view, dst);
        }

        let slot = (0..READBACK_SLOTS)
            .map(|i| (self.readback_cursor + i) % READBACK_SLOTS)
            .find(|&i| self.readbacks[i].state.load(Ordering::Acquire) == RB_IDLE);
        if slot.is_some() {
            if let Some(t) = &self.timestamps {
                let written = if two_phase { TIMESTAMPS } else { 4 };
                encoder.resolve_query_set(&t.set, 0..written, &self.stats_gpu, 0);
            }
            encoder.copy_buffer_to_buffer(
                &self.geo.counters,
                0,
                &self.stats_gpu,
                STATS_COUNTER_OFFSET,
                COUNTERS_SIZE,
            );
        }
        if let Some(i) = slot {
            encoder.copy_buffer_to_buffer(
                &self.stats_gpu,
                0,
                &self.readbacks[i].buffer,
                0,
                STATS_SIZE,
            );
        }

        self.batch.push(encoder.finish());
        let (index, error) = self.batch.submit(&self.device, &self.queue);
        if let Some(e) = &error {
            tracing::error!("frame submit rejected by wgpu: {e}");
        }
        if let Some(ext) = external.as_mut() {
            ext.submitted(&index, error.is_some());
        }
        self.external = external;
        if let Some(i) = slot {
            self.readback_cursor = (i + 1) % READBACK_SLOTS;
            let rb = &mut self.readbacks[i];
            rb.two_phase = two_phase;
            rb.frame = self.frame_index;
            rb.state.store(RB_PENDING, Ordering::Release);
            let state = rb.state.clone();
            rb.buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |r| {
                    state.store(
                        if r.is_ok() { RB_READY } else { RB_IDLE },
                        Ordering::Release,
                    );
                });
        }
        if let Some(pending) = self.capture_pending.take() {
            self.captured = self.finish_capture(&pending);
        }
        if let Some(pending) = screenshot {
            self.screenshot = self.finish_capture(&pending);
        }
        let present_start = Instant::now();
        if let Some(t) = surface_tex {
            self.queue.present(t);
        }
        let present_ms = present_start.elapsed().as_secs_f32() * 1000.0;

        Ok(FrameStats {
            cpu_ms: start.elapsed().as_secs_f32() * 1000.0,
            acquire_ms,
            present_ms,
            uploaded_bytes,
            uploaded_writes,
            pending_upload_bytes: self.uploader.pending_bytes(),
            graph_ms,
            gpu: self.latest_gpu,
        })
    }

    fn late_cube_base(&self) -> u32 {
        self.config.max_sections * 6
    }

    fn late_model_base(&self) -> u32 {
        self.config.max_sections
    }

    /// Cube and model draws from one culling phase; `counter_offset` selects its draw counters.
    fn draw_sections(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        cube_offset: u64,
        model_offset: u64,
        counter_offset: u64,
    ) {
        let cube_max = self.slot_count * 6;
        let model_max = self.slot_count;
        let dyn_lights = self.dyn_lights.as_ref().filter(|d| d.active);
        if let Some(d) = dyn_lights {
            pass.set_bind_group(2, &d.bind_group, &[]);
        }
        pass.set_pipeline(dyn_lights.map_or(&self.cube_pipeline, |d| &d.cube));
        pass.set_index_buffer(self.geo.quad_indices.slice(..), wgpu::IndexFormat::Uint32);
        if self.draw_count_supported {
            pass.multi_draw_indexed_indirect_count(
                &self.geo.cube_cmds,
                cube_offset,
                &self.geo.counters,
                counter_offset,
                cube_max,
            );
        } else {
            pass.multi_draw_indexed_indirect(&self.geo.cube_cmds, cube_offset, cube_max);
        }
        pass.set_pipeline(dyn_lights.map_or(&self.model_pipeline, |d| &d.model));
        if self.draw_count_supported {
            pass.multi_draw_indexed_indirect_count(
                &self.geo.model_cmds,
                model_offset,
                &self.geo.counters,
                counter_offset + 4,
                model_max,
            );
        } else {
            pass.multi_draw_indexed_indirect(&self.geo.model_cmds, model_offset, model_max);
        }
    }

    /// Everything after opaque terrain: overlay opaque geometry, translucent terrain and world effects.
    fn draw_late_layers<'o>(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        overlay: Option<&mut (dyn Overlay + 'o)>,
        trans_n: u32,
    ) {
        if let Some(overlay) = overlay {
            overlay.opaque(pass);
            pass.set_bind_group(0, &self.globals_bg, &[]);
        }
        if trans_n > 0 {
            match self.dyn_lights.as_ref().filter(|d| d.active) {
                Some(d) => {
                    pass.set_bind_group(2, &d.bind_group, &[]);
                    pass.set_pipeline(&d.trans);
                }
                None => pass.set_pipeline(&self.trans_pipeline),
            }
            pass.set_bind_group(1, &self.trans_geom_bg, &[]);
            pass.set_index_buffer(self.geo.quad_indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.multi_draw_indexed_indirect(&self.geo.trans_cmds, 0, trans_n);
        }
        self.effects.draw_world(pass);
    }

    /// Copies the next frame's world image, before any UI, for [`Renderer::take_capture`].
    pub fn request_capture(&mut self) {
        self.capture_requested = true;
    }

    /// The image of the last frame a capture was requested for, once.
    pub fn take_capture(&mut self) -> Option<Capture> {
        self.captured.take()
    }

    /// Copies the next frame as shown, UI included, for [`Renderer::take_screenshot`].
    pub fn request_screenshot(&mut self) {
        self.screenshot_requested = true;
    }

    pub fn take_screenshot(&mut self) -> Option<Capture> {
        self.screenshot.take()
    }

    fn record_capture(&self, encoder: &mut wgpu::CommandEncoder) -> Option<PendingCapture> {
        let tex = self
            .scene
            .as_ref()
            .or(self.post_window.as_ref())
            .map(|(t, _)| t)
            .or(self.offscreen.as_ref())?;
        let bgra = matches!(
            tex.format(),
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        let supported = bgra
            || matches!(
                tex.format(),
                wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb
            );
        if !supported {
            return None;
        }
        let (width, height) = (tex.width(), tex.height());
        let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture"),
            size: u64::from(row) * u64::from(height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Some(PendingCapture {
            buffer,
            row,
            width,
            height,
            bgra,
        })
    }

    /// Waits for the submitted copy once; captures are rare (a world icon), so the stall is acceptable.
    fn finish_capture(&self, pending: &PendingCapture) -> Option<Capture> {
        pending
            .buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let mapped = pending.buffer.slice(..).get_mapped_range().ok()?;
        let width = (pending.width * 4) as usize;
        let mut pixels = Vec::with_capacity(width * pending.height as usize);
        for y in 0..pending.height {
            let start = (y * pending.row) as usize;
            pixels.extend_from_slice(&mapped[start..start + width]);
        }
        Some(Capture {
            width: pending.width,
            height: pending.height,
            bgra: pending.bgra,
            pixels,
        })
    }

    /// Returns tightly packed RGBA8 (sRGB-encoded) pixels of the offscreen target.
    pub fn read_offscreen(&self) -> Result<Vec<u8>, RenderError> {
        let tex = self.offscreen.as_ref().ok_or(RenderError::NotOffscreen)?;
        let (w, h) = self.size;
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(row) * u64::from(h),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback"),
            });
        encoder.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .expect("mapped readback");
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let start = (y * row) as usize;
            out.extend_from_slice(&mapped[start..start + (w * 4) as usize]);
        }
        Ok(out)
    }
}

/// Corners `(0, 1, 2), (2, 1, 3)` of each quad; the vertex shader maps index `4q + k` to quad q, corner k.
fn quad_indices(quads: u32) -> Vec<u32> {
    (0..quads)
        .flat_map(|q| {
            let b = q * 4;
            [b, b + 1, b + 2, b + 2, b + 1, b + 3]
        })
        .collect()
}

fn make_cull_bg(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    geo: &Geometry,
    hiz: &HiZ,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cull"),
        layout,
        entries: &[
            bind(0, globals),
            bind(1, &geo.sections),
            bind(2, &geo.cube_cmds),
            bind(3, &geo.cube_draws),
            bind(4, &geo.model_cmds),
            bind(5, &geo.model_draws),
            bind(6, &geo.counters),
            bind(7, &geo.visibility),
            bind(9, &geo.reachable),
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(hiz.view()),
            },
        ],
    })
}

fn bind(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

struct Bindings<'a> {
    globals: &'a wgpu::Buffer,
    lightmap: &'a wgpu::Buffer,
    anim: &'a wgpu::Buffer,
}

fn make_globals_bg(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffers: &Bindings,
    textures: &[wgpu::Texture],
    dummy: &wgpu::Texture,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let views: Vec<wgpu::TextureView> = (0..MAX_TEXTURE_GROUPS)
        .map(|i| {
            textures
                .get(i)
                .unwrap_or(dummy)
                .create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                })
        })
        .collect();
    let mut entries = vec![
        bind(0, buffers.globals),
        wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::Sampler(sampler),
        },
    ];
    entries.extend(views.iter().enumerate().map(|(i, v)| wgpu::BindGroupEntry {
        binding: 2 + i as u32,
        resource: wgpu::BindingResource::TextureView(v),
    }));
    entries.push(bind(6, buffers.lightmap));
    entries.push(bind(7, buffers.anim));
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("globals"),
        layout,
        entries: &entries,
    })
}

fn create_offscreen(device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::TextureFormat as F;

    #[test]
    fn surface_prefers_a_non_srgb_format_even_when_srgb_is_listed_first() {
        let listed = [
            F::Rgba8UnormSrgb,
            F::Bgra8UnormSrgb,
            F::Rgb10a2Unorm,
            F::Bgra8Unorm,
        ];
        assert_eq!(surface_format(&listed), Some(F::Bgra8Unorm));
        let only_srgb = [F::Bgra8UnormSrgb];
        let picked = surface_format(&only_srgb).unwrap();
        assert!(!picked.remove_srgb_suffix().is_srgb());
    }
}
