//! World-driven ambient audio: situational music, biome loops/mood/additions, underwater sounds and
//! minecart rolling.

use mcr_audio::{
    AudioAttributes, FastRandom, JavaRandom, LoopFade, MoodState, Music, Situation, SoundSource,
    UnderwaterAmbient, UnderwaterLoop, VoiceId, mood_sound_position, roll_additions, select_music,
};
use mcr_client::ClientState;
use mcr_client::RegistryItem;
use mcr_data::Version;
use mcr_protocol::types::game::GameType;
use rustc_hash::FxHashMap;

use crate::audio::Audio;
use crate::effects::light_at;

const BIOME: &str = "minecraft:worldgen/biome";

/// The new state when the eye crosses the water surface; spectators keep the old one.
fn underwater_transition(was: &mut bool, underwater: bool, spectator: bool) -> Option<bool> {
    (!spectator && underwater != *was).then(|| {
        *was = underwater;
        underwater
    })
}
const DIMENSION_TYPE: &str = "minecraft:dimension_type";
const MAX_MINECARTS: usize = 64;
const MINECART_PITCH_STEP: f32 = 0.0025;

#[derive(Debug)]
struct LoopVoice {
    sound: String,
    voice: VoiceId,
    fade: LoopFade,
}

#[derive(Debug)]
struct MinecartVoice {
    voice: VoiceId,
    last: [f64; 3],
    pitch: f32,
    speed: f32,
    seen: bool,
}

#[derive(Debug)]
struct RidingVoice {
    vehicle: i32,
    underwater_variant: bool,
    voice: VoiceId,
}

pub struct Ambience {
    dimension_type: Option<i32>,
    dimension_name: String,
    dimension_attrs: AudioAttributes,
    biome_count: usize,
    biome_attrs: Vec<AudioAttributes>,
    mood: MoodState,
    rng: JavaRandom,
    fast: FastRandom,
    previous_loop: Option<String>,
    loops: Vec<LoopVoice>,
    underwater: UnderwaterAmbient,
    underwater_loop: Option<(VoiceId, UnderwaterLoop)>,
    was_underwater: bool,
    minecart_types: Vec<i32>,
    minecarts: FxHashMap<i32, MinecartVoice>,
    riding: Option<RidingVoice>,
    additions: Vec<usize>,
    last_biome: Option<usize>,
    credits_open: bool,
}

impl std::fmt::Debug for Ambience {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ambience").finish_non_exhaustive()
    }
}

impl Ambience {
    pub fn new(data: &'static Version) -> Self {
        Self {
            dimension_type: None,
            dimension_name: String::new(),
            dimension_attrs: AudioAttributes::default(),
            biome_count: usize::MAX,
            biome_attrs: Vec::new(),
            mood: MoodState::default(),
            rng: JavaRandom::new(0x0A1B_1E27),
            fast: FastRandom::new(0xA5_5EED),
            previous_loop: None,
            loops: Vec::new(),
            underwater: UnderwaterAmbient::default(),
            underwater_loop: None,
            was_underwater: false,
            minecart_types: data
                .entity_types()
                .iter()
                .filter(|(_, n)| n.ends_with("minecart"))
                .map(|(id, _)| id as i32)
                .collect(),
            minecarts: FxHashMap::default(),
            riding: None,
            additions: Vec::new(),
            last_biome: None,
            credits_open: false,
        }
    }

    /// Credits screen open: `music.credits` is wanted, and it ends with the screen.
    pub fn set_credits(&mut self, open: bool, audio: &mut Audio) {
        if self.credits_open && !open {
            audio.stop_credits_music();
        }
        self.credits_open = open;
    }

    fn refresh_registries(&mut self, state: &ClientState) {
        let type_id = state.dimension.type_id;
        let name = state.dimension.name.as_str();
        let biomes = state.registries.get(BIOME).map_or(0, |r| r.len());
        let dimension_changed = self.dimension_type != Some(type_id) || self.dimension_name != name;
        if !dimension_changed && biomes == self.biome_count {
            return;
        }
        self.dimension_attrs = state
            .registries
            .get(DIMENSION_TYPE)
            .and_then(|r| r.get(type_id))
            .map(Self::entry_attributes)
            .unwrap_or_default();
        self.biome_attrs = state
            .registries
            .get(BIOME)
            .map(|r| {
                r.entries
                    .iter()
                    .map(|e| Self::entry_attributes(e).over(&self.dimension_attrs))
                    .collect()
            })
            .unwrap_or_default();
        self.biome_count = biomes;
        self.dimension_type = Some(type_id);
        self.dimension_name = name.to_owned();
    }

