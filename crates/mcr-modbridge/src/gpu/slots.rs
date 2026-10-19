//! Java's submits as slot-tagged work the mcr render thread puts on the queue at its slot points.
//!
//! While composing, Java never submits itself: each `submit()` becomes a [`Submission`] holding the
//! command buffer and the staged bytes that must be written just before it, so it keeps exactly the
//! ordering it would have had as a direct submit.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::backend::{add_cost, cost, cost_start};
use super::context::Gpu;
use super::finisher::Encoded;
use super::slab::Handle;
use crate::abi::{
    SLOT_GUI, SLOT_IMMEDIATE, SLOT_POST_WORLD, SLOT_WORLD_AFTER, SLOT_WORLD_SOLID,
    SLOT_WORLD_TRANSLUCENT,
};
use mcr_render::FrameBatch;

/// Bounds memory if Java submits while the render thread is not draining (a stalled window).
pub const MAX_QUEUED: usize = 4096;
const LOGGED_OVERFLOWS: u64 = 10;
const POOLED_BUFFERS: usize = 64;
/// Submits tracked per encoder for blocking fence waits.
const MAX_INFLIGHT: usize = 256;
const POOLED_CAPACITY: usize = 16 << 20;
/// Idle capacity the pool may hold in all, so a burst of large submits is not kept for the session.
const POOLED_TOTAL: usize = 64 << 20;

static BYTE_POOL: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

/// A copy of `bytes` in a recycled buffer, so queued submits do not allocate per submit.
pub(crate) fn pooled_copy(bytes: &[u8]) -> Vec<u8> {
    let mut v = BYTE_POOL
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .pop()
        .unwrap_or_default();
    v.clear();
    v.extend_from_slice(bytes);
    v
}

pub(crate) fn recycle(v: Vec<u8>) {
    if v.capacity() > POOLED_CAPACITY {
        return;
    }
    let mut pool = BYTE_POOL.lock().unwrap_or_else(PoisonError::into_inner);
    let held: usize = pool.iter().map(Vec::capacity).sum();
    if pool.len() < POOLED_BUFFERS && held + v.capacity() <= POOLED_TOTAL {
        pool.push(v);
    }
}

/// Whether work tagged `slot` draws into mcr's world (scene color, and scene or private depth).
pub fn is_world_slot(slot: u32) -> bool {
    matches!(
        slot,
        SLOT_WORLD_SOLID | SLOT_WORLD_TRANSLUCENT | SLOT_WORLD_AFTER | SLOT_POST_WORLD
    )
}

/// Immediate work of frames up to `upto`: what may run before mcr's own frame.
pub fn immediate_upto(upto: u64) -> impl Fn(u32, u64) -> bool {
    move |slot, frame| slot == SLOT_IMMEDIATE && frame <= upto
}

/// Work up to Java's translucent features, which lands before mcr's translucent terrain.
pub fn world_upto(upto: u64) -> impl Fn(u32, u64) -> bool {
    move |slot, frame| {
        frame <= upto
            && matches!(
                slot,
                SLOT_IMMEDIATE | SLOT_WORLD_SOLID | SLOT_WORLD_TRANSLUCENT
            )
    }
}

/// Work before the GUI: late world features, the hand and post effects.
pub fn before_gui_upto(upto: u64) -> impl Fn(u32, u64) -> bool {
    move |slot, frame| frame <= upto && slot != SLOT_GUI
}

pub fn frame_upto(upto: u64) -> impl Fn(u32, u64) -> bool {
    move |_, frame| frame <= upto
}

pub(crate) const MAP_PENDING: u8 = 0;
pub(crate) const MAP_OK: u8 = 1;
pub(crate) const MAP_FAILED: u8 = 2;

/// Which of one encoder's submits reached the queue and which completed.
#[derive(Debug, Default)]
pub struct SubmitTrack {
    pub done: Arc<AtomicU64>,
    /// Highest submit number that reached `queue.submit`.
    pub queued: AtomicU64,
    pub inflight: Mutex<VecDeque<(u64, wgpu::SubmissionIndex)>>,
}

