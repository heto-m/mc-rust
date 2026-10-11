use std::ops::Range;

use wgpu::util::DeviceExt;

use crate::atlas::Atlas;
use crate::ui::{PageKind, Ui, Vertex};

const INITIAL_QUADS: usize = 4096;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    scale: [f32; 2],
    srgb_target: f32,
    pad: f32,
}

#[derive(Debug)]
struct GpuPage {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

#[derive(Debug)]
struct Draw {
    kind: PageKind,
    page: usize,
    scissor: [u32; 4],
    indices: Range<u32>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UiStats {
    pub draw_calls: usize,
    pub quads: usize,
}

/// GPU side of the UI: uploads atlas pages and vertices, then draws inside a caller-owned render pass.
#[derive(Debug)]
pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    page_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    globals: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    srgb_target: bool,
    sprite_pages: Vec<GpuPage>,
    glyph_pages: Vec<GpuPage>,
    vertices: wgpu::Buffer,
    vertex_capacity: usize,
    indices: wgpu::Buffer,
    index_quads: usize,
    draws: Vec<Draw>,
    stats: UiStats,
}

fn index_data(quads: usize) -> Vec<u32> {
    (0..quads as u32)
        .flat_map(|q| {
            let b = q * 4;
            [b, b + 1, b + 2, b, b + 2, b + 3]
        })
        .collect()
}

impl UiRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let page_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui page"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[Some(&globals_layout), Some(&page_layout)],
            immediate_size: 0,
        });
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Unorm8x4
                    ],
                })],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState {
                        color: premultiplied,
                        alpha: premultiplied,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui nearest"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let vertices = Self::vertex_buffer(device, INITIAL_QUADS);
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ui indices"),
            contents: bytemuck::cast_slice(&index_data(INITIAL_QUADS)),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            pipeline,
            page_layout,
            sampler,
            globals,
            globals_bg,
            srgb_target: target_format.is_srgb(),
            sprite_pages: Vec::new(),
            glyph_pages: Vec::new(),
            vertices,
            vertex_capacity: INITIAL_QUADS,
            indices,
            index_quads: INITIAL_QUADS,
            draws: Vec::new(),
            stats: UiStats::default(),
        }
    }

    fn vertex_buffer(device: &wgpu::Device, quads: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui vertices"),
            size: (quads * 4 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn stats(&self) -> UiStats {
        self.stats
    }

    /// Drops the GPU copies of the atlas pages; call after swapping in a freshly loaded `Ui`.
    pub fn reset_pages(&mut self) {
        self.sprite_pages.clear();
        self.glyph_pages.clear();
    }

    fn sync_pages(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        atlas: &mut Atlas,
        gpu: &mut Vec<GpuPage>,
    ) {
        let size = atlas.size();
        for (i, page) in atlas.pages_mut().iter_mut().enumerate() {
            if i >= gpu.len() {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("ui page"),
                    size: wgpu::Extent3d {
                        width: size,
                        height: size,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                let view = texture.create_view(&Default::default());
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("ui page"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ],
                });
                gpu.push(GpuPage {
                    texture,
                    bind_group,
                });
                page.take_dirty();
                Self::upload_rows(queue, &gpu[i].texture, &page.pixels, size, 0, size);
            } else if let Some((y0, y1)) = page.take_dirty() {
                Self::upload_rows(queue, &gpu[i].texture, &page.pixels, size, y0, y1);
            }
        }
    }

    fn upload_rows(
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        pixels: &[u8],
        size: u32,
        y0: u32,
        y1: u32,
    ) {
        let row = (size * 4) as usize;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: y0, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &pixels[y0 as usize * row..y1 as usize * row],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 4),
                rows_per_image: Some(y1 - y0),
            },
            wgpu::Extent3d {
                width: size,
                height: y1 - y0,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Uploads dirty atlas rows and this frame's vertices. Call before `draw`, outside the render pass.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, ui: &mut Ui) {
        Self::sync_pages(
            device,
            queue,
            &self.page_layout,
            &self.sampler,
            ui.sprites_mut().atlas_mut(),
            &mut self.sprite_pages,
        );
        Self::sync_pages(
            device,
            queue,
            &self.page_layout,
            &self.sampler,
            ui.fonts_mut().atlas_mut(),
            &mut self.glyph_pages,
        );
        let batch = ui.batch();
        let quads = batch.quad_count();
        if quads > self.vertex_capacity {
            self.vertex_capacity = quads.next_power_of_two();
            self.vertices = Self::vertex_buffer(device, self.vertex_capacity);
        }
        if quads > self.index_quads {
            self.index_quads = quads.next_power_of_two();
            self.indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ui indices"),
                contents: bytemuck::cast_slice(&index_data(self.index_quads)),
                usage: wgpu::BufferUsages::INDEX,
            });
        }
        queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&batch.vertices));
        let (fw, fh) = batch.framebuffer;
        let scale = batch.scale as f32;
        queue.write_buffer(
            &self.globals,
            0,
            bytemuck::bytes_of(&Globals {
                scale: [
                    2.0 * scale / fw.max(1) as f32,
                    -2.0 * scale / fh.max(1) as f32,
                ],
                srgb_target: f32::from(u8::from(self.srgb_target)),
                pad: 0.0,
            }),
        );
        self.draws.clear();
        self.draws.extend(
            batch
                .cmds
                .iter()
                .filter(|c| c.scissor[2] > 0 && c.scissor[3] > 0)
                .map(|c| Draw {
                    kind: c.page.kind,
                    page: c.page.index as usize,
                    scissor: c.scissor,
                    indices: c.quads.start * 6..c.quads.end * 6,
                }),
        );
        self.stats = UiStats {
            draw_calls: self.draws.len(),
            quads,
        };
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.draws.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.globals_bg, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        for d in &self.draws {
            let pages = match d.kind {
                PageKind::Sprite => &self.sprite_pages,
                PageKind::Glyph => &self.glyph_pages,
            };
            let [x, y, w, h] = d.scissor;
            pass.set_scissor_rect(x, y, w, h);
            pass.set_bind_group(1, &pages[d.page].bind_group, &[]);
            pass.draw_indexed(d.indices.clone(), 0, 0..1);
        }
    }
}
