//! Frame lockstep with Java: the shared frame state, the hang watchdog and the tick hand-off.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use mcr_jvm::Doorbell;

use crate::abi::{
    CAMERA_DETACHED, CAMERA_INPUT, CAMERA_MOD, FSTATE_CAMERA, FSTATE_CAMERA_PITCH,
    FSTATE_CAMERA_POS, FSTATE_CAMERA_ROLL, FSTATE_CAMERA_YAW, FSTATE_DONE, FSTATE_FLAGS,
    FSTATE_FOV_SCALE, FSTATE_HEIGHT, FSTATE_LOADED, FSTATE_NANOS, FSTATE_PERSPECTIVE, FSTATE_PROF,
    FSTATE_READY, FSTATE_SIZE, FSTATE_STEPS, FSTATE_TICKS, FSTATE_TURN, FSTATE_VIEW_ROLL,
    FSTATE_WEATHER, FSTATE_WIDTH, PERSPECTIVE_FIRST, PERSPECTIVE_NONE, PERSPECTIVE_THIRD_BACK,
    PERSPECTIVE_THIRD_FRONT, PROF_COUNT,
};

/// After loading, a Java frame slower than this detaches the bridge from rendering.
pub const HANG_TIMEOUT: Duration = Duration::from_secs(2);
/// While Java loads resources a frame may legitimately take seconds.
pub const LOADING_HANG_TIMEOUT: Duration = Duration::from_secs(60);
/// While loading, mcr shows its own frame instead of waiting longer than this for Java's.
pub const LOADING_WAIT: Duration = Duration::from_millis(50);
/// Once loaded, a frame waits at most this long; a later Java frame shows its previous GUI.
pub const FRAME_WAIT: Duration = Duration::from_millis(100);
/// A stalled Java that finishes this many frames in a row, each within [`PROMPT_FRAME`], is
/// composed again.
pub const REATTACH_FRAMES: u32 = 30;
pub const PROMPT_FRAME: Duration = Duration::from_millis(250);
/// Stalls a session recovers from; the next one detaches for good.
pub const MAX_REATTACHES: u32 = 3;
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
    fov_scale: AtomicU32,
    camera: AtomicU32,
    _pad: u32,
    camera_pos: [AtomicU64; 3],
    camera_yaw: AtomicU32,
    camera_pitch: AtomicU32,
    prof: [AtomicU32; PROF_COUNT],
    camera_roll: AtomicU32,
    perspective: AtomicU32,
    view_roll: AtomicU32,
    _pad2: u32,
}

const _: () = {
    assert!(std::mem::offset_of!(FrameState, done) == FSTATE_DONE);
    assert!(std::mem::offset_of!(FrameState, ticks) == FSTATE_TICKS);
    assert!(std::mem::offset_of!(FrameState, nanos) == FSTATE_NANOS);
    assert!(std::mem::offset_of!(FrameState, flags) == FSTATE_FLAGS);
    assert!(std::mem::offset_of!(FrameState, width) == FSTATE_WIDTH);
    assert!(std::mem::offset_of!(FrameState, height) == FSTATE_HEIGHT);
    assert!(std::mem::offset_of!(FrameState, fov_scale) == FSTATE_FOV_SCALE);
    assert!(std::mem::offset_of!(FrameState, camera) == FSTATE_CAMERA);
    assert!(std::mem::offset_of!(FrameState, camera_pos) == FSTATE_CAMERA_POS);
    assert!(std::mem::offset_of!(FrameState, camera_yaw) == FSTATE_CAMERA_YAW);
    assert!(std::mem::offset_of!(FrameState, camera_pitch) == FSTATE_CAMERA_PITCH);
    assert!(std::mem::offset_of!(FrameState, prof) == FSTATE_PROF);
    assert!(std::mem::offset_of!(FrameState, camera_roll) == FSTATE_CAMERA_ROLL);
    assert!(std::mem::offset_of!(FrameState, perspective) == FSTATE_PERSPECTIVE);
    assert!(std::mem::offset_of!(FrameState, view_roll) == FSTATE_VIEW_ROLL);
    assert!(std::mem::size_of::<FrameState>() == FSTATE_SIZE);
};