    fn entry_attributes(entry: &RegistryItem) -> AudioAttributes {
        entry
            .data
            .as_ref()
            .map(AudioAttributes::from_registry_entry)
            .unwrap_or_default()
    }

    /// The sound event of a jukebox song registry entry.
    pub fn jukebox_sound(&self, state: &ClientState, song: i32) -> Option<String> {
        let entry = state.registries.get("minecraft:jukebox_song")?.get(song)?;
        match entry.data.as_ref()?.get("sound_event")? {
            mcr_nbt::Value::String(name) => Some(name.clone()),
            mcr_nbt::Value::Compound(c) => c.get_str("sound_id").map(str::to_owned),
            _ => None,
        }
    }

    fn attrs_at(&self, state: &ClientState, pos: [i32; 3]) -> &AudioAttributes {
        biome_at(state, pos)
            .and_then(|id| self.biome_attrs.get(id))
            .unwrap_or(&self.dimension_attrs)
    }

    /// Switching dimension: drop biome sounds, mood and music of the old one.
    pub fn reset(&mut self, audio: &mut Audio) {
        for l in self.loops.drain(..) {
            audio.stop_voice(l.voice);
        }
        if let Some((voice, _)) = self.underwater_loop.take() {
            audio.stop_voice(voice);
        }
        for (_, m) in self.minecarts.drain() {
            audio.stop_voice(m.voice);
        }
        if let Some(r) = self.riding.take() {
            audio.stop_voice(r.voice);
        }
        self.previous_loop = None;
        self.mood = MoodState::default();
        self.was_underwater = false;
        self.dimension_type = None;
    }

    pub fn tick(&mut self, state: &ClientState, eye: [f64; 3], audio: &mut Audio) {
        if !state.player.loaded {
            audio.tick_music(None, 1.0, true);
            return;
        }
        let was_other_dimension = self
            .dimension_type
            .is_some_and(|t| t != state.dimension.type_id)
            || (!self.dimension_name.is_empty()
                && self.dimension_name != state.dimension.name.as_str());
        if was_other_dimension {
            self.reset(audio);
            audio.stop_music(None);
        }
        self.refresh_registries(state);
        let feet = state.player.pos();
        let feet_block = feet.map(|v| v.floor() as i32);
        let attrs = self.attrs_at(state, feet_block).clone();
        let biome = biome_at(state, feet_block);
        if biome != self.last_biome {
            self.last_biome = biome;
            let name = biome
                .and_then(|b| state.registries.get(BIOME)?.get(b as i32))
                .map_or("?", |e| e.id.as_str());
            audio.note(&format!(
                "biome={name} music={:?} loop={:?} mood={} additions={}",
                attrs
                    .background_music
                    .as_ref()
                    .and_then(|m| m.default.as_ref())
                    .map(|m| m.sound.as_str()),
                attrs
                    .ambient_sounds
                    .as_ref()
                    .and_then(|a| a.loop_sound.as_deref()),
                attrs
                    .ambient_sounds
                    .as_ref()
                    .is_some_and(|a| a.mood.is_some()),
                attrs
                    .ambient_sounds
                    .as_ref()
                    .map_or(0, |a| a.additions.len()),
            ));
        }
        let spectator = state.player.game_mode == GameType::Spectator;
        let underwater = state.player.physics.eye_in_water();

        self.tick_music(state, &attrs, underwater, audio);
        self.tick_underwater(underwater, spectator, feet, audio);
        if let Some(sounds) = &attrs.ambient_sounds {
            self.tick_biome(state, sounds, feet, eye, audio);
        } else {
            self.tick_loops(None, audio);
        }
        self.tick_minecarts(state, underwater, audio);
    }

    fn tick_music(
        &mut self,
        state: &ClientState,
        attrs: &AudioAttributes,
        underwater: bool,
        audio: &mut Audio,
    ) {
        let in_end = state.dimension.name.as_str() == "minecraft:the_end";
        let end_boss_music = state.boss_bars.values().any(|b| b.flags.play_music);
        let creative =
            state.player.game_mode == GameType::Creative && state.player.physics.abilities.may_fly;
        let credits = Music::credits();
        let situation = select_music(&Situation {
            screen_music: self.credits_open.then_some(&credits),
            in_world: true,
            in_end,
            end_boss_music,
            creative_flying: creative,
            underwater,
            background: attrs.background_music.as_ref(),
        });
        let volume = attrs.music_volume.unwrap_or(1.0);
        audio.tick_music(situation.as_ref(), volume, false);
    }

