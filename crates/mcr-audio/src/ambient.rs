//! Biome and underwater ambience rules: mood counter, additions rolls and loop fades.

use crate::attributes::{Additions, Mood};
use crate::events::JavaRandom;

const LOOP_FADE_TICKS: i32 = 40;
const SKY_MOOD_RECOVERY_RATE: f32 = 0.001;

/// Cave-sound accumulator: dark spots fill it, bright sky drains it, `1.0` fires the mood sound.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MoodState {
    moodiness: f32,
}

impl MoodState {
    pub fn moodiness(&self) -> f32 {
        self.moodiness
    }

    /// One tick at a sampled block; returns whether the mood sound is due (and resets).
    pub fn step(&mut self, mood: &Mood, sky_light: u8, block_light: u8) -> bool {
        if sky_light > 0 {
            self.moodiness -= f32::from(sky_light) / 15.0 * SKY_MOOD_RECOVERY_RATE;
        } else {
            self.moodiness -= (f32::from(block_light) - 1.0) / mood.tick_delay.max(1) as f32;
        }
        if self.moodiness >= 1.0 {
            self.moodiness = 0.0;
            true
        } else {
            self.moodiness = self.moodiness.max(0.0);
            false
        }
    }
}

/// Where the mood sound plays: past the sampled block, `offset` further along the line from the eye.
pub fn mood_sound_position(
    player_x: f64,
    eye_y: f64,
    player_z: f64,
    sample: [i32; 3],
    offset: f64,
) -> [f64; 3] {
    let dir = [
        f64::from(sample[0]) + 0.5 - player_x,
        f64::from(sample[1]) + 0.5 - eye_y,
        f64::from(sample[2]) + 0.5 - player_z,
    ];
    let distance = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if distance < 1e-6 {
        return [player_x, eye_y, player_z];
    }
    let reach = distance + offset;
    [
        player_x + dir[0] / distance * reach,
        eye_y + dir[1] / distance * reach,
        player_z + dir[2] / distance * reach,
    ]
}

/// Indices of the additions whose per-tick chance fired.
pub fn roll_additions(rng: &mut JavaRandom, additions: &[Additions], out: &mut Vec<usize>) {
    out.clear();
    for (i, a) in additions.iter().enumerate() {
        if rng.next_double() < a.tick_chance {
            out.push(i);
        }
    }
}

/// Fade envelope of a biome loop: 40 ticks in, 40 ticks out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoopFade {
    fade: i32,
    direction: i32,
}

impl LoopFade {
    pub fn fade_in(&mut self) {
        self.fade = self.fade.max(0);
        self.direction = 1;
    }

    pub fn fade_out(&mut self) {
        self.fade = self.fade.min(LOOP_FADE_TICKS);
        self.direction = -1;
    }

    /// Volume for this tick, or `None` once the fade-out is over.
    pub fn tick(&mut self) -> Option<f32> {
        if self.fade < 0 {
            return None;
        }
        self.fade += self.direction;
        Some((self.fade as f32 / LOOP_FADE_TICKS as f32).clamp(0.0, 1.0))
    }
}

/// Underwater loop envelope: fades in over 40 ticks, out at twice that rate, stops at zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnderwaterLoop {
    fade: i32,
}

