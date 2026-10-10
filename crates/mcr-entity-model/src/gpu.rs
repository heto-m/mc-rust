use bytemuck::{Pod, Zeroable, bytes_of, cast_slice};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use crate::assets::ImageRgba;
use crate::cube::Vertex;
use crate::model::Model;

pub const CLASS_SIZES: [u32; 5] = [64, 128, 256, 512, 1024];
const CLASS_INITIAL_LAYERS: [u32; 5] = [64, 16, 4, 2, 1];
const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const INITIAL_CAPACITY: usize = 1024;
pub const MAX_INSTANCES: usize = 1 << 16;
pub const MAX_BONES: usize = 1 << 20;
const MAX_CLASS_LAYERS: [u32; 5] = [1024, 256, 64, 32, 16];

/// Affine bone transform as three rows (the bottom row is always 0 0 0 1).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, PartialEq)]
pub struct Bone34(pub [[f32; 4]; 3]);

impl Bone34 {
    pub const IDENTITY: Bone34 = Bone34([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ]);

    pub fn from_mat(m: &Mat4) -> Self {
        let c = m.to_cols_array_2d();
        Bone34([
            [c[0][0], c[1][0], c[2][0], c[3][0]],
            [c[0][1], c[1][1], c[2][1], c[3][1]],
            [c[0][2], c[1][2], c[2][2], c[3][2]],
        ])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModelId(pub u32);

/// Where a texture lives: a size class (separate texture array) and a layer in it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TexId {
    pub class: u8,
    pub layer: u16,
    pub uv_scale: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Pipeline {
    Cutout = 0,
    Translucent = 1,
    /// Alpha-blended over geometry drawn earlier with identical depth (armor trims).
    Decal = 2,
    Additive = 3,
}

/// Surface the scene draws into; `reverse_z` selects a `Greater` depth test.
#[derive(Debug, Clone, Copy)]
pub struct TargetFormats {
    pub color: wgpu::TextureFormat,
    pub depth: wgpu::TextureFormat,
    pub reverse_z: bool,
    pub sample_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartInstance {
    pub model: ModelId,
    pub texture: TexId,
    pub pipeline: Pipeline,
    pub bone_base: u32,
    pub tint: [u8; 4],
    pub hurt: bool,
    pub fullbright: bool,
    pub block_light: u8,
    pub sky_light: u8,
    pub uv_offset: [f32; 2],
    pub origin: [f64; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [f32; 16],
    light0: [f32; 4],
    light1: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuInstance {
    bone_base: u32,
    layer: u32,
    uv_scale: [f32; 2],
    tint: u32,
    flags: u32,
    uv_offset: [f32; 2],
    origin: [f32; 3],
    pad: f32,
}

/// Per-frame lighting inputs.
#[derive(Debug, Clone, Copy)]
pub struct LightInfo {
    /// 0..1 scale applied to sky light (night and weather darkening).
    pub sky_brightness: f32,
}

impl Default for LightInfo {
    fn default() -> Self {
        Self {
            sky_brightness: 1.0,
        }
    }
}

#[derive(Debug)]
struct GpuModel {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

#[derive(Debug)]
struct ClassArray {
    texture: Option<wgpu::Texture>,
    view: Option<wgpu::TextureView>,
    capacity: u32,
    used: u32,
    generation: u32,
}

#[derive(Debug)]
struct Run {
    pipeline: Pipeline,
    class: u8,
    model: ModelId,
    first: u32,
    count: u32,
}

#[derive(Debug)]
pub struct EntityRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipelines: [wgpu::RenderPipeline; 4],
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    globals: wgpu::Buffer,
    output_srgb: bool,
    classes: [ClassArray; 5],
    bind_groups: [Option<(u32, wgpu::BindGroup)>; 5],
    models: Vec<Option<GpuModel>>,
    free_models: Vec<u32>,
    bones: wgpu::Buffer,
    bones_capacity: usize,
    instances: wgpu::Buffer,
    instances_capacity: usize,
    buffers_generation: u32,
    frame_bones: Vec<Bone34>,
    frame_instances: Vec<(u64, GpuInstance, [f64; 3])>,
    sorted: Vec<GpuInstance>,
    order: Vec<(u64, u32)>,
    runs: Vec<Run>,
}

fn storage_buffer(device: &wgpu::Device, label: &str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn sort_key(i: &PartInstance) -> u64 {
    (i.pipeline as u64) << 40 | u64::from(i.texture.class) << 32 | u64::from(i.model.0)
}

fn pack_rgba(c: [u8; 4]) -> u32 {
    u32::from_le_bytes(c)
}

impl EntityRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, formats: TargetFormats) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("entity"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/entity.wgsl").into()),
        });
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("entity"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage(1),
                storage(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("entity"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Uint32, 2 => Float32x3, 3 => Float32x2
        ];
        let make =
            |entry: &str, blend: Option<wgpu::BlendState>, depth_write: bool, equal: bool| {
                let compare = match (formats.reverse_z, equal) {
                    (true, false) => wgpu::CompareFunction::Greater,
                    (true, true) => wgpu::CompareFunction::GreaterEqual,
                    (false, false) => wgpu::CompareFunction::Less,
                    (false, true) => wgpu::CompareFunction::LessEqual,
                };
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &module,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &attributes,
                        })],
                    },
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: formats.depth,
                        depth_write_enabled: Some(depth_write),
                        depth_compare: Some(compare),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: wgpu::MultisampleState {
                        count: formats.sample_count.max(1),
                        ..Default::default()
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &module,
                        entry_point: Some(entry),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: formats.color,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            };
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let pipelines = [
            make("fs_cutout", None, true, false),
            make(
                "fs_blend",
                Some(wgpu::BlendState::ALPHA_BLENDING),
                false,
                false,
            ),
            make(
                "fs_blend",
                Some(wgpu::BlendState::ALPHA_BLENDING),
                false,
                true,
            ),
            make("fs_additive", Some(additive), false, true),
        ];
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("entity"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("entity globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let class = || ClassArray {
            texture: None,
            view: None,
            capacity: 0,
            used: 0,
            generation: 0,
        };
        EntityRenderer {
            device: device.clone(),
            queue: queue.clone(),
            pipelines,
            layout,
            sampler,
            globals,
            output_srgb: formats.color.is_srgb(),
            classes: [class(), class(), class(), class(), class()],
            bind_groups: [None, None, None, None, None],
            models: Vec::new(),
            free_models: Vec::new(),
            bones: storage_buffer(device, "entity bones", INITIAL_CAPACITY * 48),
            bones_capacity: INITIAL_CAPACITY,
            instances: storage_buffer(device, "entity instances", INITIAL_CAPACITY * 48),
            instances_capacity: INITIAL_CAPACITY,
            buffers_generation: 0,
            frame_bones: Vec::new(),
            frame_instances: Vec::new(),
            sorted: Vec::new(),
            order: Vec::new(),
            runs: Vec::new(),
        }
    }

    pub fn add_model(&mut self, model: &Model) -> ModelId {
        let vertices = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("entity vertices"),
                contents: cast_slice(&model.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let indices = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("entity indices"),
                contents: cast_slice(&model.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let gpu = Some(GpuModel {
            vertices,
            indices,
            index_count: model.indices.len() as u32,
        });
        if let Some(slot) = self.free_models.pop() {
            self.models[slot as usize] = gpu;
            return ModelId(slot);
        }
        self.models.push(gpu);
        ModelId(self.models.len() as u32 - 1)
    }

    /// Releases a model's buffers; the id may be reused by later `add_model` calls.
    pub fn free_model(&mut self, id: ModelId) {
        if let Some(slot) = self.models.get_mut(id.0 as usize)
            && slot.take().is_some()
        {
            self.free_models.push(id.0);
        }
    }

    /// Uploads `image` into the smallest size-class array that fits; `None` when it exceeds the largest.
    pub fn add_texture(&mut self, image: &ImageRgba) -> Option<TexId> {
        let extent = image.width.max(image.height);
        let class = CLASS_SIZES.iter().position(|&s| s >= extent)?;
        let size = CLASS_SIZES[class];
        let expected = u64::from(image.width) * u64::from(image.height) * 4;
        if image.width == 0 || image.height == 0 || image.rgba.len() as u64 != expected {
            return None;
        }
        if self.classes[class].used >= MAX_CLASS_LAYERS[class] {
            return None;
        }
        if self.classes[class].used == self.classes[class].capacity {
            self.grow_class(class);
        }
        let array = &mut self.classes[class];
        let layer = array.used;
        array.used += 1;
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: array.texture.as_ref().expect("grown above"),
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &image.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
        Some(TexId {
            class: class as u8,
            layer: layer as u16,
            uv_scale: [
                image.width as f32 / size as f32,
                image.height as f32 / size as f32,
            ],
        })
    }

    fn grow_class(&mut self, class: usize) {
        let size = CLASS_SIZES[class];
        let array = &mut self.classes[class];
        let capacity = if array.capacity == 0 {
            CLASS_INITIAL_LAYERS[class]
        } else {
            array.capacity.saturating_mul(2)
        }
        .min(MAX_CLASS_LAYERS[class]);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("entity textures"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: capacity,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        if let Some(old) = &array.texture
            && array.used > 0
        {
            let mut encoder = self.device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_texture(
                old.as_image_copy(),
                texture.as_image_copy(),
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: array.used,
                },
            );
            self.queue.submit([encoder.finish()]);
        }
        array.view = Some(texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        }));
        array.texture = Some(texture);
        array.capacity = capacity;
        array.generation += 1;
    }

    pub fn begin_frame(&mut self) {
        self.frame_bones.clear();
        self.frame_instances.clear();
    }

    /// Reserves `count` world-space bone matrices and returns their base index plus the slice to fill.
    /// `None` once the frame's bone or instance budget is exhausted.
    pub fn alloc_bones(&mut self, count: usize) -> Option<(u32, &mut [Bone34])> {
        let base = self.frame_bones.len();
        let end = base.checked_add(count)?;
        if end > MAX_BONES || self.frame_instances.len() >= MAX_INSTANCES {
            return None;
        }
        self.frame_bones.resize(end, Bone34::IDENTITY);
        Some((base as u32, &mut self.frame_bones[base..]))
    }

    pub fn push(&mut self, inst: &PartInstance) {
        if self.frame_instances.len() >= MAX_INSTANCES
            || inst.bone_base as usize >= self.frame_bones.len()
        {
            return;
        }
        let flags = u32::from(inst.fullbright)
            | u32::from(inst.hurt) << 1
            | u32::from(inst.block_light.min(15)) << 8
            | u32::from(inst.sky_light.min(15)) << 12;
        self.frame_instances.push((
            sort_key(inst),
            GpuInstance {
                bone_base: inst.bone_base,
                layer: u32::from(inst.texture.layer),
                uv_scale: inst.texture.uv_scale,
                tint: pack_rgba(inst.tint),
                flags,
                uv_offset: inst.uv_offset,
                origin: [0.0; 3],
                pad: 0.0,
            },
            inst.origin,
        ));
    }

    pub fn instance_count(&self) -> usize {
        self.frame_instances.len()
    }

    fn rebuild_bind_group(&mut self, class: usize) {
        let Some(view) = &self.classes[class].view else {
            return;
        };
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("entity"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.bones.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.instances.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let generation = self.classes[class].generation + (self.buffers_generation << 16);
        self.bind_groups[class] = Some((generation, group));
    }

    /// Sorts and uploads the frame's instances; `view_proj` is camera-relative.
    pub fn prepare(&mut self, view_proj: Mat4, camera: [f64; 3], light: LightInfo) {
        if self.frame_instances.len() > MAX_INSTANCES {
            let d = |o: &[f64; 3]| (0..3).map(|i| (o[i] - camera[i]).powi(2)).sum::<f64>();
            self.frame_instances
                .sort_unstable_by(|a, b| d(&a.2).total_cmp(&d(&b.2)));
            self.frame_instances.truncate(MAX_INSTANCES);
        }
        self.order.clear();
        self.order.extend(
            self.frame_instances
                .iter()
                .enumerate()
                .map(|(i, e)| (e.0, i as u32)),
        );
        self.order.sort_unstable();
        self.sorted.clear();
        self.runs.clear();
        for &(_, at) in &self.order {
            let (key, inst, origin) = &self.frame_instances[at as usize];
            let model = (*key & 0xFFFF_FFFF) as u32;
            let class = ((*key >> 32) & 0xFF) as u8;
            let pipeline = match key >> 40 {
                0 => Pipeline::Cutout,
                1 => Pipeline::Translucent,
                2 => Pipeline::Decal,
                _ => Pipeline::Additive,
            };
            match self.runs.last_mut() {
                Some(run)
                    if run.model.0 == model && run.class == class && run.pipeline == pipeline =>
                {
                    run.count += 1;
                }
                _ => self.runs.push(Run {
                    pipeline,
                    class,
                    model: ModelId(model),
                    first: self.sorted.len() as u32,
                    count: 1,
                }),
            }
            let mut inst = *inst;
            inst.origin = [
                (origin[0] - camera[0]) as f32,
                (origin[1] - camera[1]) as f32,
                (origin[2] - camera[2]) as f32,
            ];
            self.sorted.push(inst);
        }

        let mut rebuild = false;
        if self.frame_bones.len() > self.bones_capacity {
            self.bones_capacity = self.frame_bones.len().next_power_of_two().min(MAX_BONES);
            self.bones = storage_buffer(&self.device, "entity bones", self.bones_capacity * 48);
            rebuild = true;
        }
        if self.sorted.len() > self.instances_capacity {
            self.instances_capacity = self.sorted.len().next_power_of_two().min(MAX_INSTANCES);
            self.instances = storage_buffer(
                &self.device,
                "entity instances",
                self.instances_capacity * 48,
            );
            rebuild = true;
        }
        if rebuild {
            self.buffers_generation += 1;
        }
        let generation_now =
            |s: &Self, c: usize| s.classes[c].generation + (s.buffers_generation << 16);
        for class in 0..self.classes.len() {
            let needed = self.runs.iter().any(|r| r.class as usize == class);
            let stale = self.bind_groups[class]
                .as_ref()
                .is_none_or(|(g, _)| *g != generation_now(self, class));
            if needed && stale {
                self.rebuild_bind_group(class);
            }
        }
        self.queue
            .write_buffer(&self.bones, 0, cast_slice(&self.frame_bones));
        self.queue
            .write_buffer(&self.instances, 0, cast_slice(&self.sorted));
        let light0 = Vec3::new(0.2, 1.0, -0.7).normalize();
        let light1 = Vec3::new(-0.2, 1.0, 0.7).normalize();
        self.queue.write_buffer(
            &self.globals,
            0,
            bytes_of(&Globals {
                view_proj: view_proj.to_cols_array(),
                light0: light0.extend(0.0).to_array(),
                light1: light1.extend(0.0).to_array(),
                params: [
                    light.sky_brightness.clamp(0.0, 1.0),
                    if self.output_srgb { 1.0 } else { 0.0 },
                    0.0,
                    0.0,
                ],
            }),
        );
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let mut current: Option<(Pipeline, u8)> = None;
        for run in &self.runs {
            if current != Some((run.pipeline, run.class)) {
                if current.map(|c| c.0) != Some(run.pipeline) {
                    pass.set_pipeline(&self.pipelines[run.pipeline as usize]);
                }
                if let Some((_, group)) = &self.bind_groups[run.class as usize] {
                    pass.set_bind_group(0, group, &[]);
                }
                current = Some((run.pipeline, run.class));
            }
            let Some(model) = &self.models[run.model.0 as usize] else {
                continue;
            };
            pass.set_vertex_buffer(0, model.vertices.slice(..));
            pass.set_index_buffer(model.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..model.index_count, 0, run.first..run.first + run.count);
        }
    }

    pub fn model_count(&self) -> usize {
        self.models.len() - self.free_models.len()
    }

    pub fn draw_count(&self) -> usize {
        self.runs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_image_sizes_do_not_overflow() {
        let img = ImageRgba {
            width: 65536,
            height: 65536,
            rgba: Vec::new(),
        };
        assert_eq!(
            CLASS_SIZES
                .iter()
                .position(|&s| s >= img.width.max(img.height)),
            None
        );
        let expected = u64::from(70000u32) * u64::from(70000u32) * 4;
        assert!(expected > u64::from(u32::MAX));
    }
}
