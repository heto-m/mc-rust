//! Frame lockstep with Java: the shared frame state, the hang watchdog and the tick hand-off.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use mcr_jvm::Doorbell;

use crate::abi::{
    FSTATE_DONE, FSTATE_FLAGS, FSTATE_HEIGHT, FSTATE_LOADED, FSTATE_NANOS, FSTATE_READY,
    FSTATE_SIZE, FSTATE_TICKS, FSTATE_WIDTH,
};

/// After loading, a Java frame slower than this detaches the bridge from rendering.
pub const HANG_TIMEOUT: Duration = Duration::from_secs(2);
/// While Java loads resources a frame may legitimately take seconds.
pub const LOADING_HANG_TIMEOUT: Duration = Duration::from_secs(60);
/// While loading, mcr shows its own frame instead of waiting longer than this for Java's.
pub const LOADING_WAIT: Duration = Duration::from_millis(50);
/// Once loaded, a frame waits at most this long; a later Java frame shows its previous GUI.
pub const FRAME_WAIT: Duration = Duration::from_millis(100);
/// Java runs at most this many ticks per frame, like vanilla's `runTick`; the rest carries over.
pub const MAX_TICKS_PER_FRAME: u32 = 10;

#[repr(C, align(64))]
#[derive(Debug, Default)]
struct FrameState {
    done: AtomicU64,
    ticks: AtomicU64,
    nanos: AtomicU64,
    flags: AtomicU32,
    width: AtomicU32,
    height: AtomicU32,
}

const _: () = {
    assert!(std::mem::offset_of!(FrameState, done) == FSTATE_DONE);
    assert!(std::mem::offset_of!(FrameState, ticks) == FSTATE_TICKS);
    assert!(std::mem::offset_of!(FrameState, nanos) == FSTATE_NANOS);
    assert!(std::mem::offset_of!(FrameState, flags) == FSTATE_FLAGS);
    assert!(std::mem::offset_of!(FrameState, width) == FSTATE_WIDTH);
    assert!(std::mem::offset_of!(FrameState, height) == FSTATE_HEIGHT);
    assert!(std::mem::size_of::<FrameState>() == FSTATE_SIZE);
};

/// What Java last reported about its frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JavaFrame {
    /// Frames finished so far: the last finished index plus one.
    pub done: u64,
    /// Client ticks Java ran in total.
    pub ticks: u64,
    /// Wall time of Java's last frame, from leaving its frame wait to signalling done.
    pub nanos: u64,
    pub ready: bool,
    pub loaded: bool,
    /// Java's main render target size.
    pub size: (u32, u32),
}

/// Process-wide like the VM: Java writes the state block and rings the bell after each frame.
#[derive(Debug)]
pub(crate) struct FrameShared {
    state: Box<FrameState>,
    pub bell: Doorbell,
    gone: AtomicBool,
}

impl FrameShared {
    pub fn new() -> Self {
        Self {
            state: Box::default(),
            bell: Doorbell::new(),
            gone: AtomicBool::new(false),
        }
    }

    pub fn state_addr(&self) -> i64 {
        &raw const *self.state as i64
    }

    pub fn read(&self) -> JavaFrame {
        let s = &self.state;
        let flags = s.flags.load(Ordering::Acquire);
        JavaFrame {
            done: s.done.load(Ordering::Acquire),
            ticks: s.ticks.load(Ordering::Acquire),
            nanos: s.nanos.load(Ordering::Acquire),
            ready: flags & FSTATE_READY != 0,
            loaded: flags & FSTATE_LOADED != 0,
            size: (
                s.width.load(Ordering::Acquire),
                s.height.load(Ordering::Acquire),
            ),
        }
    }

    /// Java crashed or exited: nothing will ever signal a frame again.
    pub fn java_gone(&self) {
        self.gone.store(true, Ordering::Release);
        self.bell.notify();
    }

    pub fn is_gone(&self) -> bool {
        self.gone.load(Ordering::Acquire)
    }

