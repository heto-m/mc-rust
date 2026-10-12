//! Puts Java's GUI into mcr's frame: the render thread posts one frame to Java, encodes its world,
//! waits for Java's frame, then submits Java's buffers between its own passes.
//!
//! Java renders bottom-up (GL row order) into its own main target, so composition copies mcr's
//! scene into that target flipped, submits Java's GUI on top, and copies the result back flipped.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use mcr_jvm::PushError;
use mcr_render::{ExternalCtx, ExternalSubmitter};

use crate::abi::{self, FRAME_LOCKSTEP, FrameBegin, MSG_FRAME, Shared};
use crate::events::BridgeEvent;
use crate::frame::{JavaFrame, LinkState, Lockstep, TickBank};
use crate::gpu::backend::Backend;
use crate::pacer::{self, SharedWriter};

const SAMPLES: usize = 1 << 14;

/// Render-thread timings of the lockstep, in microseconds.
#[derive(Debug, Clone, Default)]
pub struct ComposeStats {
    pub posted: u64,
    pub composed: u64,
    /// Frames shown without Java's newest GUI because it was late (loading only).
    pub late: u64,
    pub java_ticks: u64,
    pub rust_ticks: u64,
    /// Render-thread waits for Java, one per composed frame.
    pub waits_us: Vec<u32>,
    /// Java's own frame times, one per composed frame.
    pub java_us: Vec<u32>,
    pub java_size: (u32, u32),
    pub state: Option<LinkState>,
}

impl ComposeStats {
    /// `(p50, p99, max)` of `samples`.
    pub fn percentiles(samples: &[u32]) -> (u32, u32, u32) {
        if samples.is_empty() {
            return (0, 0, 0);
        }
        let mut v = samples.to_vec();
        v.sort_unstable();
        let at = |q: usize| v[(v.len() * q / 100).min(v.len() - 1)];
        (at(50), at(99), v[v.len() - 1])
    }
}

fn push_sample(v: &mut Vec<u32>, d: Duration) {
    if v.len() >= SAMPLES {
        v.drain(..SAMPLES / 2);
    }
    v.push(d.as_micros().min(u128::from(u32::MAX)) as u32);
}

/// Copies one `Rgba8Unorm` image into another of the same size, upside down.
#[derive(Debug)]
struct Flipper {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

const FLIP_WGSL: &str = r"
@group(0) @binding(0) var src: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4f(p * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) pos: vec4f) -> @location(0) vec4f {
    let h = i32(textureDimensions(src).y);
    return textureLoad(src, vec2i(i32(pos.x), h - 1 - i32(pos.y)), 0);
}
";

impl Flipper {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mcr-modbridge flip"),
            source: wgpu::ShaderSource::Wgsl(FLIP_WGSL.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mcr-modbridge flip"),
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
            label: Some("mcr-modbridge flip"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mcr-modbridge flip"),
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
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, layout }
    }

    fn flip(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src: &wgpu::Texture,
        dst: &wgpu::Texture,
    ) {
        let view = src.create_view(&wgpu::TextureViewDescriptor::default());
        let target = dst.create_view(&wgpu::TextureViewDescriptor::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mcr-modbridge flip"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mcr-modbridge flip"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mcr-modbridge flip"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
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
        queue.submit([encoder.finish()]);
    }
}

fn composable(scene: &wgpu::Texture, main: &wgpu::Texture) -> bool {
    scene.size() == main.size()
        && scene.format() == wgpu::TextureFormat::Rgba8Unorm
        && main.format() == wgpu::TextureFormat::Rgba8Unorm
        && main
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT)
        && scene
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT)
}

/// The render thread's end of the lockstep.
#[derive(Debug)]
pub struct FrameLink {
    writer: SharedWriter,
    shared: &'static Shared,
    lockstep: Lockstep,
    ticks: TickBank,
    flipper: Option<Flipper>,
    shown: Option<wgpu::Texture>,
    notice: Option<String>,
    size_mismatch_logged: bool,
    stats: ComposeStats,
}

