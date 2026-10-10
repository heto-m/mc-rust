use std::path::Path;

use mcr_audio::{AssetStore, AudioEngine, Listener, SoundManager, SoundSource};
use mcr_data::Version;
use mcr_protocol::play::clientbound::world::SoundSource as WireSource;
use mcr_settings::{Options, SOUND_CATEGORIES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockSound {
    Break,
    Place,
    Step,
    Hit,
}

impl BlockSound {
    fn suffix(self) -> &'static str {
        match self {
            Self::Break => "break",
            Self::Place => "place",
            Self::Step => "step",
            Self::Hit => "hit",
        }
    }

    fn volume_pitch(self) -> (f32, f32) {
        match self {
            Self::Break | Self::Place => (0.5, 0.8),
            Self::Step => (0.15, 1.0),
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

pub struct Audio {
    manager: SoundManager,
    _engine: AudioEngine,
    seed: i64,
    data: &'static Version,
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
        Some(Self {
            manager,
            _engine: engine,
            seed: 1,
            data,
        })
    }

    pub fn set_listener(&mut self, position: [f64; 3], yaw_degrees: f32) {
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
    ) {
        self.seed = self
            .seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let name = event.strip_prefix("minecraft:").unwrap_or(event);
        self.manager.play(
            name,
            position.map(|p| p.map(|v| v as f32)),
            volume,
            pitch,
            category,
            self.seed,
        );
    }

    pub fn play_block(&mut self, state: u32, kind: BlockSound, pos: [f64; 3]) {
        let Some(event) = block_event(self.data, state, kind) else {
            return;
        };
        let sound_type_volume = 1.0;
        let (v, p) = kind.volume_pitch();
        let volume = match kind {
            BlockSound::Break | BlockSound::Place => (sound_type_volume + 1.0) * v,
            BlockSound::Hit => (sound_type_volume + 1.0) * v,
            BlockSound::Step => sound_type_volume * v,
        };
        self.play(&event, Some(pos), volume, p, SoundSource::Blocks);
    }
}

fn block_event(data: &Version, state: u32, kind: BlockSound) -> Option<String> {
    let block = data.block_state(state)?.block;
    let info = data.block_info(block)?;
    let ty = info.sound_type;
    if ty.is_empty() {
        return None;
    }
    let family = ty
        .strip_prefix("minecraft:")
        .unwrap_or(ty)
        .strip_suffix(".break")?;
    Some(format!("{family}.{}", kind.suffix()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_uses_the_stone_events() {
        let data = mcr_data::v26_3();
        let id = data.block_by_name("minecraft:stone").unwrap();
        let state = data.block_info(id).unwrap().default_state;
        assert_eq!(
            block_event(data, state, BlockSound::Break).as_deref(),
            Some("block.stone.break")
        );
        assert_eq!(
            block_event(data, state, BlockSound::Step).as_deref(),
            Some("block.stone.step")
        );
    }
}
