//! Where a mod-mode frame's time goes: the render thread's wait for Java and its submits, Java's
//! frame phases, and the Rust GPU work Java's downcalls do on its own thread.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::abi::{
    PROF_COUNT, PROF_EXTRACT, PROF_FRAME, PROF_GUI, PROF_PRE, PROF_RENDER, PROF_TICK, PROF_WORLD,
};
use crate::gpu::backend::{StatsSnapshot, cost};

/// Set to print a frame profile line every second and a per-mod Java breakdown every ten.
pub const PROFILE_ENV: &str = "MCR_MOD_PROFILE";
/// The Java property that starts the per-mod sampler.
pub const PROFILE_PROPERTY: &str = "mcr.profile";

/// `MCR_MOD_PROFILE` is set to anything but `0`.
pub fn enabled() -> bool {
    mode().is_some()
}

/// The per-mod sampler runs too, unless `MCR_MOD_PROFILE=frame` (JFR's sampling slows Java down).
pub fn per_mod() -> bool {
    mode().is_some_and(|m| m != "frame")
}

fn mode() -> Option<&'static str> {
    static MODE: OnceLock<Option<String>> = OnceLock::new();
    MODE.get_or_init(|| {
        std::env::var(PROFILE_ENV)
            .ok()
            .filter(|v| !v.is_empty() && v != "0")
    })
    .as_deref()
}

/// What one finished Java frame cost, as the render thread saw it complete.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct JavaSample {
    pub total_us: u32,
    pub phases: [u32; PROF_COUNT],
    pub gpu: StatsSnapshot,
}

/// Sums over a window of frames; formatted as per-frame means.
#[derive(Debug, Clone, Default)]
pub struct ProfileWindow {
    pub frames: u64,
    pub frame_ns: u64,
    pub wait_ns: u64,
    pub wait_max_ns: u64,
    pub submit_ns: u64,
    /// Frames composed with the Java frame posted one mcr frame earlier.
    pub ahead: u64,
    pub java_frames: u64,
    pub java_us: u64,
    pub phases: [u64; PROF_COUNT],
    pub gpu: StatsSnapshot,
}

fn add(a: &mut StatsSnapshot, d: &StatsSnapshot) {
    a.passes += d.passes;
    a.ops += d.ops;
    a.draws += d.draws;
    a.skipped_draws += d.skipped_draws;
    a.bind_hits += d.bind_hits;
    a.bind_misses += d.bind_misses;
    a.encode_ns += d.encode_ns;
    a.submits += d.submits;
    a.uploaded += d.uploaded;
    a.errors += d.errors;
    a.ffi_ns += d.ffi_ns;
    a.ffi_calls += d.ffi_calls;
    for i in 0..cost::COUNT {
        a.cost_ns[i] += d.cost_ns[i];
        a.cost_calls[i] += d.cost_calls[i];
    }
}

/// `now - before`, counter by counter.
pub fn delta(now: &StatsSnapshot, before: &StatsSnapshot) -> StatsSnapshot {
    let d = |a: u64, b: u64| a.saturating_sub(b);
    StatsSnapshot {
        passes: d(now.passes, before.passes),
        ops: d(now.ops, before.ops),
        draws: d(now.draws, before.draws),
        skipped_draws: d(now.skipped_draws, before.skipped_draws),
        bind_hits: d(now.bind_hits, before.bind_hits),
        bind_misses: d(now.bind_misses, before.bind_misses),
        encode_ns: d(now.encode_ns, before.encode_ns),
        submits: d(now.submits, before.submits),
        uploaded: d(now.uploaded, before.uploaded),
        errors: d(now.errors, before.errors),
        ffi_ns: d(now.ffi_ns, before.ffi_ns),
        ffi_calls: d(now.ffi_calls, before.ffi_calls),
        cost_ns: std::array::from_fn(|i| d(now.cost_ns[i], before.cost_ns[i])),
        cost_calls: std::array::from_fn(|i| d(now.cost_calls[i], before.cost_calls[i])),
    }
}

fn ns(d: Duration) -> u64 {
    d.as_nanos().min(u128::from(u64::MAX)) as u64
}

impl ProfileWindow {
    /// One mcr frame: its period, its wait for Java and its submits of Java's work.
    pub fn frame(&mut self, period: Duration, wait: Duration, submit: Duration) {
        self.frames += 1;
        self.frame_ns += ns(period);
        let w = ns(wait);
        self.wait_ns += w;
        self.wait_max_ns = self.wait_max_ns.max(w);
        self.submit_ns += ns(submit);
    }