/// FOV mods (zoom) may shrink or widen the view this much at most.
pub const FOV_SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.02..=4.0;

/// What Java last reported about its frames.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
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
    /// The factor mods applied to Java's field of view, within [`FOV_SCALE_RANGE`]; 1 when none did.
    pub fov_scale: f32,
    /// The camera a mod gave Java's last frame; `None` while Java's camera is mcr's.
    pub camera: Option<JavaCamera>,
    /// A mod replaced vanilla's rain and snow, so mcr must not draw its own.
    pub weather_replaced: bool,
    /// A mod replaced the local player's footsteps (Presence Footsteps).
    pub steps_replaced: bool,
    /// A mod consumed the last mouse turn of the local player (Freelook).
    pub turn_captured: bool,
    /// Java's camera type while it leads the perspective.
    pub perspective: Option<Perspective>,
    /// Radians about the view axis that mods added to the level's bob pose, within `[-PI, PI]`.
    pub view_roll: f32,
    /// Phase times of Java's last frame in microseconds, indexed by `abi::PROF_*`.
    pub prof: [u32; PROF_COUNT],
}

/// Java's camera type, which mcr's perspective follows while Java draws the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perspective {
    First,
    ThirdBack,
    ThirdFront,
}

impl Perspective {
    fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            PERSPECTIVE_FIRST => Some(Self::First),
            PERSPECTIVE_THIRD_BACK => Some(Self::ThirdBack),
            PERSPECTIVE_THIRD_FRONT => Some(Self::ThirdFront),
            PERSPECTIVE_NONE => None,
            _ => None,
        }
    }
}

fn wrap_angle(a: f32) -> f32 {
    let pi = std::f32::consts::PI;
    ((a + pi).rem_euclid(2.0 * pi) - pi).clamp(-pi, pi)
}

fn view_roll(bits: u32) -> f32 {
    let roll = f32::from_bits(bits);
    if roll.is_finite() {
        wrap_angle(roll)
    } else {
        0.0
    }
}

/// A camera a mod set in Java, sane but not yet bounded to the loaded world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JavaCamera {
    pub pos: [f64; 3],
    /// Radians in mcr's convention, `yaw` within `[-PI, PI]` and `pitch` within `[-PI/2, PI/2]`.
    pub yaw: f32,
    pub pitch: f32,
    /// Radians about the view axis in view space, within `[-PI, PI]`; 0 when the mod set none.
    pub roll: f32,
    /// The camera shows the local player (third person or another camera entity).
    pub detached: bool,
    /// The mod moves its camera with the movement keys, so the player stands still.
    pub input: bool,
}

impl JavaCamera {
    fn new(flags: u32, pos: [f64; 3], yaw: f32, pitch: f32, roll: f32) -> Option<Self> {
        if flags & CAMERA_MOD == 0
            || !pos.iter().all(|v| v.is_finite())
            || !yaw.is_finite()
            || !pitch.is_finite()
        {
            return None;
        }
        let half_pi = std::f32::consts::FRAC_PI_2;
        Some(Self {
            pos,
            yaw: wrap_angle(yaw),
            pitch: pitch.clamp(-half_pi, half_pi),
            roll: if roll.is_finite() {
                wrap_angle(roll)
            } else {
                0.0
            },
            detached: flags & CAMERA_DETACHED != 0,
            input: flags & CAMERA_INPUT != 0,
        })
    }

    /// Keeps the camera within `reach` blocks horizontally of `anchor` and inside `ys`, so terrain
    /// streaming and culling never see a far-off camera.
    pub fn bounded(
        mut self,
        anchor: [f64; 3],
        reach: f64,
        ys: std::ops::RangeInclusive<f64>,
    ) -> Self {
        let reach = if reach.is_finite() {
            reach.max(0.0)
        } else {
            0.0
        };
        for a in [0, 2] {
            self.pos[a] = self.pos[a].clamp(anchor[a] - reach, anchor[a] + reach);
        }
        if ys.start() <= ys.end() {
            self.pos[1] = self.pos[1].clamp(*ys.start(), *ys.end());
        }
        self
    }
}

