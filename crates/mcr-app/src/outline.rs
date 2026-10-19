use mcr_render::glam::{DVec3, Vec3, Vec4};
use mcr_render::wgpu::{self, util::DeviceExt};
use mcr_render::{Camera, OverlayCtx};

const SHADER: &str = "
struct In { @location(0) pos: vec4<f32>, @location(1) color: vec4<f32> };
struct Out { @builtin(position) pos: vec4<f32>, @location(0) color: vec4<f32> };
@vertex fn vs(v: In) -> Out { var o: Out; o.pos = v.pos; o.color = v.color; return o; }
@fragment fn fs(i: Out) -> @location(0) vec4<f32> { return i.color; }
";
const MAX_VERTICES: usize = 1 << 16;
const NEAR_W: f32 = 0.06;
const DEPTH_BIAS: f32 = 1.003;
const LINE_PX: f32 = 2.0;
const COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.4];

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 4],
    color: [f32; 4],
}

const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (1, 3),
    (3, 2),
    (2, 0),
    (4, 5),
    (5, 7),
    (7, 6),
    (6, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

#[derive(Debug, Clone, Copy)]
pub struct WorldBox {
    pub min: DVec3,
    pub max: DVec3,
}

/// A coloured debug line, `width` in pixels.
#[derive(Debug, Clone, Copy)]
pub struct WorldLine {
    pub a: DVec3,
    pub b: DVec3,
    pub color: [f32; 4],
    pub width: f32,
}

impl WorldLine {
    /// The twelve edges of a box.
    pub fn cuboid(min: DVec3, max: DVec3, color: [f32; 4], width: f32, out: &mut Vec<Self>) {
        let corner = |i: usize| {
            DVec3::new(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
            )
        };
        out.extend(EDGES.iter().map(|&(a, b)| Self {
            a: corner(a),
            b: corner(b),
            color,
            width,
        }));
    }
}

/// Wireframe boxes and debug lines with depth test, drawn as thin camera-facing quads.
#[derive(Debug)]
pub struct OutlineRenderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    vertices: Vec<Vertex>,
}

impl OutlineRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("outline"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("outline"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("outline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
                })],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("outline vertices"),
            contents: &vec![0u8; MAX_VERTICES * std::mem::size_of::<Vertex>()],
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            pipeline,
            buffer,
            vertices: Vec::new(),
        }
    }

    pub fn build(
        &mut self,
        boxes: &[WorldBox],
        lines: &[WorldLine],
        camera: &Camera,
        size: (u32, u32),
    ) {
        self.vertices.clear();
        let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
        let view_proj = camera.view_proj(w / h);
        let clip = |p: DVec3| view_proj * (p - camera.position).as_vec3().extend(1.0);
        for b in boxes {
            let corners: [Vec4; 8] = std::array::from_fn(|i| {
                clip(DVec3::new(
                    if i & 1 == 0 { b.min.x } else { b.max.x },
                    if i & 2 == 0 { b.min.y } else { b.max.y },
                    if i & 4 == 0 { b.min.z } else { b.max.z },
                ))
            });
            for (a, c) in EDGES {
                self.push_edge(corners[a], corners[c], (w, h), COLOR, LINE_PX);
            }
        }
        for l in lines {
            self.push_edge(clip(l.a), clip(l.b), (w, h), l.color, l.width);
        }
    }

    fn push_edge(
        &mut self,
        mut a: Vec4,
        mut b: Vec4,
        (w, h): (f32, f32),
        color: [f32; 4],
        width: f32,
    ) {
        if a.w < NEAR_W && b.w < NEAR_W {
            return;
        }
        if a.w < NEAR_W {
            a = a.lerp(b, (NEAR_W - a.w) / (b.w - a.w));
        } else if b.w < NEAR_W {
            b = b.lerp(a, (NEAR_W - b.w) / (a.w - b.w));
        }
        let ndc = |v: Vec4| Vec3::new(v.x / v.w, v.y / v.w, v.z / v.w);
        let (pa, pb) = (ndc(a), ndc(b));
        let to_px = |p: Vec3| (p.x * w * 0.5, p.y * h * 0.5);
        let ((ax, ay), (bx, by)) = (to_px(pa), to_px(pb));
        let (dx, dy) = (bx - ax, by - ay);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1.0e-4 || self.vertices.len() + 6 > MAX_VERTICES {
            return;
        }
        let (nx, ny) = (-dy / len * width * 0.5, dx / len * width * 0.5);
        let off = |s: f32| (nx * s / (w * 0.5), ny * s / (h * 0.5));
        let corner = |p: Vec3, s: f32| {
            let (ox, oy) = off(s);
            Vertex {
                pos: [p.x + ox, p.y + oy, (p.z * DEPTH_BIAS).min(1.0), 1.0],
                color,
            }
        };
        let quad = [
            corner(pa, 1.0),
            corner(pa, -1.0),
            corner(pb, -1.0),
            corner(pb, 1.0),
        ];
        self.vertices
            .extend([quad[0], quad[1], quad[2], quad[0], quad[2], quad[3]]);
    }

    pub fn record(&self, ctx: &mut OverlayCtx<'_>) {
        if self.vertices.is_empty() {
            return;
        }
        ctx.queue
            .write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.vertices));
        let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("outline"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: ctx.depth,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.vertices.len() as u32, 0..1);
    }
}