    fn tick_underwater(
        &mut self,
        underwater: bool,
        spectator: bool,
        feet: [f64; 3],
        audio: &mut Audio,
    ) {
        if let Some(underwater) =
            underwater_transition(&mut self.was_underwater, underwater, spectator)
        {
            let event = if underwater {
                "ambient.underwater.enter"
            } else {
                "ambient.underwater.exit"
            };
            audio.play(event, Some(feet), 1.0, 1.0, SoundSource::Ambient);
            if underwater
                && self.underwater_loop.is_none()
                && let Some(voice) = audio.play_voice_looping(
                    "ambient.underwater.loop",
                    None,
                    0.0,
                    1.0,
                    SoundSource::Ambient,
                    true,
                )
            {
                self.underwater_loop = Some((voice, UnderwaterLoop::default()));
            }
        }
        if let Some((voice, fade)) = &mut self.underwater_loop {
            match fade.tick(self.was_underwater) {
                Some(volume) => audio.set_voice_volume(*voice, volume),
                None => {
                    audio.stop_voice(*voice);
                    self.underwater_loop = None;
                }
            }
        }
        let roll = self.fast.f32();
        if let Some(sound) = self.underwater.tick(self.was_underwater, roll) {
            audio.play(sound.event(), None, 1.0, 1.0, SoundSource::Ambient);
        }
    }

    fn tick_biome(
        &mut self,
        state: &ClientState,
        sounds: &mcr_audio::AmbientSounds,
        feet: [f64; 3],
        eye: [f64; 3],
        audio: &mut Audio,
    ) {
        self.tick_loops(sounds.loop_sound.as_deref(), audio);
        roll_additions(&mut self.rng, &sounds.additions, &mut self.additions);
        for &i in &self.additions {
            audio.play(
                &sounds.additions[i].sound,
                None,
                1.0,
                1.0,
                SoundSource::Ambient,
            );
        }
        let Some(mood) = &sounds.mood else { return };
        let extent = mood.block_search_extent;
        let span = extent * 2 + 1;
        let sample = [
            (feet[0] + f64::from(self.rng.next_int(span) - extent)).floor() as i32,
            (eye[1] + f64::from(self.rng.next_int(span) - extent)).floor() as i32,
            (feet[2] + f64::from(self.rng.next_int(span) - extent)).floor() as i32,
        ];
        let (block, sky) =
            light_at(&state.chunks, sample[0], sample[1], sample[2]).unwrap_or((0, 0));
        if self.mood.step(mood, sky, block) {
            let at = mood_sound_position(feet[0], eye[1], feet[2], sample, mood.offset);
            audio.play(&mood.sound, Some(at), 1.0, 1.0, SoundSource::Ambient);
        }
    }

    fn tick_loops(&mut self, current: Option<&str>, audio: &mut Audio) {
        if current != self.previous_loop.as_deref() {
            self.previous_loop = current.map(str::to_owned);
            for l in &mut self.loops {
                l.fade.fade_out();
            }
            if let Some(sound) = current {
                if let Some(l) = self.loops.iter_mut().find(|l| l.sound == sound) {
                    l.fade.fade_in();
                } else if let Some(voice) =
                    audio.play_voice_looping(sound, None, 0.0, 1.0, SoundSource::Ambient, true)
                {
                    let mut fade = LoopFade::default();
                    fade.fade_in();
                    self.loops.push(LoopVoice {
                        sound: sound.to_owned(),
                        voice,
                        fade,
                    });
                }
            }
        }
        self.loops.retain_mut(|l| match l.fade.tick() {
            Some(volume) => {
                audio.set_voice_volume(l.voice, volume);
                true
            }
            None => {
                audio.stop_voice(l.voice);
                false
            }
        });
    }

    fn tick_minecarts(&mut self, state: &ClientState, underwater: bool, audio: &mut Audio) {
        for m in self.minecarts.values_mut() {
            m.seen = false;
        }
        let mut seen = 0;
        for e in state.entities.iter() {
            if !self.minecart_types.contains(&e.kind) {
                continue;
            }
            seen += 1;
            if seen > MAX_MINECARTS {
                break;
            }
            let cart = match self.minecarts.get_mut(&e.id) {
                Some(c) => c,
                None => {
                    let Some(voice) = audio.play_voice_looping(
                        "entity.minecart.riding",
                        Some(e.pos),
                        0.0,
                        1.0,
                        SoundSource::Neutral,
                        true,
                    ) else {
                        continue;
                    };
                    self.minecarts.entry(e.id).or_insert(MinecartVoice {
                        voice,
                        last: e.pos,
                        pitch: 0.0,
                        speed: 0.0,
                        seen: true,
                    })
                }
            };
            cart.seen = true;
            let speed = ((e.pos[0] - cart.last[0]).powi(2) + (e.pos[2] - cart.last[2]).powi(2))
                .sqrt() as f32;
            cart.last = e.pos;
            cart.speed = speed;
            if speed >= 0.01 {
                cart.pitch = (cart.pitch + MINECART_PITCH_STEP).clamp(0.0, 1.0);
                let volume = speed.clamp(0.0, 0.5) / 0.5 * 0.7;
                audio.set_voice_volume(cart.voice, volume);
            } else {
                cart.pitch = 0.0;
                audio.set_voice_volume(cart.voice, 0.0);
            }
            audio.set_voice_pitch(cart.voice, cart.pitch);
            audio.set_voice_position(cart.voice, e.pos);
        }
        self.minecarts.retain(|_, m| {
            if !m.seen {
                audio.stop_voice(m.voice);
            }
            m.seen
        });
        self.tick_riding(state, underwater, audio);
    }

