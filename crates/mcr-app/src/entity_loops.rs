//! Sounds vanilla keeps updating on the client for entities: elytra wind, bees, guardian beams,
//! sniffer digging.

use mcr_audio::{
    ElytraLoop, GuardianBeam, LoopLevel, SoundSource, VoiceId, bee_level, is_angry, nearest,
    reconcile,
};
use mcr_client::ClientState;
use mcr_data::Version;
use mcr_entity_model::idx;
use mcr_protocol::types::entity_data::{EntityDataValue, SnifferState};
use rustc_hash::FxHashMap;

use crate::audio::Audio;

pub const MAX_BEES: usize = 12;
const MAX_GUARDIANS: usize = 16;
const GUARDIAN_ATTACK_EVENT: i8 = 21;
const SNIFFER_DIGGING_EVENT: i8 = 63;
const MAX_SNIFFERS: usize = 16;
/// The event can arrive a tick before the synched state turns to digging.
const SNIFFER_GRACE_TICKS: u8 = 2;

#[derive(Debug)]
struct ElytraVoice {
    voice: VoiceId,
    state: ElytraLoop,
}

#[derive(Debug)]
struct BeeVoice {
    voice: VoiceId,
    aggressive: bool,
    last: [f64; 3],
}

#[derive(Debug)]
struct GuardianVoice {
    voice: VoiceId,
    beam: GuardianBeam,
}

#[derive(Debug, Default)]
pub struct EntityLoops {
    bee: Option<i32>,
    guardian: Option<i32>,
    elder_guardian: Option<i32>,
    elytra: Option<ElytraVoice>,
    was_gliding: bool,
    bees: FxHashMap<i32, BeeVoice>,
    beams: FxHashMap<i32, GuardianVoice>,
    pending_beams: Vec<i32>,
    sniffers: FxHashMap<i32, (VoiceId, u8)>,
    pending_sniffers: Vec<i32>,
    candidates: Vec<(i32, f64)>,
    starts: Vec<i32>,
    stops: Vec<i32>,
}

fn flag(data: &FxHashMap<u8, EntityDataValue>, index: u8) -> bool {
    matches!(data.get(&index), Some(EntityDataValue::Boolean(true)))
}

impl EntityLoops {
    pub fn new(data: &'static Version) -> Self {
        let id = |name: &str| data.entity_type_by_name(name).map(|i| i as i32);
        Self {
            bee: id("minecraft:bee"),
            guardian: id("minecraft:guardian"),
            elder_guardian: id("minecraft:elder_guardian"),
            ..Self::default()
        }
    }

    pub fn reset(&mut self, audio: &mut Audio) {
        if let Some(e) = self.elytra.take() {
            audio.stop_voice(e.voice);
        }
        for (_, b) in self.bees.drain() {
            audio.stop_voice(b.voice);
        }
        for (_, g) in self.beams.drain() {
            audio.stop_voice(g.voice);
        }
        self.pending_beams.clear();
        for (_, (v, _)) in self.sniffers.drain() {
            audio.stop_voice(v);
        }
        self.pending_sniffers.clear();
        self.was_gliding = false;
    }

    pub fn on_entity_event(&mut self, entity_id: i32, event: i8) {
        if event == GUARDIAN_ATTACK_EVENT && self.pending_beams.len() < MAX_GUARDIANS {
            self.pending_beams.push(entity_id);
        }
        if event == SNIFFER_DIGGING_EVENT && self.pending_sniffers.len() < MAX_SNIFFERS {
            self.pending_sniffers.push(entity_id);
        }
    }

    pub fn tick(&mut self, state: &ClientState, audio: &mut Audio) {
        self.tick_elytra(state, audio);
        self.tick_bees(state, audio);
        self.tick_beams(state, audio);
        self.tick_sniffers(state, audio);
    }

    /// Vanilla's `SnifferSoundInstance`: one digging sound that follows the sniffer and stops with the dig.
    fn tick_sniffers(&mut self, state: &ClientState, audio: &mut Audio) {
        for id in std::mem::take(&mut self.pending_sniffers) {
            let Some(e) = state.entities.get(id) else {
                continue;
            };
            if flag(&e.data, idx::SILENT) {
                continue;
            }
            if let Some((old, _)) = self.sniffers.remove(&id) {
                audio.stop_voice(old);
            }
            if self.sniffers.len() < MAX_SNIFFERS
                && let Some(voice) = audio.play(
                    "entity.sniffer.digging",
                    Some(e.pos),
                    1.0,
                    1.0,
                    SoundSource::Neutral,
                )
            {
                self.sniffers.insert(id, (voice, 0));
            }
        }
        self.sniffers.retain(|&id, (voice, age)| {
            *age = age.saturating_add(1);
            match state.entities.get(id) {
                Some(e) if *age <= SNIFFER_GRACE_TICKS || sniffer_digs(e) => {
                    audio.set_voice_position(*voice, e.pos);
                    true
                }
                _ => {
                    audio.stop_voice(*voice);
                    false
                }
            }
        });
    }

