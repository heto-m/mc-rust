use mcr_assets::{ResourceManager, decode_sprite};
use wgpu::util::DeviceExt;

const FACE_PATH: &str = "assets/minecraft/textures/gui/title/background/panorama_";
/// `CubeMapTexture`: array layers +X, -X, +Y, -Y, +Z, -Z come from these files.
const LAYER_FILES: [u8; 6] = [1, 3, 5, 4, 0, 2];
const MAX_FACE: u32 = 4096;
const FOV_DEGREES: f32 = 85.0;
const NEAR: f32 = 0.05;
const FAR: f32 = 10.0;
const PITCH_DEGREES: f32 = 10.0;

/// Vanilla's cube, one quad per face, wound as `CubeMap#initializeVertices` does.
const CUBE: [[f32; 3]; 24] = [
    [-1.0, -1.0, 1.0],
    [-1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, 1.0, -1.0],
    [-1.0, -1.0, -1.0],
    [-1.0, -1.0, -1.0],
    [-1.0, 1.0, -1.0],
    [-1.0, 1.0, 1.0],
    [-1.0, -1.0, 1.0],
    [-1.0, -1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, -1.0, -1.0],
    [-1.0, 1.0, 1.0],
    [-1.0, 1.0, -1.0],
    [1.0, 1.0, -1.0],
    [1.0, 1.0, 1.0],
];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    srgb_target: [f32; 4],
}

struct Faces {
    size: u32,
    rgba: Vec<u8>,
}

#[derive(Debug)]
struct Gpu {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    globals: wgpu::Buffer,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    srgb_target: bool,
}

/// What the panorama draws into.
#[derive(Clone, Copy, Debug)]
pub struct PanoramaTarget<'a> {
    pub view: &'a wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    pub size: (u32, u32),
}

/// The title screen's rotating cubemap (`CubeMap`): six faces from the resource packs, drawn before the GUI.
#[derive(Debug)]
pub struct Panorama {
    faces: Option<(u32, Vec<u8>)>,
    format: Option<wgpu::TextureFormat>,
    gpu: Option<Gpu>,
}

impl Panorama {
    /// `None` when a face is missing, unreadable, not square, oversized or differs in size from the first.
    pub fn load(resources: &ResourceManager) -> Option<Self> {
        let faces = read_faces(resources)?;
        Some(Self {
            faces: Some((faces.size, faces.rgba)),
            format: None,
            gpu: None,
        })
    }

    /// Draws the cube over the whole target, turned `spin` degrees (`PanoramaRenderState#spin`).
    pub fn record(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: PanoramaTarget<'_>,
        spin: f32,
    ) {
        let PanoramaTarget {
            view: color,
            format,
            size,
        } = target;
        if self.format != Some(format) {
            self.gpu = None;
        }
        if self.gpu.is_none() {
            let Some((face, rgba)) = &self.faces else {
                return;
            };
            self.gpu = Some(Gpu::new(device, queue, format, *face, rgba));
            self.format = Some(format);
        }
        let Some(gpu) = &self.gpu else { return };
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        let globals = Globals {
            view_proj: view_projection(aspect, PITCH_DEGREES, spin),
            srgb_target: [f32::from(u8::from(gpu.srgb_target)); 4],
        };
        queue.write_buffer(&gpu.globals, 0, bytemuck::bytes_of(&globals));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("panorama"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&gpu.pipeline);
        pass.set_bind_group(0, &gpu.bind_group, &[]);
        pass.set_vertex_buffer(0, gpu.vertices.slice(..));
        pass.set_index_buffer(gpu.indices.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..36, 0, 0..1);
    }
}

fn read_faces(resources: &ResourceManager) -> Option<Faces> {
    let mut size = None;
    let mut layers = Vec::new();
    for file in LAYER_FILES {
        let path = format!("{FACE_PATH}{file}.png");
        let png = resources.read(&path).ok().flatten()?;
        let sprite = decode_sprite(&path, &png, None).ok()?;
        let face = *size.get_or_insert(sprite.width);
        if sprite.width != face || sprite.height != face || face == 0 || face > MAX_FACE {
            tracing::warn!("panorama face {path} is not a {face}px square within bounds");
            return None;
        }
        layers.push(flipped_rows(&sprite.frames.into_iter().next()?, face));
    }
    Some(Faces {
        size: size?,
        rgba: layers.concat(),
    })
}

