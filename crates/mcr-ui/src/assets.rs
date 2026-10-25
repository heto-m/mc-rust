use std::path::{Path, PathBuf};

use mcr_assets::{AssetError, AssetSource, ResourceManager};
use rustc_hash::FxHashMap;
use serde_json::Value;

use crate::error::{Result, UiError};

const FONT_PREFIX: &str = "minecraft/font/";
const LANG_PREFIX: &str = "minecraft/lang/";
const PANORAMA_PREFIX: &str = "minecraft/textures/gui/title/background/";
const PACK_META: &str = "pack.mcmeta";

/// Files the client jar omits (`unifont.zip`, other languages, the language list, the title panorama) live in the launcher asset index.
#[derive(Debug)]
pub struct AssetIndexSource {
    objects_dir: PathBuf,
    hashes: FxHashMap<String, String>,
}

impl AssetIndexSource {
    pub fn open(assets_dir: &Path, index: &str) -> Result<Self> {
        let path = assets_dir.join("indexes").join(format!("{index}.json"));
        let bytes = std::fs::read(&path).map_err(AssetError::from)?;
        let json: Value = serde_json::from_slice(&bytes).map_err(|source| UiError::Json {
            path: path.display().to_string(),
            source,
        })?;
        let hashes = json
            .get("objects")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter(|(key, _)| {
                [FONT_PREFIX, LANG_PREFIX, PANORAMA_PREFIX]
                    .iter()
                    .any(|prefix| key.starts_with(prefix))
                    || *key == PACK_META
            })
            .filter_map(|(key, obj)| Some((key.clone(), obj.get("hash")?.as_str()?.to_owned())))
            .collect();
        Ok(Self {
            objects_dir: assets_dir.join("objects"),
            hashes,
        })
    }
}

impl AssetSource for AssetIndexSource {
    fn read(&self, path: &str) -> mcr_assets::Result<Option<Vec<u8>>> {
        let key = if path == PACK_META {
            Some(PACK_META)
        } else {
            path.strip_prefix("assets/")
        };
        let Some(hash) = key.and_then(|key| self.hashes.get(key)) else {
            return Ok(None);
        };
        match std::fs::read(self.objects_dir.join(&hash[..2]).join(hash)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn list(&self) -> mcr_assets::Result<Vec<String>> {
        Ok(self
            .hashes
            .keys()
            .map(|k| {
                if k == PACK_META {
                    k.clone()
                } else {
                    format!("assets/{k}")
                }
            })
            .collect())
    }
}

/// The client jar, plus the launcher asset index (`assets_dir`, index id) when available.
pub fn vanilla_resources(
    jar: &Path,
    asset_index: Option<(&Path, &str)>,
) -> Result<ResourceManager> {
    let mut resources = ResourceManager::vanilla(jar)?;
    if let Some((dir, index)) = asset_index {
        match AssetIndexSource::open(dir, index) {
            Ok(source) => resources.push_pack("asset-index", Box::new(source))?,
            Err(e) => tracing::warn!("asset index unavailable: {e}"),
        }
    }
    Ok(resources)
}