    fn tick_elytra(&mut self, state: &ClientState, audio: &mut Audio) {
        let p = &state.player.physics;
        let gliding = p.fall_flying;
        if gliding && !self.was_gliding && self.elytra.is_none() {
            self.elytra = audio
                .play_voice_looping(
                    "item.elytra.flying",
                    Some(p.pos),
                    ElytraLoop::START_VOLUME,
                    1.0,
                    SoundSource::Players,
                    true,
                )
                .map(|voice| ElytraVoice {
                    voice,
                    state: ElytraLoop::default(),
                });
        }
        self.was_gliding = gliding;
        let Some(e) = &mut self.elytra else { return };
        let speed_squared = p.vel.iter().map(|v| v * v).sum();
        match e.state.tick(gliding, speed_squared) {
            Some(level) => {
                apply(audio, e.voice, level, false);
                audio.set_voice_position(e.voice, p.pos);
            }
            None => {
                audio.stop_voice(e.voice);
                self.elytra = None;
            }
        }
    }

    fn tick_bees(&mut self, state: &ClientState, audio: &mut Audio) {
        let Some(bee) = self.bee else { return };
        let listener = state.player.physics.pos;
        self.candidates.clear();
        for e in state.entities.iter().filter(|e| e.kind == bee) {
            let d: f64 = (0..3).map(|i| (e.pos[i] - listener[i]).powi(2)).sum();
            self.candidates.push((e.id, d));
        }
        nearest(&mut self.candidates, MAX_BEES);
        reconcile(
            &self.candidates,
            self.bees.keys().copied(),
            &mut self.starts,
            &mut self.stops,
        );
        for id in self.stops.drain(..) {
            if let Some(b) = self.bees.remove(&id) {
                audio.stop_voice(b.voice);
            }
        }
        for &id in &self.starts {
            let Some(e) = state.entities.get(id) else {
                continue;
            };
            let aggressive = bee_angry(e, state.world.game_time);
            if let Some(voice) = start_bee(audio, e.pos, aggressive) {
                self.bees.insert(
                    id,
                    BeeVoice {
                        voice,
                        aggressive,
                        last: e.pos,
                    },
                );
            }
        }
        for (&id, b) in &mut self.bees {
            let Some(e) = state.entities.get(id) else {
                continue;
            };
            let aggressive = bee_angry(e, state.world.game_time);
            if aggressive != b.aggressive {
                audio.stop_voice(b.voice);
                b.aggressive = aggressive;
                if let Some(voice) = start_bee(audio, e.pos, aggressive) {
                    b.voice = voice;
                }
            }
            let speed = (e.pos[0] - b.last[0]).hypot(e.pos[2] - b.last[2]) as f32;
            b.last = e.pos;
            apply(
                audio,
                b.voice,
                bee_level(speed, flag(&e.data, idx::BABY)),
                flag(&e.data, idx::SILENT),
            );
            audio.set_voice_position(b.voice, e.pos);
        }
    }

    fn tick_beams(&mut self, state: &ClientState, audio: &mut Audio) {
        for id in std::mem::take(&mut self.pending_beams) {
            let Some(e) = state.entities.get(id) else {
                continue;
            };
            let elder = Some(e.kind) == self.elder_guardian;
            if !(elder || Some(e.kind) == self.guardian)
                || self.beams.contains_key(&id)
                || self.beams.len() >= MAX_GUARDIANS
            {
                continue;
            }
            if let Some(voice) = audio.play_voice_looping(
                "entity.guardian.attack",
                None,
                0.0,
                0.7,
                SoundSource::Hostile,
                true,
            ) {
                self.beams.insert(
                    id,
                    GuardianVoice {
                        voice,
                        beam: GuardianBeam::new(elder),
                    },
                );
            }
        }
        self.beams.retain(|&id, g| {
            let Some(e) = state.entities.get(id) else {
                audio.stop_voice(g.voice);
                return false;
            };
            let target = match e.data.get(&idx::GUARDIAN_ATTACK_TARGET) {
                Some(EntityDataValue::Int(t)) => *t,
                _ => 0,
            };
            let level = g.beam.tick(target);
            apply(audio, g.voice, level, flag(&e.data, idx::SILENT));
            true
        });
    }
}

fn sniffer_digs(e: &mcr_client::Entity) -> bool {
    matches!(
        e.data.get(&idx::SNIFFER_STATE),
        Some(EntityDataValue::SnifferState(
            SnifferState::Digging | SnifferState::Searching
        ))
    )
}

fn bee_angry(e: &mcr_client::Entity, game_time: i64) -> bool {
    matches!(
        e.data.get(&idx::BEE_ANGER_END),
        Some(EntityDataValue::Long(end)) if is_angry(*end, game_time)
    )
}

fn start_bee(audio: &mut Audio, pos: [f64; 3], aggressive: bool) -> Option<VoiceId> {
    let event = if aggressive {
        "entity.bee.loop_aggressive"
    } else {
        "entity.bee.loop"
    };
    audio.play_voice_looping(event, Some(pos), 0.0, 1.0, SoundSource::Neutral, true)
}

fn apply(audio: &mut Audio, voice: VoiceId, level: LoopLevel, silent: bool) {
    audio.set_voice_volume(voice, if silent { 0.0 } else { level.volume });
    audio.set_voice_pitch(voice, level.pitch);
}
