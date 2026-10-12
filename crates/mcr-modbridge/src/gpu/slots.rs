//! Java's submits as slot-tagged work the mcr render thread puts on the queue at its slot points.
//!
//! While composing, Java never submits itself: each `submit()` becomes a [`Submission`] holding the
//! command buffer and the staged bytes that must be written just before it, so it keeps exactly the
//! ordering it would have had as a direct submit.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::context::Gpu;
use super::slab::Handle;
use crate::abi::{
    SLOT_GUI, SLOT_IMMEDIATE, SLOT_POST_WORLD, SLOT_WORLD_AFTER, SLOT_WORLD_SOLID,
    SLOT_WORLD_TRANSLUCENT,
};

/// Bounds memory if Java submits while the render thread is not draining (a stalled window).
pub const MAX_QUEUED: usize = 4096;
const LOGGED_OVERFLOWS: u64 = 10;
const POOLED_BUFFERS: usize = 64;
const POOLED_CAPACITY: usize = 16 << 20;

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
    if pool.len() < POOLED_BUFFERS {
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
}

#[derive(Debug)]
pub struct Submission {
    pub slot: u32,
    pub frame: u64,
    pub writes: Vec<(wgpu::Buffer, u64, Vec<u8>)>,
    pub cmd: Option<wgpu::CommandBuffer>,
    /// Readback staging buffers to map once the copy into them is on the queue.
    pub maps: Vec<(wgpu::Buffer, Arc<AtomicU8>)>,
    pub track: Arc<SubmitTrack>,
    pub n: u64,
}

impl Submission {
    pub fn bytes(&self) -> u64 {
        self.writes.iter().map(|(_, _, d)| d.len() as u64).sum()
    }

    /// Writes, submits and arms the completion and readback callbacks.
    pub fn execute(self, gpu: &Gpu) {
        for (buffer, offset, data) in self.writes {
            gpu.queue.write_buffer(&buffer, offset, &data);
            recycle(data);
        }
        let index = gpu.queue.submit(self.cmd);
        let (done, n) = (self.track.done.clone(), self.n);
        gpu.queue.on_submitted_work_done(move || {
            done.fetch_max(n, Ordering::AcqRel);
        });
        self.track.inflight().push_back((n, index));
        self.track.queued.fetch_max(n, Ordering::AcqRel);
        for (staging, flag) in self.maps {
            staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                flag.store(
                    if r.is_ok() { MAP_OK } else { MAP_FAILED },
                    Ordering::Release,
                );
            });
        }
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

    /// The leading run of immediate work: everything that may run before mcr's own frame.
    pub fn pop_immediate(&mut self) -> Option<T> {
        self.pop_while(|slot| slot == SLOT_IMMEDIATE)
    }

    /// The next item if its slot passes `keep`; FIFO order is never broken.
    pub fn pop_while(&mut self, keep: impl Fn(u32) -> bool) -> Option<T> {
        match self.items.front() {
            Some((slot, _, _)) if keep(*slot) => self.items.pop_front().map(|(_, _, t)| t),
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
}

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
}

impl Slots {
    /// From now on Java's submits wait for the render thread.
    pub fn activate(&self) {
        self.active.store(true, Ordering::Release);
    }

