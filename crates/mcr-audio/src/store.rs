use std::fs;
use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;
use serde::Deserialize;

use crate::error::AudioError;

#[derive(Deserialize)]
struct Index {
    objects: FxHashMap<String, IndexEntry>,
}

#[derive(Deserialize)]
struct IndexEntry {
    hash: String,
}

/// Launcher-layout asset store: `<root>/indexes/<id>.json` and `<root>/objects/<hh>/<hash>`.
#[derive(Debug)]
pub struct AssetStore {
    root: PathBuf,
    hashes: FxHashMap<String, String>,
}

impl AssetStore {
    pub fn open(root: impl Into<PathBuf>, index_id: &str) -> Result<Self, AudioError> {
        let root = root.into();
        let path = root.join("indexes").join(format!("{index_id}.json"));
        let text = fs::read_to_string(&path).map_err(|source| AudioError::Io { path, source })?;
        Self::from_index(root, &text)
    }

    pub fn open_latest(root: impl Into<PathBuf>) -> Result<Self, AudioError> {
        let root = root.into();
        let dir = root.join("indexes");
        let io = |source| AudioError::Io {
            path: dir.clone(),
            source,
        };
        let mut best: Option<(u64, String)> = None;
        for entry in fs::read_dir(&dir).map_err(io)? {
            let name = entry
                .map_err(io)?
                .file_name()
                .to_string_lossy()
                .into_owned();
            let Some(stem) = name.strip_suffix(".json") else {
                continue;
            };
            let rank = stem.parse::<u64>().unwrap_or(0);
            if best.as_ref().is_none_or(|(r, _)| rank > *r) {
                best = Some((rank, stem.to_string()));
            }
        }
        let (_, id) = best.ok_or_else(|| AudioError::MissingAsset("asset index".into()))?;
        Self::open(root, &id)
    }

    pub fn from_index(root: PathBuf, index_json: &str) -> Result<Self, AudioError> {
        let index: Index = serde_json::from_str(index_json)?;
        let hashes = index
            .objects
            .into_iter()
            .map(|(name, e)| (name, e.hash))
            .collect();
        Ok(Self { root, hashes })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>, AudioError> {
        let hash = self
            .hashes
            .get(name)
            .filter(|h| h.len() >= 2)
            .ok_or_else(|| AudioError::MissingAsset(name.to_string()))?;
        let path = self.root.join("objects").join(&hash[..2]).join(hash);
        fs::read(&path).map_err(|source| AudioError::Io { path, source })
    }

    pub fn sounds_json(&self) -> Result<Vec<u8>, AudioError> {
        self.read("minecraft/sounds.json")
    }

    pub fn sound_file(&self, sound_name: &str) -> Result<Vec<u8>, AudioError> {
        self.read(&sound_asset_name(sound_name))
    }
}

/// `block/stone/step1` -> `minecraft/sounds/block/stone/step1.ogg`; `ns:x` uses namespace `ns`.
pub fn sound_asset_name(sound_name: &str) -> String {
    let (ns, path) = sound_name
        .split_once(':')
        .unwrap_or(("minecraft", sound_name));
    format!("{ns}/sounds/{path}.ogg")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_names_follow_namespace() {
        assert_eq!(
            sound_asset_name("block/stone/step1"),
            "minecraft/sounds/block/stone/step1.ogg"
        );
        assert_eq!(sound_asset_name("mod:a/b"), "mod/sounds/a/b.ogg");
    }

    #[test]
    fn reads_object_by_hash_path() {
        let root = std::env::temp_dir().join(format!("mcr-audio-store-{}", std::process::id()));
        fs::create_dir_all(root.join("objects/ab")).unwrap();
        fs::write(root.join("objects/ab/abcdef"), b"data").unwrap();
        let store = AssetStore::from_index(
            root.clone(),
            r#"{"objects":{"minecraft/sounds/x.ogg":{"hash":"abcdef","size":4}}}"#,
        )
        .unwrap();
        assert_eq!(store.sound_file("x").unwrap(), b"data");
        assert!(matches!(
            store.sound_file("y"),
            Err(AudioError::MissingAsset(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }
}
