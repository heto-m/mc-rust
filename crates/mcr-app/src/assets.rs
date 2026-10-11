use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use mcr_assets::ResourceManager;
use mcr_entity_model::SceneAssets;
use mcr_mesh::{BiomeColors, ColorMaps, ModelCache};

pub struct Assets {
    pub cache: Arc<ModelCache>,
    pub resources: Arc<ResourceManager>,
    pub scene: Option<Arc<SceneAssets>>,
    maps: ColorMaps,
    biome_dir: PathBuf,
}

impl std::fmt::Debug for Assets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Assets")
            .field("biome_dir", &self.biome_dir)
            .finish_non_exhaustive()
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
            biome_dir: jar
                .parent()
                .unwrap_or(Path::new("."))
                .join("generated/data/minecraft/worldgen/biome"),
        })
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.resources.read(path).ok().flatten()
    }

    pub fn biome_colors(&self, names: &[String]) -> Arc<BiomeColors> {
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        Arc::new(BiomeColors::from_json_dir(
            &self.biome_dir,
            &refs,
            &self.maps,
        ))
    }
}