    /// Blocks until Java finished at least `count` frames or `deadline` passed.
    pub fn wait_done(&self, count: u64, deadline: Instant) -> bool {
        loop {
            let seen = self.bell.seq();
            if self.state.done.load(Ordering::Acquire) >= count {
                return true;
            }
            if self.is_gone() {
                return false;
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            if left.is_zero() {
                return false;
            }
            self.bell.wait(seen, Some(left));
        }
    }

    #[cfg(test)]
    pub fn simulate_done(&self, count: u64, ticks: u64, flags: u32) {
        self.state.ticks.store(ticks, Ordering::Release);
        self.state.flags.store(flags, Ordering::Release);
        self.state.done.store(count, Ordering::Release);
        self.bell.notify();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    /// Java has not reached its frame loop yet; nothing is posted or awaited.
    Booting,
    /// Lockstep: one frame posted at a time, its GUI composed in the same mcr frame.
    Running,
    /// Java missed the hang deadline or died; mcr renders natively and never waits again.
    Detached,
}

/// The render thread's view of the lockstep and its hang watchdog.
#[derive(Debug, Clone)]
pub struct Lockstep {
    state: LinkState,
    next: u64,
    outstanding: Option<(u64, Instant)>,
}

impl Default for Lockstep {
    fn default() -> Self {
        Self::new()
    }
}

impl Lockstep {
    pub fn new() -> Self {
        Self {
            state: LinkState::Booting,
            next: 0,
            outstanding: None,
        }
    }

    pub fn state(&self) -> LinkState {
        self.state
    }

    /// Java entered its frame loop.
    pub fn ready(&mut self) {
        if self.state == LinkState::Booting {
            self.state = LinkState::Running;
        }
    }

    pub fn detach(&mut self) {
        self.state = LinkState::Detached;
    }

    pub fn outstanding(&self) -> Option<u64> {
        self.outstanding.map(|(i, _)| i)
    }

    /// A detached bridge still gets frames, one at a time, so Java keeps ticking.
    pub fn can_post(&self) -> bool {
        self.state != LinkState::Booting && self.outstanding.is_none()
    }

    pub fn post(&mut self, now: Instant) -> u64 {
        let index = self.next;
        self.next += 1;
        self.outstanding = Some((index, now));
        index
    }

    /// The post did not reach Java (ring full); the next frame reuses the index.
    pub fn unpost(&mut self) {
        if self.outstanding.take().is_some() {
            self.next -= 1;
        }
    }

    /// Java finished `done` frames in total; `true` if that completes the outstanding one.
    pub fn complete(&mut self, done: u64) -> bool {
        match self.outstanding {
            Some((index, _)) if done > index => {
                self.outstanding = None;
                true
            }
            _ => false,
        }
    }

    fn limit(loaded: bool) -> Duration {
        if loaded {
            HANG_TIMEOUT
        } else {
            LOADING_HANG_TIMEOUT
        }
    }

    /// How long the render thread may block for the outstanding frame this mcr frame.
    pub fn wait_until(&self, now: Instant, loaded: bool) -> Option<Instant> {
        if self.state != LinkState::Running {
            return None;
        }
        let (_, posted) = self.outstanding?;
        let wait = if loaded { FRAME_WAIT } else { LOADING_WAIT };
        Some((posted + Self::limit(loaded)).min(now + wait))
    }

    /// Detaches when the outstanding frame is past its hang deadline; `true` on that transition.
    pub fn expire(&mut self, now: Instant, loaded: bool) -> bool {
        match (self.state, self.outstanding) {
            (LinkState::Running, Some((_, posted))) if now >= posted + Self::limit(loaded) => {
                self.state = LinkState::Detached;
                true
            }
            _ => false,
        }
    }
}

/// Ticks Rust ran that Java has not been told about yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct TickBank {
    pending: u64,
    partial: f32,
    posted: u64,
}

impl TickBank {
    pub fn add(&mut self, ticks: u32, partial: f32) {
        self.pending += u64::from(ticks);
        self.partial = if partial.is_finite() {
            partial.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    /// Everything banked goes into the next frame; Java carries any excess over its cap.
    pub fn take(&mut self) -> (u32, f32) {
        let ticks = self.pending.min(u64::from(u32::MAX)) as u32;
        self.pending -= u64::from(ticks);
        self.posted += u64::from(ticks);
        (ticks, self.partial)
    }

    pub fn posted(&self) -> u64 {
        self.posted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacer::TickClock;

    #[test]
    fn lockstep_posts_one_frame_at_a_time() {
        let t0 = Instant::now();
        let mut l = Lockstep::new();
        assert!(!l.can_post(), "nothing is posted while Java boots");
        l.ready();
        assert!(l.can_post());
        assert_eq!(l.post(t0), 0);
        assert!(!l.can_post());
        assert!(!l.complete(0));
        assert!(l.complete(1));
        assert!(l.can_post());
        assert_eq!(l.post(t0), 1);
        assert_eq!(l.outstanding(), Some(1));
    }

    #[test]
    fn watchdog_detaches_after_two_seconds_once_loaded() {
        let t0 = Instant::now();
        let mut l = Lockstep::new();
        l.ready();
        l.post(t0);
        assert_eq!(l.wait_until(t0, true), Some(t0 + FRAME_WAIT));
        let late = t0 + Duration::from_millis(1950);
        assert_eq!(l.wait_until(late, true), Some(t0 + HANG_TIMEOUT));
        assert!(!l.expire(t0 + Duration::from_millis(1999), true));
        assert_eq!(l.state(), LinkState::Running);
        assert!(l.expire(t0 + HANG_TIMEOUT, true));
        assert_eq!(l.state(), LinkState::Detached);
        assert!(!l.expire(t0 + HANG_TIMEOUT * 2, true), "detaches once");
        assert_eq!(
            l.wait_until(t0, true),
            None,
            "a detached bridge is never awaited"
        );
        assert!(!l.can_post(), "the stuck frame is still outstanding");
        assert!(l.complete(1));
        assert!(
            l.can_post(),
            "a detached Java that recovers keeps getting frames"
        );
    }

    #[test]
    fn loading_frames_wait_briefly_and_hang_late() {
        let t0 = Instant::now();
        let mut l = Lockstep::new();
        l.ready();
        l.post(t0);
        let later = t0 + Duration::from_secs(10);
        assert_eq!(l.wait_until(later, false), Some(later + LOADING_WAIT));
        assert!(!l.expire(later, false));
        assert!(l.expire(t0 + LOADING_HANG_TIMEOUT, false));
    }

    #[test]
    fn frame_shared_wait_sees_java_done_and_death() {
        let shared = FrameShared::new();
        let start = Instant::now();
        assert!(!shared.wait_done(1, start + Duration::from_millis(20)));
        assert!(start.elapsed() >= Duration::from_millis(20));
        shared.simulate_done(1, 3, FSTATE_READY | FSTATE_LOADED);
        assert!(shared.wait_done(1, Instant::now() + Duration::from_secs(1)));
        let f = shared.read();
        assert_eq!((f.done, f.ticks, f.ready, f.loaded), (1, 3, true, true));
        shared.java_gone();
        let start = Instant::now();
        assert!(!shared.wait_done(2, start + Duration::from_secs(5)));
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "death ends the wait"
        );
    }

    /// Java's side of `DeltaTracker.Timer.advanceGameTime`: run up to the cap, carry the rest.
    #[derive(Default)]
    struct JavaTimer {
        pending: u64,
        ran: u64,
    }

    impl JavaTimer {
        fn frame(&mut self, ticks: u32) {
            self.pending += u64::from(ticks);
            let run = self.pending.min(u64::from(MAX_TICKS_PER_FRAME));
            self.pending -= run;
            self.ran += run;
        }
    }

    #[test]
    fn tick_counts_agree_over_a_minute_of_variable_frames() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rand = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut clock = TickClock::default();
        let mut bank = TickBank::default();
        let mut lock = Lockstep::new();
        lock.ready();
        let mut java = JavaTimer::default();
        let mut rust_ticks = 0u64;
        let mut elapsed = Duration::ZERO;
        let now = Instant::now();
        let mut java_busy = 0u64;
        while elapsed < Duration::from_secs(60) {
            let dt = match rand() % 100 {
                0 => Duration::from_millis(700),
                1..=9 => Duration::from_millis(40 + rand() % 60),
                _ => Duration::from_micros(2_000 + rand() % 15_000),
            };
            elapsed += dt;
            let (ticks, partial) = clock.advance(dt);
            assert!((0.0..=1.0).contains(&partial));
            rust_ticks += u64::from(ticks);
            bank.add(ticks, partial);
            if lock.outstanding().is_some() && java_busy == 0 {
                lock.complete(lock.outstanding().unwrap() + 1);
            }
            java_busy = java_busy.saturating_sub(1);
            if lock.can_post() {
                lock.post(now);
                let (t, _) = bank.take();
                java.frame(t);
                if rand() % 20 == 0 {
                    java_busy = 1 + rand() % 3;
                }
            }
        }
        for _ in 0..64 {
            if let Some(i) = lock.outstanding() {
                lock.complete(i + 1);
            }
            lock.post(now);
            let (t, _) = bank.take();
            java.frame(t);
        }
        assert!(rust_ticks > 1000, "{rust_ticks}");
        assert_eq!(bank.posted(), rust_ticks);
        assert_eq!(
            java.ran, rust_ticks,
            "Java ran exactly the ticks Rust decided"
        );
    }
}