fn fov_scale(bits: u32) -> f32 {
    let scale = f32::from_bits(bits);
    if scale.is_finite() && scale > 0.0 {
        scale.clamp(*FOV_SCALE_RANGE.start(), *FOV_SCALE_RANGE.end())
    } else {
        1.0
    }
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
            fov_scale: fov_scale(s.fov_scale.load(Ordering::Acquire)),
            camera: JavaCamera::new(
                s.camera.load(Ordering::Acquire),
                s.camera_pos
                    .each_ref()
                    .map(|v| f64::from_bits(v.load(Ordering::Acquire))),
                f32::from_bits(s.camera_yaw.load(Ordering::Acquire)),
                f32::from_bits(s.camera_pitch.load(Ordering::Acquire)),
                f32::from_bits(s.camera_roll.load(Ordering::Acquire)),
            ),
            weather_replaced: flags & FSTATE_WEATHER != 0,
            steps_replaced: flags & FSTATE_STEPS != 0,
            turn_captured: flags & FSTATE_TURN != 0,
            perspective: Perspective::from_raw(s.perspective.load(Ordering::Acquire)),
            view_roll: view_roll(s.view_roll.load(Ordering::Acquire)),
            prof: s.prof.each_ref().map(|v| v.load(Ordering::Acquire)),
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

    #[cfg(test)]
    pub fn simulate_prof(&self, prof: [u32; PROF_COUNT]) {
        for (dst, v) in self.state.prof.iter().zip(prof) {
            dst.store(v, Ordering::Release);
        }
    }

    #[cfg(test)]
    pub fn simulate_view(&self, perspective: u32, view_roll: f32) {
        self.state.perspective.store(perspective, Ordering::Release);
        self.state
            .view_roll
            .store(view_roll.to_bits(), Ordering::Release);
    }

    #[cfg(test)]
    pub fn simulate_camera(&self, flags: u32, pos: [f64; 3], yaw: f32, pitch: f32, roll: f32) {
        let s = &self.state;
        for (dst, v) in s.camera_pos.iter().zip(pos) {
            dst.store(v.to_bits(), Ordering::Release);
        }
        s.camera_yaw.store(yaw.to_bits(), Ordering::Release);
        s.camera_pitch.store(pitch.to_bits(), Ordering::Release);
        s.camera_roll.store(roll.to_bits(), Ordering::Release);
        s.camera.store(flags, Ordering::Release);
    }
}

/// Chooses [`Pipelining`]: `lockstep`/`0`, `ahead`/`1`, anything else is automatic.
pub const PIPELINE_ENV: &str = "MCR_MOD_PIPELINE";
/// While mcr's frame rate is capped, Java runs ahead once its frames take longer than this...
pub const AHEAD_ON: Duration = Duration::from_millis(8);
/// ...and goes back to lockstep once they are shorter than this.
pub const AHEAD_OFF: Duration = Duration::from_millis(4);

/// How Java's frames line up with mcr's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pipelining {
    /// Java renders the frame mcr posted at its start, composed in that same mcr frame: lowest
    /// latency, but mcr waits for all of Java's frame.
    Lockstep,
    /// Java renders the next frame while mcr draws and presents this one; its frame is composed one
    /// mcr frame later, its world redrawn from mcr's current camera.
    Ahead,
    /// Ahead while mcr's frame rate is uncapped or Java cannot keep up, lockstep otherwise.
    Auto,
}

impl Pipelining {
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "0" | "lockstep" => Self::Lockstep,
            "1" | "ahead" => Self::Ahead,
            _ => Self::Auto,
        }
    }

    pub(crate) fn from_env() -> Self {
        std::env::var(PIPELINE_ENV).map_or(Self::Auto, |v| Self::parse(&v))
    }
}

/// Decides [`Pipelining::Auto`] from Java's mean frame time, with hysteresis.
#[derive(Debug, Clone, Copy, Default)]
pub struct AheadPolicy {
    ahead: bool,
    java_ms: f32,
}