/// `CubeMapTexture` stacks the faces with `copyRect(.., mirrorY = true)`.
fn flipped_rows(rgba: &[u8], face: u32) -> Vec<u8> {
    let row = face as usize * 4;
    rgba.chunks_exact(row).rev().flatten().copied().collect()
}

impl Gpu {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        face: u32,
        rgba: &[u8],
    ) -> Self {
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("panorama cube"),
                size: wgpu::Extent3d {
                    width: face,
                    height: face,
                    depth_or_array_layers: 6,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("panorama"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("panorama globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("panorama"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("panorama"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("panorama"),
            source: wgpu::ShaderSource::Wgsl(include_str!("panorama.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("panorama"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("panorama"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("panorama vertices"),
            contents: bytemuck::cast_slice(&CUBE),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices: Vec<u16> = (0..6u16)
            .flat_map(|q| {
                let b = q * 4;
                [b, b + 1, b + 2, b, b + 2, b + 3]
            })
            .collect();
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("panorama indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            pipeline,
            bind_group,
            globals,
            vertices,
            indices,
            srgb_target: format.is_srgb(),
        }
    }
}

type Mat3 = [[f32; 3]; 3];

fn mul3(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    out
}

fn rotation_x(radians: f32) -> Mat3 {
    let (s, c) = radians.sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}

fn rotation_y(radians: f32) -> Mat3 {
    let (s, c) = radians.sin_cos();
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}

/// `CubeMap#render`: `perspective(85°) * rotX(180°) * rotX(pitch) * rotY(yaw)`, column-major, depth 0..1.
fn view_projection(aspect: f32, pitch: f32, yaw: f32) -> [[f32; 4]; 4] {
    let view = mul3(
        &mul3(
            &rotation_x(std::f32::consts::PI),
            &rotation_x(pitch.to_radians()),
        ),
        &rotation_y(yaw.to_radians()),
    );
    let f = 1.0 / (FOV_DEGREES.to_radians() / 2.0).tan();
    let depth = FAR / (NEAR - FAR);
    let row = |r: usize| -> [f32; 3] { view[r] };
    let mut out = [[0.0; 4]; 4];
    for (c, column) in out.iter_mut().take(3).enumerate() {
        *column = [
            f / aspect * row(0)[c],
            f * row(1)[c],
            depth * row(2)[c],
            -row(2)[c],
        ];
    }
    out[3][2] = NEAR * FAR / (NEAR - FAR);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(m: &[[f32; 4]; 4], v: [f32; 3]) -> [f32; 4] {
        let mut out = [0.0; 4];
        for (r, o) in out.iter_mut().enumerate() {
            *o = (0..3).map(|c| m[c][r] * v[c]).sum::<f32>() + m[3][r];
        }
        out
    }

    #[test]
    fn straight_ahead_lands_mid_screen_at_ten_degrees_pitch() {
        let m = view_projection(1.0, 0.0, 0.0);
        let [x, y, z, w] = project(&m, [0.0, 0.0, 1.0]);
        assert!(x.abs() < 1e-5 && y.abs() < 1e-5);
        assert!(w > 0.0 && z / w > 0.0 && z / w < 1.0);
    }

    #[test]
    fn corners_in_front_of_the_near_plane_stay_inside_the_depth_range() {
        for spin in [-180.0, -37.0, 0.0, 91.0] {
            let m = view_projection(16.0 / 9.0, 10.0, spin);
            for v in CUBE {
                let [_, _, z, w] = project(&m, v);
                assert!(w.abs() < 10.0);
                if w >= NEAR {
                    assert!(z >= 0.0 && z <= w);
                }
            }
        }
    }
}