impl SubmitTrack {
    pub fn inflight(&self) -> MutexGuard<'_, VecDeque<(u64, wgpu::SubmissionIndex)>> {
        self.inflight.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn record(&self, n: u64, index: wgpu::SubmissionIndex) {
        let done = self.done.load(Ordering::Acquire);
        push_pruned(&mut self.inflight(), done, (n, index));
    }
}

/// Waits rarely prune: Java's fences mostly name submits the GPU already finished.
fn push_pruned<T>(inflight: &mut VecDeque<(u64, T)>, done: u64, entry: (u64, T)) {
    while inflight.front().is_some_and(|(k, _)| *k <= done) || inflight.len() >= MAX_INFLIGHT {
        // A wait for a dropped submit waits for a later one, which is still correct.
        inflight.pop_front();
    }
    inflight.push_back(entry);
}

#[derive(Debug)]
pub struct Submission {
    pub slot: u32,
    pub frame: u64,
    pub writes: Vec<(wgpu::Buffer, u64, Vec<u8>)>,
    pub cmd: Option<Encoded>,
    /// Readback staging buffers to map once the copy into them is on the queue.
    pub maps: Vec<(wgpu::Buffer, Arc<AtomicU8>)>,
    pub track: Arc<SubmitTrack>,
    pub n: u64,
    /// The Java submit this work belongs to: its transient memory and upload ring are only reused
    /// by later submits.
    pub epoch: u64,
}

impl Submission {
    pub fn bytes(&self) -> u64 {
        self.writes.iter().map(|(_, _, d)| d.len() as u64).sum()
    }

    /// Writes, submits and arms the completion and readback callbacks.
    pub fn execute(self, gpu: &Gpu) {
        let start = cost_start();
        for (buffer, offset, data) in self.writes {
            gpu.queue.write_buffer(&buffer, offset, &data);
            recycle(data);
        }
        add_cost(cost::QUEUE_WRITE, start);
        let start = cost_start();
        if self.cmd.is_none() && self.maps.is_empty() {
            // Pending writes go out with the next submit, still ahead of any later command buffer.
            let (done, n) = (self.track.done.clone(), self.n);
            gpu.queue.on_submitted_work_done(move || {
                done.fetch_max(n, Ordering::AcqRel);
            });
            self.track.queued.fetch_max(n, Ordering::AcqRel);
            add_cost(cost::QUEUE_SUBMIT, start);
            return;
        }
        let index = gpu.queue.submit(self.cmd.and_then(Encoded::take));
        let (done, n) = (self.track.done.clone(), self.n);
        gpu.queue.on_submitted_work_done(move || {
            done.fetch_max(n, Ordering::AcqRel);
        });
        self.track.record(n, index);
        self.track.queued.fetch_max(n, Ordering::AcqRel);
        for (staging, flag) in self.maps {
            staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                flag.store(
                    if r.is_ok() { MAP_OK } else { MAP_FAILED },
                    Ordering::Release,
                );
            });
        }
        add_cost(cost::QUEUE_SUBMIT, start);
    }
}

/// FIFO of deferred work, generic so the ordering rules are testable without a GPU.
#[derive(Debug)]
pub struct SlotQueue<T> {
    items: VecDeque<(u32, u64, T)>,
}

impl<T> Default for SlotQueue<T> {
    fn default() -> Self {
        Self {
            items: VecDeque::new(),
        }
    }
}

impl<T> SlotQueue<T> {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn push(&mut self, slot: u32, frame: u64, item: T) {
        self.items.push_back((slot, frame, item));
    }

    /// The leading run of immediate work of frames up to `upto`: what may run before mcr's frame.
    pub fn pop_immediate(&mut self, upto: u64) -> Option<T> {
        self.pop_while(immediate_upto(upto)).map(|(_, _, t)| t)
    }

