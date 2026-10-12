use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mcr_jvm::{Doorbell, PushError, RingWriter};

use crate::abi::{FrameBegin, MSG_FRAME, MSG_SHUTDOWN, Shared};

pub const FRAME_RATE: u32 = 60;
pub const TICK_RATE: f64 = 20.0;
const MAX_TICKS_PER_FRAME: u32 = 10;
const SHUTDOWN_PUSH_TIMEOUT: Duration = Duration::from_secs(5);

/// Splits wall time into whole game ticks and a partial tick, like vanilla's `DeltaTracker`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TickClock {
    pending: f64,
}

impl TickClock {
    pub fn advance(&mut self, elapsed: Duration) -> (u32, f32) {
        self.pending += elapsed.as_secs_f64() * TICK_RATE;
        let ticks = (self.pending.floor() as u32).min(MAX_TICKS_PER_FRAME);
        self.pending = (self.pending - f64::from(ticks)).min(1.0);
        (ticks, self.pending.fract() as f32)
    }
}

#[derive(Debug)]
pub(crate) struct Controls {
    pub stop: AtomicBool,
    pub paused: AtomicBool,
    pub size: AtomicU32,
    /// The render thread posts frames itself (composition); the pacer only sends the shutdown.
    pub external: AtomicBool,
}

impl Controls {
    pub fn new(width: u32, height: u32) -> Arc<Self> {
        Arc::new(Self {
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            size: AtomicU32::new(pack(width, height)),
            external: AtomicBool::new(false),
        })
    }
}

/// The control ring's single writer, shared by the pacer and the render thread's frame link.
pub(crate) type SharedWriter = Arc<Mutex<RingWriter>>;

pub(crate) fn lock(writer: &SharedWriter) -> MutexGuard<'_, RingWriter> {
    writer.lock().unwrap_or_else(PoisonError::into_inner)
}

fn pack(width: u32, height: u32) -> u32 {
    (width.min(0xFFFF) << 16) | height.min(0xFFFF)
}

pub(crate) fn spawn(
    writer: SharedWriter,
    shared: &'static Shared,
    controls: Arc<Controls>,
) -> std::io::Result<JoinHandle<()>> {
    run(writer, &shared.bell, controls, move || {
        shared.net.flush_spill()
    })
}

fn run(
    writer: SharedWriter,
    bell: &'static Doorbell,
    controls: Arc<Controls>,
    before_frame: impl Fn() + Send + 'static,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("mcr-bridge-pacer".into())
        .spawn(move || {
            let period = Duration::from_secs(1) / FRAME_RATE;
            let mut clock = TickClock::default();
            let mut last = Instant::now();
            let mut next = last;
            let mut index = 0u64;
            while !controls.stop.load(Ordering::Acquire) {
                let now = Instant::now();
                if controls.paused.load(Ordering::Acquire)
                    || controls.external.load(Ordering::Acquire)
                {
                    last = now;
                } else {
                    before_frame();
                    let (ticks_to_do, partial_tick) = clock.advance(now - last);
                    last = now;
                    let size = controls.size.load(Ordering::Relaxed);
                    let frame = FrameBegin {
                        index,
                        ticks_to_do,
                        width: size >> 16,
                        height: size & 0xFFFF,
                        partial_tick,
                        ..FrameBegin::default()
                    };
                    let pushed = lock(&writer).try_push(MSG_FRAME, &frame.encode());
                    match pushed {
                        Ok(()) => {
                            index += 1;
                            bell.notify();
                        }
                        Err(PushError::Full) => {}
                        Err(e) => tracing::warn!("bridge frame push failed: {e}"),
                    }
                }
                next += period;
                let now = Instant::now();
                if next > now {
                    std::thread::sleep(next - now);
                } else {
                    next = now;
                }
            }
            let deadline = Instant::now() + SHUTDOWN_PUSH_TIMEOUT;
            loop {
                let pushed = lock(&writer).try_push(MSG_SHUTDOWN, &[]);
                match pushed {
                    Ok(()) => break,
                    Err(PushError::Full) if Instant::now() < deadline => {
                        bell.notify();
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => {
                        tracing::error!("bridge shutdown push failed: {e}");
                        break;
                    }
                }
            }
            bell.notify();
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_clock_runs_at_20_hz() {
        let mut c = TickClock::default();
        let frame = Duration::from_secs(1) / 60;
        let mut ticks = 0;
        for _ in 0..60 {
            ticks += c.advance(frame).0;
        }
        assert!((19..=20).contains(&ticks), "{ticks}");
        let (t, partial) = c.advance(Duration::from_millis(25));
        assert!(t <= 1 && (0.0..1.0).contains(&partial));
    }

    #[test]
    fn tick_clock_caps_catch_up() {
        let mut c = TickClock::default();
        assert_eq!(c.advance(Duration::from_secs(60)).0, MAX_TICKS_PER_FRAME);
        assert!(c.advance(Duration::ZERO).0 <= 1);
    }

    #[test]
    fn pacer_posts_frames_then_shutdown() {
        let ring = mcr_jvm::Ring::new(4096).unwrap();
        let bell: &'static Doorbell = Box::leak(Box::new(Doorbell::new()));
        let controls = Controls::new(854, 480);
        let mut reader = ring.reader().unwrap();
        let writer = Arc::new(Mutex::new(ring.writer().unwrap()));
        let handle = run(writer, bell, controls.clone(), || {}).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        controls.stop.store(true, Ordering::Release);
        handle.join().unwrap();
        let mut kinds = Vec::new();
        let mut first = None;
        while let Some(msg) = reader.try_pop().unwrap() {
            if first.is_none() && msg.kind() == MSG_FRAME {
                first = Some(msg.payload().to_vec());
            }
            kinds.push(msg.kind());
        }
        assert_eq!(kinds.last(), Some(&MSG_SHUTDOWN));
        assert!(kinds.iter().filter(|&&k| k == MSG_FRAME).count() >= 2);
        let first = first.unwrap();
        assert_eq!(u64::from_ne_bytes(first[..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_ne_bytes(first[12..16].try_into().unwrap()), 854);
    }
}
