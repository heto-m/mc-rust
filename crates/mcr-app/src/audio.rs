use std::io::Write;
use std::path::Path;

use std::sync::Arc;

use mcr_assets::ResourceManager;
use mcr_audio::{
    AssetStore, AudioEngine, Listener, Music, MusicFrequency, MusicInput, MusicManager,
    SoundManager, SoundOverlay, SoundSource, SoundType, VoiceId,
};
use mcr_data::Version;
use mcr_protocol::play::clientbound::world::SoundSource as WireSource;
use mcr_settings::{Options, SOUND_CATEGORIES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockSound {
    Break,
    Place,
    Hit,
}

impl BlockSound {
    fn volume_pitch(self) -> (f32, f32) {
        match self {
            Self::Break | Self::Place => (0.5, 0.8),
            Self::Hit => (0.125, 0.5),
        }
    }
}

pub fn wire_category(source: WireSource) -> SoundSource {
    match source {
        WireSource::Master => SoundSource::Master,
        WireSource::Music => SoundSource::Music,
        WireSource::Records => SoundSource::Records,
        WireSource::Weather => SoundSource::Weather,
        WireSource::Blocks => SoundSource::Blocks,
        WireSource::Hostile => SoundSource::Hostile,
        WireSource::Neutral => SoundSource::Neutral,
        WireSource::Players => SoundSource::Players,
        WireSource::Ambient => SoundSource::Ambient,
        WireSource::Voice => SoundSource::Voice,
        WireSource::Ui => SoundSource::Ui,
    }
}

const MAX_RECORDS: usize = 64;
const RECORD_RANGE: f64 = 64.0;
const SOUND_LOG_ENV: &str = "MCR_SOUND_LOG";

pub struct Audio {
    manager: SoundManager,
    _engine: AudioEngine,
    seed: i64,
    data: &'static Version,
    music: MusicManager,
    music_voice: Option<VoiceId>,
    frequency: MusicFrequency,
    records: Vec<([i32; 3], VoiceId)>,
    ended: Vec<VoiceId>,
    listener: [f64; 3],
    log: Option<std::fs::File>,
    tick: u64,
}

pub fn music_frequency(value: mcr_settings::value::MusicFrequency) -> MusicFrequency {
    match value {
        mcr_settings::value::MusicFrequency::Default => MusicFrequency::Default,
        mcr_settings::value::MusicFrequency::Frequent => MusicFrequency::Frequent,
        mcr_settings::value::MusicFrequency::Constant => MusicFrequency::Constant,
    }
}

impl std::fmt::Debug for Audio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audio").finish_non_exhaustive()
    }
}

impl Audio {
    /// `None` without an output device or sound assets: the client then runs silently.
    pub fn start(
        assets_dir: &Path,
        index: &str,
        options: &Options,
        data: &'static Version,
    ) -> Option<Self> {
        let store = AssetStore::open(assets_dir, index)
            .or_else(|_| AssetStore::open_latest(assets_dir))
            .map_err(|e| tracing::warn!("sound assets unavailable: {e}"))
            .ok()?;
        let (engine, handle) = AudioEngine::start()
            .map_err(|e| tracing::warn!("audio unavailable: {e}"))
            .ok()?;
        let mut manager = SoundManager::new(store, handle)
            .map_err(|e| tracing::warn!("sounds.json unavailable: {e}"))
            .ok()?;
        for (i, name) in SOUND_CATEGORIES.iter().enumerate() {
            if let Some(category) = SoundSource::from_name(name) {
                manager.set_category_volume(category, options.sound_volumes[i] as f32);
            }
        }
        let frequency = music_frequency(options.music_frequency);
        let log = std::env::var_os(SOUND_LOG_ENV).and_then(|path| {
            std::fs::File::create(path)
                .map_err(|e| tracing::warn!("sound log unavailable: {e}"))
                .ok()
        });
        Some(Self {
            manager,
            _engine: engine,
            seed: 1,
            data,
            music: MusicManager::new(0x5EED, frequency),
            music_voice: None,
            frequency,
            records: Vec::new(),
            ended: Vec::new(),
            listener: [0.0; 3],
            log,
            tick: 0,
        })
    }

