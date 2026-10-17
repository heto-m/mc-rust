//! Sounds the local player makes by moving: footsteps, swimming, splashes, landings and ladders.
//! Other entities' movement sounds arrive from the server as sound packets.

use mcr_audio::{
    AboveKind, FallTracker, FastRandom, SoundCue, SoundSource, StepMove, StepOutcome, StepTracker,
    fall_cues, fall_damage, fall_modifier, splash_event, step_cues, swim_cue,
};
use mcr_client::ClientState;
use mcr_data::Version;

use crate::audio::{Audio, sound_type};

const MAX_STEP_DELTA: f64 = 8.0;
const CRYSTAL_COOLDOWN_TICKS: u64 = 20;
const ON_POS_OFFSET: f64 = 0.2;

pub struct MovementSounds {
    data: &'static Version,
    steps: StepTracker,
    fall: FallTracker,
    last: Option<[f64; 3]>,
    was_in_water: bool,
    last_crystal_tick: u64,
    crystal_intensity: f32,
    rng: FastRandom,
    /// A mod in Java plays the footsteps instead (Presence Footsteps).
    pub steps_replaced: bool,
}

impl std::fmt::Debug for MovementSounds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MovementSounds").finish_non_exhaustive()
    }
}

impl MovementSounds {
    pub fn new(data: &'static Version) -> Self {
        Self {
            data,
            steps: StepTracker::default(),
            fall: FallTracker::default(),
            last: None,
            was_in_water: false,
            last_crystal_tick: 0,
            crystal_intensity: 0.0,
            rng: FastRandom::new(0x0005_7E95),
            steps_replaced: false,
        }
    }

    pub fn reset(&mut self) {
        self.last = None;
        self.fall = FallTracker::default();
        self.was_in_water = false;
    }

    fn tag(&self, state: &ClientState, tag: &str, block: u32) -> bool {
        state
            .registries
            .is_in_tag("minecraft:block", tag, block as i32)
    }

    fn block_at(&self, state: &ClientState, pos: [i32; 3]) -> Option<(u32, u32)> {
        let s = u32::from(state.chunks.block(pos[0], pos[1], pos[2])?.0);
        Some((s, self.data.block_state(s)?.block))
    }

    pub fn tick(&mut self, state: &ClientState, ticks: u64, audio: &mut Audio) {
        let player = &state.player;
        let p = &player.physics;
        let pos = p.pos;
        let Some(last) = self.last.replace(pos) else {
            return;
        };
        let delta = [pos[0] - last[0], pos[1] - last[1], pos[2] - last[2]];
        if delta
            .iter()
            .any(|d| !d.is_finite() || d.abs() > MAX_STEP_DELTA)
        {
            self.reset();
            self.last = Some(pos);
            return;
        }
        let on_pos = [
            pos[0].floor() as i32,
            (pos[1] - ON_POS_OFFSET).floor() as i32,
            pos[2].floor() as i32,
        ];
        let supporting = self.block_at(state, on_pos);
        let climbable = supporting.is_some_and(|(_, b)| {
            self.tag(state, "minecraft:climbable", b)
                || self.data.block_name(b) == Some("minecraft:powder_snow")
        });
        let in_water = p.in_water();
        let spectator = p.spectator;
        let riding = player.vehicle.is_some();

        let landed = self.fall.update(
            pos[1],
            p.on_ground,
            in_water || climbable || p.abilities.flying || p.fall_flying || riding,
        );
        if let Some(distance) = landed
            && !p.abilities.may_fly
            && !spectator
        {
            self.landing(distance, supporting, p.crouching, pos, audio);
        }

        if in_water && !self.was_in_water && !spectator {
            let (event, volume) = splash_event(p.vel);
            let pitch = 1.0 + (self.rng.f32() - self.rng.f32()) * 0.4;
            audio.play(event, Some(pos), volume, pitch, SoundSource::Players);
        }
        self.was_in_water = in_water;

        let silent = p.abilities.flying || (p.on_ground && p.crouching) || riding;
        if silent {
            return;
        }
        let horizontal = delta[0].hypot(delta[2]) as f32;
        let total = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt() as f32;
        let on_rails = supporting.is_some_and(|(_, b)| self.tag(state, "minecraft:rails", b));
        let stepping = (p.on_ground || climbable || (p.crouching && delta[1] == 0.0) || on_rails)
            && !p.swimming;
        let outcome = self.steps.advance(&StepMove {
            horizontal,
            total,
            climbing: climbable,
            supported_by_air: supporting
                .is_none_or(|(s, _)| self.data.block_state(s).is_some_and(|i| i.is_air())),
            stepping,
            in_water,
        });
        match outcome {
            StepOutcome::Quiet => {}
            StepOutcome::Step if self.steps_replaced => {}
            StepOutcome::Step => self.footstep(state, on_pos, in_water, ticks, pos, p.vel, audio),
            StepOutcome::Swim => {
                let cue = self.swim(p.vel);
                self.play_cues(audio, pos, &[cue]);
            }
        }
    }

