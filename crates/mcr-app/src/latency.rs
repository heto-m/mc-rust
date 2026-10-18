//! Mouse-turn latency: from the look a mouse event produced to the end of the first frame that shows it.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

const PENDING: usize = 512;
const SAMPLES: usize = 1 << 14;
/// Degrees: a shown look this close to a recorded one is that look (Java's camera round-trips through f32).
const SAME: f32 = 1.0e-3;

/// Looks (yaw, pitch in degrees) the mouse produced and the frames that showed them.
#[derive(Debug, Default)]
pub struct TurnLatency {
    pending: VecDeque<(Instant, f32, f32)>,
    samples_us: Vec<u32>,
    /// Frames that showed a look older than every unshown one.
    stale: u32,
}

impl TurnLatency {
    /// The mouse turned the player to `yaw`/`pitch` at `at`.
    pub fn look(&mut self, at: Instant, yaw: f32, pitch: f32) {
        if self.pending.len() == PENDING {
            self.pending.pop_front();
        }
        self.pending.push_back((at, yaw, pitch));
    }

    /// A frame showing the look `yaw`/`pitch` finished at `presented`; its latency counts from the
    /// oldest look it is the first to show.
    pub fn shown(&mut self, yaw: f32, pitch: f32, presented: Instant) {
        let Some(newest) = self
            .pending
            .iter()
            .rposition(|&(_, y, p)| yaw_diff(y, yaw) < SAME && (p - pitch).abs() < SAME)
        else {
            if !self.pending.is_empty() {
                self.stale += 1;
            }
            return;
        };
        let oldest = self.pending[0].0;
        self.pending.drain(..=newest);
        if self.samples_us.len() >= SAMPLES {
            self.samples_us.drain(..SAMPLES / 2);
        }
        let us = presented.saturating_duration_since(oldest).as_micros();
        self.samples_us.push(us.min(u128::from(u32::MAX)) as u32);
    }

    pub fn reset(&mut self) {
        self.pending.clear();
        self.samples_us.clear();
        self.stale = 0;
    }

    /// `(p50, p99, max)` of the samples so far, with the sample and stale-frame counts.
    pub fn summary(&self) -> Option<TurnSummary> {
        if self.samples_us.is_empty() {
            return None;
        }
        let mut v = self.samples_us.clone();
        v.sort_unstable();
        let at =
            |q: usize| Duration::from_micros(u64::from(v[(v.len() * q / 100).min(v.len() - 1)]));
        Some(TurnSummary {
            p50: at(50),
            p99: at(99),
            max: Duration::from_micros(u64::from(v[v.len() - 1])),
            samples: v.len(),
            stale: self.stale,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TurnSummary {
    pub p50: Duration,
    pub p99: Duration,
    pub max: Duration,
    pub samples: usize,
    pub stale: u32,
}

impl std::fmt::Display for TurnSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        write!(
            f,
            "turn latency p50 {:.2} p99 {:.2} max {:.2} ms ({} turns, {} stale frames)",
            ms(self.p50),
            ms(self.p99),
            ms(self.max),
            self.samples,
            self.stale
        )
    }
}

fn yaw_diff(a: f32, b: f32) -> f32 {
    ((a - b + 180.0).rem_euclid(360.0) - 180.0).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_frame_showing_the_newest_look_counts_from_the_oldest_unshown_one() {
        let t0 = Instant::now();
        let mut l = TurnLatency::default();
        l.look(t0, 10.0, 0.0);
        l.look(t0 + ms(2), 11.0, 0.0);
        l.shown(11.0, 0.0, t0 + ms(5));
        let s = l.summary().unwrap();
        assert_eq!((s.p50, s.samples, s.stale), (ms(5), 1, 0));
        l.shown(11.0, 0.0, t0 + ms(6));
        assert_eq!(
            l.summary().unwrap().samples,
            1,
            "an unchanged look is no new turn"
        );
    }

    #[test]
    fn a_frame_from_an_older_camera_waits_until_the_turn_shows() {
        let t0 = Instant::now();
        let mut l = TurnLatency::default();
        l.look(t0, 0.0, 0.0);
        l.look(t0 + ms(1), 0.5, 0.0);
        l.look(t0 + ms(2), 1.0, 0.0);
        l.shown(0.0, 0.0, t0 + ms(3));
        l.shown(359.9, 0.0, t0 + ms(4));
        l.shown(1.0, 0.0, t0 + ms(40));
        let s = l.summary().unwrap();
        assert_eq!(s.samples, 2);
        assert_eq!(s.max, ms(39), "the stale frames held the turn back");
        assert_eq!(s.stale, 1);
    }

    #[test]
    fn yaw_wraps_and_the_pending_list_is_bounded() {
        let t0 = Instant::now();
        let mut l = TurnLatency::default();
        for i in 0..PENDING * 2 {
            l.look(t0, (i % 360) as f32, 0.0);
        }
        assert_eq!(l.pending.len(), PENDING);
        l.look(t0, 359.9995, 0.0);
        l.shown(-0.0004, 0.0, t0 + ms(1));
        assert_eq!(l.summary().unwrap().samples, 1);
        assert!(l.pending.is_empty());
    }
}