impl UnderwaterLoop {
    pub fn tick(&mut self, underwater: bool) -> Option<f32> {
        if self.fade < 0 {
            return None;
        }
        self.fade = if underwater {
            self.fade + 1
        } else {
            self.fade - 2
        }
        .min(LOOP_FADE_TICKS);
        Some((self.fade as f32 / LOOP_FADE_TICKS as f32).clamp(0.0, 1.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnderwaterSound {
    Additions,
    Rare,
    UltraRare,
}

impl UnderwaterSound {
    pub fn event(self) -> &'static str {
        match self {
            Self::Additions => "ambient.underwater.loop.additions",
            Self::Rare => "ambient.underwater.loop.additions.rare",
            Self::UltraRare => "ambient.underwater.loop.additions.ultra_rare",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnderwaterAmbient {
    tick_delay: i32,
}

impl UnderwaterAmbient {
    /// `roll` is a uniform float in `[0, 1)`.
    pub fn tick(&mut self, underwater: bool, roll: f32) -> Option<UnderwaterSound> {
        self.tick_delay -= 1;
        if self.tick_delay > 0 || !underwater {
            return None;
        }
        let sound = if roll < 1.0e-4 {
            UnderwaterSound::UltraRare
        } else if roll < 0.001 {
            UnderwaterSound::Rare
        } else if roll < 0.01 {
            UnderwaterSound::Additions
        } else {
            return None;
        };
        self.tick_delay = 0;
        Some(sound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mood(delay: i32) -> Mood {
        Mood {
            sound: "ambient.cave".into(),
            tick_delay: delay,
            block_search_extent: 8,
            offset: 2.0,
        }
    }

    #[test]
    fn dark_unlit_spot_fires_after_tick_delay_samples() {
        let m = mood(6000);
        let mut state = MoodState::default();
        let mut fired_at = None;
        for tick in 1..=7000 {
            if state.step(&m, 0, 0) {
                fired_at = Some(tick);
                break;
            }
        }
        assert!((5990..=6010).contains(&fired_at.unwrap()), "{fired_at:?}");
    }

    #[test]
    fn lit_spots_never_fire_and_sky_drains_to_zero() {
        let m = mood(100);
        let mut state = MoodState::default();
        for _ in 0..50 {
            state.step(&m, 0, 0);
        }
        assert!(state.moodiness() > 0.0);
        for _ in 0..10_000 {
            assert!(!state.step(&m, 15, 0));
        }
        assert_eq!(state.moodiness(), 0.0);
        for _ in 0..1000 {
            assert!(!state.step(&m, 0, 15));
        }
    }

    #[test]
    fn mood_position_extends_past_the_block() {
        let p = mood_sound_position(0.0, 0.0, 0.0, [2, 0, 0], 2.0);
        let reach = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        let block = (2.5f64 * 2.5 + 0.5 * 0.5 * 2.0).sqrt();
        assert!((reach - (block + 2.0)).abs() < 1e-9);
        assert!((p[0] / reach - 2.5 / block).abs() < 1e-9);
        assert_eq!(mood_sound_position(0.5, 0.5, 0.5, [0, 0, 0], 2.0), [0.5; 3]);
    }

    #[test]
    fn additions_roll_respects_chance() {
        let mut rng = JavaRandom::new(7);
        let list = [
            Additions {
                sound: "a".into(),
                tick_chance: 0.0,
            },
            Additions {
                sound: "b".into(),
                tick_chance: 1.0,
            },
        ];
        let mut out = Vec::new();
        for _ in 0..100 {
            roll_additions(&mut rng, &list, &mut out);
            assert_eq!(out, [1]);
        }
    }

    #[test]
    fn loop_fade_ramps_and_stops() {
        let mut fade = LoopFade::default();
        fade.fade_in();
        let mut last = 0.0;
        for _ in 0..40 {
            last = fade.tick().unwrap();
        }
        assert_eq!(last, 1.0);
        fade.fade_out();
        let mut ticks = 0;
        while fade.tick().is_some() {
            ticks += 1;
            assert!(ticks < 100);
        }
        assert!(ticks >= 40);
    }

    #[test]
    fn underwater_loop_leaves_twice_as_fast() {
        let mut l = UnderwaterLoop::default();
        for _ in 0..40 {
            l.tick(true);
        }
        let mut out = 0;
        while l.tick(false).is_some() {
            out += 1;
        }
        assert_eq!(out, 21);
    }

    #[test]
    fn underwater_additions_need_water_and_a_low_roll() {
        let mut u = UnderwaterAmbient::default();
        assert_eq!(u.tick(false, 0.0), None);
        assert_eq!(u.tick(true, 0.5), None);
        assert_eq!(u.tick(true, 0.005), Some(UnderwaterSound::Additions));
        assert_eq!(u.tick(true, 0.0005), Some(UnderwaterSound::Rare));
        assert_eq!(u.tick(true, 0.00001), Some(UnderwaterSound::UltraRare));
    }
}
