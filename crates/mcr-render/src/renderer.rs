use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

use bytemuck::{Pod, Zeroable};

use crate::arena::Arena;
use crate::camera::Camera;
use crate::mesh::SectionMesh;
use crate::quad::{ModelQuad, PackedQuad};
use crate::staging::{Dest, Uploader};
use crate::texture::{TextureArrayData, downsample};

const QUAD_SRC: &str = include_str!("shaders/quad.wgsl");
const GLOBALS_SRC: &str = include_str!("shaders/globals.wgsl");
const TERRAIN_SRC: &str = include_str!("shaders/terrain.wgsl");
const CULL_SRC: &str = include_str!("shaders/cull.wgsl");

pub(crate) const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SKY_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.17,
    g: 0.42,
    b: 1.0,
    a: 1.0,
};
const READBACK_SLOTS: usize = 3;
const STATS_COUNTER_OFFSET: u64 = 256;
const STATS_SIZE: u64 = 512;
const DRAW_CMD_SIZE: u64 = 16;
const DRAW_DATA_SIZE: u64 = 16;

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
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SectionMetaGpu {
    origin: [i32; 4],
    bases: [u32; 4],
    counts_a: [u32; 4],
    counts_b: [u32; 4],
}

#[derive(Debug)]
struct SectionSlot {
    index: u32,
    cube: Option<(u32, u32)>,
    model: Option<(u32, u32)>,
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
    globals_bgl: wgpu::BindGroupLayout,
    globals_bg: wgpu::BindGroup,
    geom_bg: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    texture: wgpu::Texture,

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

fn create_block_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    data: &TextureArrayData,
) -> wgpu::Texture {
    let mips = data.mip_levels();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("block textures"),
        size: wgpu::Extent3d {
            width: data.size,
            height: data.size,
            depth_or_array_layers: data.layers.len() as u32,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, base) in data.layers.iter().enumerate() {
        let mut level = base.clone();
        let mut size = data.size;
        for mip in 0..mips {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &level,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size * 4),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
            if size > 1 {
                level = downsample(&level, size);
                size /= 2;
            }
        }
    }
    texture
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

        let globals_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::VERTEX),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
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
        let texture = create_block_texture(&device, &queue, &TextureArrayData::placeholder(16));
        let globals_bg = make_globals_bg(&device, &globals_bgl, &globals, &texture, &sampler);

        let terrain_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain"),
            source: wgpu::ShaderSource::Wgsl([QUAD_SRC, GLOBALS_SRC, TERRAIN_SRC].concat().into()),
        });
        let draw_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[Some(&globals_bgl), Some(&geom_bgl)],
            immediate_size: 0,
        });
        let make_pipeline = |label: &str, vs_entry: &str, cull: Option<wgpu::Face>| {
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
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &terrain_module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(color_format.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let cube_pipeline = make_pipeline("cube", "vs_cube", Some(wgpu::Face::Back));
        let model_pipeline = make_pipeline("model", "vs_model", None);

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
            globals_bgl,
            globals_bg,
            geom_bg,
            sampler,
            texture,
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
        self.texture = create_block_texture(&self.device, &self.queue, data);
        self.globals_bg = make_globals_bg(
            &self.device,
            &self.globals_bgl,
            &self.globals,
            &self.texture,
            &self.sampler,
        );
    }

    pub fn memory_stats(&self) -> MemoryStats {
        let tex = &self.texture;
        let texels: u64 = (0..tex.mip_level_count())
            .map(|m| u64::from((tex.width() >> m).max(1)).pow(2))
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
            ]
            .iter()
            .map(|b| b.size())
            .sum(),
            texture_bytes: texels * 4 * u64::from(tex.depth_or_array_layers()),
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
        let model_total = mesh.model.len() as u32;

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
        let model = if model_total > 0 {
            match self.model_arena.alloc(model_total) {
                Some(o) => Some((o, model_total)),
                None => {
                    if let Some((o, n)) = cube {
                        self.cube_arena.free(o, n);
                    }
                    return Err(RenderError::ArenaFull("model"));
                }
            }
        } else {
            None
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
                    if let Some((o, n)) = cube {
                        self.cube_arena.free(o, n);
                    }
                    if let Some((o, n)) = model {
                        self.model_arena.free(o, n);
                    }
                    return Err(RenderError::TooManySections);
                }
            },
        };
        if let Some(old) = old {
            if let Some((o, n)) = old.cube {
                self.cube_arena.free(o, n);
            }
            if let Some((o, n)) = old.model {
                self.model_arena.free(o, n);
            }
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
            self.uploader.push(
                Dest::Model,
                u64::from(base) * ModelQuad::SIZE as u64,
                bytemuck::cast_slice(&mesh.model),
            );
        }
        let meta = SectionMetaGpu {
            origin: [key[0] * 16, key[1] * 16, key[2] * 16, 1],
            bases: [
                cube.map_or(0, |c| c.0),
                model.map_or(0, |m| m.0),
                model_total,
                0,
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
        self.sections
            .insert(key, SectionSlot { index, cube, model });
        Ok(())
    }

    pub fn remove_section(&mut self, key: [i32; 3]) {
        let Some(slot) = self.sections.remove(&key) else {
            return;
        };
        if let Some((o, n)) = slot.cube {
            self.cube_arena.free(o, n);
        }
        if let Some((o, n)) = slot.model {
            self.model_arena.free(o, n);
        }
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
                };
                queue.write_buffer(buffer, offset, bytes);
            });
        (f.bytes, f.writes)
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
        let start = Instant::now();
        let (uploaded_bytes, uploaded_writes) = self.flush_uploads();
        self.collect_gpu_stats();

        let (w, h) = self.size;
        let cam = frame.camera;
        let frustum = cam.frustum(w as f32 / h as f32);
        let ci = cam.block_pos();
        let cf = cam.frac();
        let globals = GlobalsGpu {
            view_proj: cam.view_proj(w as f32 / h as f32).to_cols_array(),
            planes: frustum.0.map(|p| p.to_array()),
            cam_int: [ci.x, ci.y, ci.z, 0],
            cam_frac: [cf.x, cf.y, cf.z, 0.0],
            params: [self.slot_count, 0, 0, 0],
            sun: [frame.sky_light, 0.0, 0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

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
                        load: wgpu::LoadOp::Clear(SKY_CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
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

fn make_globals_bg(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    texture: &wgpu::Texture,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("globals"),
        layout,
        entries: &[
            bind(0, globals),
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
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
