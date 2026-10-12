use glam::Mat4;

use crate::camera::Camera;

/// The frame state an [`ExternalSubmitter`] sees at each hook point.
#[derive(Debug)]
pub struct ExternalCtx<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    /// The `Rgba8Unorm` scene image the frame renders into (top row first).
    pub scene: &'a wgpu::Texture,
    /// The scene's reverse-Z `Depth32Float` depth (cleared to 0, `GreaterEqual`).
    pub depth: &'a wgpu::TextureView,
    pub size: (u32, u32),
    pub camera: &'a Camera,
    /// Camera-relative rotation; translation is `camera.position`.
    pub view: Mat4,
    /// Reverse-Z infinite projection.
    pub proj: Mat4,
}

/// Work from outside the renderer that must land between its passes on the same queue.
///
/// At each hook the renderer has submitted everything it recorded so far, so command buffers the
/// hook submits run after that work and before the rest of the frame.
pub trait ExternalSubmitter {
    /// Before the frame's own work is recorded.
    fn before_frame(&mut self, ctx: &ExternalCtx<'_>);

    /// After all opaque terrain, before translucent terrain and world effects.
    fn after_opaque(&mut self, _ctx: &ExternalCtx<'_>) {}

    /// After the world and world-space overlays, before UI overlays and the present blit.
    fn after_world(&mut self, ctx: &ExternalCtx<'_>);
}

/// Copies the scene image into a surface view of another format.
#[derive(Debug)]
pub(crate) struct PresentBlit {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

const BLIT_WGSL: &str = r"
@group(0) @binding(0) var src: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4f(p * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) pos: vec4f) -> @location(0) vec4f {
    return textureLoad(src, vec2i(pos.xy), 0);
}
";

impl PresentBlit {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("present blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("present blit"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("present blit"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("present blit"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, layout }
    }

    pub fn record(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
    ) {
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("present blit"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(src),
            }],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("present blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}
