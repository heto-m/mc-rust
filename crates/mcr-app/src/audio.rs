use std::io::Write;
use std::path::Path;

use std::sync::Arc;

use mcr_assets::ResourceManager;
use mcr_audio::{
    AssetStore, AudioEngine, Listener, Music, MusicFrequency, MusicInput, MusicManager,
    SoundManager, SoundOverlay, SoundSource, SoundType, VoiceId,
};
use mcr_data::Version;
use mcr_modbridge::sound::SoundLink;
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

/// `Identifier#toShortLanguageKey` of a music file, `/` turned into `.`.
pub fn music_lang_key(sound: &str) -> String {
    let (namespace, path) = sound.split_once(':').unwrap_or(("minecraft", sound));
    let key = if namespace == "minecraft" {
        path.to_owned()
    } else {
        format!("{namespace}.{path}")
    };
    key.replace('/', ".")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MusicEvent {
    Started(String),
    Stopped,
}

const MAX_RECORDS: usize = 64;
/// More than the mixer can hold, so a voice it stole only lingers until the oldest is dropped.
const MAX_TRACKED: usize = 320;
const MAX_BOUND: usize = 128;
const MAX_SUBTITLE_CUES: usize = 64;
/// `Minecraft#runTick` pauses everything but these while a local world is paused.
const PAUSED_CATEGORIES: [SoundSource; 9] = [
    SoundSource::Master,
    SoundSource::Records,
    SoundSource::Weather,
    SoundSource::Blocks,
    SoundSource::Hostile,
    SoundSource::Neutral,
    SoundSource::Players,
    SoundSource::Ambient,
    SoundSource::Voice,
];
/// Menus never drain music events, so only the latest few are kept.
const MAX_MUSIC_EVENTS: usize = 16;
const RECORD_RANGE: f64 = 64.0;
const SOUND_LOG_ENV: &str = "MCR_SOUND_LOG";

fn push_capped<T>(queue: &mut Vec<T>, item: T, cap: usize) {
    if queue.len() >= cap {
        queue.remove(0);
    }
    queue.push(item);
}

/// A playing voice: its event (hashed, for `stop_sound`) and category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tracked {
    id: VoiceId,
    event: u64,
    category: SoundSource,
}

/// A sound the subtitle overlay should list: `SoundEventListener#onPlaySound`.
#[derive(Debug, Clone, PartialEq)]
pub struct SubtitleCue {
    pub key: String,
    pub pos: [f64; 3],
    pub range: f32,
    pub looping: bool,
}

fn event_hash(event: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    event
        .strip_prefix("minecraft:")
        .unwrap_or(event)
        .hash(&mut h);
    h.finish()
}

/// Which playing voices belong to which event, category and entity.
#[derive(Debug, Default)]
struct VoiceBook {
    tracked: Vec<Tracked>,
    /// `EntityBoundSoundInstance`: voices that follow an entity until it is gone.
    bound: Vec<(VoiceId, i32)>,
}

impl VoiceBook {
    fn track(&mut self, id: VoiceId, event: &str, category: SoundSource) {
        if self.tracked.len() >= MAX_TRACKED {
            self.tracked.remove(0);
        }
        self.tracked.push(Tracked {
            id,
            event: event_hash(event),
            category,
        });
    }

    fn bind(&mut self, id: VoiceId, entity: i32) {
        if self.bound.len() >= MAX_BOUND {
            self.bound.remove(0);
        }
        self.bound.push((id, entity));
    }

    /// Removes and returns the voices a `stop_sound` for `name` and `category` stops.
    fn take_matching(&mut self, name: Option<&str>, category: Option<SoundSource>) -> Vec<VoiceId> {
        let event = name.map(event_hash);
        let mut stopped = Vec::new();
        self.tracked.retain(|t| {
            let hit =
                event.is_none_or(|e| e == t.event) && category.is_none_or(|c| c == t.category);
            if hit {
                stopped.push(t.id);
            }
            !hit
        });
        self.forget(&stopped);
        stopped
    }