impl FrameLink {
    pub(crate) fn new(writer: SharedWriter, shared: &'static Shared) -> Self {
        Self {
            writer,
            shared,
            lockstep: Lockstep::new(),
            ticks: TickBank::default(),
            flipper: None,
            shown: None,
            notice: None,
            size_mismatch_logged: false,
            stats: ComposeStats::default(),
        }
    }

    /// Ticks Rust ran since the last frame and its partial tick; Java runs exactly these once it
    /// reached its frame loop (ticks before that are dropped, as Java has no game to tick yet).
    pub fn add_ticks(&mut self, ticks: u32, partial: f32) {
        let ticks = if self.lockstep.state() == LinkState::Booting {
            0
        } else {
            ticks
        };
        self.ticks.add(ticks, partial);
    }

    pub fn state(&self) -> LinkState {
        self.lockstep.state()
    }

    /// Java's GUI is being composed, so mcr's own HUD and title screen stay hidden.
    pub fn gui_active(&self) -> bool {
        self.lockstep.state() == LinkState::Running && self.shown.is_some()
    }

    /// A one-time message for the user when Java stopped rendering.
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    pub fn java(&self) -> JavaFrame {
        self.shared.frame.read()
    }

    pub fn stats(&self) -> ComposeStats {
        let java = self.java();
        ComposeStats {
            java_ticks: java.ticks,
            rust_ticks: self.ticks.posted(),
            java_size: java.size,
            state: Some(self.lockstep.state()),
            ..self.stats.clone()
        }
    }

    /// Starts the timing samples and counters afresh (tick totals are kept).
    pub fn reset_stats(&mut self) {
        self.stats = ComposeStats::default();
    }

    fn detach(&mut self, reason: &str) {
        self.lockstep.detach();
        self.shown = None;
        tracing::error!("mod bridge detached from rendering: {reason}");
        self.notice = Some(format!(
            "Mods stopped drawing ({reason}); showing mcr's own HUD"
        ));
        abi::emit(BridgeEvent::RenderDetached {
            reason: reason.into(),
        });
    }

