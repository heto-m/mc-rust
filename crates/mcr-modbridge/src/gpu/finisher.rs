//! Finishes Java's queued command encoders on a worker thread.
//!
//! wgpu records commands and encodes them only in `CommandEncoder::finish`, which made `finish` the
//! largest Rust cost inside Java's frame. Queued work is consumed by the render thread later anyway,
//! so the encoder goes to this worker and the render thread takes the buffer when it drains.

use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};

use super::backend::{add_cost, cost, cost_start};
use super::context::Gpu;

/// Encoders waiting for the worker; a full queue finishes on the caller's thread instead.
const QUEUE: usize = 256;

#[derive(Debug, Default)]
struct Slot {
    state: Mutex<Option<Option<wgpu::CommandBuffer>>>,
    done: Condvar,
}

/// A command buffer that is ready or being finished by the worker.
#[derive(Debug)]
pub struct Encoded(Inner);

#[derive(Debug)]
enum Inner {
    Ready(Option<wgpu::CommandBuffer>),
    Pending(Arc<Slot>),
}

impl Encoded {
    pub fn ready(buffer: wgpu::CommandBuffer) -> Self {
        Self(Inner::Ready(Some(buffer)))
    }

    /// The finished buffer, waiting for the worker if needed; `None` when its recording failed.
    pub fn take(self) -> Option<wgpu::CommandBuffer> {
        match self.0 {
            Inner::Ready(b) => b,
            Inner::Pending(slot) => {
                let mut state = slot.state.lock().unwrap_or_else(PoisonError::into_inner);
                loop {
                    if let Some(b) = state.take() {
                        return b;
                    }
                    state = slot
                        .done
                        .wait(state)
                        .unwrap_or_else(PoisonError::into_inner);
                }
            }
        }
    }
}

struct Job {
    encoder: wgpu::CommandEncoder,
    slot: Arc<Slot>,
}

static WORKER: OnceLock<Option<Mutex<SyncSender<Job>>>> = OnceLock::new();

/// Finishes `encoder` on the worker; finishes it here when the worker is unavailable or busy.
pub fn finish(gpu: &'static Gpu, encoder: wgpu::CommandEncoder) -> Encoded {
    let sender = WORKER.get_or_init(|| spawn(gpu)).as_ref();
    let slot = Arc::new(Slot::default());
    let job = Job {
        encoder,
        slot: slot.clone(),
    };
    let job = match sender {
        Some(tx) => match tx
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .try_send(job)
        {
            Ok(()) => return Encoded(Inner::Pending(slot)),
            Err(e) => match e {
                std::sync::mpsc::TrySendError::Full(job)
                | std::sync::mpsc::TrySendError::Disconnected(job) => job,
            },
        },
        None => job,
    };
    Encoded(Inner::Ready(finish_now(gpu, job.encoder)))
}

fn spawn(gpu: &'static Gpu) -> Option<Mutex<SyncSender<Job>>> {
    let (tx, rx) = sync_channel(QUEUE);
    std::thread::Builder::new()
        .name("mcr-gpu-finish".into())
        .spawn(move || run(gpu, rx))
        .map_err(|e| tracing::warn!("no command encoder worker: {e}"))
        .ok()?;
    Some(Mutex::new(tx))
}

fn run(gpu: &'static Gpu, rx: Receiver<Job>) {
    while let Ok(job) = rx.recv() {
        let buffer = finish_now(gpu, job.encoder);
        *job.slot
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(buffer);
        job.slot.done.notify_all();
    }
}

/// Finishes `encoder`, dropping the buffer when its recording failed: an invalid buffer would sink
/// mcr's whole frame batch.
fn finish_now(gpu: &Gpu, encoder: wgpu::CommandEncoder) -> Option<wgpu::CommandBuffer> {
    let start = cost_start();
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let buffer = encoder.finish();
    let error = pollster::block_on(scope.pop());
    add_cost(cost::FINISH, start);
    match error {
        None => Some(buffer),
        Some(e) => {
            tracing::warn!("java command buffer dropped: {e}");
            None
        }
    }
}