    fn forget(&mut self, gone: &[VoiceId]) {
        self.tracked.retain(|t| !gone.contains(&t.id));
        self.bound.retain(|(v, _)| !gone.contains(v));
    }

    /// Moves each bound voice to its entity; returns the voices whose entity is gone.
    fn follow(
        &mut self,
        position_of: impl Fn(i32) -> Option<[f64; 3]>,
        mut moved: impl FnMut(VoiceId, [f64; 3]),
    ) -> Vec<VoiceId> {
        let mut gone = Vec::new();
        self.bound
            .retain(|&(id, entity)| match position_of(entity) {
                Some(pos) => {
                    moved(id, pos);
                    true
                }
                None => {
                    gone.push(id);
                    false
                }
            });
        self.tracked.retain(|t| !gone.contains(&t.id));
        gone
    }
}

pub struct Audio {
    manager: SoundManager,
    book: VoiceBook,
    moves: Vec<(VoiceId, [f64; 3])>,
    subtitles_on: bool,
    subtitle_cues: Vec<SubtitleCue>,
    paused: bool,
    _engine: AudioEngine,
    seed: i64,
    data: &'static Version,
    music: MusicManager,
    music_voice: Option<VoiceId>,
    music_events: Vec<MusicEvent>,
    frequency: MusicFrequency,
    records: Vec<([i32; 3], VoiceId)>,
    ended: Vec<VoiceId>,
    listener: [f64; 3],
    log: Option<std::fs::File>,
    tick: u64,
    java: Option<JavaSound>,
    /// The `soundDevice` the engine was opened for.
    device: String,
    delayed: Vec<DelayedSound>,
}

const MAX_DELAYED: usize = 256;

/// `SoundManager#playDelayed`: a distant sound that starts once the sound has travelled.
#[derive(Debug, Clone, PartialEq)]
struct DelayedSound {
    due: u64,
    event: String,
    position: [f64; 3],
    volume: f32,
    pitch: f32,
    category: SoundSource,
}

/// `ClientLevel#playSound` with `distanceDelay`: beyond ten blocks sound travels at 40 blocks a
/// second, which is half a tick per block; nearer sounds play at once.
pub fn distance_delay_ticks(listener: [f64; 3], at: [f64; 3]) -> u64 {
    let d2: f64 = (0..3).map(|i| (listener[i] - at[i]).powi(2)).sum();
    if d2 > 100.0 {
        (d2.sqrt() / 40.0 * 20.0) as u64
    } else {
        0
    }
}

/// Mod mode: Java's sound engine plays through this mixer and its sliders set every volume.
struct JavaSound {
    link: SoundLink,
    volume_seq: u32,
}

