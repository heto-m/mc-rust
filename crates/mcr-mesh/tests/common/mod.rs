#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use mcr_assets::ResourceManager;
use mcr_mesh::{BiomeColors, ColorMaps, ModelCache};

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

pub fn biome_dir() -> PathBuf {
    cache_dir().join("generated/data/minecraft/worldgen/biome")
}

pub fn jar_available() -> bool {
    jar_path().exists()
}

pub struct Fixture {
    pub resources: ResourceManager,
    pub cache: Arc<ModelCache>,
    pub colors: Arc<BiomeColors>,
    pub biome_names: Vec<String>,
}

pub fn fixture() -> Option<&'static Fixture> {
    static FIXTURE: OnceLock<Option<Fixture>> = OnceLock::new();
    FIXTURE
        .get_or_init(|| {
            if !jar_available() {
                eprintln!("client jar missing, skipping");
                return None;
            }
            let resources = ResourceManager::vanilla(jar_path()).expect("open jar");
            let cache = ModelCache::build(&resources, mcr_data::v26_3()).expect("bake");
            let mut names: Vec<String> = std::fs::read_dir(biome_dir())
                .map(|d| {
                    d.filter_map(|e| e.ok())
                        .filter_map(|e| {
                            e.file_name()
                                .to_str()
                                .and_then(|n| n.strip_suffix(".json"))
                                .map(|n| format!("minecraft:{n}"))
                        })
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let maps = ColorMaps::load(&resources);
            let colors = BiomeColors::from_json_dir(&biome_dir(), &refs, &maps);
            Some(Fixture {
                resources,
                cache: Arc::new(cache),
                colors: Arc::new(colors),
                biome_names: names,
            })
        })
        .as_ref()
}

pub fn state_of(name: &str, props: &[(&str, &str)]) -> u16 {
    let v = mcr_data::v26_3();
    let block = v.block_by_name(name).expect("block");
    let info = v.block_info(block).expect("info");
    (info.states.0..info.states.1)
        .find(|&s| {
            let have: Vec<_> = v.state_properties(s).collect();
            props.iter().all(|p| have.contains(p))
        })
        .expect("state") as u16
}
