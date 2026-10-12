//! Puts Java's frame into mcr's: the render thread posts one frame to Java, encodes its opaque
//! terrain, waits for Java's frame, then submits Java's buffers between its own passes.
//!
//! Java's world slots draw straight into mcr's scene with mcr's own projection. Java's GUI slot draws
//! into the scene too, top-down through vertex variants without naga's Y flip, so nothing is copied.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use mcr_jvm::PushError;
use mcr_render::{Camera, ExternalCtx, ExternalSubmitter};

use crate::abi::{
    self, FRAME_CAMERA_HANDOFF, FRAME_DETACHED, FRAME_LOCKSTEP, FRAME_SECTIONS, FrameBegin,
    MSG_FRAME, Shared,
};
use crate::events::BridgeEvent;
use crate::frame::{JavaCamera, JavaFrame, LinkState, Lockstep, TickBank};
use crate::gpu::backend::Backend;
use crate::input::ModInput;
use crate::pacer::{self, SharedWriter};
use crate::sync::{self, Pending};

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
    /// Render-thread CPU time putting Java's GUI into the scene, one per composed frame.
    pub gui_us: Vec<u32>,
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

/// Copies `src` into `dst` when both have the same size and format.
fn copy_same(device: &wgpu::Device, queue: &wgpu::Queue, src: &wgpu::Texture, dst: &wgpu::Texture) {
    if src.size() != dst.size() || src.format() != dst.format() {
        return;
    }
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mcr-modbridge composite copy"),
    });
    encoder.copy_texture_to_texture(src.as_image_copy(), dst.as_image_copy(), src.size());
    queue.submit([encoder.finish()]);
}

/// A texture to hold a copy of `like`.
fn composite_copy(device: &wgpu::Device, like: &wgpu::Texture) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mcr-modbridge last composite"),
        size: like.size(),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: like.format(),
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// The render thread's end of the lockstep.
#[derive(Debug)]
pub struct FrameLink {
    writer: SharedWriter,
    shared: &'static Shared,
    lockstep: Lockstep,
    ticks: TickBank,
    /// Java's GUI reached the scene in a composed frame.
    shown: bool,
    /// The last composite while Java loads, shown again when its next frame is late.
    last: Option<wgpu::Texture>,
    notice: Option<String>,
    size_mismatch_logged: bool,
    stats: ComposeStats,
    /// The posted frame whose world slots ran at `after_opaque` and whose GUI is composed next.
    composing: Option<u64>,
    /// The frame whose world slots are drained and whose GUI lands at `before_ui`, after post effects.
    gui_pending: Option<u64>,
    /// `after_opaque` already spent this frame's wait budget.
    opaque_waited: bool,
    detached_camera: bool,
    input: ModInput,
    pending: Pending,
    render_distance: u32,
    entity_scale: f32,
    /// The player's camera while mcr renders from a mod's camera instead.
    player_camera: Option<Camera>,
    /// The graph search last copied for Java, if it fit.
    sections: Option<u64>,
}

impl FrameLink {
    pub(crate) fn new(writer: SharedWriter, shared: &'static Shared, input: ModInput) -> Self {
        Self {
            writer,
            shared,
            lockstep: Lockstep::new(),
            ticks: TickBank::default(),
            shown: false,
            last: None,
            notice: None,
            size_mismatch_logged: false,
            stats: ComposeStats::default(),
            composing: None,
            gui_pending: None,
            opaque_waited: false,
            detached_camera: false,
            input,
            pending: Pending::default(),
            render_distance: 0,
            entity_scale: 0.0,
            player_camera: None,
            sections: None,
        }
    }

    /// mcr renders this frame from a mod's camera (see [`Self::java_camera`]) and `player` is the
    /// camera vanilla would use; `None` when mcr renders the player camera.
    pub fn set_camera_handoff(&mut self, player: Option<Camera>) {
        self.player_camera = player;
    }

    /// The camera a mod gave Java's last frame, while Java draws the world.
    pub fn java_camera(&self) -> Option<JavaCamera> {
        self.world_active().then(|| self.java().camera).flatten()
    }

    /// A mod replaced vanilla's rain and snow in Java's last frame.
    pub fn weather_replaced(&self) -> bool {
        self.world_active() && self.java().weather_replaced
    }

    /// mcr's camera is in third person this frame, so Java draws the local player and no hand.
    pub fn set_third_person(&mut self, on: bool) {
        self.detached_camera = on;
    }

    /// mcr's render distance (chunks, as its fog uses it) and entity distance scaling; Java's
    /// effective render distance follows them.
    pub fn set_view(&mut self, render_distance: u32, entity_scale: f32) {
        self.render_distance = render_distance;
        self.entity_scale = if entity_scale.is_finite() {
            entity_scale
        } else {
            0.0
        };
    }

