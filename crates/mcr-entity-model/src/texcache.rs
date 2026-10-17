use rustc_hash::{FxHashMap, FxHashSet};

use crate::assets::{ImageRgba, SceneAssets};
use crate::gpu::{EntityRenderer, TexId};

/// Idle frames after which a re-creatable `dyn:` texture gives its layer back.
pub const IDLE_FRAMES: u64 = 1800;
/// Bound on live re-creatable `dyn:` textures (skins, trims).
pub const MAX_EVICTABLE: usize = 384;
const EVICT_SLACK: usize = 32;
/// Unresolvable keys (inline painting variants are server-named) remembered as missing.
const MAX_MISSING_KEYS: usize = 4096;
const SWEEP_INTERVAL: u64 = 64;

#[derive(Debug, Clone, Copy)]
struct Entry {
    id: TexId,
    last_used: u64,
    shared: bool,
}

/// Texture keys are resource paths under `textures/` without extension; keys starting with `dyn:`
/// are composed at runtime (skins, trims, maps) and must be registered before use. Skins and
/// trims are re-registered by their callers every time they are planned, so they are evicted when
/// idle; maps are kept until [`TextureCache::remove`].
#[derive(Debug, Default)]
pub struct TextureCache {
    ids: FxHashMap<String, Entry>,
    registered: FxHashSet<String>,
    pending: Vec<(String, ImageRgba)>,
    missing: Option<TexId>,
    missing_keys: usize,
    evictable: usize,
    last_sweep: u64,
}

fn evictable_key(key: &str) -> bool {
    key.starts_with("dyn:skin:") || key.starts_with("dyn:trim:")
}

impl TextureCache {
    pub fn is_registered(&self, key: &str) -> bool {
        self.registered.contains(key) || self.ids.contains_key(key)
    }

    /// Queues a runtime-composed image; the first registration of a key wins.
    pub fn register(&mut self, key: &str, make: impl FnOnce() -> ImageRgba) {
        if !self.is_registered(key) {
            self.registered.insert(key.to_owned());
            self.pending.push((key.to_owned(), make()));
        }
    }

    /// Replaces the image of a key (changed skins); the old layer is freed on the next flush.
    pub fn replace(&mut self, key: &str, image: ImageRgba) {
        self.pending.retain(|(k, _)| k != key);
        self.registered.insert(key.to_owned());
        self.pending.push((key.to_owned(), image));
    }

    /// Drops a key and frees its texture layer.
    pub fn remove(&mut self, key: &str, gpu: &mut EntityRenderer) {
        self.registered.remove(key);
        self.pending.retain(|(k, _)| k != key);
        self.release(key, gpu);
    }

