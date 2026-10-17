use std::cell::RefCell;

use glam::Mat4;

use crate::camera::Camera;
use crate::graph::ReachedSections;

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
    /// The sections the visibility graph reached from `camera` this frame, when it ran.
    pub sections: Option<ReachedSections<'a>>,
    /// The frame's command buffers so far, not yet submitted.
    pub batch: &'a FrameBatch,
}

/// One frame's command buffers in queue order, submitted together at the end of the frame.
///
/// Host writes (`queue.write_buffer`) land before the whole next submit, so a write that must follow
/// batched work needs [`FrameBatch::submit`] first. While a hook runs, the renderer's own encoder is
/// held open and only finished when the hook adds work after it.
#[derive(Debug, Default)]
pub struct FrameBatch {
    buffers: RefCell<Vec<wgpu::CommandBuffer>>,
    open: RefCell<Option<wgpu::CommandEncoder>>,
}

impl FrameBatch {
    /// Runs `buffer` after everything batched so far.
    pub fn push(&self, buffer: wgpu::CommandBuffer) {
        self.close();
        self.buffers.borrow_mut().push(buffer);
    }

    /// Submits everything batched so far; a rejected submit returns wgpu's error.
    pub fn submit(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::SubmissionIndex, Option<wgpu::Error>) {
        self.close();
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let index = queue.submit(self.buffers.borrow_mut().drain(..));
        (index, pollster::block_on(scope.pop()))
    }

    /// Keeps the renderer's encoder open across a hook.
    pub(crate) fn hold(&self, encoder: wgpu::CommandEncoder) {
        self.close();
        *self.open.borrow_mut() = Some(encoder);
    }

    /// The held encoder when the hook added nothing, else a new one.
    pub(crate) fn resume(&self, device: &wgpu::Device, label: &str) -> wgpu::CommandEncoder {
        self.open.borrow_mut().take().unwrap_or_else(|| {
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) })
        })
    }

    fn close(&self) {
        if let Some(encoder) = self.open.borrow_mut().take() {
            self.buffers.borrow_mut().push(encoder.finish());
        }
    }
}

/// Work from outside the renderer that must land between its passes on the same queue.
///
/// At each hook the renderer has batched everything it recorded so far, so command buffers the hook
/// adds to [`ExternalCtx::batch`] run after that work and before the rest of the frame.
pub trait ExternalSubmitter {
    /// Before the frame's own work is recorded.
    fn before_frame(&mut self, ctx: &ExternalCtx<'_>);

    /// After all opaque terrain, before translucent terrain and world effects.
    fn after_opaque(&mut self, _ctx: &ExternalCtx<'_>) {}

    /// After the world and world-space overlays, before post effects, UI overlays and the present blit.
    fn after_world(&mut self, ctx: &ExternalCtx<'_>);

    /// After post effects, before UI overlays and the present blit: where an external GUI lands.
    fn before_ui(&mut self, _ctx: &ExternalCtx<'_>) {}

    /// The frame's batch went to the queue as `index`; `rejected` when wgpu refused it.
    fn submitted(&mut self, _index: &wgpu::SubmissionIndex, _rejected: bool) {}
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

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    #[test]
    fn a_held_encoder_closes_only_when_work_follows_it() {
        let Some((device, queue)) = device() else {
            eprintln!("skipped: no adapter");
            return;
        };
        let buffer = |usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 4,
                usage,
                mapped_at_creation: false,
            })
        };
        let copy = wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
        let (one, two, dst) = (buffer(copy), buffer(copy), buffer(copy));
        let out = buffer(wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ);
        queue.write_buffer(&one, 0, &[1; 4]);
        queue.write_buffer(&two, 0, &[2; 4]);
        let batch = FrameBatch::default();
        let mut mcr = device.create_command_encoder(&Default::default());
        mcr.copy_buffer_to_buffer(&one, 0, &dst, 0, 4);
        batch.hold(mcr);
        let mut mcr = batch.resume(&device, "same");
        assert!(
            batch.buffers.borrow().is_empty(),
            "an empty hook keeps it open"
        );
        batch.hold(mcr);
        let mut java = device.create_command_encoder(&Default::default());
        java.copy_buffer_to_buffer(&two, 0, &dst, 0, 4);
        batch.push(java.finish());
        mcr = batch.resume(&device, "after");
        assert_eq!(batch.buffers.borrow().len(), 2);
        mcr.copy_buffer_to_buffer(&dst, 0, &out, 0, 4);
        batch.push(mcr.finish());
        let (index, error) = batch.submit(&device, &queue);
        assert!(error.is_none());
        out.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(index),
                timeout: None,
            })
            .unwrap();
        assert_eq!(
            &*out.slice(..).get_mapped_range().unwrap(),
            &[2; 4],
            "in order"
        );
    }
}