    /// mcr's vanilla key bindings as `(mapping, key)` names; sent before Java's next frame when changed.
    pub fn set_key_bindings<'a>(&mut self, bindings: impl IntoIterator<Item = (&'a str, &'a str)>) {
        self.pending.keys(sync::encode_keys(bindings));
    }

    /// The server pack files mcr applied, first pushed first; Java loads the same set.
    pub fn set_server_packs(&mut self, paths: &[std::path::PathBuf]) {
        self.pending.packs(sync::encode_packs(paths));
    }

    /// Java draws the world's entities, block entities and hand this frame; mcr's own stay off.
    pub fn world_active(&self) -> bool {
        self.gui_active() && self.java().loaded
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
        self.lockstep.state() == LinkState::Running && self.shown
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

    /// The lockstep already detached; drops Java's GUI and tells the user.
    fn report_detached(&mut self, reason: &str) {
        self.input.queue().discard();
        self.shown = false;
        self.last = None;
        tracing::error!("mod bridge detached from rendering: {reason}");
        self.notice = Some(format!(
            "Mods stopped drawing ({reason}); showing mcr's own HUD"
        ));
        abi::emit(BridgeEvent::RenderDetached {
            reason: reason.into(),
        });
    }

    /// Java is gone or offline: a live link reports it, a stalled one just stops recovering.
    fn detach_for_good(&mut self, reason: &str) {
        let live = self.lockstep.state() != LinkState::Detached;
        self.lockstep.detach();
        if live {
            self.report_detached(reason);
        }
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
        let c = self.player_camera.as_ref().unwrap_or(ctx.camera);
        let r = ctx.camera;
        let sections = ctx.sections.as_ref().is_some_and(|set| {
            if self.sections.is_none_or(|g| g != set.generation) {
                self.sections = self.shared.sections.publish(set).then_some(set.generation);
            }
            self.sections == Some(set.generation)
        });
        let frame = FrameBegin {
            index,
            ticks_to_do,
            width: ctx.size.0,
            height: ctx.size.1,
            partial_tick,
            scale: 1.0,
            flags: FRAME_LOCKSTEP
                | if self.detached_camera {
                    FRAME_DETACHED
                } else {
                    0
                }
                | if self.player_camera.is_some() {
                    FRAME_CAMERA_HANDOFF
                } else {
                    0
                }
                | if sections { FRAME_SECTIONS } else { 0 },
            camera_pos: c.position.to_array(),
            yaw: c.yaw,
            pitch: c.pitch,
            view: ctx.view.to_cols_array(),
            proj: ctx.proj.to_cols_array(),
            render_distance: self.render_distance,
            entity_scale: self.entity_scale,
            render_pos: r.position.to_array(),
            render_yaw: r.yaw,
            render_pitch: r.pitch,
        };
        let pushed = {
            let mut writer = pacer::lock(&self.writer);
            self.pending
                .flush(|kind, payload| match writer.try_push(kind, payload) {
                    Ok(()) => true,
                    Err(PushError::Full) => false,
                    Err(e) => {
                        tracing::warn!("bridge settings push failed: {e}");
                        true
                    }
                });
            let mut input = self.input.queue();
            if self.lockstep.state() == LinkState::Running {
                input.flush(&mut writer, &self.shared.input);
            } else {
                input.discard();
            }
            writer.try_push(MSG_FRAME, &frame.encode())
        };
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
        self.composing = None;
        self.gui_pending = None;
        self.opaque_waited = false;
        backend.slots.set_scene(ctx.scene, ctx.depth);
        if self.shared.frame.is_gone() {
            self.detach_for_good("Java stopped");
        }
        if self.shared.net.is_detached() && self.lockstep.state() != LinkState::Booting {
            self.detach_for_good("Java lost its connection");
        }
        let java = self.java();
        if java.ready {
            self.lockstep.ready();
        }
        let late = self.lockstep.outstanding();
        if self.lockstep.complete(java.done, Instant::now()) {
            // A frame that missed its compose still runs, onto Java's own target, before the next one.
            backend.slots.drain_all(backend.gpu, late.unwrap_or(0));
        }
        if self.lockstep.reattach() {
            tracing::info!("mod bridge re-attached to rendering");
            self.notice = Some("Mods are drawing again".into());
            abi::emit(BridgeEvent::RenderResumed);
        }
        match self.lockstep.state() {
            LinkState::Running => backend.slots.drain_immediate(backend.gpu),
            LinkState::Booting | LinkState::Detached => {
                backend.slots.drain_all(backend.gpu, u64::MAX)
            }
        };
        self.post(ctx);
    }

    /// Waits for Java's frame and submits its work up to its translucent features, so they land
    /// before mcr's translucent terrain.
    fn after_opaque(&mut self, _ctx: &ExternalCtx<'_>) {
        let Some(backend) = Self::backend() else {
            return;
        };
        let Some(index) = self.lockstep.outstanding() else {
            return;
        };
        if self.lockstep.state() != LinkState::Running {
            return;
        }
        self.opaque_waited = true;
        if !self.wait(index) {
            return;
        }
        backend.slots.drain_world(backend.gpu);
        self.composing = Some(index);
    }

    /// Bounded wait for Java's frame `index`; on time-out it detaches or counts a late frame.
    fn wait(&mut self, index: u64) -> bool {
        let start = Instant::now();
        let loaded = self.java().loaded;
        let done = match self.lockstep.wait_until(start, loaded) {
            Some(deadline) => self.shared.frame.wait_done(index + 1, deadline),
            None => false,
        };
        let waited = start.elapsed();
        let java = self.java();
        if done {
            self.lockstep.complete(java.done, Instant::now());
            push_sample(&mut self.stats.waits_us, waited);
            push_sample(&mut self.stats.java_us, Duration::from_nanos(java.nanos));
        }
        done
    }

    fn after_world(&mut self, ctx: &ExternalCtx<'_>) {
        let Some(backend) = Self::backend() else {
            return;
        };
        let gpu = backend.gpu;
        if let Some(index) = self.composing.take() {
            self.compose(backend, index);
            self.stats.composed += 1;
            return;
        }
        let Some(index) = self.lockstep.outstanding() else {
            if self.lockstep.state() != LinkState::Running {
                backend.slots.drain_all(gpu, u64::MAX);
            }
            return;
        };
        if self.lockstep.state() != LinkState::Running {
            self.lockstep.complete(self.java().done, Instant::now());
            backend.slots.drain_all(gpu, u64::MAX);
            return;
        }
        let start = Instant::now();
        let loaded = self.java().loaded;
        let deadline = if self.opaque_waited {
            Some(start)
        } else {
            self.lockstep.wait_until(start, loaded)
        };
        let done = match deadline {
            Some(deadline) => self.shared.frame.wait_done(index + 1, deadline),
            None => false,
        };
        let waited = start.elapsed();
        let java = self.java();
        if done {
            self.lockstep.complete(java.done, Instant::now());
            push_sample(&mut self.stats.waits_us, waited);
            push_sample(&mut self.stats.java_us, Duration::from_nanos(java.nanos));
            self.compose(backend, index);
            self.stats.composed += 1;
        } else if self.shared.frame.is_gone() {
            self.detach_for_good("Java stopped");
            backend.slots.drain_all(gpu, u64::MAX);
        } else if self.lockstep.expire(Instant::now(), java.loaded) {
            self.report_detached(&format!(
                "no frame for {:.1}s",
                waited
                    .as_secs_f32()
                    .max(crate::frame::HANG_TIMEOUT.as_secs_f32())
            ));
            backend.slots.drain_all(gpu, u64::MAX);
        } else {
            self.stats.late += 1;
            backend.slots.drain_immediate(gpu);
            // Loading screens stay up; in game the live world beats a frozen composite.
            if !java.loaded
                && let Some(last) = &self.last
            {
                copy_same(ctx.device, ctx.queue, last, ctx.scene);
            }
        }
    }

    fn compose(&mut self, backend: &'static Backend, index: u64) {
        backend.slots.drain_before_gui(backend.gpu);
        self.gui_pending = Some(index);
    }

    /// Java's GUI slot draws straight into the scene, top-down, when its main target matches it.
    fn before_ui(&mut self, ctx: &ExternalCtx<'_>) {
        let (Some(index), Some(backend)) = (self.gui_pending.take(), Self::backend()) else {
            return;
        };
        let start = Instant::now();
        let composed = backend
            .slots
            .main_color()
            .is_some_and(|m| m.size() == ctx.scene.size());
        if !composed && !self.size_mismatch_logged {
            self.size_mismatch_logged = true;
            tracing::warn!(
                scene = ?ctx.size,
                java = ?self.java().size,
                "java main target does not match the scene; its GUI is skipped this frame"
            );
        }
        backend.slots.drain_all(backend.gpu, index);
        if composed {
            self.size_mismatch_logged = false;
            self.shown = true;
            if self.java().loaded {
                self.last = None;
            } else {
                let last = match self.last.take() {
                    Some(t) if t.size() == ctx.scene.size() => t,
                    _ => composite_copy(ctx.device, ctx.scene),
                };
                copy_same(ctx.device, ctx.queue, ctx.scene, &last);
                self.last = Some(last);
            }
        }
        push_sample(&mut self.stats.gui_us, start.elapsed());
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

    fn after_opaque(&mut self, ctx: &ExternalCtx<'_>) {
        self.lock().after_opaque(ctx);
    }

    fn after_world(&mut self, ctx: &ExternalCtx<'_>) {
        self.lock().after_world(ctx);
    }

    fn before_ui(&mut self, ctx: &ExternalCtx<'_>) {
        self.lock().before_ui(ctx);
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
