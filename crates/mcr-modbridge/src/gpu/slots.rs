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
use crate::abi::{SLOT_GUI, SLOT_IMMEDIATE};

/// Bounds memory if Java submits while the render thread is not draining (a stalled window).
pub const MAX_QUEUED: usize = 4096;
const LOGGED_OVERFLOWS: u64 = 10;

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
        for (buffer, offset, data) in &self.writes {
            gpu.queue.write_buffer(buffer, *offset, data);
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
        match self.items.front() {
            Some((SLOT_IMMEDIATE, _, _)) => self.items.pop_front().map(|(_, _, t)| t),
            _ => None,
        }
    }

    /// Everything, in submission order; GUI work of frames before `frame` is stale.
    pub fn pop_any(&mut self, frame: u64) -> Option<(T, bool)> {
        self.items
            .pop_front()
            .map(|(slot, f, t)| (t, slot == SLOT_GUI && f < frame))
    }
}

/// Composition state shared by Java's recording threads and the render thread.
#[derive(Debug, Default)]
pub struct Slots {
    active: AtomicBool,
    slot: AtomicU32,
    frame: AtomicU64,
    queue: Mutex<SlotQueue<Submission>>,
    overflows: AtomicU64,
    main_color: Mutex<Option<(Handle, wgpu::Texture)>>,
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
        self.slot.store(slot.min(SLOT_GUI), Ordering::Release);
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

    /// Submits the immediate work queued ahead of any GUI work.
    pub fn drain_immediate(&self, gpu: &Gpu) -> usize {
        let mut q = self.lock();
        let mut n = 0;
        while let Some(s) = q.pop_immediate() {
            run_scoped(gpu, s);
            n += 1;
        }
        self.submitted.fetch_add(n as u64, Ordering::Relaxed);
        n
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
