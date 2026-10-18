use std::path::{Path, PathBuf};

use mcr_protocol::version::{SUPPORTED_VERSIONS, is_supported};
use serde_json::Value;

use crate::profile::{LATEST_RELEASE, valid_version_id};
use crate::store::write_atomic;

/// Matches the singleplayer manifest download limit.
pub const MAX_MANIFEST_BYTES: u64 = 8 << 20;
pub const MAX_VERSIONS: usize = 4096;
pub const CACHE_FILE: &str = "cache/version_manifest_v2.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionKind {
    Release,
    Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionEntry {
    pub id: String,
    pub kind: VersionKind,
    pub supported: bool,
}

/// Releases and snapshots, newest first, as the manifest orders them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionList {
    pub latest_release: Option<String>,
    pub entries: Vec<VersionEntry>,
    /// Whether this came from a manifest rather than mcr's own list.
    pub from_manifest: bool,
}

impl Default for VersionList {
    fn default() -> Self {
        Self::builtin()
    }
}

impl VersionList {
    /// Only what mcr supports, for a start without any manifest.
    pub fn builtin() -> Self {
        Self {
            latest_release: None,
            entries: SUPPORTED_VERSIONS
                .iter()
                .map(|id| VersionEntry {
                    id: (*id).to_owned(),
                    kind: VersionKind::Release,
                    supported: true,
                })
                .collect(),
            from_manifest: false,
        }
    }

    /// Mojang's `version_manifest_v2.json`, bounded; old alpha and beta are left out.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return None;
        }
        let root: Value = serde_json::from_slice(bytes).ok()?;
        let list = root["versions"].as_array()?;
        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for v in list.iter().take(MAX_VERSIONS) {
            let Some(id) = v["id"].as_str().filter(|id| valid_version_id(id)) else {
                continue;
            };
            let kind = match v["type"].as_str() {
                Some("release") => VersionKind::Release,
                Some("snapshot") => VersionKind::Snapshot,
                _ => continue,
            };
            if !seen.insert(id) {
                continue;
            }
            entries.push(VersionEntry {
                id: id.to_owned(),
                kind,
                supported: is_supported(id),
            });
        }
        for (i, id) in SUPPORTED_VERSIONS.iter().enumerate() {
            if !entries.iter().any(|e| e.id == *id) {
                entries.insert(
                    i.min(entries.len()),
                    VersionEntry {
                        id: (*id).to_owned(),
                        kind: VersionKind::Release,
                        supported: true,
                    },
                );
            }
        }
        let latest_release = root["latest"]["release"]
            .as_str()
            .filter(|id| valid_version_id(id))
            .map(str::to_owned);
        Some(Self {
            latest_release,
            entries,
            from_manifest: true,
        })
    }

    pub fn visible(&self, snapshots: bool) -> impl Iterator<Item = &VersionEntry> {
        self.entries
            .iter()
            .filter(move |e| snapshots || e.kind == VersionKind::Release)
    }

    pub fn latest_supported_release(&self) -> &str {
        self.entries
            .iter()
            .find(|e| e.supported && e.kind == VersionKind::Release)
            .map_or(SUPPORTED_VERSIONS[0], |e| e.id.as_str())
    }

    /// The newest release when mcr cannot play it yet.
    pub fn newer_unsupported_release(&self) -> Option<&str> {
        self.latest_release
            .as_deref()
            .filter(|id| !is_supported(id))
    }

    /// The version a profile plays, or `None` when mcr cannot play it.
    pub fn resolve<'a>(&'a self, version: &'a str) -> Option<&'a str> {
        if version == LATEST_RELEASE {
            Some(self.latest_supported_release())
        } else {
            is_supported(version).then_some(version)
        }
    }
}

/// The last manifest fetched, for offline starts.
#[derive(Debug, Clone)]
pub struct ManifestCache {
    path: PathBuf,
}