    /// The next item with its slot and frame if they pass `keep`; FIFO order is never broken.
    pub fn pop_while(&mut self, keep: impl Fn(u32, u64) -> bool) -> Option<(u32, u64, T)> {
        match self.items.front() {
            Some((slot, frame, _)) if keep(*slot, *frame) => self.items.pop_front(),
            _ => None,
        }
    }

    /// Everything, in submission order; frame work of frames before `frame` is stale.
    pub fn pop_any(&mut self, frame: u64) -> Option<(T, bool)> {
        self.items
            .pop_front()
            .map(|(slot, f, t)| (t, slot != SLOT_IMMEDIATE && f < frame))
    }
}

/// Composition state shared by Java's recording threads and the render thread.
/// mcr's scene targets, which Java's world slots render into instead of its main target.
#[derive(Debug, Clone)]
pub struct SceneTargets {
    pub color: wgpu::Texture,
    pub color_view: wgpu::TextureView,
    pub depth: wgpu::TextureView,
    /// `POST_WORLD`'s depth, so the hand's depth clear never touches the scene depth.
    pub private_depth: Option<wgpu::TextureView>,
}

/// Where a virtual main-target attachment lands for the current slot.
#[derive(Debug, Clone)]
pub struct WorldTargets {
    pub color: wgpu::TextureView,
    pub color_texture: wgpu::Texture,
    pub depth: wgpu::TextureView,
    /// Depth is mcr's scene depth: Java's clears of it are dropped.
    pub scene_depth: bool,
    pub size: (u32, u32),
    /// The GUI slot: projected geometry is drawn top-down and scissors flip, as Java's projection
    /// is not flipped there.
    pub gui: bool,
}

/// Where Java's world slots of one frame read their level projection, and the camera that frame
/// was recorded with, so a frame composed late can be drawn from mcr's current camera.
#[derive(Debug, Clone)]
pub struct LevelView {
    pub frame: u64,
    pub buffer: wgpu::Buffer,
    pub offset: u64,
    /// Java's camera position: its world geometry is relative to it.
    pub camera: [f64; 3],
    /// The inverse of Java's view rotation (column-major), which its model-view matrices carry.
    pub inverse_rotation: [f32; 16],
}

/// Level views kept for frames Java recorded but mcr has not composed yet.
const KEPT_VIEWS: usize = 4;

/// A level projection rewrite that lands just before a frame's first world-slot submit.
#[derive(Debug)]
struct Patch {
    frame: u64,
    buffer: wgpu::Buffer,
    offset: u64,
    bytes: [u8; 64],
}

/// Queued work moved into mcr's frame batch, finished once the batch is on the queue.
#[derive(Debug)]
struct Batched {
    track: Arc<SubmitTrack>,
    n: u64,
    maps: Vec<(wgpu::Buffer, Arc<AtomicU8>)>,
}

#[derive(Debug, Default)]
struct BatchState {
    /// `(encoder track, epoch)` of every Java command buffer in the open batch.
    keys: Vec<(usize, u64)>,
    pending: Vec<Batched>,
}

/// Rotating 256-byte slots the level projection patch is copied from.
const PATCH_SLOTS: u64 = 4;
const PATCH_STRIDE: u64 = 256;
/// How long Java's overflow path waits for an open frame batch before submitting anyway.
const OVERFLOW_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Debug, Default)]
pub struct Slots {
    active: AtomicBool,
    slot: AtomicU32,
    frame: AtomicU64,
    queue: Mutex<SlotQueue<Submission>>,
    overflows: AtomicU64,
    main_color: Mutex<Option<(Handle, wgpu::Texture)>>,
    scene: Mutex<Option<SceneTargets>>,
    pub stale: AtomicU64,
    pub submitted: AtomicU64,
    views: Mutex<VecDeque<LevelView>>,
    patch: Mutex<Option<Patch>>,
    /// Held while queued work is popped and submitted, so submits never reorder across threads.
    exec: Mutex<()>,
    batch: Mutex<BatchState>,
    /// Drained work waits in mcr's frame batch, not yet on the queue.
    batch_open: AtomicBool,
    /// A frame batch was rejected: Java's work is submitted on its own from then on.
    unbatched: AtomicBool,
    patch_source: Mutex<Option<wgpu::Buffer>>,
    patch_slot: AtomicU64,
}