    fn tick_riding(&mut self, state: &ClientState, underwater: bool, audio: &mut Audio) {
        let vehicle = state
            .player
            .vehicle
            .and_then(|id| state.entities.get(id))
            .filter(|e| self.minecart_types.contains(&e.kind))
            .map(|e| e.id);
        if let Some(r) = &self.riding
            && (vehicle != Some(r.vehicle) || r.underwater_variant != underwater)
            && let Some(r) = self.riding.take()
        {
            audio.stop_voice(r.voice);
        }
        let Some(vehicle) = vehicle else { return };
        if self.riding.is_none() {
            let event = if underwater {
                "entity.minecart.inside.underwater"
            } else {
                "entity.minecart.inside"
            };
            self.riding = audio
                .play_voice_looping(event, None, 0.0, 1.0, SoundSource::Neutral, true)
                .map(|voice| RidingVoice {
                    vehicle,
                    underwater_variant: underwater,
                    voice,
                });
        }
        if let Some(r) = &self.riding {
            let speed = self.minecarts.get(&vehicle).map_or(0.0, |m| m.speed);
            let volume = if speed >= 0.01 {
                speed.clamp(0.0, 1.0) * 0.75
            } else {
                0.0
            };
            audio.set_voice_volume(r.voice, volume);
        }
    }
}

pub fn biome_at(state: &ClientState, pos: [i32; 3]) -> Option<usize> {
    let chunks = &state.chunks;
    let column = chunks.column(mcr_world::ChunkPos::new(pos[0] >> 4, pos[2] >> 4))?;
    let rel = pos[1] - chunks.config().min_y();
    if rel < 0 {
        return None;
    }
    let section = column.section((rel >> 4) as usize)?;
    Some(
        section
            .biome(
                ((pos[0] & 15) >> 2) as usize,
                ((rel & 15) >> 2) as usize,
                ((pos[2] & 15) >> 2) as usize,
            )
            .0 as usize,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcr_physics::{PlayerInput, PlayerPhysics};
    use mcr_world::{BlockState, ChunkColumn, ChunkPos, ChunkStore, WorldConfig};

    fn pool() -> ChunkStore {
        let data = mcr_data::v26_3();
        let cfg = WorldConfig::new(data.state_count() as u32, 80, -64, 384).unwrap();
        let mut store = ChunkStore::new(cfg);
        store.insert(ChunkPos::new(0, 0), ChunkColumn::empty(&cfg));
        let state = |name| {
            let id = data.block_by_name(name).unwrap();
            BlockState(data.block_info(id).unwrap().default_state as u16)
        };
        for x in 0..16 {
            for z in 0..16 {
                store.set_block(x, 55, z, state("minecraft:stone"));
                for y in 56..64 {
                    store.set_block(x, y, z, state("minecraft:water"));
                }
            }
        }
        store
    }

    #[test]
    fn sinking_to_a_pool_bottom_enters_water_once() {
        let world = pool();
        let mut p = PlayerPhysics::new([8.5, 61.33, 8.5]);
        let mut was = false;
        let mut transitions = Vec::new();
        let mut top_ninth_ticks = 0;
        for _ in 0..800 {
            p.tick(&PlayerInput::default(), &world);
            let eye = p.pos[1] + f64::from(p.eye_height());
            if eye.fract() > 8.0 / 9.0 {
                top_ninth_ticks += 1;
            }
            transitions.extend(underwater_transition(&mut was, p.eye_in_water(), false));
        }
        assert!(p.on_ground, "never reached the bottom");
        assert!(top_ninth_ticks > 0);
        assert_eq!(transitions, [true]);
    }

    #[test]
    fn spectators_never_toggle_underwater() {
        let mut was = false;
        assert_eq!(underwater_transition(&mut was, true, true), None);
        assert_eq!(underwater_transition(&mut was, true, false), Some(true));
        assert_eq!(underwater_transition(&mut was, false, false), Some(false));
    }
}
