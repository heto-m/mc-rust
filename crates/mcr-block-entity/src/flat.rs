use bytemuck::{Pod, Zeroable, bytes_of, cast_slice};
use glam::Mat4;
use mcr_entity_model::{ImageRgba, TargetFormats};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, PartialEq)]
pub struct FlatVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FlatKind {
    Portal,
    BeamSolid,
    BeamGlow,
    Text,
    /// Text display with polygon offset, as vanilla's default display mode.
    DisplayText,
    /// Text display with `see_through`: ignores the depth buffer.
    DisplayTextSeeThrough,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FlatBind {
    Portal,
    Gateway,
    Beam,
    GatewayBeam,
    /// The white pixel of glyph page 0, for text backgrounds; sorts before the pages that draw over it.
    Background,
    Page(u32),
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [f32; 16],
    params: [f32; 4],
    fog: [f32; 4],
    fog_color: [f32; 4],
}

#[derive(Debug)]
struct Group {
    kind: FlatKind,
    bind: FlatBind,
    /// Back-to-front rank of a text display; its draws sort by this before the bind.
    seq: u32,
    vertices: Vec<FlatVertex>,
}

#[derive(Debug)]
struct Draw {
    kind: FlatKind,
    bind: FlatBind,
    first_quad: u32,
    quads: u32,
}

#[derive(Debug)]
struct Texture {
    view: wgpu::TextureView,
    group: Option<wgpu::BindGroup>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DepthMode {
    Test,
    Offset,
    Always,
}

pub const MAX_QUADS: usize = 1 << 17;

/// Pipelines for geometry that does not fit the skinned entity renderer: portals, beam shells and sign text.
#[derive(Debug)]
pub struct FlatRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipelines: [wgpu::RenderPipeline; 6],
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    repeat: wgpu::Sampler,
    clamp: wgpu::Sampler,
    white: wgpu::TextureView,
    sky: Option<Texture>,
    portal: Option<Texture>,
    beam: Option<Texture>,
    gateway_beam: Option<Texture>,
    pages: Vec<Texture>,
    page_revision: u64,
    output_srgb: bool,
    groups: Vec<Group>,
    quads: usize,
    draws: Vec<Draw>,
    vertices: wgpu::Buffer,
    vertex_capacity: usize,
    indices: wgpu::Buffer,
    index_quads: usize,
    portal_layers: f32,
    fog: [f32; 4],
    fog_color: [f32; 4],
}

/// Groups empty last frame are dropped, or text-display ranks long gone would pile up.
fn begin_groups(groups: &mut Vec<Group>) {
    groups.retain(|g| !g.vertices.is_empty());
    for g in groups.iter_mut() {
        g.vertices.clear();
    }
}

fn push_group(
    groups: &mut Vec<Group>,
    kind: FlatKind,
    bind: FlatBind,
    seq: u32,
    quad: &[FlatVertex; 4],
) {
    let at = match groups
        .iter()
        .position(|g| g.kind == kind && g.bind == bind && g.seq == seq)
    {
        Some(i) => i,
        None => {
            groups.push(Group {
                kind,
                bind,
                seq,
                vertices: Vec::new(),
            });
            groups.len() - 1
        }
    };
    groups[at].vertices.extend_from_slice(quad);
}

fn index_data(quads: usize) -> Vec<u32> {
    (0..quads as u32)
        .flat_map(|q| {
            let b = q * 4;
            [b, b + 1, b + 2, b, b + 2, b + 3]
        })
        .collect()
}

fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &ImageRgba,
    layout: &wgpu::BindGroupLayout,
    white: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    second: Option<&wgpu::TextureView>,
) -> Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("flat texture"),
        size: wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
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
    let view = texture.create_view(&Default::default());
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("flat texture"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(second.unwrap_or(white)),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    Texture {
        view,
        group: Some(group),
    }
}