impl Slots {
    /// From now on Java's submits wait for the render thread.
    pub fn activate(&self) {
        self.active.store(true, Ordering::Release);
    }

    /// Java submits directly again (shutdown); whatever was queued is submitted first.
    pub fn deactivate(&self, gpu: &Gpu) {
        self.active.store(false, Ordering::Release);
        self.drain_all(gpu, None);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub fn set_slot(&self, slot: u32, frame: u64) {
        let slot = if slot > SLOT_POST_WORLD {
            SLOT_IMMEDIATE
        } else {
            slot
        };
        self.slot.store(slot, Ordering::Release);
        self.frame.store(frame, Ordering::Release);
    }

    pub fn current(&self) -> (u32, u64) {
        (
            self.slot.load(Ordering::Acquire),
            self.frame.load(Ordering::Acquire),
        )
    }

    fn lock(&self) -> MutexGuard<'_, SlotQueue<Submission>> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queues `sub`, or runs it now when not composing or when the queue is full.
    pub fn defer(&self, gpu: &Gpu, sub: Submission) {
        if !self.is_active() {
            sub.execute(gpu);
            return;
        }
        let mut q = self.lock();
        if q.len() >= MAX_QUEUED {
            drop(q);
            let start = std::time::Instant::now();
            while self.batch_open.load(Ordering::Acquire) && start.elapsed() < OVERFLOW_WAIT {
                std::thread::sleep(std::time::Duration::from_micros(200));
            }
            let _exec = self.exec.lock().unwrap_or_else(PoisonError::into_inner);
            let mut q = self.lock();
            let n = self.overflows.fetch_add(1, Ordering::Relaxed);
            if n < LOGGED_OVERFLOWS {
                tracing::warn!("mod gpu slot queue full; submitting from the Java thread");
            }
            while let Some((s, _)) = q.pop_any(u64::MAX) {
                s.execute(gpu);
            }
            sub.execute(gpu);
            return;
        }
        q.push(sub.slot, sub.frame, sub);
    }

    pub fn queued(&self) -> usize {
        self.lock().len()
    }

    /// Submits the immediate work of frames up to `upto` queued ahead of any frame work.
    pub fn drain_immediate(&self, gpu: &Gpu, batch: Option<&FrameBatch>, upto: u64) -> usize {
        self.drain_while(gpu, batch, 0, immediate_upto(upto))
    }

    /// Submits the leading work up to Java's translucent features, at mcr's `after_opaque`.
    pub fn drain_world(&self, gpu: &Gpu, batch: Option<&FrameBatch>, upto: u64) -> usize {
        self.drain_while(gpu, batch, 0, world_upto(upto))
    }

    /// Submits the leading work that comes before the GUI (late world features, hand, post effects).
    pub fn drain_before_gui(&self, gpu: &Gpu, batch: Option<&FrameBatch>, upto: u64) -> usize {
        self.drain_while(gpu, batch, 0, before_gui_upto(upto))
    }

    /// Submits everything of frames up to `frame`, in order; frame work of earlier frames is stale.
    pub fn drain_frame(&self, gpu: &Gpu, batch: Option<&FrameBatch>, frame: u64) -> usize {
        self.drain_while(gpu, batch, frame, frame_upto(frame))
    }

    /// Submits everything queued, in order.
    pub fn drain_all(&self, gpu: &Gpu, batch: Option<&FrameBatch>) -> usize {
        self.drain_frame(gpu, batch, u64::MAX)
    }

    /// Pops work while `keep` allows; with a `batch`, its command buffers join mcr's frame batch
    /// instead of going to the queue one by one.
    fn drain_while(
        &self,
        gpu: &Gpu,
        batch: Option<&FrameBatch>,
        stale_before: u64,
        keep: impl Fn(u32, u64) -> bool,
    ) -> usize {
        let _exec = self.exec.lock().unwrap_or_else(PoisonError::into_inner);
        let batch = batch.filter(|_| !self.unbatched.load(Ordering::Acquire));
        let mut patch = self
            .patch
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let scope = batch.map(|_| gpu.device.push_error_scope(wgpu::ErrorFilter::Validation));
        let mut n = 0;
        // The queue lock is held per pop only, so Java keeps queueing while this submits.
        while let Some((slot, frame, s)) = self.lock().pop_while(&keep) {
            if slot != SLOT_IMMEDIATE && frame < stale_before && stale_before != u64::MAX {
                let k = self.stale.fetch_add(1, Ordering::Relaxed);
                if k < LOGGED_OVERFLOWS {
                    tracing::warn!(frame, now = stale_before, "stale java frame work");
                }
            }
            let item_patch = is_world_slot(slot)
                .then(|| patch.take_if(|p| p.frame == frame))
                .flatten();
            match batch {
                Some(batch) => self.batch_one(gpu, batch, s, item_patch),
                None => {
                    if let Some(p) = item_patch {
                        gpu.queue.write_buffer(&p.buffer, p.offset, &p.bytes);
                    }
                    run_scoped(gpu, s);
                }
            }
            n += 1;
        }
        if let Some(scope) = scope
            && let Some(e) = pollster::block_on(scope.pop())
        {
            tracing::warn!("java work rejected by wgpu: {e}");
        }
        if patch.is_some() {
            let mut slot = self.patch.lock().unwrap_or_else(PoisonError::into_inner);
            if slot.is_none() {
                *slot = patch;
            }
        }
        self.submitted.fetch_add(n as u64, Ordering::Relaxed);
        n
    }

    /// Writes `sub`'s staged bytes now and moves its command buffer into `batch`. Writes land before
    /// the whole batch, so the batch goes to the queue first when it holds another Java submit's work.
    fn batch_one(&self, gpu: &Gpu, batch: &FrameBatch, sub: Submission, patch: Option<Patch>) {
        let key = (Arc::as_ptr(&sub.track) as usize, sub.epoch);
        let conflict = writes_conflict(
            &self
                .batch
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .keys,
            key,
            !sub.writes.is_empty(),
        );
        if conflict {
            let (index, error) = batch.submit(&gpu.device, &gpu.queue);
            self.submitted(gpu, &index, error.is_some());
        }
        self.batch_open.store(true, Ordering::Release);
        if let Some(p) = patch {
            batch.push(self.patch_buffer(gpu, &p));
        }
        let start = cost_start();
        for (buffer, offset, data) in sub.writes {
            gpu.queue.write_buffer(&buffer, offset, &data);
            recycle(data);
        }
        add_cost(cost::QUEUE_WRITE, start);
        let mut state = self.batch.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(cmd) = sub.cmd.and_then(Encoded::take) {
            batch.push(cmd);
            state.keys.push(key);
        }
        state.pending.push(Batched {
            track: sub.track,
            n: sub.n,
            maps: sub.maps,
        });
    }

    /// A command buffer copying `patch` over the level projection, between the batched work around it.
    fn patch_buffer(&self, gpu: &Gpu, patch: &Patch) -> wgpu::CommandBuffer {
        let source = self
            .patch_source
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert_with(|| {
                gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("mcr-modbridge projection patch"),
                    size: PATCH_SLOTS * PATCH_STRIDE,
                    usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .clone();
        let offset = self.patch_slot.fetch_add(1, Ordering::Relaxed) % PATCH_SLOTS * PATCH_STRIDE;
        gpu.queue.write_buffer(&source, offset, &patch.bytes);
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mcr-modbridge projection patch"),
            });
        encoder.copy_buffer_to_buffer(&source, offset, &patch.buffer, patch.offset, 64);
        encoder.finish()
    }

    /// mcr submitted its frame batch as `index`: the Java work in it is now on the queue.
    pub fn submitted(&self, gpu: &Gpu, index: &wgpu::SubmissionIndex, rejected: bool) {
        let start = cost_start();
        let mut state = self.batch.lock().unwrap_or_else(PoisonError::into_inner);
        state.keys.clear();
        let mut last: Option<(Arc<SubmitTrack>, u64)> = None;
        for b in state.pending.drain(..) {
            b.track.record(b.n, index.clone());
            b.track.queued.fetch_max(b.n, Ordering::AcqRel);
            for (staging, flag) in b.maps {
                staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                    flag.store(
                        if r.is_ok() { MAP_OK } else { MAP_FAILED },
                        Ordering::Release,
                    );
                });
            }
            if let Some((t, n)) = &mut last
                && Arc::ptr_eq(t, &b.track)
            {
                *n = (*n).max(b.n);
            } else if let Some((t, n)) = last.replace((b.track, b.n)) {
                notify_done(gpu, &t, n);
            }
        }
        if let Some((t, n)) = last {
            notify_done(gpu, &t, n);
        }
        drop(state);
        self.batch_open.store(false, Ordering::Release);
        if rejected && !self.unbatched.swap(true, Ordering::AcqRel) {
            tracing::error!(
                "a frame batch with Java's work was rejected; Java's work is submitted on its own from now on"
            );
        }
        add_cost(cost::QUEUE_SUBMIT, start);
    }

    /// Java's level projection for `view.frame`; the oldest of [`KEPT_VIEWS`] is dropped.
    pub fn report_level_view(&self, view: LevelView) {
        let mut views = self.views.lock().unwrap_or_else(PoisonError::into_inner);
        views.retain(|v| v.frame != view.frame);
        if views.len() >= KEPT_VIEWS {
            views.pop_front();
        }
        views.push_back(view);
    }

    pub fn level_view(&self, frame: u64) -> Option<LevelView> {
        self.views
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|v| v.frame == frame)
            .cloned()
    }

    /// Rewrites `view`'s projection with `matrix` (column-major) right before its frame's first
    /// world-slot work is submitted.
    pub fn arm_patch(&self, view: &LevelView, matrix: &[f32; 16]) {
        let mut bytes = [0u8; 64];
        for (dst, v) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(matrix) {
            *dst = v.to_le_bytes();
        }
        *self.patch.lock().unwrap_or_else(PoisonError::into_inner) = Some(Patch {
            frame: view.frame,
            buffer: view.buffer.clone(),
            offset: view.offset,
            bytes,
        });
    }

    /// mcr's scene for the frames Java records next; called by the render thread every frame.
    pub fn set_scene(&self, color: &wgpu::Texture, depth: &wgpu::TextureView) {
        let mut scene = self.scene.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(s) = scene.as_ref()
            && s.color == *color
            && s.depth == *depth
        {
            return;
        }
        *scene = Some(SceneTargets {
            color: color.clone(),
            color_view: color.create_view(&wgpu::TextureViewDescriptor::default()),
            depth: depth.clone(),
            private_depth: None,
        });
    }

    /// The scene targets for work tagged `slot`: the world slots, and the GUI slot while Java's main
    /// target matches the scene's size; `None` otherwise.
    pub fn world_targets(&self, device: &wgpu::Device, slot: u32) -> Option<WorldTargets> {
        let gui = slot == SLOT_GUI;
        if !(is_world_slot(slot) || gui) || !self.is_active() {
            return None;
        }
        let mut scene = self.scene.lock().unwrap_or_else(PoisonError::into_inner);
        let s = scene.as_mut()?;
        let size = (s.color.width(), s.color.height());
        if gui
            && self
                .main_color()
                .is_none_or(|m| (m.width(), m.height()) != size)
        {
            return None;
        }
        let private = slot == SLOT_POST_WORLD || gui;
        let depth = if private {
            let fits = s.private_depth.as_ref().is_some_and(|d| {
                let t = d.texture();
                (t.width(), t.height()) == size
            });
            if !fits {
                s.private_depth = Some(private_depth(device, size));
            }
            s.private_depth.clone().expect("created above")
        } else {
            s.depth.clone()
        };
        Some(WorldTargets {
            color: s.color_view.clone(),
            color_texture: s.color.clone(),
            depth,
            scene_depth: !private,
            size,
            gui,
        })
    }

    /// mcr's scene color when Java's main target of `size` stands for it this frame.
    pub fn scene_for_main(&self, size: (u32, u32)) -> Option<wgpu::Texture> {
        if !self.is_active() {
            return None;
        }
        let scene = self.scene.lock().unwrap_or_else(PoisonError::into_inner);
        scene
            .as_ref()
            .map(|s| s.color.clone())
            .filter(|c| (c.width(), c.height()) == size)
    }

    pub fn set_main_color(&self, handle: Handle, texture: wgpu::Texture) {
        *self
            .main_color
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some((handle, texture));
    }

    pub fn forget(&self, handle: Handle) {
        let mut m = self
            .main_color
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if m.as_ref().is_some_and(|(h, _)| *h == handle) {
            *m = None;
        }
    }

    pub fn main_color(&self) -> Option<wgpu::Texture> {
        self.main_color
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|(_, t)| t.clone())
    }
}