impl AheadPolicy {
    /// Java finished a frame that took `took`.
    pub fn java_frame(&mut self, took: Duration) {
        let ms = took.as_secs_f32() * 1000.0;
        if ms.is_finite() {
            self.java_ms += (ms - self.java_ms) * 0.1;
        }
    }

    /// Whether Java's frames run one mcr frame ahead from now on.
    pub fn decide(&mut self, mode: Pipelining, uncapped: bool) -> bool {
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        self.ahead = match mode {
            Pipelining::Lockstep => false,
            Pipelining::Ahead => true,
            Pipelining::Auto if uncapped => true,
            Pipelining::Auto if self.ahead => self.java_ms > ms(AHEAD_OFF),
            Pipelining::Auto => self.java_ms > ms(AHEAD_ON),
        };
        self.ahead
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    /// Java has not reached its frame loop yet; nothing is posted or awaited.
    Booting,
    /// Lockstep: one frame posted at a time, its GUI composed in the same mcr frame.
    Running,
    /// Java missed the hang deadline or died; mcr renders natively and does not wait for it. A
    /// stall (not a death) re-attaches once Java is prompt again.
    Detached,
}

/// The render thread's view of the lockstep and its hang watchdog.
#[derive(Debug, Clone)]
pub struct Lockstep {
    state: LinkState,
    next: u64,
    outstanding: Option<(u64, Instant)>,
    /// Prompt frames in a row since a recoverable stall; `None` once detached for good.
    recovering: Option<u32>,
    reattaches: u32,
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
            recovering: None,
            reattaches: 0,
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

    /// Detaches for good: Java is gone or lost its connection.
    pub fn detach(&mut self) {
        self.state = LinkState::Detached;
        self.recovering = None;
    }

