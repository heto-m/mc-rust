//! Parameters of the looping sounds vanilla starts on the client for entities.

pub const ELYTRA_RAMP_START: u32 = 20;
pub const ELYTRA_RAMP_END: u32 = 40;
const GUARDIAN_PITCH_BASE: f32 = 0.7;
const GUARDIAN_PITCH_SPAN: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoopLevel {
    pub volume: f32,
    pub pitch: f32,
}

/// `ElytraOnPlayerSoundInstance`: silent for a second, fading in over the next, then speed driven.
#[derive(Clone, Copy, Debug, Default)]
pub struct ElytraLoop {
    time: u32,
}

impl ElytraLoop {
    pub const START_VOLUME: f32 = 0.1;

    /// `None` once the loop is over: the player stopped gliding after the start-up window.
    pub fn tick(&mut self, gliding: bool, speed_squared: f64) -> Option<LoopLevel> {
        self.time += 1;
        if self.time > ELYTRA_RAMP_START && !gliding {
            return None;
        }
        let mut volume = if speed_squared >= 1.0e-7 {
            (speed_squared as f32 / 4.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if self.time < ELYTRA_RAMP_START {
            volume = 0.0;
        } else if self.time < ELYTRA_RAMP_END {
            volume *= (self.time - ELYTRA_RAMP_START) as f32 / 20.0;
        }
        let pitch = if volume > 0.8 {
            1.0 + (volume - 0.8)
        } else {
            1.0
        };
        Some(LoopLevel { volume, pitch })
    }
}

/// `BeeSoundInstance` (vanilla feeds the clamped speed straight into `lerp`): louder and higher the faster the bee flies, mute when it hovers.
pub fn bee_level(horizontal_speed: f32, baby: bool) -> LoopLevel {
    if !horizontal_speed.is_finite() || horizontal_speed < 0.01 {
        return LoopLevel {
            volume: 0.0,
            pitch: 0.0,
        };
    }
    let (min, max) = if baby { (1.1, 1.5) } else { (0.7, 1.1) };
    LoopLevel {
        volume: horizontal_speed.clamp(0.0, 0.5) * 1.2,
        pitch: min + horizontal_speed.clamp(min, max) * (max - min),
    }
}

/// The `n` closest `(id, squared distance)` pairs, nearest first; ties keep the lower id.
pub fn nearest(candidates: &mut Vec<(i32, f64)>, n: usize) {
    candidates.retain(|c| c.1.is_finite());
    candidates.sort_unstable_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    candidates.truncate(n);
}

/// Ids in `wanted` that lack a loop, and ids with a loop that are no longer wanted.
pub fn reconcile(
    wanted: &[(i32, f64)],
    active: impl Iterator<Item = i32>,
    starts: &mut Vec<i32>,
    stops: &mut Vec<i32>,
) {
    starts.clear();
    stops.clear();
    let mut have: Vec<i32> = active.collect();
    for (id, _) in wanted {
        match have.iter().position(|h| h == id) {
            Some(i) => {
                have.swap_remove(i);
            }
            None => starts.push(*id),
        }
    }
    stops.extend(have);
}

/// `NeutralMob#isAngry`: the anger end time lies in the future.
pub fn is_angry(anger_end_time: i64, game_time: i64) -> bool {
    anger_end_time.saturating_sub(game_time) > 0
}

/// `GuardianAttackSoundInstance`: follows the beam charge, restarting whenever the target changes.
#[derive(Clone, Copy, Debug)]
pub struct GuardianBeam {
    duration: u32,
    target: i32,
    time: u32,
}

impl GuardianBeam {
    pub fn new(elder: bool) -> Self {
        Self {
            duration: if elder { 60 } else { 80 },
            target: 0,
            time: 0,
        }
    }

    pub fn has_target(&self) -> bool {
        self.target != 0
    }

    pub fn tick(&mut self, target: i32) -> LoopLevel {
        if target != self.target {
            self.target = target;
            self.time = 0;
        } else if target != 0 && self.time < self.duration {
            self.time += 1;
        }
        let scale = self.time as f32 / self.duration as f32;
        LoopLevel {
            volume: scale * scale,
            pitch: GUARDIAN_PITCH_BASE + GUARDIAN_PITCH_SPAN * scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elytra_is_silent_then_fades_in_then_follows_speed() {
        let mut l = ElytraLoop::default();
        for _ in 0..19 {
            assert_eq!(l.tick(true, 4.0).unwrap().volume, 0.0);
        }
        assert_eq!(l.tick(true, 4.0).unwrap().volume, 0.0);
        let half = l.tick(true, 4.0).unwrap().volume;
        assert!((half - 0.05).abs() < 1e-6, "{half}");
        for _ in 0..18 {
            l.tick(true, 4.0);
        }
        assert_eq!(l.tick(true, 4.0).unwrap().volume, 1.0);
        assert_eq!(l.tick(true, 1.0).unwrap().volume, 0.25);
        assert_eq!(l.tick(true, 0.0).unwrap().volume, 0.0);
    }

    #[test]
    fn elytra_pitch_rises_above_loud_threshold() {
        let mut l = ElytraLoop::default();
        for _ in 0..40 {
            l.tick(true, 4.0);
        }
        let loud = l.tick(true, 4.0).unwrap();
        assert!((loud.pitch - 1.2).abs() < 1e-6);
        assert_eq!(l.tick(true, 1.0).unwrap().pitch, 1.0);
    }

    #[test]
    fn elytra_survives_the_startup_window_without_gliding() {
        let mut l = ElytraLoop::default();
        for _ in 0..20 {
            assert!(l.tick(false, 0.0).is_some());
        }
        assert!(l.tick(false, 0.0).is_none());
        let mut l = ElytraLoop::default();
        for _ in 0..100 {
            assert!(l.tick(true, 0.0).is_some());
        }
    }

    #[test]
    fn bee_hovering_is_mute_and_flying_ramps_to_the_cap() {
        assert_eq!(bee_level(0.005, false).volume, 0.0);
        assert_eq!(bee_level(f32::NAN, false).volume, 0.0);
        assert!((bee_level(0.25, false).volume - 0.3).abs() < 1e-6);
        assert!((bee_level(5.0, false).volume - 0.6).abs() < 1e-6);
        assert!((bee_level(5.0, false).pitch - 1.14).abs() < 1e-5);
        assert!(bee_level(0.3, true).pitch > bee_level(0.3, false).pitch);
    }

    #[test]
    fn only_the_nearest_bees_are_kept() {
        let mut c: Vec<(i32, f64)> = (0..20).map(|i| (i, f64::from(20 - i))).collect();
        c.push((99, f64::NAN));
        nearest(&mut c, 5);
        assert_eq!(
            c.iter().map(|x| x.0).collect::<Vec<_>>(),
            [19, 18, 17, 16, 15]
        );
        let mut few = vec![(1, 4.0)];
        nearest(&mut few, 5);
        assert_eq!(few.len(), 1);
    }

    #[test]
    fn reconcile_starts_missing_and_stops_dropped() {
        let (mut starts, mut stops) = (Vec::new(), Vec::new());
        reconcile(
            &[(1, 0.0), (2, 0.0), (3, 0.0)],
            [2, 9].into_iter(),
            &mut starts,
            &mut stops,
        );
        assert_eq!(starts, [1, 3]);
        assert_eq!(stops, [9]);
    }

    #[test]
    fn anger_lasts_until_the_end_time() {
        assert!(is_angry(101, 100));
        assert!(!is_angry(100, 100));
        assert!(!is_angry(-1, 0));
        assert!(!is_angry(i64::MIN, 5));
    }

    #[test]
    fn guardian_beam_charges_and_restarts_on_a_new_target() {
        let mut g = GuardianBeam::new(false);
        assert_eq!(g.tick(0).volume, 0.0);
        assert!(!g.has_target());
        assert_eq!(g.tick(7).volume, 0.0);
        for _ in 0..39 {
            g.tick(7);
        }
        let half = g.tick(7);
        assert!((half.volume - 0.25).abs() < 1e-3 && (half.pitch - 0.95).abs() < 1e-3);
        for _ in 0..100 {
            g.tick(7);
        }
        let full = g.tick(7);
        assert_eq!((full.volume, full.pitch), (1.0, 1.2));
        assert_eq!(g.tick(8).volume, 0.0);
        assert_eq!(g.tick(0).volume, 0.0);
    }

    #[test]
    fn elder_guardian_charges_faster() {
        let (mut a, mut e) = (GuardianBeam::new(false), GuardianBeam::new(true));
        a.tick(1);
        e.tick(1);
        for _ in 0..30 {
            a.tick(1);
            e.tick(1);
        }
        assert!(e.tick(1).volume > a.tick(1).volume);
    }
}