fn private_depth(device: &wgpu::Device, (width, height): (u32, u32)) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("mcr-modbridge post-world depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// Writes land before the whole batch, so they may only join one holding command buffers of the same
/// Java submit: its transient memory and upload ring are not reused before it ends.
fn writes_conflict(batched: &[(usize, u64)], key: (usize, u64), writes: bool) -> bool {
    writes && batched.iter().any(|k| *k != key)
}

fn notify_done(gpu: &Gpu, track: &SubmitTrack, n: u64) {
    let done = track.done.clone();
    gpu.queue.on_submitted_work_done(move || {
        done.fetch_max(n, Ordering::AcqRel);
    });
}

/// Invalid Java buffers must not reach mcr's uncaptured-error handler on the render thread.
fn run_scoped(gpu: &Gpu, sub: Submission) {
    let start = cost_start();
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    add_cost(cost::QUEUE_OTHER, start);
    sub.execute(gpu);
    let start = cost_start();
    if let Some(e) = pollster::block_on(scope.pop()) {
        tracing::warn!("java submission rejected by wgpu: {e}");
    }
    add_cost(cost::QUEUE_OTHER, start);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain<T>(q: &mut SlotQueue<T>, keep: impl Fn(u32, u64) -> bool) -> Vec<T> {
        let mut out = Vec::new();
        while let Some((_, _, t)) = q.pop_while(&keep) {
            out.push(t);
        }
        out
    }

    #[test]
    fn writes_of_another_submit_flush_the_batch() {
        let batched = [(1, 7), (1, 7)];
        assert!(!writes_conflict(&batched, (1, 7), true), "same submit");
        assert!(
            writes_conflict(&batched, (1, 8), true),
            "next submit reuses memory"
        );
        assert!(writes_conflict(&batched, (2, 7), true), "another encoder");
        assert!(
            !writes_conflict(&batched, (1, 8), false),
            "no writes to hoist"
        );
        assert!(!writes_conflict(&[], (1, 8), true));
    }

    #[test]
    fn the_byte_pool_holds_a_bounded_total() {
        for _ in 0..256 {
            recycle(Vec::with_capacity(8 << 20));
        }
        let pool = BYTE_POOL.lock().unwrap_or_else(PoisonError::into_inner);
        assert!(pool.iter().map(Vec::capacity).sum::<usize>() <= POOLED_TOTAL);
    }

    #[test]
    fn finished_submits_leave_the_inflight_list() {
        let mut inflight = VecDeque::new();
        for n in 1..=100_000u64 {
            push_pruned(&mut inflight, n.saturating_sub(3), (n, ()));
        }
        assert!(inflight.len() <= 4, "{}", inflight.len());
        assert_eq!(inflight.back().map(|e| e.0), Some(100_000));
    }

    #[test]
    fn immediate_prefix_stops_at_gui_work() {
        let mut q = SlotQueue::default();
        q.push(SLOT_IMMEDIATE, 4, "upload");
        q.push(SLOT_IMMEDIATE, 5, "pre-gui");
        q.push(SLOT_GUI, 5, "gui");
        q.push(SLOT_IMMEDIATE, 5, "end");
        assert_eq!(q.pop_immediate(5), Some("upload"));
        assert_eq!(q.pop_immediate(5), Some("pre-gui"));
        assert_eq!(q.pop_immediate(5), None, "gui work waits for its slot");
        assert_eq!(q.pop_any(5), Some(("gui", false)));
        assert_eq!(q.pop_any(5), Some(("end", false)));
        assert!(q.is_empty());
    }

    #[test]
    fn world_slots_drain_in_two_steps_around_mcr_translucency() {
        let mut q = SlotQueue::default();
        q.push(SLOT_IMMEDIATE, 7, "lightmap");
        q.push(SLOT_WORLD_SOLID, 7, "solid entities");
        q.push(SLOT_WORLD_TRANSLUCENT, 7, "translucent entities");
        q.push(SLOT_WORLD_AFTER, 7, "outlines");
        q.push(SLOT_POST_WORLD, 7, "hand");
        q.push(SLOT_GUI, 7, "hud");
        q.push(SLOT_IMMEDIATE, 7, "tail");
        assert_eq!(
            drain(&mut q, world_upto(7)),
            ["lightmap", "solid entities", "translucent entities"]
        );
        assert_eq!(drain(&mut q, before_gui_upto(7)), ["outlines", "hand"]);
        assert_eq!(drain(&mut q, frame_upto(7)), ["hud", "tail"]);
    }

    #[test]
    fn a_frame_recorded_ahead_waits_behind_the_one_being_composed() {
        let mut q = SlotQueue::default();
        q.push(SLOT_IMMEDIATE, 7, "lightmap 7");
        q.push(SLOT_WORLD_TRANSLUCENT, 7, "entities 7");
        q.push(SLOT_IMMEDIATE, 8, "upload 8");
        q.push(SLOT_WORLD_SOLID, 8, "entities 8");
        q.push(SLOT_GUI, 8, "hud 8");
        assert_eq!(drain(&mut q, immediate_upto(7)), ["lightmap 7"]);
        assert_eq!(
            drain(&mut q, world_upto(7)),
            ["entities 7"],
            "frame 8 has no GUI cut yet but stays queued"
        );
        assert!(drain(&mut q, before_gui_upto(7)).is_empty());
        assert!(drain(&mut q, frame_upto(7)).is_empty());
        assert_eq!(
            drain(&mut q, frame_upto(8)),
            ["upload 8", "entities 8", "hud 8"]
        );
    }

    #[test]
    fn only_frame_slots_draw_into_the_world() {
        assert!(!is_world_slot(SLOT_IMMEDIATE));
        assert!(!is_world_slot(SLOT_GUI));
        assert!(is_world_slot(SLOT_WORLD_SOLID) && is_world_slot(SLOT_POST_WORLD));
        assert!(!is_world_slot(99));
    }

    #[test]
    fn pooled_copies_reuse_buffers() {
        let a = pooled_copy(&[1, 2, 3]);
        recycle(a);
        let b = pooled_copy(&[4]);
        assert_eq!(b, [4]);
    }

    #[test]
    fn old_gui_work_is_submitted_but_flagged_stale() {
        let mut q = SlotQueue::default();
        q.push(SLOT_GUI, 2, "old");
        q.push(SLOT_GUI, 3, "current");
        assert_eq!(q.pop_any(3), Some(("old", true)));
        assert_eq!(q.pop_any(3), Some(("current", false)));
    }

    #[test]
    fn upload_is_submitted_before_the_gui_draw_using_it() {
        let mut q = SlotQueue::default();
        q.push(SLOT_IMMEDIATE, 0, "reload texture upload");
        q.push(SLOT_GUI, 1, "draw with texture");
        let mut order = drain(&mut q, immediate_upto(1));
        order.extend(drain(&mut q, frame_upto(1)));
        assert_eq!(order, ["reload texture upload", "draw with texture"]);
    }
}