impl ManifestCache {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join(CACHE_FILE),
        }
    }

    pub fn load(&self) -> Option<VersionList> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(&self.path)
            .ok()?
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        VersionList::parse(&bytes)
    }

    /// Parses `bytes` and only then replaces the cache with them.
    pub fn store(&self, bytes: &[u8]) -> Option<VersionList> {
        let list = VersionList::parse(bytes)?;
        if let Err(e) = write_atomic(&self.path, bytes) {
            eprintln!("could not cache the version manifest: {e}");
        }
        Some(list)
    }
}

#[cfg(test)]
mod tests {
    use mcr_protocol::version::GAME_VERSION;

    use super::*;

    fn manifest() -> String {
        r#"{"latest":{"release":"26.4","snapshot":"26w50a"},"versions":[
            {"id":"26w50a","type":"snapshot"},
            {"id":"26.4","type":"release"},
            {"id":"26.3","type":"release"},
            {"id":"26.2","type":"release"},
            {"id":"26.2","type":"release"},
            {"id":"b1.7.3","type":"old_beta"},
            {"id":"../evil","type":"release"},
            {"id":"weird","type":"pending"}]}"#
            .to_owned()
    }

    #[test]
    fn releases_snapshots_and_support() {
        let list = VersionList::parse(manifest().as_bytes()).unwrap();
        let releases: Vec<_> = list.visible(false).map(|e| e.id.as_str()).collect();
        assert_eq!(releases, ["26.4", "26.3", "26.2"]);
        assert_eq!(list.visible(true).count(), 4);
        let supported: Vec<_> = list.entries.iter().filter(|e| e.supported).collect();
        let listed = ["26.3", "26.2"];
        assert_eq!(
            supported.len(),
            listed.iter().filter(|v| is_supported(v)).count()
        );
        assert_eq!(supported[0].id, GAME_VERSION);
        assert_eq!(list.latest_supported_release(), GAME_VERSION);
        assert_eq!(list.newer_unsupported_release(), Some("26.4"));
        assert_eq!(list.resolve(LATEST_RELEASE), Some(GAME_VERSION));
        assert_eq!(list.resolve("26.2"), is_supported("26.2").then_some("26.2"));
        assert_eq!(list.resolve("b1.7.3"), None);
    }

    #[test]
    fn hostile_manifests() {
        assert!(VersionList::parse(b"{").is_none());
        assert!(VersionList::parse(b"{}").is_none());
        let huge = format!(
            r#"{{"versions":[{}]}}"#,
            vec![r#"{"id":"1.0","type":"release"}"#; 10_000].join(",")
        );
        let list = VersionList::parse(huge.as_bytes()).unwrap();
        assert_eq!(list.entries.len(), 1 + SUPPORTED_VERSIONS.len());
        let many: Vec<String> = (0..10_000)
            .map(|i| format!(r#"{{"id":"v{i}","type":"snapshot"}}"#))
            .collect();
        let list = VersionList::parse(format!(r#"{{"versions":[{}]}}"#, many.join(",")).as_bytes())
            .unwrap();
        assert_eq!(list.entries.len(), MAX_VERSIONS + SUPPORTED_VERSIONS.len());
        assert!(list.entries.iter().any(|e| e.id == GAME_VERSION));
    }

    #[test]
    fn builtin_without_manifest() {
        let list = VersionList::builtin();
        assert!(!list.from_manifest);
        assert_eq!(list.latest_supported_release(), GAME_VERSION);
        assert_eq!(list.newer_unsupported_release(), None);
    }

    #[test]
    fn cache_keeps_only_parsable_manifests() {
        let dir = crate::accounts::tests::temp_dir("manifest");
        let cache = ManifestCache::new(&dir);
        assert!(cache.load().is_none());
        assert!(cache.store(b"not json").is_none());
        assert!(cache.load().is_none());
        cache.store(manifest().as_bytes()).unwrap();
        assert_eq!(
            cache.load().unwrap().latest_release.as_deref(),
            Some("26.4")
        );
        assert!(cache.store(b"[]").is_none());
        assert!(cache.load().is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
