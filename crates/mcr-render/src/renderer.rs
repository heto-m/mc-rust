use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

use bytemuck::{Pod, Zeroable};

use crate::animation::{ANIM_TABLE_LEN, AnimationTable, SpriteAnimation};
use crate::arena::Arena;
use crate::camera::Camera;
use crate::effects::{CloudSettings, DestroyDecal, EffectAssets, Effects, WeatherSource};
use crate::environment::{Environment, Skybox};
use crate::mesh::SectionMesh;
use crate::mesh::TINT_TABLE_WORDS;
use crate::overlay::{Overlay, OverlayCtx};
use crate::quad::{ModelQuad, PackedQuad};
use crate::staging::{Dest, Uploader};
use crate::texture::{MAX_TEXTURE_GROUPS, TextureArrayData, TextureGroupData};

const QUAD_SRC: &str = include_str!("shaders/quad.wgsl");
const GLOBALS_SRC: &str = include_str!("shaders/globals.wgsl");
const TERRAIN_SRC: &str = include_str!("shaders/terrain.wgsl");
const CULL_SRC: &str = include_str!("shaders/cull.wgsl");

pub(crate) const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const TICKS_PER_SECOND: f64 = 20.0;
const LIGHTMAP_BYTES: u64 = 256 * 16;
const READBACK_SLOTS: usize = 3;
const STATS_COUNTER_OFFSET: u64 = 256;
const STATS_SIZE: u64 = 512;
const DRAW_CMD_SIZE: u64 = 16;
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
    pub cube_draws: u32,
    pub model_draws: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FrameStats {
    pub cpu_ms: f32,
    pub uploaded_bytes: usize,
    pub uploaded_writes: usize,
    pub pending_upload_bytes: usize,
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

#[derive(Debug)]
struct SectionSlot {
    index: u32,
    cube: Option<(u32, u32)>,
    model: Option<(u32, u32)>,
    translucent: Option<(u32, u32)>,
    tint: Option<u32>,
}

#[derive(Debug)]
struct Readback {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
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
}

struct Geometry {
    sections: wgpu::Buffer,
    cube_arena: wgpu::Buffer,
    model_arena: wgpu::Buffer,
    cube_cmds: wgpu::Buffer,
    cube_draws: wgpu::Buffer,
    model_cmds: wgpu::Buffer,
    model_draws: wgpu::Buffer,
    counters: wgpu::Buffer,
    tints: wgpu::Buffer,
    trans_cmds: wgpu::Buffer,
    trans_draws: wgpu::Buffer,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_info: wgpu::AdapterInfo,
    draw_count_supported: bool,
    config: RendererConfig,

    surface: Option<Surface>,
    offscreen: Option<wgpu::Texture>,
    size: (u32, u32),
    depth_view: wgpu::TextureView,

    globals: wgpu::Buffer,
    geo: Geometry,
    cull_pipeline: wgpu::ComputePipeline,
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

    cube_arena: Arena,
    model_arena: Arena,
    sections: HashMap<[i32; 3], SectionSlot>,
    free_slots: Vec<u32>,
    slot_count: u32,
    uploader: Uploader,

    timestamps: Option<Timestamps>,
    stats_gpu: wgpu::Buffer,
    readbacks: Vec<Readback>,
    readback_cursor: usize,
    latest_gpu: Option<GpuTimes>,
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
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
        let features = wgpu::Features::INDIRECT_FIRST_INSTANCE | (have & optional);
        let al = adapter.limits();
        let limits = wgpu::Limits {
            max_storage_buffer_binding_size: al.max_storage_buffer_binding_size,
            max_buffer_size: al.max_buffer_size,
            max_texture_array_layers: al.max_texture_array_layers,
            ..wgpu::Limits::default()
        };
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
            cfg.format = formats
                .iter()
                .copied()
                .find(|f| *f == COLOR_FORMAT || *f == wgpu::TextureFormat::Bgra8UnormSrgb)
                .unwrap_or(cfg.format);
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
            }
        });
        let color_format = surface.as_ref().map_or(COLOR_FORMAT, |s| s.config.format);
        let offscreen = surface
            .is_none()
            .then(|| create_offscreen(&device, width, height));

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
                max_sections * 6 * DRAW_CMD_SIZE,
                wgpu::BufferUsages::INDIRECT,
            ),
            cube_draws: storage_buffer(
                &device,
                "cube draws",
                max_sections * 6 * DRAW_DATA_SIZE,
                wgpu::BufferUsages::empty(),
            ),
            model_cmds: storage_buffer(
                &device,
                "model cmds",
                max_sections * DRAW_CMD_SIZE,
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
                8,
                wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_SRC,
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
            ],
        });
        let cull_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cull"),
            layout: &cull_bgl,
            entries: &[
                bind(0, &globals),
                bind(1, &geo.sections),
                bind(2, &geo.cube_cmds),
                bind(3, &geo.cube_draws),
                bind(4, &geo.model_cmds),
                bind(5, &geo.model_draws),
                bind(6, &geo.counters),
            ],
        });
        let cull_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cull"),
            source: wgpu::ShaderSource::Wgsl([GLOBALS_SRC, CULL_SRC].concat().into()),
        });
        let cull_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("cull"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("cull"),
                    bind_group_layouts: &[Some(&cull_bgl)],
                    immediate_size: 0,
                }),
            ),
            module: &cull_module,
            entry_point: Some("cull"),
            compilation_options: Default::default(),
            cache: None,
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
                uniform_entry(6, wgpu::ShaderStages::VERTEX),
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
        let make_pipeline =
            |label: &str, vs_entry: &str, fs_entry: &str, cull: Option<wgpu::Face>, blend: bool| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&draw_layout),
                    vertex: wgpu::VertexState {
                        module: &terrain_module,
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
                        module: &terrain_module,
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
                    count: 4,
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
            cube_arena: Arena::new(cube_quads),
            model_arena: Arena::new(model_quads),
            depth_view: depth_view(&device, width, height),
            size: (width, height),
            device,
            queue,
            adapter_info,
            config,
            surface,
            offscreen,
            globals,
            geo,
            cull_pipeline,
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
            timestamps,
            stats_gpu,
            readbacks,
            readback_cursor: 0,
            latest_gpu: None,
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
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
        self.depth_view = depth_view(&self.device, width, height);
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
                &g.tints,
                &g.trans_cmds,
                &g.trans_draws,
            ]
            .iter()
            .map(|b| b.size())
            .sum(),
            texture_bytes,
            target_bytes: u64::from(w) * u64::from(h) * 8,
        }
    }

    pub fn allocator_report(&self) -> Option<wgpu::AllocatorReport> {
        self.device.generate_allocator_report()
    }

    /// Replaces (or inserts) the geometry of the section at `key` (section coordinates).
    pub fn set_section(&mut self, key: [i32; 3], mesh: &SectionMesh) -> Result<(), RenderError> {
        if mesh.is_empty() {
            self.remove_section(key);
            return Ok(());
        }
        let cube_total = mesh.cube_count() as u32;
        let model_main = mesh.model.len() as u32;
        let translucent_total = mesh.translucent.len() as u32;
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
        if let Some(old) = old {
            self.release(&old);
        }

        let mut counts = [0u32; 6];
        if let Some((base, _)) = cube {
            let mut dst = u64::from(base) * PackedQuad::SIZE as u64;
            for (face, quads) in mesh.faces.iter().enumerate() {
                counts[face] = quads.len() as u32;
                let bytes: &[u8] = bytemuck::cast_slice(quads);
                self.uploader.push(Dest::Cube, dst, bytes);
                dst += bytes.len() as u64;
            }
        }
        if let Some((base, _)) = model {
            let at = u64::from(base) * ModelQuad::SIZE as u64;
            self.uploader
                .push(Dest::Model, at, bytemuck::cast_slice(&mesh.model));
            self.uploader.push(
                Dest::Model,
                at + u64::from(model_main) * ModelQuad::SIZE as u64,
                bytemuck::cast_slice(&mesh.translucent),
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
        self.uploader.end_batch();
        let translucent = model
            .filter(|_| translucent_total > 0)
            .map(|(base, _)| (base + model_main, translucent_total));
        self.sections.insert(
            key,
            SectionSlot {
                index,
                cube,
                model,
                translucent,
                tint,
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

    pub fn remove_section(&mut self, key: [i32; 3]) {
        let Some(slot) = self.sections.remove(&key) else {
            return;
        };
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
        let f = self
            .uploader
            .flush(self.config.upload_budget_bytes, |dest, offset, bytes| {
                let buffer = match dest {
                    Dest::Cube => &geo.cube_arena,
                    Dest::Model => &geo.model_arena,
                    Dest::Sections => &geo.sections,
                    Dest::Tints => &geo.tints,
                };
                queue.write_buffer(buffer, offset, bytes);
            });
        (f.bytes, f.writes)
    }

    /// Writes back-to-front draws for sections with translucent quads and returns their count.
    fn build_translucent(
        &mut self,
        frustum: &crate::Frustum,
        cam_int: [i32; 3],
        cam_frac: glam::Vec3,
    ) -> u32 {
        let mut visible: Vec<(f32, glam::Vec3, u32, u32, u32)> = Vec::new();
        for (key, slot) in &self.sections {
            let Some((base, count)) = slot.translucent else {
                continue;
            };
            let lo = glam::Vec3::new(
                (key[0] * 16 - cam_int[0]) as f32,
                (key[1] * 16 - cam_int[1]) as f32,
                (key[2] * 16 - cam_int[2]) as f32,
            ) - cam_frac;
            if !frustum.intersects_aabb(lo, lo + glam::Vec3::splat(16.0)) {
                continue;
            }
            let tint_base = slot.tint.map_or(NO_TINT_TABLE, |t| {
                (TINT_PALETTE_WORDS + t as usize * TINT_TABLE_WORDS) as u32
            });
            visible.push((
                (lo + glam::Vec3::splat(8.0)).length_squared(),
                lo,
                base,
                count,
                tint_base,
            ));
        }
        visible.sort_by(|a, b| b.0.total_cmp(&a.0));
        visible.truncate(self.config.max_translucent_sections as usize);
        let cmds: Vec<[u32; 4]> = visible
            .iter()
            .enumerate()
            .map(|(i, v)| [v.3 * 6, 1, 0, i as u32])
            .collect();
        let draws: Vec<DrawDataGpu> = visible
            .iter()
            .map(|v| DrawDataGpu {
                origin: v.1.to_array(),
                first_quad: v.2,
                tint_base: v.4,
                pad: [0; 3],
            })
            .collect();
        if !cmds.is_empty() {
            self.queue
                .write_buffer(&self.geo.trans_cmds, 0, bytemuck::cast_slice(&cmds));
            self.queue
                .write_buffer(&self.geo.trans_draws, 0, bytemuck::cast_slice(&draws));
        }
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
                let ts: &[u64] = bytemuck::cast_slice(&view[..32]);
                let counters: &[u32] =
                    bytemuck::cast_slice(&view[STATS_COUNTER_OFFSET as usize..][..8]);
                let period = self.timestamps.as_ref().map_or(0.0, |t| t.period_ns);
                let ms = |a: u64, b: u64| b.saturating_sub(a) as f32 * period / 1.0e6;
                self.latest_gpu = Some(GpuTimes {
                    cull_ms: ms(ts[0], ts[1]),
                    draw_ms: ms(ts[2], ts[3]),
                    cube_draws: counters[0],
                    model_draws: counters[1],
                });
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
        self.surface
            .as_ref()
            .map_or(COLOR_FORMAT, |s| s.config.format)
    }

    /// `overlay` records into the frame's encoder after the world pass; depth is kept for it.
    pub fn render_with_overlay(
        &mut self,
        frame: &FrameInputs,
        overlay: Option<&mut dyn Overlay>,
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
        let globals = GlobalsGpu {
            view_proj: view_proj.to_cols_array(),
            planes: frustum.0.map(|p| p.to_array()),
            cam_int: [ci.x, ci.y, ci.z, 0],
            cam_frac: [cf.x, cf.y, cf.z, 0.0],
            params: [self.slot_count, 0, 0, 0],
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
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        let trans_n = self.build_translucent(&frustum, [ci.x, ci.y, ci.z], cf);
        self.effects.prepare(&env, cam.position.to_array());

        let mut surface_tex = None;
        let color_view = match (&self.surface, &self.offscreen) {
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
                let view = tex
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                surface_tex = Some(tex);
                view
            }
            (None, Some(t)) => t.create_view(&wgpu::TextureViewDescriptor::default()),
            (None, None) => unreachable!("renderer always has a target"),
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let slots = u64::from(self.slot_count);
        encoder.clear_buffer(&self.geo.counters, 0, None);
        if !self.draw_count_supported {
            encoder.clear_buffer(&self.geo.cube_cmds, 0, Some(slots * 6 * DRAW_CMD_SIZE));
            encoder.clear_buffer(&self.geo.model_cmds, 0, Some(slots * DRAW_CMD_SIZE));
        }

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("cull"),
                timestamp_writes: self.timestamps.as_ref().map(|t| {
                    wgpu::ComputePassTimestampWrites {
                        query_set: &t.set,
                        beginning_of_pass_write_index: Some(0),
                        end_of_pass_write_index: Some(1),
                    }
                }),
            });
            pass.set_pipeline(&self.cull_pipeline);
            pass.set_bind_group(0, &self.cull_bg, &[]);
            pass.dispatch_workgroups(self.slot_count.div_ceil(64).max(1), 1, 1);
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
                timestamp_writes: self.timestamps.as_ref().map(|t| {
                    wgpu::RenderPassTimestampWrites {
                        query_set: &t.set,
                        beginning_of_pass_write_index: Some(2),
                        end_of_pass_write_index: Some(3),
                    }
                }),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_bind_group(1, &self.geom_bg, &[]);
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);
            self.effects.draw_sky(&mut pass);
            pass.set_bind_group(1, &self.geom_bg, &[]);
            let cube_max = self.slot_count * 6;
            let model_max = self.slot_count;
            pass.set_pipeline(&self.cube_pipeline);
            if self.draw_count_supported {
                pass.multi_draw_indirect_count(
                    &self.geo.cube_cmds,
                    0,
                    &self.geo.counters,
                    0,
                    cube_max,
                );
            } else {
                pass.multi_draw_indirect(&self.geo.cube_cmds, 0, cube_max);
            }
            pass.set_pipeline(&self.model_pipeline);
            if self.draw_count_supported {
                pass.multi_draw_indirect_count(
                    &self.geo.model_cmds,
                    0,
                    &self.geo.counters,
                    4,
                    model_max,
                );
            } else {
                pass.multi_draw_indirect(&self.geo.model_cmds, 0, model_max);
            }
            if trans_n > 0 {
                pass.set_pipeline(&self.trans_pipeline);
                pass.set_bind_group(1, &self.trans_geom_bg, &[]);
                pass.multi_draw_indirect(&self.geo.trans_cmds, 0, trans_n);
            }
            self.effects.draw_world(&mut pass);
        }

        if let Some(overlay) = overlay {
            overlay.record(&mut OverlayCtx {
                device: &self.device,
                queue: &self.queue,
                encoder: &mut encoder,
                color: &color_view,
                depth: &self.depth_view,
                format: self
                    .surface
                    .as_ref()
                    .map_or(COLOR_FORMAT, |s| s.config.format),
                size: self.size,
                camera: frame.camera,
            });
        }

        let slot = (0..READBACK_SLOTS)
            .map(|i| (self.readback_cursor + i) % READBACK_SLOTS)
            .find(|&i| self.readbacks[i].state.load(Ordering::Acquire) == RB_IDLE);
        if slot.is_some() {
            if let Some(t) = &self.timestamps {
                encoder.resolve_query_set(&t.set, 0..4, &self.stats_gpu, 0);
            }
            encoder.copy_buffer_to_buffer(
                &self.geo.counters,
                0,
                &self.stats_gpu,
                STATS_COUNTER_OFFSET,
                8,
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

        self.queue.submit([encoder.finish()]);
        if let Some(i) = slot {
            self.readback_cursor = (i + 1) % READBACK_SLOTS;
            let rb = &self.readbacks[i];
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
        if let Some(t) = surface_tex {
            self.queue.present(t);
        }

        Ok(FrameStats {
            cpu_ms: start.elapsed().as_secs_f32() * 1000.0,
            uploaded_bytes,
            uploaded_writes,
            pending_upload_bytes: self.uploader.pending_bytes(),
            gpu: self.latest_gpu,
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
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