    pub fn java(&mut self, s: &JavaSample) {
        self.java_frames += 1;
        self.java_us += u64::from(s.total_us);
        for (sum, v) in self.phases.iter_mut().zip(s.phases) {
            *sum += u64::from(v);
        }
        add(&mut self.gpu, &s.gpu);
    }

    /// Per-frame means in milliseconds, `-` placeholders when nothing was measured.
    pub fn line(&self) -> String {
        let f = self.frames.max(1) as f64;
        let ms = |ns: u64| ns as f64 / 1e6 / f;
        let mut out = format!(
            "{:.0} fps ({:.3} ms) | wait {:.3} ms (max {:.2}) | submit {:.3} ms",
            if self.frame_ns == 0 {
                0.0
            } else {
                f * 1e9 / self.frame_ns as f64
            },
            ms(self.frame_ns),
            ms(self.wait_ns),
            self.wait_max_ns as f64 / 1e6,
            ms(self.submit_ns),
        );
        if self.ahead > 0 {
            out += &format!(" | ahead {:.0}%", self.ahead as f64 * 100.0 / f);
        }
        if self.java_frames == 0 {
            return out;
        }
        let j = self.java_frames as f64;
        let us = |v: u64| v as f64 / 1e3 / j;
        let p = |i: usize| us(self.phases[i]);
        let render_other = p(PROF_RENDER) - p(PROF_WORLD) - p(PROF_GUI);
        let frame_other = p(PROF_FRAME) - p(PROF_EXTRACT) - p(PROF_RENDER);
        let rest = us(self.java_us) - p(PROF_PRE) - p(PROF_TICK) - p(PROF_FRAME);
        let g = &self.gpu;
        out += &format!(
            " | java {:.3} ms: pre {:.3} tick {:.3} extract {:.3} world {:.3} gui {:.3} render-other {:.3} frame-other {:.3} rest {:.3}",
            us(self.java_us),
            p(PROF_PRE),
            p(PROF_TICK),
            p(PROF_EXTRACT),
            p(PROF_WORLD),
            p(PROF_GUI),
            render_other.max(0.0),
            frame_other.max(0.0),
            rest.max(0.0),
        );
        out += &format!(
            " | in rust {:.3} ms ({:.0} calls), pass encode {:.3} ms | {:.0} passes {:.0} draws {:.0} submits {:.0} KiB uploaded per java frame",
            g.ffi_ns as f64 / 1e6 / j,
            g.ffi_calls as f64 / j,
            g.encode_ns as f64 / 1e6 / j,
            g.passes as f64 / j,
            g.draws as f64 / j,
            g.submits as f64 / j,
            g.uploaded as f64 / 1024.0 / j,
        );
        if g.cost_calls.iter().any(|&c| c > 0) {
            out += " | rust us per java frame:";
            for (i, name) in cost::NAMES.iter().enumerate() {
                if g.cost_calls[i] > 0 {
                    out += &format!(
                        " {name} {:.1} ({:.1})",
                        g.cost_ns[i] as f64 / 1e3 / j,
                        g.cost_calls[i] as f64 / j
                    );
                }
            }
        }
        out
    }
}

/// The render thread's frame profile: a window logged every second when [`enabled`], and a
/// running total for benchmarks.
#[derive(Debug)]
pub struct FrameProfiler {
    log: bool,
    window: ProfileWindow,
    window_start: Instant,
    total: ProfileWindow,
    /// Java frames over twice the running mean, which make the frame time's tail.
    slow: ProfileWindow,
    frame_start: Option<Instant>,
    wait: Duration,
    submit: Duration,
    ahead: bool,
    gpu_seen: StatsSnapshot,
    line: Option<String>,
}

pub const LOG_EVERY: Duration = Duration::from_secs(1);

impl FrameProfiler {
    pub fn new(log: bool) -> Self {
        Self {
            log,
            window: ProfileWindow::default(),
            window_start: Instant::now(),
            total: ProfileWindow::default(),
            slow: ProfileWindow::default(),
            frame_start: None,
            wait: Duration::ZERO,
            submit: Duration::ZERO,
            ahead: false,
            gpu_seen: StatsSnapshot::default(),
            line: None,
        }
    }

    /// A new mcr frame starts at `now`, closing the previous one.
    pub fn begin_frame(&mut self, now: Instant) {
        if let Some(start) = self.frame_start {
            let period = now.saturating_duration_since(start);
            for w in [&mut self.window, &mut self.total] {
                w.frame(period, self.wait, self.submit);
                w.ahead += u64::from(self.ahead);
            }
        }
        self.frame_start = Some(now);
        self.wait = Duration::ZERO;
        self.submit = Duration::ZERO;
        self.ahead = false;
        if self.log && now.saturating_duration_since(self.window_start) >= LOG_EVERY {
            self.line = Some(self.window.line());
            self.window = ProfileWindow::default();
            self.window_start = now;
        }
    }