    /// Detached by a stall that may still re-attach.
    pub fn recoverable(&self) -> bool {
        self.state == LinkState::Detached && self.recovering.is_some()
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

    /// Java finished `done` frames in total by `now`; `true` if that completes the outstanding one.
    pub fn complete(&mut self, done: u64, now: Instant) -> bool {
        match self.outstanding {
            Some((index, posted)) if done > index => {
                self.outstanding = None;
                if let Some(prompt) = &mut self.recovering {
                    let on_time = now.saturating_duration_since(posted) <= PROMPT_FRAME;
                    *prompt = if on_time { *prompt + 1 } else { 0 };
                }
                true
            }
            _ => false,
        }
    }

    /// Re-attaches a stalled bridge after enough prompt frames; `true` on that transition.
    pub fn reattach(&mut self) -> bool {
        if self.state != LinkState::Detached
            || !matches!(self.recovering, Some(n) if n >= REATTACH_FRAMES)
        {
            return false;
        }
        self.state = LinkState::Running;
        self.recovering = None;
        self.reattaches += 1;
        true
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
                self.recovering = (self.reattaches < MAX_REATTACHES).then_some(0);
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
    fn pipelining_modes_parse() {
        assert_eq!(Pipelining::parse("0"), Pipelining::Lockstep);
        assert_eq!(Pipelining::parse(" lockstep\n"), Pipelining::Lockstep);
        assert_eq!(Pipelining::parse("ahead"), Pipelining::Ahead);
        assert_eq!(Pipelining::parse("1"), Pipelining::Ahead);
        assert_eq!(Pipelining::parse("auto"), Pipelining::Auto);
        assert_eq!(Pipelining::parse(""), Pipelining::Auto);
    }

    #[test]
    fn auto_runs_ahead_when_uncapped_or_when_java_is_slow() {
        let mut p = AheadPolicy::default();
        assert!(
            !p.decide(Pipelining::Auto, false),
            "fast Java under vsync stays in lockstep"
        );
        assert!(
            p.decide(Pipelining::Auto, true),
            "uncapped frames run ahead"
        );
        assert!(!p.decide(Pipelining::Lockstep, true));
        assert!(p.decide(Pipelining::Ahead, false));
        let mut p = AheadPolicy::default();
        for _ in 0..100 {
            p.java_frame(Duration::from_millis(12));
        }
        assert!(
            p.decide(Pipelining::Auto, false),
            "slow Java runs ahead under vsync"
        );
        for _ in 0..100 {
            p.java_frame(Duration::from_millis(6));
        }
        assert!(
            p.decide(Pipelining::Auto, false),
            "hysteresis keeps it ahead"
        );
        for _ in 0..100 {
            p.java_frame(Duration::from_millis(2));
        }
        assert!(!p.decide(Pipelining::Auto, false), "fast again: lockstep");
        for _ in 0..100 {
            p.java_frame(Duration::from_millis(6));
        }
        assert!(
            !p.decide(Pipelining::Auto, false),
            "and stays there until slow"
        );
    }

    #[test]
    fn fov_scale_is_sanitized() {
        assert_eq!(fov_scale(0), 1.0);
        assert_eq!(fov_scale(f32::NAN.to_bits()), 1.0);
        assert_eq!(fov_scale(f32::INFINITY.to_bits()), 1.0);
        assert_eq!(fov_scale((-0.5f32).to_bits()), 1.0);
        assert_eq!(fov_scale(0.25f32.to_bits()), 0.25);
        assert_eq!(fov_scale(1.0e-9f32.to_bits()), 0.02);
        assert_eq!(fov_scale(100.0f32.to_bits()), 4.0);
        assert_eq!(FrameShared::new().read().fov_scale, 1.0);
    }

    #[test]
    fn java_cameras_are_sanitized() {
        let shared = FrameShared::new();
        assert_eq!(shared.read().camera, None);
        shared.simulate_camera(CAMERA_DETACHED, [1.0, 2.0, 3.0], 0.5, 0.25, 0.0);
        assert_eq!(
            shared.read().camera,
            None,
            "without CAMERA_MOD Java's camera is mcr's"
        );
        shared.simulate_camera(CAMERA_MOD | CAMERA_INPUT, [1.0, 2.0, 3.0], 7.0, -3.0, -7.0);
        let cam = shared.read().camera.unwrap();
        assert_eq!(cam.pos, [1.0, 2.0, 3.0]);
        assert!((cam.roll - (std::f32::consts::TAU - 7.0)).abs() < 1e-5);
        assert!((cam.yaw - (7.0 - std::f32::consts::TAU)).abs() < 1e-5);
        assert_eq!(cam.pitch, -std::f32::consts::FRAC_PI_2);
        assert!(cam.input && !cam.detached);
        for bad in [f64::NAN, f64::INFINITY] {
            shared.simulate_camera(CAMERA_MOD, [0.0, bad, 0.0], 0.0, 0.0, 0.0);
            assert_eq!(shared.read().camera, None);
        }
        shared.simulate_camera(CAMERA_MOD, [0.0; 3], f32::NAN, 0.0, 0.0);
        assert_eq!(shared.read().camera, None);
        shared.simulate_camera(CAMERA_MOD, [0.0; 3], 0.0, 0.0, f32::INFINITY);
        assert_eq!(
            shared.read().camera.unwrap().roll,
            0.0,
            "a bad roll is dropped, not the camera"
        );
    }

    #[test]
    fn perspective_and_view_roll_are_sanitized() {
        let shared = FrameShared::new();
        let java = shared.read();
        assert_eq!((java.perspective, java.view_roll), (None, 0.0));
        for (raw, want) in [
            (PERSPECTIVE_FIRST, Some(Perspective::First)),
            (PERSPECTIVE_THIRD_BACK, Some(Perspective::ThirdBack)),
            (PERSPECTIVE_THIRD_FRONT, Some(Perspective::ThirdFront)),
            (PERSPECTIVE_NONE, None),
            (4, None),
            (u32::MAX, None),
        ] {
            shared.simulate_view(raw, 0.0);
            assert_eq!(shared.read().perspective, want, "{raw}");
        }
        shared.simulate_view(0, 0.25);
        assert_eq!(shared.read().view_roll, 0.25);
        shared.simulate_view(0, 7.0);
        assert!((shared.read().view_roll - (7.0 - std::f32::consts::TAU)).abs() < 1e-5);
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            shared.simulate_view(0, bad);
            assert_eq!(shared.read().view_roll, 0.0);
        }
    }

    #[test]
    fn java_cameras_stay_near_the_player() {
        let cam = JavaCamera::new(CAMERA_MOD, [1e9, -1e9, -5.0], 0.0, 0.0, 0.0).unwrap();
        let b = cam.bounded([10.0, 64.0, 0.0], 100.0, -128.0..=384.0);
        assert_eq!(b.pos, [110.0, -128.0, -5.0]);
        let b = cam.bounded([0.0; 3], f64::NAN, 1.0..=0.0);
        assert_eq!(b.pos, [0.0, -1e9, 0.0]);
    }

    #[test]
    fn lockstep_posts_one_frame_at_a_time() {
        let t0 = Instant::now();
        let mut l = Lockstep::new();
        assert!(!l.can_post(), "nothing is posted while Java boots");
        l.ready();
        assert!(l.can_post());
        assert_eq!(l.post(t0), 0);
        assert!(!l.can_post());
        assert!(!l.complete(0, t0));
        assert!(l.complete(1, t0));
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
        assert!(l.complete(1, t0 + HANG_TIMEOUT * 3));
        assert!(
            l.can_post(),
            "a detached Java that recovers keeps getting frames"
        );
    }

    fn stall(l: &mut Lockstep, t: Instant) -> Instant {
        let index = l.post(t);
        let late = t + HANG_TIMEOUT;
        assert!(l.expire(late, true));
        assert!(l.complete(index + 1, late));
        late
    }

    fn prompt_frames(l: &mut Lockstep, mut t: Instant, n: u32) -> Instant {
        for _ in 0..n {
            let index = l.post(t);
            t += Duration::from_millis(16);
            assert!(l.complete(index + 1, t));
        }
        t
    }

    #[test]
    fn a_stall_reattaches_after_prompt_frames() {
        let mut l = Lockstep::new();
        l.ready();
        let mut t = stall(&mut l, Instant::now());
        assert!(l.recoverable());
        t = prompt_frames(&mut l, t, REATTACH_FRAMES - 1);
        let index = l.post(t);
        assert!(l.complete(index + 1, t + PROMPT_FRAME * 2));
        assert!(!l.reattach(), "a slow frame restarts the count");
        t = prompt_frames(&mut l, t + PROMPT_FRAME * 2, REATTACH_FRAMES - 1);
        assert!(!l.reattach());
        t = prompt_frames(&mut l, t, 1);
        assert!(l.reattach());
        assert_eq!(l.state(), LinkState::Running);
        assert!(!l.reattach(), "re-attaches once");
        assert!(l.wait_until(t, true).is_none(), "nothing outstanding");
        l.post(t);
        assert!(l.wait_until(t, true).is_some(), "awaited again");
    }

    #[test]
    fn deaths_and_repeated_stalls_detach_for_good() {
        let mut l = Lockstep::new();
        l.ready();
        let mut t = Instant::now();
        for _ in 0..MAX_REATTACHES {
            t = stall(&mut l, t);
            t = prompt_frames(&mut l, t, REATTACH_FRAMES);
            assert!(l.reattach());
        }
        t = stall(&mut l, t);
        assert!(!l.recoverable());
        prompt_frames(&mut l, t, REATTACH_FRAMES * 2);
        assert!(!l.reattach(), "the last stall is final");

        let mut l = Lockstep::new();
        l.ready();
        let t = stall(&mut l, Instant::now());
        l.detach();
        prompt_frames(&mut l, t, REATTACH_FRAMES * 2);
        assert!(!l.reattach(), "Java gone is final");
        assert_eq!(l.state(), LinkState::Detached);
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
        shared.simulate_prof([1, 2, 3, 4, 5, 6, 7, 8]);
        let f = shared.read();
        assert_eq!((f.done, f.ticks, f.ready, f.loaded), (1, 3, true, true));
        assert_eq!(f.prof, [1, 2, 3, 4, 5, 6, 7, 8]);
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
                lock.complete(lock.outstanding().unwrap() + 1, now);
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
                lock.complete(i + 1, now);
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
