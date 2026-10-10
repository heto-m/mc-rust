use rustc_hash::{FxHashMap, FxHashSet};

use crate::assets::{ImageRgba, SceneAssets};
use crate::gpu::{EntityRenderer, TexId};

/// Texture keys are resource paths under `textures/` without extension; keys starting with `dyn:`
/// are composed at runtime (skins, trims) and must be registered before use.
#[derive(Debug, Default)]
pub struct TextureCache {
    ids: FxHashMap<String, TexId>,
    registered: FxHashSet<String>,
    pending: Vec<(String, ImageRgba)>,
    missing: Option<TexId>,
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

    /// Replaces the image of a key (changed skins).
    pub fn replace(&mut self, key: &str, image: ImageRgba) {
        self.ids.remove(key);
        self.registered.insert(key.to_owned());
        self.pending.push((key.to_owned(), image));
    }

    pub(crate) fn flush(&mut self, gpu: &mut EntityRenderer) {
        for (key, image) in self.pending.drain(..) {
            if let Some(id) = gpu.add_texture(&image) {
                self.ids.insert(key, id);
            }
        }
    }

    pub(crate) fn resolve(
        &mut self,
        key: &str,
        gpu: &mut EntityRenderer,
        assets: &SceneAssets,
    ) -> TexId {
        if let Some(id) = self.ids.get(key) {
            return *id;
        }
        let image = if key.starts_with("dyn:") {
            None
        } else {
            assets.read_texture(key).map(|mut img| {
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
                self.ids.insert(key.to_owned(), id);
                id
            }
            None => {
                let id = *self.missing.get_or_insert_with(|| {
                    gpu.add_texture(&crate::assets::missing_image())
                        .expect("16x16 fits the smallest class")
                });
                if !key.starts_with("dyn:") {
                    self.ids.insert(key.to_owned(), id);
                }
                id
            }
        }
    }
}
