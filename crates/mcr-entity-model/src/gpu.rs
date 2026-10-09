use bytemuck::{Pod, Zeroable, bytes_of, cast_slice};
use glam::{Mat4, Vec2};
use wgpu::util::DeviceExt;

use crate::assets::ImageRgba;
use crate::cube::Vertex;
use crate::model::Model;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureLayer(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelHandle(usize);

/// Entity textures are padded into array layers of the largest texture's size.
#[derive(Debug, Default)]
pub struct TextureArrayBuilder {
    images: Vec<ImageRgba>,
}

impl TextureArrayBuilder {
    pub fn add(&mut self, image: ImageRgba) -> TextureLayer {
        self.images.push(image);
        TextureLayer(self.images.len() as u32 - 1)
    }
}

/// One entity to draw: `transform` maps model space (sixteenths, y down) to world space.
#[derive(Debug, Clone, Copy)]
pub struct Instance<'a> {
    pub transform: Mat4,
    pub bones: &'a [Mat4],
    pub texture: TextureLayer,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [f32; 16],
    light_dir: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuInstance {
    bone_base: u32,
    layer: u32,
    uv_scale: [f32; 2],
}

#[derive(Debug)]
struct GpuModel {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    bone_count: usize,
}

#[derive(Debug)]
struct Draw {
    model: usize,
    first: u32,
    count: u32,
}

#[derive(Debug)]
pub struct EntityRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    globals: wgpu::Buffer,
    texture_view: wgpu::TextureView,
    layer_uv_scale: Vec<Vec2>,
    sampler: wgpu::Sampler,
    models: Vec<GpuModel>,
    bones: wgpu::Buffer,
    bones_capacity: usize,
    instances: wgpu::Buffer,
    instances_capacity: usize,
    bind_group: wgpu::BindGroup,
    draws: Vec<Draw>,
}

const INITIAL_CAPACITY: usize = 1024;

impl EntityRenderer {
    /// `reverse_z` selects `Greater` depth comparison to match a reverse-Z camera.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
        reverse_z: bool,
        textures: &TextureArrayBuilder,
    ) -> Self {
        let (texture_view, layer_uv_scale) = upload_textures(device, queue, textures);
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
                    visibility: wgpu::ShaderStages::VERTEX,
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
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("entity"),
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
                format: depth_format,
                depth_write_enabled: Some(true),
                depth_compare: Some(if reverse_z {
                    wgpu::CompareFunction::Greater
                } else {
                    wgpu::CompareFunction::Less
                }),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(color_format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
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
        let bones = storage_buffer(device, "entity bones", INITIAL_CAPACITY * 64);
        let instances = storage_buffer(device, "entity instances", INITIAL_CAPACITY * 16);
        let bind_group = make_bind_group(
            device,
            &layout,
            &globals,
            &bones,
            &instances,
            &texture_view,
            &sampler,
        );
        EntityRenderer {
            pipeline,
            layout,
            globals,
            texture_view,
            layer_uv_scale,
            sampler,
            models: Vec::new(),
            bones,
            bones_capacity: INITIAL_CAPACITY,
            instances,
            instances_capacity: INITIAL_CAPACITY,
            bind_group,
            draws: Vec::new(),
        }
    }

    pub fn add_model(&mut self, device: &wgpu::Device, model: &Model) -> ModelHandle {
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("entity vertices"),
            contents: cast_slice(&model.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("entity indices"),
            contents: cast_slice(&model.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.models.push(GpuModel {
            vertices,
            indices,
            index_count: model.indices.len() as u32,
            bone_count: model.bones.len(),
        });
        ModelHandle(self.models.len() - 1)
    }

    /// Uploads this frame's instances (bone matrices pre-multiplied by each transform); one draw per model.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_proj: Mat4,
        batches: &[(ModelHandle, &[Instance])],
    ) {
        let mut bone_mats: Vec<Mat4> = Vec::new();
        let mut records: Vec<GpuInstance> = Vec::new();
        self.draws.clear();
        for (handle, instances) in batches {
            let model = &self.models[handle.0];
            let first = records.len() as u32;
            for inst in *instances {
                debug_assert_eq!(inst.bones.len(), model.bone_count);
                let scale = self.layer_uv_scale[inst.texture.0 as usize];
                records.push(GpuInstance {
                    bone_base: bone_mats.len() as u32,
                    layer: inst.texture.0,
                    uv_scale: scale.to_array(),
                });
                bone_mats.extend(inst.bones.iter().map(|b| inst.transform * *b));
            }
            self.draws.push(Draw {
                model: handle.0,
                first,
                count: instances.len() as u32,
            });
        }

        let mut rebuild = false;
        if bone_mats.len() > self.bones_capacity {
            self.bones_capacity = bone_mats.len().next_power_of_two();
            self.bones = storage_buffer(device, "entity bones", self.bones_capacity * 64);
            rebuild = true;
        }
        if records.len() > self.instances_capacity {
            self.instances_capacity = records.len().next_power_of_two();
            self.instances =
                storage_buffer(device, "entity instances", self.instances_capacity * 16);
            rebuild = true;
        }
        if rebuild {
            self.bind_group = make_bind_group(
                device,
                &self.layout,
                &self.globals,
                &self.bones,
                &self.instances,
                &self.texture_view,
                &self.sampler,
            );
        }
        queue.write_buffer(&self.bones, 0, cast_slice(&bone_mats));
        queue.write_buffer(&self.instances, 0, cast_slice(&records));
        queue.write_buffer(
            &self.globals,
            0,
            bytes_of(&Globals {
                view_proj: view_proj.to_cols_array(),
                light_dir: [0.3, 0.9, -0.5, 0.0],
            }),
        );
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        for d in &self.draws {
            let model = &self.models[d.model];
            pass.set_vertex_buffer(0, model.vertices.slice(..));
            pass.set_index_buffer(model.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..model.index_count, 0, d.first..d.first + d.count);
        }
    }
}

fn storage_buffer(device: &wgpu::Device, label: &str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn make_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    bones: &wgpu::Buffer,
    instances: &wgpu::Buffer,
    texture: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("entity"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: bones.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: instances.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(texture),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn upload_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    builder: &TextureArrayBuilder,
) -> (wgpu::TextureView, Vec<Vec2>) {
    let width = builder.images.iter().map(|i| i.width).max().unwrap_or(1);
    let height = builder.images.iter().map(|i| i.height).max().unwrap_or(1);
    let layers = builder.images.len().max(1) as u32;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("entity textures"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut scales = Vec::new();
    for (layer, img) in builder.images.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer as u32,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &img.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(img.width * 4),
                rows_per_image: Some(img.height),
            },
            wgpu::Extent3d {
                width: img.width,
                height: img.height,
                depth_or_array_layers: 1,
            },
        );
        scales.push(Vec2::new(
            img.width as f32 / width as f32,
            img.height as f32 / height as f32,
        ));
    }
    if scales.is_empty() {
        scales.push(Vec2::ONE);
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    (view, scales)
}
