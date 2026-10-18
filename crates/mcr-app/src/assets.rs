use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use mcr_assets::{AssetSource, ResourceManager, ZipSource};
use mcr_client::BuiltinData;
use mcr_entity_model::SceneAssets;
use mcr_mesh::{BiomeClimate, BiomeColors, ColorMaps, ModelCache};

const BIOME: &str = "minecraft:worldgen/biome";

/// Registry entries from the vanilla data in the client jar; resource packs never supply them.
pub fn jar_registry_data(jar: &Path) -> BuiltinData {
    let source = match ZipSource::open(jar) {
        Ok(source) => Some(source),
        Err(e) => {
            eprintln!(
                "assets: no built-in registry data from {}: {e}",
                jar.display()
            );
            None
        }
    };
    BuiltinData::new(move |registry, entry| {
        let path = mcr_client::data_pack_path(registry, entry)?;
        let bytes = source.as_ref()?.read(&path).ok()??;
        mcr_nbt::json::compound_from_slice(&bytes)
    })
}

pub struct Assets {
    pub cache: Arc<ModelCache>,
    pub resources: Arc<ResourceManager>,
    pub scene: Option<Arc<SceneAssets>>,
    maps: ColorMaps,
    pub builtin: BuiltinData,
}

impl std::fmt::Debug for Assets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Assets").finish_non_exhaustive()
    }
}

impl Assets {
    pub fn load(jar: &Path) -> Result<Self> {
        let resources = ResourceManager::vanilla(jar)
            .with_context(|| format!("opening client jar {}", jar.display()))?;
        Self::from_resources(jar, resources)
    }

    /// Bakes everything from `resources`: the vanilla jar plus whatever packs were stacked on it.
    pub fn from_resources(jar: &Path, resources: ResourceManager) -> Result<Self> {
        let cache = ModelCache::build(&resources, mcr_data::v26_3()).context("baking models")?;
        if !cache.missing_sprites.is_empty() || cache.unbaked_states > 0 {
            eprintln!(
                "assets: {} missing sprites, {} unbaked states",
                cache.missing_sprites.len(),
                cache.unbaked_states
            );
        }
        let resources = Arc::new(resources);
        let extracted = jar.parent().unwrap_or(Path::new(".")).join("extracted");
        let scene = match SceneAssets::load(resources.clone(), &extracted) {
            Ok(s) => Some(Arc::new(s)),
            Err(e) => {
                eprintln!("assets: entity models unavailable: {e}");
                None
            }
        };
        Ok(Self {
            cache: Arc::new(cache),
            maps: ColorMaps::load(&resources),
            resources,
            scene,
            builtin: jar_registry_data(jar),
        })
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.resources.read(path).ok().flatten()
    }

    /// Colours per biome in registry order, from the jar's biome data; unknown biomes get plains.
    pub fn biome_colors(&self, names: &[String]) -> Arc<BiomeColors> {
        let mut missing = 0;
        let climates: Vec<BiomeClimate> = names
            .iter()
            .map(|name| {
                self.builtin
                    .get(BIOME, name)
                    .and_then(|c| BiomeClimate::from_nbt(&c))
                    .unwrap_or_else(|| {
                        missing += 1;
                        BiomeClimate::default()
                    })
            })
            .collect();
        if missing > 0 {
            eprintln!("assets: {missing} biomes without colours in the jar, using plains");
        }
        Arc::new(BiomeColors::from_climates(&climates, &self.maps))
    }
}