impl FlatRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, formats: TargetFormats) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("flat"),
            source: wgpu::ShaderSource::Wgsl(include_str!("flat.wgsl").into()),
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("flat globals"),
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
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("flat textures"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("flat"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x2, 2 => Unorm8x4
        ];
        let make =
            |entry: &str, blend: Option<wgpu::BlendState>, depth_write: bool, mode: DepthMode| {
                let compare = match (mode, formats.reverse_z) {
                    (DepthMode::Always, _) => wgpu::CompareFunction::Always,
                    (_, true) => wgpu::CompareFunction::Greater,
                    (_, false) => wgpu::CompareFunction::Less,
                };
                let bias = match mode {
                    DepthMode::Offset => wgpu::DepthBiasState {
                        constant: if formats.reverse_z { 16 } else { -16 },
                        slope_scale: if formats.reverse_z { 2.0 } else { -2.0 },
                        clamp: 0.0,
                    },
                    _ => Default::default(),
                };
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &module,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<FlatVertex>() as u64,
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
                        bias,
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
        let alpha = Some(wgpu::BlendState::ALPHA_BLENDING);
        let pipelines = [
            make("fs_portal", None, true, DepthMode::Test),
            make("fs_tex", alpha, true, DepthMode::Test),
            make("fs_tex", alpha, false, DepthMode::Test),
            make("fs_tex", alpha, false, DepthMode::Test),
            make("fs_tex", alpha, false, DepthMode::Offset),
            make("fs_tex", alpha, false, DepthMode::Always),
        ];
        let repeat = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("flat repeat"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let clamp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("flat clamp"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("flat globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("flat globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let white_tex = upload(
            device,
            queue,
            &ImageRgba::solid(1, 1, [255; 4]),
            &texture_layout,
            &device
                .create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default()),
            &clamp,
            None,
        );
        let vertex_capacity = 4096;
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("flat vertices"),
            size: (vertex_capacity * std::mem::size_of::<FlatVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_quads = vertex_capacity / 4;
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("flat indices"),
            contents: cast_slice(&index_data(index_quads)),
            usage: wgpu::BufferUsages::INDEX,
        });
        FlatRenderer {
            device: device.clone(),
            queue: queue.clone(),
            pipelines,
            globals,
            globals_group,
            texture_layout,
            repeat,
            clamp,
            white: white_tex.view,
            sky: None,
            portal: None,
            beam: None,
            gateway_beam: None,
            pages: Vec::new(),
            page_revision: u64::MAX,
            output_srgb: formats.color.is_srgb(),
            groups: Vec::new(),
            quads: 0,
            draws: Vec::new(),
            vertices,
            vertex_capacity,
            indices,
            index_quads,
            portal_layers: 15.0,
            fog: [1.0e9; 4],
            fog_color: [0.0; 4],
        }
    }

    pub fn set_portal_textures(&mut self, sky: &ImageRgba, portal: &ImageRgba) {
        let portal_tex = upload(
            &self.device,
            &self.queue,
            portal,
            &self.texture_layout,
            &self.white,
            &self.repeat,
            None,
        );
        let sky_tex = upload(
            &self.device,
            &self.queue,
            sky,
            &self.texture_layout,
            &self.white,
            &self.repeat,
            Some(&portal_tex.view),
        );
        self.sky = Some(sky_tex);
        self.portal = Some(portal_tex);
    }

    pub fn set_beam_texture(&mut self, gateway: bool, image: &ImageRgba) {
        let tex = upload(
            &self.device,
            &self.queue,
            image,
            &self.texture_layout,
            &self.white,
            &self.repeat,
            None,
        );
        if gateway {
            self.gateway_beam = Some(tex);
        } else {
            self.beam = Some(tex);
        }
    }

    pub fn has_portal(&self) -> bool {
        self.sky.is_some()
    }

    pub fn has_beam(&self, gateway: bool) -> bool {
        if gateway {
            self.gateway_beam.is_some()
        } else {
            self.beam.is_some()
        }
    }

    /// Mirrors glyph atlas pages (RGBA, square) when the atlas revision changed.
    pub fn sync_pages<'a>(
        &mut self,
        revision: u64,
        size: u32,
        pages: impl ExactSizeIterator<Item = &'a [u8]>,
    ) {
        if revision == self.page_revision {
            return;
        }
        self.page_revision = revision;
        self.pages.clear();
        for pixels in pages {
            let image = ImageRgba {
                width: size,
                height: size,
                rgba: pixels.to_vec(),
            };
            self.pages.push(upload(
                &self.device,
                &self.queue,
                &image,
                &self.texture_layout,
                &self.white,
                &self.clamp,
                None,
            ));
        }
    }

    pub fn begin_frame(&mut self) {
        begin_groups(&mut self.groups);
        self.quads = 0;
        self.draws.clear();
    }

    pub fn quad_count(&self) -> usize {
        self.quads
    }

    pub fn push_quad(&mut self, kind: FlatKind, bind: FlatBind, quad: [FlatVertex; 4]) {
        self.push_quad_ranked(kind, bind, 0, quad);
    }

    /// Like `push_quad`; display text of both kinds draws in ascending `seq`, so callers number
    /// the displays from the farthest to the nearest.
    pub fn push_quad_ranked(
        &mut self,
        kind: FlatKind,
        bind: FlatBind,
        seq: u32,
        quad: [FlatVertex; 4],
    ) {
        if self.quads >= MAX_QUADS {
            return;
        }
        push_group(&mut self.groups, kind, bind, seq, &quad);
        self.quads += 1;
    }

    pub fn set_fog(&mut self, fog: [f32; 4], color: [f32; 3]) {
        self.fog = fog;
        self.fog_color = [color[0], color[1], color[2], 1.0];
    }

    pub fn prepare(&mut self, view_proj: Mat4, time: f32, portal_gateway: bool) {
        self.portal_layers = if portal_gateway { 16.0 } else { 15.0 };
        let mut order: Vec<usize> = (0..self.groups.len())
            .filter(|&i| !self.groups[i].vertices.is_empty())
            .collect();
        order.sort_by_key(|&i| {
            let g = &self.groups[i];
            match g.kind {
                FlatKind::DisplayText | FlatKind::DisplayTextSeeThrough => {
                    (FlatKind::DisplayText, g.seq, g.bind)
                }
                kind => (kind, 0, g.bind),
            }
        });
        let total: usize = order.iter().map(|&i| self.groups[i].vertices.len()).sum();
        if total > self.vertex_capacity {
            self.vertex_capacity = total.next_power_of_two();
            self.vertices = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("flat vertices"),
                size: (self.vertex_capacity * std::mem::size_of::<FlatVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if self.vertex_capacity / 4 > self.index_quads {
            self.index_quads = self.vertex_capacity / 4;
            self.indices = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("flat indices"),
                    contents: cast_slice(&index_data(self.index_quads)),
                    usage: wgpu::BufferUsages::INDEX,
                });
        }
        let mut all: Vec<FlatVertex> = Vec::with_capacity(total);
        for i in order {
            let g = &self.groups[i];
            self.draws.push(Draw {
                kind: g.kind,
                bind: g.bind,
                first_quad: (all.len() / 4) as u32,
                quads: (g.vertices.len() / 4) as u32,
            });
            all.extend_from_slice(&g.vertices);
        }
        if !all.is_empty() {
            self.queue.write_buffer(&self.vertices, 0, cast_slice(&all));
        }
        self.queue.write_buffer(
            &self.globals,
            0,
            bytes_of(&Globals {
                view_proj: view_proj.to_cols_array(),
                params: [
                    time,
                    if self.output_srgb { 1.0 } else { 0.0 },
                    self.portal_layers,
                    0.0,
                ],
                fog: self.fog,
                fog_color: self.fog_color,
            }),
        );
    }

    fn group_for(&self, bind: FlatBind) -> Option<&wgpu::BindGroup> {
        let texture = match bind {
            FlatBind::Portal | FlatBind::Gateway => self.sky.as_ref(),
            FlatBind::Beam => self.beam.as_ref(),
            FlatBind::GatewayBeam => self.gateway_beam.as_ref(),
            FlatBind::Background => self.pages.first(),
            FlatBind::Page(i) => self.pages.get(i as usize),
        };
        texture.and_then(|t| t.group.as_ref())
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw_filtered(pass, |_| true);
    }

    pub fn draw_opaque(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw_filtered(pass, |k| {
            matches!(k, FlatKind::Portal | FlatKind::BeamSolid)
        });
    }

    pub fn draw_blended(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw_filtered(pass, |k| {
            matches!(
                k,
                FlatKind::BeamGlow
                    | FlatKind::Text
                    | FlatKind::DisplayText
                    | FlatKind::DisplayTextSeeThrough
            )
        });
    }

    fn draw_filtered(&self, pass: &mut wgpu::RenderPass<'_>, keep: impl Fn(FlatKind) -> bool) {
        if self.draws.is_empty() {
            return;
        }
        pass.set_bind_group(0, &self.globals_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut current = None;
        for d in self.draws.iter().filter(|d| keep(d.kind)) {
            let Some(group) = self.group_for(d.bind) else {
                continue;
            };
            if current != Some(d.kind) {
                pass.set_pipeline(&self.pipelines[d.kind as usize]);
                current = Some(d.kind);
            }
            pass.set_bind_group(1, group, &[]);
            pass.draw_indexed(d.first_quad * 6..(d.first_quad + d.quads) * 6, 0, 0..1);
        }
    }

    pub fn draw_calls(&self) -> usize {
        self.draws.len()
    }
}

#[cfg(test)]
mod group_tests {
    use super::*;

    #[test]
    fn groups_of_vanished_text_displays_are_dropped() {
        let mut groups = Vec::new();
        let quad = [FlatVertex::zeroed(); 4];
        for frame in 0..1000u32 {
            begin_groups(&mut groups);
            for seq in frame..frame + 8 {
                push_group(
                    &mut groups,
                    FlatKind::DisplayText,
                    FlatBind::Background,
                    seq,
                    &quad,
                );
            }
        }
        assert!(groups.len() <= 16, "{}", groups.len());
    }
}