    fn swim(&mut self, velocity: [f64; 3]) -> SoundCue {
        let mut cue = swim_cue(velocity);
        cue.pitch = 1.0 + (self.rng.f32() - self.rng.f32()) * 0.4;
        cue
    }

    fn play_cues(&self, audio: &mut Audio, pos: [f64; 3], cues: &[SoundCue]) {
        for c in cues {
            audio.play(&c.event, Some(pos), c.volume, c.pitch, SoundSource::Players);
        }
    }

    fn landing(
        &mut self,
        distance: f64,
        supporting: Option<(u32, u32)>,
        sneaking: bool,
        pos: [f64; 3],
        audio: &mut Audio,
    ) {
        let block_name = supporting.and_then(|(_, b)| self.data.block_name(b));
        let modifier = block_name.map_or(1.0, |n| fall_modifier(n, sneaking));
        let damage = fall_damage(distance, modifier);
        let ty = supporting.and_then(|(s, _)| sound_type(self.data, s));
        let cues = fall_cues(damage, ty.as_ref());
        self.play_cues(audio, pos, &cues);
    }

    #[allow(clippy::too_many_arguments)]
    fn footstep(
        &mut self,
        state: &ClientState,
        on_pos: [i32; 3],
        in_water: bool,
        ticks: u64,
        pos: [f64; 3],
        velocity: [f64; 3],
        audio: &mut Audio,
    ) {
        let Some((on_state, on_block)) = self.block_at(state, on_pos) else {
            return;
        };
        let Some(on) = sound_type(self.data, on_state) else {
            return;
        };
        let above_pos = [on_pos[0], on_pos[1] + 1, on_pos[2]];
        let above = self.block_at(state, above_pos).and_then(|(s, b)| {
            let kind = if self.tag(state, "minecraft:combination_step_sound_blocks", b) {
                AboveKind::Combination
            } else if self.tag(state, "minecraft:inside_step_sound_blocks", b) {
                AboveKind::Inside
            } else {
                return None;
            };
            Some((sound_type(self.data, s)?, kind))
        });
        let swim = in_water.then(|| self.swim(velocity));
        let cues = step_cues(&on, above.as_ref().map(|(t, k)| (t, *k)), in_water, swim);
        self.play_cues(audio, pos, &cues);
        if !in_water
            && self.tag(state, "minecraft:crystal_sound_blocks", on_block)
            && ticks >= self.last_crystal_tick + CRYSTAL_COOLDOWN_TICKS
        {
            let elapsed = (ticks - self.last_crystal_tick) as f32;
            self.crystal_intensity =
                (self.crystal_intensity * 0.997f32.powf(elapsed) + 0.07).min(1.0);
            let pitch = 0.5 + self.crystal_intensity * self.rng.f32() * 1.2;
            let volume = 0.1 + self.crystal_intensity * 1.2;
            audio.play(
                "block.amethyst_block.chime",
                Some(pos),
                volume,
                pitch,
                SoundSource::Players,
            );
            self.last_crystal_tick = ticks;
        }
    }
}
