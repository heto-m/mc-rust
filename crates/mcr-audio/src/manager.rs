use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::category::SoundSource;
use crate::decode::{Pcm, decode_ogg, open_stream};
use crate::error::AudioError;
use crate::events::{JavaRandom, SoundEvents};
use crate::mixer::{AudioHandle, Listener, PlayRequest, Source, Spatial, VoiceId};
use crate::store::{AssetStore, SoundOverlay};

#[derive(Debug)]
pub struct SoundManager {
    store: AssetStore,
    events: SoundEvents,
    handle: AudioHandle,
    cache: FxHashMap<String, Arc<Pcm>>,
}

impl SoundManager {
    pub fn new(store: AssetStore, handle: AudioHandle) -> Result<Self, AudioError> {
        let events = SoundEvents::parse(&String::from_utf8_lossy(&store.sounds_json()?))?;
        Ok(Self::with_events(store, events, handle))
    }

    pub fn with_events(store: AssetStore, events: SoundEvents, handle: AudioHandle) -> Self {
        Self {
            store,
            events,
            handle,
            cache: FxHashMap::default(),
        }
    }

    /// Re-reads the sound definitions and files with `overlay` (resource packs) layered on top.
    pub fn reload(&mut self, overlay: Option<SoundOverlay>, packs: &[(String, String)]) {
        let mut events = self
            .store
            .sounds_json()
            .ok()
            .and_then(|b| SoundEvents::parse(&String::from_utf8_lossy(&b)).ok())
            .unwrap_or_default();
        for (namespace, json) in packs {
            if let Err(e) = events.merge_namespaced(namespace, json) {
                tracing::warn!("pack sounds.json for {namespace}: {e}");
            }
        }
        self.store.set_overlay(overlay);
        self.events = events;
        self.cache.clear();
    }

    pub fn events(&self) -> &SoundEvents {
        &self.events
    }

    pub fn handle(&mut self) -> &mut AudioHandle {
        &mut self.handle
    }

    pub fn set_listener(&mut self, listener: Listener) {
        self.handle.set_listener(listener);
    }

    pub fn set_category_volume(&mut self, category: SoundSource, volume: f32) {
        self.handle.set_category_volume(category, volume);
    }

    /// Decodes every `preload: true` sound so the first play does not hit the disk.
    pub fn preload(&mut self) -> usize {
        let names: Vec<String> = self.events.preloads().map(str::to_string).collect();
        names.iter().filter(|n| self.cached(n).is_ok()).count()
    }

    /// Returns `None` for unknown events, missing or undecodable files, or a full command queue.
    pub fn play(
        &mut self,
        event: &str,
        position: Option<[f32; 3]>,
        volume: f32,
        pitch: f32,
        category: SoundSource,
        seed: i64,
    ) -> Option<VoiceId> {
        let sound = self.events.pick(event, &mut JavaRandom::new(seed))?;
        let volume = volume * sound.volume;
        let pitch = pitch * sound.pitch;
        let spatial = match position {
            Some(position) => Spatial::Positioned {
                position,
                max_distance: volume.max(1.0) * sound.attenuation_distance,
            },
            None => Spatial::Global,
        };
        let source = if sound.stream {
            let bytes = self.store.sound_file(&sound.name).ok()?;
            Source::Stream(open_stream(bytes).ok()?)
        } else {
            Source::Static(self.cached(&sound.name).ok()?)
        };
        self.handle.play(PlayRequest {
            source,
            volume,
            pitch,
            category,
            spatial,
            looping: false,
        })
    }

    fn cached(&mut self, name: &str) -> Result<Arc<Pcm>, AudioError> {
        if let Some(pcm) = self.cache.get(name) {
            return Ok(pcm.clone());
        }
        let pcm = Arc::new(decode_ogg(self.store.sound_file(name)?)?);
        self.cache.insert(name.to_string(), pcm.clone());
        Ok(pcm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mixer::Mixer;

    fn manager(events: &str) -> (SoundManager, Mixer) {
        let (mixer, handle) = Mixer::with_queue(48_000);
        let store = AssetStore::from_index(std::env::temp_dir(), r#"{"objects":{}}"#).unwrap();
        let events = SoundEvents::parse(events).unwrap();
        (SoundManager::with_events(store, events, handle), mixer)
    }

    #[test]
    fn unknown_event_plays_nothing() {
        let (mut m, mut mixer) = manager(r#"{"a":{"sounds":["x"]}}"#);
        assert!(
            m.play("nope", None, 1.0, 1.0, SoundSource::Blocks, 1)
                .is_none()
        );
        mixer.render(&mut [0.0; 512]);
        assert_eq!(mixer.active_voices(), 0);
    }

    #[test]
    fn missing_file_is_skipped_quietly() {
        let (mut m, _) = manager(r#"{"a":{"sounds":["x"]}}"#);
        assert!(
            m.play("a", None, 1.0, 1.0, SoundSource::Blocks, 1)
                .is_none()
        );
    }
}