    /// Frame on which the key's texture was last resolved or uploaded.
    pub fn last_used(&self, key: &str) -> Option<u64> {
        self.ids.get(key).map(|e| e.last_used)
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    fn release(&mut self, key: &str, gpu: &mut EntityRenderer) {
        if let Some(entry) = self.ids.remove(key) {
            self.evictable -= usize::from(evictable_key(key));
            if entry.shared {
                self.missing_keys -= 1;
            } else {
                gpu.free_texture(entry.id);
            }
        }
    }

    fn insert(&mut self, key: String, id: TexId, shared: bool, frame: u64) {
        self.evictable += usize::from(evictable_key(&key));
        self.missing_keys += usize::from(shared);
        self.ids.insert(
            key,
            Entry {
                id,
                last_used: frame,
                shared,
            },
        );
    }

    /// Frees idle skin/trim textures and keeps their count under the cap; call once per frame.
    pub fn sweep(&mut self, gpu: &mut EntityRenderer) {
        let frame = gpu.frame();
        if frame.saturating_sub(self.last_sweep) < SWEEP_INTERVAL && self.evictable <= MAX_EVICTABLE
        {
            return;
        }
        self.last_sweep = frame;
        let idle: Vec<String> = self
            .ids
            .iter()
            .filter(|(k, e)| evictable_key(k) && frame.saturating_sub(e.last_used) >= IDLE_FRAMES)
            .map(|(k, _)| k.clone())
            .collect();
        for key in idle {
            self.registered.remove(&key);
            self.release(&key, gpu);
        }
        self.evict_over_cap(gpu, 0);
    }

    fn evict_over_cap(&mut self, gpu: &mut EntityRenderer, slack: usize) {
        if self.evictable + slack <= MAX_EVICTABLE {
            return;
        }
        let frame = gpu.frame();
        let mut candidates: Vec<(u64, String)> = self
            .ids
            .iter()
            .filter(|(k, e)| evictable_key(k) && e.last_used < frame)
            .map(|(k, e)| (e.last_used, k.clone()))
            .collect();
        candidates.sort_unstable();
        let target = MAX_EVICTABLE.saturating_sub(EVICT_SLACK.min(MAX_EVICTABLE));
        let excess = self.evictable.saturating_sub(target);
        for (_, key) in candidates.into_iter().take(excess) {
            self.registered.remove(&key);
            self.release(&key, gpu);
        }
    }

    pub(crate) fn flush(&mut self, gpu: &mut EntityRenderer) {
        if self.pending.is_empty() {
            return;
        }
        let frame = gpu.frame();
        for (key, image) in std::mem::take(&mut self.pending) {
            if evictable_key(&key) {
                self.evict_over_cap(gpu, 1);
            }
            let old = self.ids.get(&key).copied();
            match gpu.add_texture(&image) {
                Some(id) => {
                    if old.is_some() {
                        self.release(&key, gpu);
                    }
                    self.insert(key, id, false, frame);
                }
                None => {
                    self.registered.remove(&key);
                    self.release(&key, gpu);
                }
            }
        }
    }

    pub(crate) fn resolve(
        &mut self,
        key: &str,
        gpu: &mut EntityRenderer,
        assets: &SceneAssets,
    ) -> TexId {
        self.resolve_with(key, gpu, |k| assets.read_texture(k))
    }

    fn resolve_with(
        &mut self,
        key: &str,
        gpu: &mut EntityRenderer,
        read: impl FnOnce(&str) -> Option<ImageRgba>,
    ) -> TexId {
        let frame = gpu.frame();
        if let Some(entry) = self.ids.get_mut(key) {
            entry.last_used = frame;
            return entry.id;
        }
        let image = if key.starts_with("dyn:") {
            None
        } else {
            read(key).map(|mut img| {
                if (key.starts_with("block/") || key.starts_with("item/")) && img.height > img.width
                {
                    img.height = img.width;
                    img.rgba.truncate((img.width * img.width * 4) as usize);
                }
                img
            })
        };
        match image.and_then(|img| gpu.add_texture(&img)) {
            Some(id) => {
                self.insert(key.to_owned(), id, false, frame);
                id
            }
            None => {
                let id = *self.missing.get_or_insert_with(|| {
                    gpu.add_texture(&crate::assets::missing_image())
                        .expect("16x16 fits the smallest class")
                });
                if !key.starts_with("dyn:") && self.missing_keys < MAX_MISSING_KEYS {
                    self.insert(key.to_owned(), id, true, frame);
                }
                id
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::TargetFormats;

    fn renderer() -> Option<EntityRenderer> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()?;
        Some(EntityRenderer::new(
            &device,
            &queue,
            TargetFormats {
                color: wgpu::TextureFormat::Rgba8Unorm,
                depth: wgpu::TextureFormat::Depth32Float,
                reverse_z: true,
                sample_count: 1,
            },
        ))
    }

    fn skin(n: u32) -> ImageRgba {
        ImageRgba::solid(64, 64, [n as u8, (n >> 8) as u8, 0, 255])
    }

    #[test]
    fn unresolvable_server_keys_stay_bounded() {
        let Some(mut gpu) = renderer() else { return };
        let mut cache = TextureCache::default();
        let first = cache.resolve_with("painting/hostile", &mut gpu, |_| None);
        for n in 0..20_000u32 {
            let id = cache.resolve_with(&format!("painting/hostile_{n}"), &mut gpu, |_| None);
            assert!(id == first);
        }
        assert_eq!(cache.len(), MAX_MISSING_KEYS);
        assert_eq!(gpu.live_layers(0), 1);
    }

    #[test]
    fn churning_dyn_keys_stay_bounded_and_live_keys_resolve() {
        let Some(mut gpu) = renderer() else { return };
        let mut cache = TextureCache::default();
        let missing = {
            let mut probe = TextureCache::default();
            probe.resolve_with("nothing", &mut gpu, |_| None)
        };
        for n in 0..10_000u32 {
            gpu.begin_frame();
            cache.sweep(&mut gpu);
            let key = format!("dyn:skin:{n}");
            cache.register(&key, || skin(n));
            cache.flush(&mut gpu);
            let id = cache.resolve_with(&key, &mut gpu, |_| None);
            assert_ne!(id, missing, "live key {key} fell back to missing");
            if n >= 3 {
                let older = format!("dyn:skin:{}", n - 3);
                let id = cache.resolve_with(&older, &mut gpu, |_| None);
                assert_ne!(id, missing, "recent key {older} fell back to missing");
            }
            assert!(cache.evictable <= MAX_EVICTABLE + 1);
            assert!(gpu.live_layers(0) as usize <= MAX_EVICTABLE + 8);
        }
        assert!(gpu.allocated_layers(0) as usize <= MAX_EVICTABLE + 64);
    }

    #[test]
    fn replace_frees_the_old_layer_and_reuse_waits_for_frames_in_flight() {
        let Some(mut gpu) = renderer() else { return };
        let mut cache = TextureCache::default();
        gpu.begin_frame();
        cache.replace("dyn:map/1", skin(1));
        cache.flush(&mut gpu);
        let first = cache.resolve_with("dyn:map/1", &mut gpu, |_| None);
        cache.replace("dyn:map/1", skin(2));
        cache.flush(&mut gpu);
        let second = cache.resolve_with("dyn:map/1", &mut gpu, |_| None);
        assert_ne!(first.layer, second.layer);
        let third = gpu.add_texture(&skin(3)).expect("layer");
        assert_ne!(
            third.layer, first.layer,
            "layer reused within the same frame"
        );
        for _ in 0..8 {
            gpu.begin_frame();
        }
        let fourth = gpu.add_texture(&skin(4)).expect("layer");
        assert_eq!(fourth.layer, first.layer);
        assert_eq!(gpu.live_layers(0), 3);
    }

    #[test]
    fn double_free_does_not_duplicate_a_layer() {
        let Some(mut gpu) = renderer() else { return };
        let id = gpu.add_texture(&skin(1)).expect("layer");
        gpu.free_texture(id);
        gpu.free_texture(id);
        for _ in 0..8 {
            gpu.begin_frame();
        }
        let a = gpu.add_texture(&skin(2)).expect("layer");
        let b = gpu.add_texture(&skin(3)).expect("layer");
        assert_ne!(a.layer, b.layer);
    }
}
