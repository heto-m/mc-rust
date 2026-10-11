//! Local-player movement sound rules: step cadence, step/swim/splash cues and fall landings.

use crate::block_sounds::SoundType;

pub const STEP_SCALE: f32 = 0.6;
const SAFE_FALL_DISTANCE: f64 = 3.0;
const MAX_FALL_DISTANCE: f64 = 1.0e6;

/// Distance accumulator that decides when the next footstep is due.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepTracker {
    move_dist: f32,
    next_step: f32,
}

impl Default for StepTracker {
    fn default() -> Self {
        Self {
            move_dist: 0.0,
            next_step: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepMove {
    pub horizontal: f32,
    pub total: f32,
    pub climbing: bool,
    pub supported_by_air: bool,
    pub stepping: bool,
    pub in_water: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    Quiet,
    Step,
    Swim,
}

impl StepTracker {
    /// `stepping` is vanilla's "on ground, climbing, sneaking level or on rails, and not swimming".
    pub fn advance(&mut self, m: &StepMove) -> StepOutcome {
        let distance = if m.climbing { m.total } else { m.horizontal };
        if distance.is_finite() {
            self.move_dist += distance * STEP_SCALE;
        }
        if self.move_dist <= self.next_step || m.supported_by_air {
            return StepOutcome::Quiet;
        }
        if m.stepping {
            self.next_step = self.move_dist.trunc() + 1.0;
            StepOutcome::Step
        } else if m.in_water {
            self.next_step = self.move_dist.trunc() + 1.0;
            StepOutcome::Swim
        } else {
            StepOutcome::Quiet
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AboveKind {
    Inside,
    Combination,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SoundCue {
    pub event: String,
    pub volume: f32,
    pub pitch: f32,
}

fn cue(event: String, volume: f32, pitch: f32) -> SoundCue {
    SoundCue {
        event,
        volume,
        pitch,
    }
}

fn step(t: &SoundType<'_>) -> SoundCue {
    cue(t.step_event(), t.volume * 0.15, t.pitch)
}

fn muffled(t: &SoundType<'_>) -> SoundCue {
    cue(t.step_event(), t.volume * 0.05, t.pitch * 0.8)
}

/// Vanilla's `Player.playStepSound`. `swim` is the swim cue to prepend when in water.
pub fn step_cues(
    on: &SoundType<'_>,
    above: Option<(&SoundType<'_>, AboveKind)>,
    in_water: bool,
    swim: Option<SoundCue>,
) -> Vec<SoundCue> {
    if in_water {
        return swim.into_iter().chain([muffled(on)]).collect();
    }
    match above {
        Some((a, AboveKind::Combination)) => vec![step(a), muffled(on)],
        Some((a, AboveKind::Inside)) => vec![step(a)],
        _ => vec![step(on)],
    }
}

/// `Entity.waterSwimSound` for the player: speed is the squared-and-damped motion length.
pub fn swim_cue(velocity: [f64; 3]) -> SoundCue {
    cue(
        "entity.player.swim".to_owned(),
        motion_loudness(velocity, 0.35),
        1.0,
    )
}

fn motion_loudness(v: [f64; 3], scale: f32) -> f32 {
    let speed = ((v[0] * v[0] * 0.2 + v[1] * v[1] + v[2] * v[2] * 0.2) as f32).sqrt();
    if speed.is_finite() {
        (speed * scale).min(1.0)
    } else {
        0.0
    }
}

/// `Entity.doWaterSplashEffect` for the player entering water.
pub fn splash_event(velocity: [f64; 3]) -> (&'static str, f32) {
    let loudness = motion_loudness(velocity, 0.2);
    if loudness < 0.25 {
        ("entity.player.splash", loudness)
    } else {
        ("entity.player.splash.high_speed", loudness)
    }
}

/// How much a block softens (or removes) fall damage; `None` for the default of 1.
pub fn fall_modifier(block_name: &str, sneaking: bool) -> f32 {
    match block_name {
        "minecraft:hay_block" => 0.2,
        n if n.ends_with("_bed") => 0.5,
        "minecraft:slime_block" if !sneaking => 0.0,
        _ => 1.0,
    }
}

pub fn fall_damage(fall_distance: f64, modifier: f32) -> i32 {
    let distance = fall_distance.clamp(0.0, MAX_FALL_DISTANCE);
    let base = distance + 1.0e-6 - SAFE_FALL_DISTANCE;
    (base * f64::from(modifier)).floor().max(0.0) as i32
}

pub fn fall_cues(damage: i32, landed_on: Option<&SoundType<'_>>) -> Vec<SoundCue> {
    if damage <= 0 {
        return Vec::new();
    }
    let event = if damage > 4 {
        "entity.player.big_fall"
    } else {
        "entity.player.small_fall"
    };
    let mut cues = vec![cue(event.to_owned(), 1.0, 1.0)];
    if let Some(t) = landed_on {
        cues.push(cue(t.fall_event(), t.volume * 0.5, t.pitch * 0.75));
    }
    cues
}

/// Highest point of the current airborne stretch, so a landing knows how far it fell.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FallTracker {
    peak_y: Option<f64>,
}

impl FallTracker {
    /// Returns the fall distance on the tick the player lands.
    pub fn update(&mut self, y: f64, on_ground: bool, in_fluid_or_flying: bool) -> Option<f64> {
        if !y.is_finite() {
            self.peak_y = None;
            return None;
        }
        if in_fluid_or_flying {
            self.peak_y = None;
            return None;
        }
        if on_ground {
            let fell = self.peak_y.take().map(|peak| peak - y);
            return fell.filter(|d| *d > 0.0);
        }
        self.peak_y = Some(self.peak_y.map_or(y, |p| p.max(y)));
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(horizontal: f32) -> StepMove {
        StepMove {
            horizontal,
            total: horizontal,
            climbing: false,
            supported_by_air: false,
            stepping: true,
            in_water: false,
        }
    }

    fn stone() -> SoundType<'static> {
        SoundType::from_break_event("block.stone.break").unwrap()
    }

    #[test]
    fn a_step_every_two_thirds_of_a_block_squared() {
        let mut t = StepTracker::default();
        let mut steps = 0;
        for _ in 0..100 {
            if t.advance(&walk(0.2158)) == StepOutcome::Step {
                steps += 1;
            }
        }
        // 100 ticks * 0.2158 * 0.6 = 12.9 -> 12 whole step intervals.
        assert_eq!(steps, 12);
    }

    #[test]
    fn standing_still_and_airborne_are_silent() {
        let mut t = StepTracker::default();
        for _ in 0..1000 {
            assert_eq!(t.advance(&walk(0.0)), StepOutcome::Quiet);
        }
        let mut air = walk(5.0);
        air.stepping = false;
        assert_eq!(t.advance(&air), StepOutcome::Quiet);
    }

    #[test]
    fn swimming_in_water_swims_instead_of_stepping() {
        let mut t = StepTracker::default();
        let mut m = walk(2.0);
        m.stepping = false;
        m.in_water = true;
        assert_eq!(t.advance(&m), StepOutcome::Swim);
    }

    #[test]
    fn climbing_counts_vertical_motion() {
        let mut t = StepTracker::default();
        let m = StepMove {
            horizontal: 0.0,
            total: 0.2,
            climbing: true,
            supported_by_air: false,
            stepping: true,
            in_water: false,
        };
        let steps = (0..100)
            .filter(|_| t.advance(&m) == StepOutcome::Step)
            .count();
        assert!((11..=12).contains(&steps));
    }

    #[test]
    fn step_volume_is_fifteen_percent_of_the_type() {
        let cues = step_cues(&stone(), None, false, None);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].event, "block.stone.step");
        assert!((cues[0].volume - 0.15).abs() < 1e-6);
        assert_eq!(cues[0].pitch, 1.0);
    }

    #[test]
    fn metal_steps_are_higher_pitched() {
        let metal = SoundType::from_break_event("block.metal.break").unwrap();
        assert_eq!(step_cues(&metal, None, false, None)[0].pitch, 1.5);
    }

    #[test]
    fn walking_through_snow_layer_uses_the_layer_sound() {
        let snow = SoundType::from_break_event("block.snow.break").unwrap();
        let cues = step_cues(&stone(), Some((&snow, AboveKind::Inside)), false, None);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].event, "block.snow.step");
    }

    #[test]
    fn combination_blocks_add_a_muffled_ground_step() {
        let grass = SoundType::from_break_event("block.grass.break").unwrap();
        let cues = step_cues(
            &stone(),
            Some((&grass, AboveKind::Combination)),
            false,
            None,
        );
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].event, "block.grass.step");
        assert_eq!(cues[1].event, "block.stone.step");
        assert!((cues[1].volume - 0.05).abs() < 1e-6);
        assert!((cues[1].pitch - 0.8).abs() < 1e-6);
    }

    #[test]
    fn in_water_the_swim_sound_precedes_a_muffled_step() {
        let cues = step_cues(&stone(), None, true, Some(swim_cue([0.0, 0.0, 0.1])));
        assert_eq!(cues[0].event, "entity.player.swim");
        assert_eq!(cues[1].event, "block.stone.step");
    }

    #[test]
    fn splash_loudness_picks_the_variant() {
        assert_eq!(splash_event([0.0, -0.1, 0.0]).0, "entity.player.splash");
        let (event, volume) = splash_event([0.0, -1.5, 0.0]);
        assert_eq!(event, "entity.player.splash.high_speed");
        assert!(volume > 0.25 && volume <= 1.0);
        assert_eq!(splash_event([f64::NAN, 0.0, 0.0]).1, 0.0);
    }

    #[test]
    fn fall_damage_follows_the_safe_distance() {
        assert_eq!(fall_damage(3.0, 1.0), 0);
        assert_eq!(fall_damage(4.0, 1.0), 1);
        assert_eq!(fall_damage(10.0, 1.0), 7);
        assert_eq!(fall_damage(10.0, 0.2), 1);
        assert_eq!(fall_damage(10.0, 0.0), 0);
        assert_eq!(fall_damage(f64::INFINITY, 1.0), 999_997);
    }

    #[test]
    fn landing_cues_pick_small_or_big_and_add_the_block() {
        assert!(fall_cues(0, Some(&stone())).is_empty());
        let small = fall_cues(2, Some(&stone()));
        assert_eq!(small[0].event, "entity.player.small_fall");
        assert_eq!(small[1].event, "block.stone.fall");
        assert!((small[1].volume - 0.5).abs() < 1e-6);
        assert!((small[1].pitch - 0.75).abs() < 1e-6);
        assert_eq!(fall_cues(5, None)[0].event, "entity.player.big_fall");
        assert_eq!(fall_cues(4, None)[0].event, "entity.player.small_fall");
    }

    #[test]
    fn fall_modifiers() {
        assert_eq!(fall_modifier("minecraft:hay_block", false), 0.2);
        assert_eq!(fall_modifier("minecraft:red_bed", false), 0.5);
        assert_eq!(fall_modifier("minecraft:slime_block", false), 0.0);
        assert_eq!(fall_modifier("minecraft:slime_block", true), 1.0);
        assert_eq!(fall_modifier("minecraft:stone", false), 1.0);
    }

    #[test]
    fn fall_tracker_measures_from_the_peak() {
        let mut f = FallTracker::default();
        assert_eq!(f.update(70.0, true, false), None);
        f.update(72.0, false, false);
        f.update(75.0, false, false);
        f.update(71.0, false, false);
        assert_eq!(f.update(66.0, true, false), Some(9.0));
        assert_eq!(f.update(66.0, true, false), None);
        f.update(80.0, false, false);
        assert_eq!(f.update(75.0, false, true), None);
        assert_eq!(f.update(60.0, true, false), None);
    }
}
