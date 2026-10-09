#![allow(dead_code)]

use std::path::PathBuf;

use mcr_assets::ResourceManager;
use serde_json::Value;

fn cache_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .map(|dir| dir.join(".cache/versions/26.3"))
        .find(|dir| dir.exists())
        .unwrap_or_else(|| manifest.join("../../.cache/versions/26.3"))
}

pub fn jar_path() -> PathBuf {
    std::env::var_os("MCR_CLIENT_JAR")
        .map(PathBuf::from)
        .unwrap_or_else(|| cache_dir().join("client.jar"))
}

pub fn report_path() -> PathBuf {
    std::env::var_os("MCR_BLOCKS_REPORT")
        .map(PathBuf::from)
        .unwrap_or_else(|| cache_dir().join("generated/reports/blocks.json"))
}

pub fn vanilla() -> ResourceManager {
    ResourceManager::vanilla(jar_path()).expect("open client jar")
}

pub struct BlockStates {
    pub name: String,
    pub states: Vec<Vec<(String, String)>>,
}

/// Every block with every state, from the data generator's blocks report.
pub fn all_block_states() -> Vec<BlockStates> {
    let bytes = std::fs::read(report_path()).expect("read blocks report");
    let root: Value = serde_json::from_slice(&bytes).expect("parse blocks report");
    root.as_object()
        .expect("report object")
        .iter()
        .map(|(name, block)| BlockStates {
            name: name.clone(),
            states: block["states"]
                .as_array()
                .expect("states")
                .iter()
                .map(|s| {
                    s["properties"]
                        .as_object()
                        .map(|p| {
                            p.iter()
                                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect(),
        })
        .collect()
}

pub fn as_pairs(props: &[(String, String)]) -> Vec<(&str, &str)> {
    props
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}