    /// Java submits directly again (shutdown); whatever was queued is submitted first.
    pub fn deactivate(&self, gpu: &Gpu) {
        self.active.store(false, Ordering::Release);
        self.drain_all(gpu, u64::MAX);
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

    /// Submits the immediate work queued ahead of any frame work.
    pub fn drain_immediate(&self, gpu: &Gpu) -> usize {
        self.drain_while(gpu, |slot| slot == SLOT_IMMEDIATE)
    }

    /// Submits the leading work up to Java's translucent features, at mcr's `after_opaque`.
    pub fn drain_world(&self, gpu: &Gpu) -> usize {
        self.drain_while(gpu, |slot| {
            matches!(
                slot,
                SLOT_IMMEDIATE | SLOT_WORLD_SOLID | SLOT_WORLD_TRANSLUCENT
            )
        })
    }

    /// Submits the leading work that comes before the GUI (late world features, hand, post effects).
    pub fn drain_before_gui(&self, gpu: &Gpu) -> usize {
        self.drain_while(gpu, |slot| slot != SLOT_GUI)
    }

    fn drain_while(&self, gpu: &Gpu, keep: impl Fn(u32) -> bool) -> usize {
        let mut q = self.lock();
        let mut n = 0;
        while let Some(s) = q.pop_while(&keep) {
            run_scoped(gpu, s);
            n += 1;
        }
        self.submitted.fetch_add(n as u64, Ordering::Relaxed);
        n
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

    /// The scene targets for work tagged `slot`, or `None` outside the world slots.
    pub fn world_targets(&self, device: &wgpu::Device, slot: u32) -> Option<WorldTargets> {
        if !is_world_slot(slot) || !self.is_active() {
            return None;
        }
        let mut scene = self.scene.lock().unwrap_or_else(PoisonError::into_inner);
        let s = scene.as_mut()?;
        let size = (s.color.width(), s.color.height());
        let depth = if slot == SLOT_POST_WORLD {
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
            scene_depth: slot != SLOT_POST_WORLD,
            size,
        })
    }

    /// Submits everything queued, in order; `frame` is the one being composed.
    pub fn drain_all(&self, gpu: &Gpu, frame: u64) -> usize {
        let mut q = self.lock();
        let mut n = 0;
        while let Some((s, stale)) = q.pop_any(frame) {
            if stale {
                let k = self.stale.fetch_add(1, Ordering::Relaxed);
                if k < LOGGED_OVERFLOWS {
                    tracing::warn!(frame = s.frame, now = frame, "stale java gui buffer");
                }
            }
            run_scoped(gpu, s);
            n += 1;
        }
        self.submitted.fetch_add(n as u64, Ordering::Relaxed);
        n
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

/// Invalid Java buffers must not reach mcr's uncaptured-error handler on the render thread.
fn run_scoped(gpu: &Gpu, sub: Submission) {
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    sub.execute(gpu);
    if let Some(e) = pollster::block_on(scope.pop()) {
        tracing::warn!("java submission rejected by wgpu: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immediate_prefix_stops_at_gui_work() {
        let mut q = SlotQueue::default();
        q.push(SLOT_IMMEDIATE, 4, "upload");
        q.push(SLOT_IMMEDIATE, 5, "pre-gui");
        q.push(SLOT_GUI, 5, "gui");
        q.push(SLOT_IMMEDIATE, 5, "end");
        assert_eq!(q.pop_immediate(), Some("upload"));
        assert_eq!(q.pop_immediate(), Some("pre-gui"));
        assert_eq!(q.pop_immediate(), None, "gui work waits for its slot");
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
        let world = |s| {
            matches!(
                s,
                SLOT_IMMEDIATE | SLOT_WORLD_SOLID | SLOT_WORLD_TRANSLUCENT
            )
        };
        let mut order = Vec::new();
        while let Some(t) = q.pop_while(world) {
            order.push(t);
        }
        assert_eq!(
            order,
            ["lightmap", "solid entities", "translucent entities"]
        );
        order.clear();
        while let Some(t) = q.pop_while(|s| s != SLOT_GUI) {
            order.push(t);
        }
        assert_eq!(order, ["outlines", "hand"]);
        assert_eq!(q.pop_any(7), Some(("hud", false)));
        assert_eq!(q.pop_any(7), Some(("tail", false)));
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
        let mut order = Vec::new();
        while let Some(t) = q.pop_immediate() {
            order.push(t);
        }
        while let Some((t, _)) = q.pop_any(1) {
            order.push(t);
        }
        assert_eq!(order, ["reload texture upload", "draw with texture"]);
    }
}