    fn backend() -> Option<&'static Backend> {
        crate::gpu::backend().ok()
    }

    fn post(&mut self, ctx: &ExternalCtx<'_>) {
        if !self.lockstep.can_post() {
            return;
        }
        self.shared.net.flush_spill();
        let index = self.lockstep.post(Instant::now());
        let (ticks_to_do, partial_tick) = self.ticks.take();
        let c = ctx.camera;
        let frame = FrameBegin {
            index,
            ticks_to_do,
            width: ctx.size.0,
            height: ctx.size.1,
            partial_tick,
            scale: 1.0,
            flags: FRAME_LOCKSTEP,
            camera_pos: c.position.to_array(),
            yaw: c.yaw,
            pitch: c.pitch,
            view: ctx.view.to_cols_array(),
            proj: ctx.proj.to_cols_array(),
        };
        let pushed = pacer::lock(&self.writer).try_push(MSG_FRAME, &frame.encode());
        match pushed {
            Ok(()) => {
                self.stats.posted += 1;
                self.shared.bell.notify();
            }
            Err(e) => {
                if !matches!(e, PushError::Full) {
                    tracing::warn!("bridge frame push failed: {e}");
                }
                self.lockstep.unpost();
                self.ticks.add(ticks_to_do, partial_tick);
            }
        }
    }

    fn before_frame(&mut self, ctx: &ExternalCtx<'_>) {
        let Some(backend) = Self::backend() else {
            return;
        };
        if self.shared.frame.is_gone() && self.lockstep.state() != LinkState::Detached {
            self.detach("Java stopped");
        }
        let java = self.java();
        if java.ready {
            self.lockstep.ready();
        }
        let late = self.lockstep.outstanding();
        if self.lockstep.complete(java.done) {
            // A frame that missed its compose still runs, onto Java's own target, before the next one.
            backend.slots.drain_all(backend.gpu, late.unwrap_or(0));
        }
        match self.lockstep.state() {
            LinkState::Running => backend.slots.drain_immediate(backend.gpu),
            LinkState::Booting | LinkState::Detached => {
                backend.slots.drain_all(backend.gpu, u64::MAX)
            }
        };
        self.post(ctx);
    }

    fn after_world(&mut self, ctx: &ExternalCtx<'_>) {
        let Some(backend) = Self::backend() else {
            return;
        };
        let gpu = backend.gpu;
        let Some(index) = self.lockstep.outstanding() else {
            if self.lockstep.state() != LinkState::Running {
                backend.slots.drain_all(gpu, u64::MAX);
            }
            return;
        };
        if self.lockstep.state() != LinkState::Running {
            self.lockstep.complete(self.java().done);
            backend.slots.drain_all(gpu, u64::MAX);
            return;
        }
        let start = Instant::now();
        let loaded = self.java().loaded;
        let done = match self.lockstep.wait_until(start, loaded) {
            Some(deadline) => self.shared.frame.wait_done(index + 1, deadline),
            None => false,
        };
        let waited = start.elapsed();
        let java = self.java();
        if done {
            self.lockstep.complete(java.done);
            push_sample(&mut self.stats.waits_us, waited);
            push_sample(&mut self.stats.java_us, Duration::from_nanos(java.nanos));
            self.compose(ctx, backend, index);
            self.stats.composed += 1;
        } else if self.lockstep.expire(Instant::now(), java.loaded) || self.shared.frame.is_gone() {
            let reason = if self.shared.frame.is_gone() {
                "Java stopped".to_owned()
            } else {
                format!(
                    "no frame for {:.1}s",
                    waited
                        .as_secs_f32()
                        .max(crate::frame::HANG_TIMEOUT.as_secs_f32())
                )
            };
            self.detach(&reason);
            backend.slots.drain_all(gpu, u64::MAX);
        } else {
            self.stats.late += 1;
            backend.slots.drain_immediate(gpu);
            // Loading screens stay up; in game the live world beats a frozen composite.
            if !java.loaded
                && let Some(main) = self.shown.clone()
                && composable(ctx.scene, &main)
            {
                self.flipper(ctx.device)
                    .flip(ctx.device, ctx.queue, &main, ctx.scene);
            }
        }
    }

    fn flipper(&mut self, device: &wgpu::Device) -> &Flipper {
        self.flipper.get_or_insert_with(|| Flipper::new(device))
    }

    fn compose(&mut self, ctx: &ExternalCtx<'_>, backend: &'static Backend, index: u64) {
        let main = backend
            .slots
            .main_color()
            .filter(|m| composable(ctx.scene, m));
        if main.is_none() && !self.size_mismatch_logged {
            self.size_mismatch_logged = true;
            tracing::warn!(
                scene = ?ctx.size,
                java = ?self.java().size,
                "java main target does not match the scene; its GUI is skipped this frame"
            );
        }
        if let Some(main) = &main {
            self.size_mismatch_logged = false;
            self.flipper(ctx.device)
                .flip(ctx.device, ctx.queue, ctx.scene, main);
        }
        backend.slots.drain_all(backend.gpu, index);
        if let Some(main) = main {
            self.flipper(ctx.device)
                .flip(ctx.device, ctx.queue, &main, ctx.scene);
            self.shown = Some(main);
        }
    }
}

/// A cloneable handle on the [`FrameLink`]; the renderer drives it through [`ExternalSubmitter`].
#[derive(Debug, Clone)]
pub struct ModFrames(Arc<Mutex<FrameLink>>);

impl ModFrames {
    pub(crate) fn new(link: FrameLink) -> Self {
        Self(Arc::new(Mutex::new(link)))
    }

    pub fn lock(&self) -> MutexGuard<'_, FrameLink> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ExternalSubmitter for ModFrames {
    fn before_frame(&mut self, ctx: &ExternalCtx<'_>) {
        self.lock().before_frame(ctx);
    }

    fn after_world(&mut self, ctx: &ExternalCtx<'_>) {
        self.lock().after_world(ctx);
    }
}

/// Java's GPU work now waits for the render thread; call once, before Java creates its device.
pub(crate) fn activate() -> Result<(), crate::gpu::GpuError> {
    if !crate::gpu::context::installed() {
        return Err(crate::gpu::GpuError::NotInstalled);
    }
    let backend = crate::gpu::backend().map_err(|e| crate::gpu::GpuError::Device(e.to_string()))?;
    backend.slots.activate();
    Ok(())
}
