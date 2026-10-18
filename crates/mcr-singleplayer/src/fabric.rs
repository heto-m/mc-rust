//! A Fabric dedicated server for a modded singleplayer world: Fabric's server launcher over the same
//! server jar, with the mod set's server-side jars.

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::{Result, SpError};

/// Written under the cache by `cargo xtask fabric-fetch` or `fabric-server`; paths are cache-relative.
pub const SERVER_MANIFEST: &str = "fabric/server.json";
const MAX_MANIFEST_BYTES: u64 = 1 << 20;
const MAX_CLASS_PATH: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FabricServer {
    pub main_class: String,
    pub class_path: Vec<PathBuf>,
}

impl FabricServer {
    /// The profile under `cache`, with every library present.
    pub fn load(cache: &Path) -> Result<Self> {
        let path = cache.join(SERVER_MANIFEST);
        if std::fs::metadata(&path)?.len() > MAX_MANIFEST_BYTES {
            return Err(SpError::TooLarge(
                path.display().to_string(),
                MAX_MANIFEST_BYTES,
            ));
        }
        let server = Self::parse(&std::fs::read_to_string(&path)?, cache)?;
        if let Some(missing) = server.class_path.iter().find(|p| !p.is_file()) {
            return Err(SpError::data(format!("{} is missing", missing.display())));
        }
        Ok(server)
    }

    /// `loader_version`, when given, must match the manifest's.
    pub fn parse_for(text: &str, cache: &Path, loader_version: Option<&str>) -> Result<Self> {
        let root: Value = serde_json::from_str(text)
            .map_err(|e| SpError::data(format!("{SERVER_MANIFEST}: {e}")))?;
        if root["format"].as_u64() != Some(1) {
            return Err(SpError::data(format!("{SERVER_MANIFEST}: unknown format")));
        }
        if loader_version.is_some_and(|v| root["loader_version"].as_str() != Some(v)) {
            return Err(SpError::data(format!(
                "{SERVER_MANIFEST}: another loader version"
            )));
        }
        let entries = root["class_path"]
            .as_array()
            .filter(|a| a.len() <= MAX_CLASS_PATH)
            .ok_or_else(|| SpError::data(format!("{SERVER_MANIFEST}: no class_path")))?;
        let class_path = entries
            .iter()
            .map(|v| {
                let rel = Path::new(v.as_str().unwrap_or_default());
                let inside = !rel.as_os_str().is_empty()
                    && rel.components().all(|c| matches!(c, Component::Normal(_)));
                if inside {
                    Ok(cache.join(rel))
                } else {
                    Err(SpError::data(format!(
                        "{SERVER_MANIFEST}: a path leaves the cache"
                    )))
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let main_class = root["main_class"]
            .as_str()
            .filter(|m| !m.is_empty() && m.len() <= 256)
            .ok_or_else(|| SpError::data(format!("{SERVER_MANIFEST}: no main_class")))?
            .to_owned();
        Ok(Self {
            main_class,
            class_path,
        })
    }

    pub fn parse(text: &str, cache: &Path) -> Result<Self> {
        Self::parse_for(text, cache, None)
    }
}

/// The `-Dfabric.addMods=@file` list: one absolute jar path per line.
pub fn mods_list(jars: &[PathBuf]) -> Result<String> {
    let mut out = String::new();
    for jar in jars {
        let abs = std::path::absolute(jar)?;
        let text = abs
            .to_str()
            .filter(|t| !t.contains('\n'))
            .ok_or_else(|| SpError::data(format!("{} is not a usable path", abs.display())))?;
        out.push_str(text);
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_paths_stay_in_the_cache() {
        let ok = r#"{"format":1,"loader_version":"0.19.5","main_class":"K","class_path":["fabric/a.jar"]}"#;
        let p = FabricServer::parse_for(ok, Path::new("/c"), Some("0.19.5")).unwrap();
        assert_eq!(p.class_path, [PathBuf::from("/c/fabric/a.jar")]);
        assert_eq!(p.main_class, "K");
        for bad in ["/abs.jar", "../x.jar", "fabric/../../x.jar", ""] {
            let text = ok.replace("fabric/a.jar", bad);
            assert!(
                FabricServer::parse(&text, Path::new("/c")).is_err(),
                "{bad}"
            );
        }
        assert!(FabricServer::parse_for(ok, Path::new("/c"), Some("0.0.1")).is_err());
        assert!(FabricServer::parse(r#"{"format":2}"#, Path::new("/c")).is_err());
    }

    #[test]
    fn mods_list_is_one_absolute_path_per_line() {
        let text = mods_list(&[PathBuf::from("/m/a.jar"), PathBuf::from("/m/b c.jar")]).unwrap();
        assert_eq!(text, "/m/a.jar\n/m/b c.jar\n");
    }
}