/// `audio` unless Java owns world sounds (mod mode): server sounds, music, ambience, entity sounds.
pub fn world_audio(audio: &mut Option<Audio>) -> Option<&mut Audio> {
    audio.as_mut().filter(|a| !a.java_owned())
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
        let (engine, handle) = AudioEngine::start(Some(&options.sound_device.0))
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
            book: VoiceBook::default(),
            moves: Vec::new(),
            subtitles_on: options.show_subtitles,
            subtitle_cues: Vec::new(),
            paused: false,
            _engine: engine,
            seed: 1,
            data,
            music: MusicManager::new(0x5EED, frequency),
            music_voice: None,
            music_events: Vec::new(),
            frequency,
            records: Vec::new(),
            ended: Vec::new(),
            listener: [0.0; 3],
            log,
            tick: 0,
            delayed: Vec::new(),
            java: None,
            device: options.sound_device.0.clone(),
        })
    }

    /// Hands Java's channel commands to this mixer; from then on Java owns world sounds.
    pub fn attach_java(&mut self, link: SoundLink) -> bool {
        let Some(voices) = link.voices(self._engine.sample_rate()) else {
            return false;
        };
        if !self
            .manager
            .handle()
            .send(mcr_audio::Command::AttachProducer(Box::new(voices)))
        {
            return false;
        }
        self.music
            .stop_playing(None, &mut mcr_audio::MusicActions::default());
        self.stop_music_voice();
        self.java = Some(JavaSound {
            link,
            volume_seq: 0,
        });
        true
    }

    pub fn java_owned(&self) -> bool {
        self.java.is_some()
    }

    fn sync_java_volumes(&mut self) {
        let Some(java) = &mut self.java else { return };
        let Some((seq, volumes)) = java.link.java_volumes() else {
            return;
        };
        if seq == java.volume_seq {
            return;
        }
        java.volume_seq = seq;
        for (category, volume) in SoundSource::ALL.into_iter().zip(volumes) {
            self.manager.set_category_volume(category, volume);
        }
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
        self.subtitles_on = options.show_subtitles;
        if !self.subtitles_on {
            self.subtitle_cues.clear();
        }
        let frequency = music_frequency(options.music_frequency);
        if frequency != self.frequency {
            self.frequency = frequency;
            self.music.set_frequency(frequency, None);
        }
        if self.java_owned() {
            return;
        }
        for (i, name) in SOUND_CATEGORIES.iter().enumerate() {
            if let Some(category) = SoundSource::from_name(name) {
                self.manager
                    .set_category_volume(category, options.sound_volumes[i] as f32);
            }
        }
    }

    /// Like `SoundManager#reload` after `soundDevice` changes: every sound stops and the new
    /// device takes over. Java's sounds stay on the old mixer, so mod mode keeps its device.
    pub fn sync_device(&mut self, options: &Options) {
        let wanted = &options.sound_device.0;
        if *wanted == self.device || self.java.is_some() {
            return;
        }
        self.device = wanted.clone();
        let (engine, handle) = match AudioEngine::start(Some(wanted)) {
            Ok(started) => started,
            Err(e) => {
                tracing::warn!("audio device {wanted:?} unavailable: {e}");
                return;
            }
        };
        *self.manager.handle() = handle;
        self._engine = engine;
        self.book = VoiceBook::default();
        self.moves.clear();
        self.subtitle_cues.clear();
        self.music_voice = None;
        self.records.clear();
        self.ended.clear();
        self.apply_volumes(options);
        self.click();
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
        self.play_seeded(event, position, volume, pitch, category, self.seed, looping)
    }

    /// Plays `event` after `ticks`, or now when that is zero.
    pub fn play_delayed(
        &mut self,
        event: &str,
        position: [f64; 3],
        volume: f32,
        pitch: f32,
        category: SoundSource,
        ticks: u64,
    ) {
        if ticks == 0 {
            self.play(event, Some(position), volume, pitch, category);
            return;
        }
        if self.delayed.len() >= MAX_DELAYED {
            self.delayed.remove(0);
        }
        self.delayed.push(DelayedSound {
            due: self.tick.saturating_add(ticks),
            event: event.to_owned(),
            position,
            volume,
            pitch,
            category,
        });
    }

    /// `ClientLevel#playSeededSound`: the server's seed picks the variant.
    #[allow(clippy::too_many_arguments)]
    pub fn play_seeded(
        &mut self,
        event: &str,
        position: Option<[f64; 3]>,
        volume: f32,
        pitch: f32,
        category: SoundSource,
        seed: i64,
        looping: bool,
    ) -> Option<VoiceId> {
        let name = event.strip_prefix("minecraft:").unwrap_or(event);
        let id = self.manager.play_looping(
            name,
            position.map(|p| p.map(|v| v as f32)),
            volume,
            pitch,
            category,
            seed,
            looping,
        );
        self.log_sound(name, position, volume, pitch, category, id.is_some());
        if let Some(id) = id {
            self.book.track(id, name, category);
            self.cue_subtitle(name, position, looping);
        }
        id
    }

    fn cue_subtitle(&mut self, event: &str, position: Option<[f64; 3]>, looping: bool) {
        if !self.subtitles_on {
            return;
        }
        let Some(key) = self.manager.events().subtitle(event) else {
            return;
        };
        if self.subtitle_cues.len() >= MAX_SUBTITLE_CUES {
            self.subtitle_cues.remove(0);
        }
        let range = self.manager.last_range();
        self.subtitle_cues.push(SubtitleCue {
            key: key.to_owned(),
            pos: position.unwrap_or(self.listener),
            range,
            looping,
        });
    }

    /// Sounds started since the last call that carry a subtitle.
    pub fn take_subtitle_cues(&mut self) -> Vec<SubtitleCue> {
        std::mem::take(&mut self.subtitle_cues)
    }

    /// `ClientPacketListener#handleStopSoundEvent`: by event name, by category, both, or everything.
    pub fn stop_sound(&mut self, name: Option<&str>, category: Option<SoundSource>) {
        let everything = name.is_none() && category.is_none();
        self.delayed.retain(|d| {
            let named = name.is_none_or(|n| {
                d.event.strip_prefix("minecraft:").unwrap_or(&d.event)
                    == n.strip_prefix("minecraft:").unwrap_or(n)
            });
            let sourced = category.is_none_or(|c| d.category == c);
            !(named && sourced)
        });
        let stopped = self.book.take_matching(name, category);
        let source = category.map_or("-", SoundSource::name);
        self.note(&format!(
            "stop_sound name={} source={source} stopped={}",
            name.unwrap_or("-"),
            stopped.len()
        ));
        if everything {
            self.manager.handle().send(mcr_audio::Command::StopAll);
            self.book = VoiceBook::default();
            self.records.clear();
            if self.music_voice.take().is_some() {
                self.push_music_event(MusicEvent::Stopped);
            }
            return;
        }
        for &id in &stopped {
            self.manager.stop(id);
        }
        self.forget(&stopped);
    }

    /// Drops bookkeeping for voices that are gone.
    fn forget(&mut self, gone: &[VoiceId]) {
        if gone.is_empty() {
            return;
        }
        self.book.forget(gone);
        self.records.retain(|(_, v)| !gone.contains(v));
        if self.music_voice.is_some_and(|v| gone.contains(&v)) {
            self.music_voice = None;
            self.push_music_event(MusicEvent::Stopped);
        }
    }

    /// Plays a sound that follows `entity` (`EntityBoundSoundInstance`).
    #[allow(clippy::too_many_arguments)]
    pub fn play_bound(
        &mut self,
        event: &str,
        entity: i32,
        position: [f64; 3],
        volume: f32,
        pitch: f32,
        category: SoundSource,
        seed: i64,
    ) -> Option<VoiceId> {
        let id = self.play_seeded(event, Some(position), volume, pitch, category, seed, false)?;
        self.book.bind(id, entity);
        Some(id)
    }

    /// Once per tick: bound voices move with their entity and stop once it is removed.
    pub fn tick_bound(&mut self, position_of: impl Fn(i32) -> Option<[f64; 3]>) {
        if self.book.bound.is_empty() {
            return;
        }
        let mut moves = std::mem::take(&mut self.moves);
        let gone = self
            .book
            .follow(position_of, |id, pos| moves.push((id, pos)));
        for (id, pos) in moves.drain(..) {
            self.set_voice_position(id, pos);
        }
        self.moves = moves;
        for id in gone {
            self.manager.stop(id);
        }
    }

    /// Pauses or resumes every world sound while a local world is paused; music and UI keep playing.
    pub fn pause_world(&mut self, paused: bool) {
        if paused == self.paused || self.java_owned() {
            return;
        }
        self.paused = paused;
        for category in PAUSED_CATEGORIES {
            self.manager
                .handle()
                .send(mcr_audio::Command::PauseCategory(category, paused));
        }
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
        if !self.delayed.is_empty() {
            let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.delayed)
                .into_iter()
                .partition(|d| d.due <= tick);
            self.delayed = waiting;
            for d in due {
                self.play(&d.event, Some(d.position), d.volume, d.pitch, d.category);
            }
        }
        self.sync_java_volumes();
        self.ended.clear();
        self.manager.handle().take_ended(&mut self.ended);
        if self.ended.is_empty() {
            return;
        }
        let ended = std::mem::take(&mut self.ended);
        self.forget(&ended);
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
        self.book.bound.clear();
        self.pause_world(false);
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
        if self.java_owned() {
            return;
        }
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
            if self.music_voice.is_some()
                && let Some(file) = self.manager.last_sound()
            {
                let key = music_lang_key(file);
                self.push_music_event(MusicEvent::Started(key));
            }
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
            self.push_music_event(MusicEvent::Stopped);
        }
    }

    /// Drops the credits track when the credits screen closes.
    pub fn stop_credits_music(&mut self) {
        if self.music.is_playing(&Music::credits()) {
            self.stop_music(None);
        }
    }

    /// Track starts and stops since the last call, oldest first.
    fn push_music_event(&mut self, event: MusicEvent) {
        push_capped(&mut self.music_events, event, MAX_MUSIC_EVENTS);
    }

    pub fn take_music_events(&mut self) -> Vec<MusicEvent> {
        std::mem::take(&mut self.music_events)
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
    fn stop_sound_matches_by_name_and_source() {
        let mut book = VoiceBook::default();
        book.track(VoiceId(1), "entity.pig.ambient", SoundSource::Neutral);
        book.track(
            VoiceId(2),
            "minecraft:entity.pig.ambient",
            SoundSource::Hostile,
        );
        book.track(VoiceId(3), "music.game", SoundSource::Music);
        book.track(VoiceId(4), "block.chest.open", SoundSource::Blocks);
        assert_eq!(
            book.take_matching(
                Some("minecraft:entity.pig.ambient"),
                Some(SoundSource::Hostile)
            ),
            [VoiceId(2)]
        );
        assert_eq!(
            book.take_matching(None, Some(SoundSource::Music)),
            [VoiceId(3)]
        );
        assert_eq!(
            book.take_matching(Some("entity.pig.ambient"), None),
            [VoiceId(1)]
        );
        assert!(book.take_matching(Some("nothing"), None).is_empty());
        assert_eq!(book.take_matching(None, None), [VoiceId(4)]);
        assert!(book.tracked.is_empty());
    }

    #[test]
    fn bound_sounds_follow_their_entity_and_stop_with_it() {
        let mut book = VoiceBook::default();
        book.track(VoiceId(7), "entity.minecart.riding", SoundSource::Neutral);
        book.bind(VoiceId(7), 42);
        book.track(VoiceId(8), "entity.cat.purr", SoundSource::Neutral);
        book.bind(VoiceId(8), 43);
        let mut moved = Vec::new();
        let gone = book.follow(
            |e| (e == 42 || e == 43).then_some([f64::from(e), 64.0, 0.0]),
            |id, pos| moved.push((id, pos)),
        );
        assert!(gone.is_empty());
        assert_eq!(
            moved,
            [
                (VoiceId(7), [42.0, 64.0, 0.0]),
                (VoiceId(8), [43.0, 64.0, 0.0])
            ]
        );
        let gone = book.follow(|e| (e == 43).then_some([0.0; 3]), |_, _| {});
        assert_eq!(gone, [VoiceId(7)]);
        assert_eq!(book.bound, [(VoiceId(8), 43)]);
        assert_eq!(book.tracked.len(), 1);
        for i in 0..10_000 {
            book.track(VoiceId(i), "x", SoundSource::Blocks);
            book.bind(VoiceId(i), i as i32);
        }
        assert!(book.tracked.len() <= MAX_TRACKED && book.bound.len() <= MAX_BOUND);
    }

    #[test]
    fn undrained_music_events_stay_bounded() {
        let mut queue = Vec::new();
        for i in 0..10_000 {
            push_capped(
                &mut queue,
                MusicEvent::Started(i.to_string()),
                MAX_MUSIC_EVENTS,
            );
        }
        assert_eq!(queue.len(), MAX_MUSIC_EVENTS);
        assert!(matches!(&queue[MAX_MUSIC_EVENTS - 1], MusicEvent::Started(k) if k == "9999"));
    }

    #[test]
    fn music_files_map_to_language_keys() {
        assert_eq!(music_lang_key("music/game/clark"), "music.game.clark");
        assert_eq!(
            music_lang_key("minecraft:music/game/creative/aria_math"),
            "music.game.creative.aria_math"
        );
        assert_eq!(music_lang_key("pack:music/x"), "pack.music.x");
    }

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