    pub fn waited(&mut self, d: Duration) {
        self.wait += d;
    }

    pub fn submitted(&mut self, d: Duration) {
        self.submit += d;
    }

    /// This mcr frame composes a Java frame posted in an earlier mcr frame.
    pub fn composed_ahead(&mut self) {
        self.ahead = true;
    }

    /// Java finished a frame; `gpu` is the backend's counters now.
    pub fn java_done(&mut self, total_us: u32, phases: [u32; PROF_COUNT], gpu: StatsSnapshot) {
        let sample = JavaSample {
            total_us,
            phases,
            gpu: delta(&gpu, &self.gpu_seen),
        };
        self.gpu_seen = gpu;
        let t = &self.total;
        if t.java_frames >= 100 && u64::from(total_us) * t.java_frames > 2 * t.java_us {
            self.slow.java(&sample);
        }
        self.window.java(&sample);
        self.total.java(&sample);
    }

    /// The Java frames over twice the mean so far.
    pub fn slow(&self) -> &ProfileWindow {
        &self.slow
    }

    pub fn take_line(&mut self) -> Option<String> {
        self.line.take()
    }

    pub fn total(&self) -> &ProfileWindow {
        &self.total
    }

    pub fn reset(&mut self) {
        self.total = ProfileWindow::default();
        self.slow = ProfileWindow::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_reports_means() {
        let mut w = ProfileWindow::default();
        assert!(w.line().starts_with("0 fps"));
        for _ in 0..4 {
            w.frame(
                Duration::from_millis(2),
                Duration::from_millis(1),
                Duration::from_micros(100),
            );
        }
        let mut phases = [0; PROF_COUNT];
        phases[PROF_TICK] = 100;
        phases[PROF_FRAME] = 1500;
        phases[PROF_RENDER] = 1000;
        phases[PROF_WORLD] = 600;
        phases[PROF_GUI] = 300;
        phases[PROF_EXTRACT] = 400;
        let gpu = StatsSnapshot {
            passes: 10,
            ffi_ns: 500_000,
            ffi_calls: 40,
            ..StatsSnapshot::default()
        };
        for _ in 0..2 {
            w.java(&JavaSample {
                total_us: 1800,
                phases,
                gpu,
            });
        }
        let line = w.line();
        assert!(
            line.starts_with("500 fps (2.000 ms) | wait 1.000 ms"),
            "{line}"
        );
        assert!(line.contains("java 1.800 ms"), "{line}");
        assert!(line.contains("render-other 0.100"), "{line}");
        assert!(line.contains("frame-other 0.100"), "{line}");
        assert!(line.contains("rest 0.200"), "{line}");
        assert!(line.contains("in rust 0.500 ms (40 calls)"), "{line}");
        assert!(line.contains("10 passes"), "{line}");
    }

    #[test]
    fn profiler_closes_frames_and_logs_once_a_second() {
        let mut p = FrameProfiler::new(true);
        let t0 = Instant::now();
        p.begin_frame(t0);
        p.waited(Duration::from_micros(300));
        p.submitted(Duration::from_micros(50));
        p.composed_ahead();
        let gpu = StatsSnapshot {
            draws: 100,
            ..StatsSnapshot::default()
        };
        p.java_done(900, [0; PROF_COUNT], gpu);
        p.java_done(900, [0; PROF_COUNT], gpu);
        p.begin_frame(t0 + Duration::from_millis(2));
        assert_eq!(p.total().frames, 1);
        assert_eq!(p.total().wait_ns, 300_000);
        assert_eq!(p.total().ahead, 1);
        assert_eq!(p.total().gpu.draws, 100, "counters are deltas");
        assert!(p.take_line().is_none());
        p.begin_frame(t0 + LOG_EVERY + Duration::from_millis(2));
        assert_eq!(p.total().ahead, 1);
        let line = p.take_line().unwrap();
        assert!(line.contains("ahead 50%"), "{line}");
        p.reset();
        assert_eq!(p.total().frames, 0);
    }

    #[test]
    fn deltas_saturate() {
        let a = StatsSnapshot {
            draws: 5,
            ..StatsSnapshot::default()
        };
        let b = StatsSnapshot {
            draws: 7,
            ffi_ns: 3,
            ..StatsSnapshot::default()
        };
        assert_eq!(delta(&b, &a).draws, 2);
        assert_eq!(delta(&a, &b).ffi_ns, 0);
    }
}