    /// Layers the resource stack's sounds and `sounds.json` files over the launcher assets.
    pub fn reload(&mut self, resources: &Arc<ResourceManager>) {
        let files = resources.clone();
        let overlay: SoundOverlay =
            Arc::new(move |name| files.read(&format!("assets/{name}")).ok().flatten());
        let mut packs = Vec::new();
        for path in resources.list_prefix("assets/") {
            let mut parts = path.split('/');
            if let (Some("assets"), Some(namespace), Some("sounds.json"), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            {
                for bytes in resources.read_all(path) {
                    packs.push((
                        namespace.to_owned(),
                        String::from_utf8_lossy(&bytes).into_owned(),
                    ));
                }
            }
        }
        self.manager.reload(Some(overlay), &packs);
    }

    pub fn apply_volumes(&mut self, options: &Options) {
        let frequency = music_frequency(options.music_frequency);
        if frequency != self.frequency {
            self.frequency = frequency;
            self.music.set_frequency(frequency, None);
        }
        for (i, name) in SOUND_CATEGORIES.iter().enumerate() {
            if let Some(category) = SoundSource::from_name(name) {
                self.manager
                    .set_category_volume(category, options.sound_volumes[i] as f32);
            }
        }
    }

    pub fn click(&mut self) {
        self.play("ui.button.click", None, 0.25, 1.0, SoundSource::Ui);
    }

    pub fn set_listener(&mut self, position: [f64; 3], yaw_degrees: f32) {
        self.listener = position;
        self.manager.set_listener(Listener {
            position: position.map(|v| v as f32),
            yaw_degrees,
        });
    }

    pub fn play(
        &mut self,
        event: &str,
        position: Option<[f64; 3]>,
        volume: f32,
        pitch: f32,
        category: SoundSource,
    ) -> Option<VoiceId> {
        self.play_voice_looping(event, position, volume, pitch, category, false)
    }

    pub fn play_voice_looping(
        &mut self,
        event: &str,
        position: Option<[f64; 3]>,
        volume: f32,
        pitch: f32,
        category: SoundSource,
        looping: bool,
    ) -> Option<VoiceId> {
        self.seed = self
            .seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let name = event.strip_prefix("minecraft:").unwrap_or(event);
        let id = self.manager.play_looping(
            name,
            position.map(|p| p.map(|v| v as f32)),
            volume,
            pitch,
            category,
            self.seed,
            looping,
        );
        self.log_sound(name, position, volume, pitch, category, id.is_some());
        id
    }

    pub fn note(&mut self, text: &str) {
        let tick = self.tick;
        if let Some(log) = &mut self.log {
            let _ = writeln!(log, "tick={tick} note={text}");
        }
    }

    fn log_sound(
        &mut self,
        event: &str,
        position: Option<[f64; 3]>,
        volume: f32,
        pitch: f32,
        category: SoundSource,
        started: bool,
    ) {
        let tick = self.tick;
        let Some(log) = &mut self.log else { return };
        let at = position.map_or_else(
            || "-".to_owned(),
            |p| format!("{:.1},{:.1},{:.1}", p[0], p[1], p[2]),
        );
        let _ = writeln!(
            log,
            "tick={tick} sound={event} cat={} vol={volume:.3} pitch={pitch:.3} at={at}{}",
            category.name(),
            if started { "" } else { " (not played)" }
        );
    }

    pub fn stop_voice(&mut self, id: VoiceId) {
        self.manager.stop(id);
    }

    pub fn set_voice_volume(&mut self, id: VoiceId, volume: f32) {
        self.manager.set_voice_volume(id, volume);
    }

    pub fn set_voice_pitch(&mut self, id: VoiceId, pitch: f32) {
        self.manager
            .handle()
            .send(mcr_audio::Command::SetVoicePitch(id, pitch));
    }

    pub fn set_voice_position(&mut self, id: VoiceId, position: [f64; 3]) {
        self.manager
            .handle()
            .send(mcr_audio::Command::SetVoicePosition(
                id,
                position.map(|v| v as f32),
            ));
    }

    /// Once per client tick: learns which tracked voices finished by themselves.
    pub fn poll(&mut self, tick: u64) {
        self.tick = tick;
        self.ended.clear();
        self.manager.handle().take_ended(&mut self.ended);
        if self.ended.is_empty() {
            return;
        }
        let ended = std::mem::take(&mut self.ended);
        self.records.retain(|(_, v)| !ended.contains(v));
        if self.music_voice.is_some_and(|v| ended.contains(&v)) {
            self.music_voice = None;
        }
        self.ended = ended;
    }

    pub fn play_record(&mut self, sound: &str, pos: [i32; 3]) {
        self.stop_record(pos);
        let center = pos.map(|v| f64::from(v) + 0.5);
        if self.records.len() >= MAX_RECORDS {
            return;
        }
        if let Some(id) = self.play(sound, Some(center), 4.0, 1.0, SoundSource::Records) {
            self.records.push((pos, id));
        }
    }

    pub fn stop_record(&mut self, pos: [i32; 3]) {
        if let Some(i) = self.records.iter().position(|(p, _)| *p == pos) {
            let (_, id) = self.records.swap_remove(i);
            self.manager.stop(id);
        }
    }

    /// Leaving a world: records and every non-music, non-UI voice stop.
    pub fn stop_world_sounds(&mut self) {
        self.records.clear();
        for category in [
            SoundSource::Records,
            SoundSource::Weather,
            SoundSource::Blocks,
            SoundSource::Hostile,
            SoundSource::Neutral,
            SoundSource::Players,
            SoundSource::Ambient,
            SoundSource::Voice,
        ] {
            self.manager
                .handle()
                .send(mcr_audio::Command::StopCategory(category));
        }
    }

    fn record_nearby(&self) -> bool {
        self.records.iter().any(|(p, _)| {
            let d = [
                f64::from(p[0]) + 0.5 - self.listener[0],
                f64::from(p[1]) + 0.5 - self.listener[1],
                f64::from(p[2]) + 0.5 - self.listener[2],
            ];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < RECORD_RANGE * RECORD_RANGE
        })
    }

    pub fn tick_music(&mut self, situation: Option<&Music>, target_volume: f32, loading: bool) {
        let input = MusicInput {
            situation,
            target_volume,
            loading,
            track_active: self.music_voice.is_some(),
            record_playing: self.record_nearby(),
        };
        let actions = self.music.tick(&input);
        if actions.stop {
            self.stop_music_voice();
        }
        if let Some(music) = actions.start {
            self.stop_music_voice();
            let gain = actions.gain.unwrap_or(1.0);
            self.music_voice = self.play(&music.sound, None, gain, 1.0, SoundSource::Music);
        } else if let (Some(gain), Some(voice)) = (actions.gain, self.music_voice) {
            self.manager.set_voice_volume(voice, gain);
        }
    }

    /// Dimension change: vanilla drops the current track and waits before the next.
    pub fn stop_music(&mut self, situation: Option<&Music>) {
        let mut actions = mcr_audio::MusicActions::default();
        self.music.stop_playing(situation, &mut actions);
        self.stop_music_voice();
    }

    fn stop_music_voice(&mut self) {
        if let Some(voice) = self.music_voice.take() {
            self.manager.stop(voice);
        }
    }

    /// Vanilla's pickup sounds: pitch is the difference of two uniform draws, scaled and offset.
    pub fn play_pickup(&mut self, pos: [f64; 3], experience_orb: bool) {
        let spread = self.unit() - self.unit();
        if experience_orb {
            self.play(
                "entity.experience_orb.pickup",
                Some(pos),
                0.1,
                spread * 0.35 + 0.9,
                SoundSource::Players,
            );
        } else {
            self.play(
                "entity.item.pickup",
                Some(pos),
                0.2,
                spread * 1.4 + 2.0,
                SoundSource::Players,
            );
        }
    }

    pub fn play_voice(&mut self, event: &str) {
        let pitch = (self.unit() - self.unit()) * 0.2 + 1.0;
        self.play(event, None, 1.0, pitch, SoundSource::Players);
    }

    fn unit(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.seed as u64) >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn play_block(&mut self, state: u32, kind: BlockSound, pos: [f64; 3]) {
        let Some(ty) = sound_type(self.data, state) else {
            return;
        };
        let event = match kind {
            BlockSound::Break => ty.break_event(),
            BlockSound::Place => ty.place_event(),
            BlockSound::Hit => ty.hit_event(),
        };
        let (v, p) = kind.volume_pitch();
        let volume = match kind {
            BlockSound::Break | BlockSound::Place | BlockSound::Hit => (ty.volume + 1.0) * v,
        };
        self.play(&event, Some(pos), volume, p * ty.pitch, SoundSource::Blocks);
    }
}

pub fn sound_type(data: &'static Version, state: u32) -> Option<SoundType<'static>> {
    let block = data.block_state(state)?.block;
    SoundType::from_break_event(data.block_info(block)?.sound_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_uses_the_stone_events() {
        let data = mcr_data::v26_3();
        let id = data.block_by_name("minecraft:stone").unwrap();
        let state = data.block_info(id).unwrap().default_state;
        let ty = sound_type(data, state).unwrap();
        assert_eq!(ty.break_event(), "block.stone.break");
        assert_eq!(ty.step_event(), "block.stone.step");
    }
}
