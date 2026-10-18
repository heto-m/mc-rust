use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use mcr_assets::{AssetSource, ResourceManager, ZipSource};
use mcr_client::BuiltinData;
use mcr_data::Version;
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

/// Mod jars stack above vanilla in the given order; one that cannot be read is skipped.
pub fn push_mod_jars(resources: &mut ResourceManager, mods: &[PathBuf]) {
    for jar in mods {
        if let Err(e) = resources.push_mod_jar(jar) {
            eprintln!("assets: skipping resources of {}: {e}", jar.display());
        }
    }
}

fn bake(resources: &ResourceManager, data: &Version) -> Result<ModelCache> {
    let cache = ModelCache::build(resources, data).context("baking models")?;
    if !cache.missing_sprites.is_empty() || cache.unbaked_states > 0 {
        eprintln!(
            "assets: {} missing sprites, {} unbaked states ({})",
            cache.missing_sprites.len(),
            cache.unbaked_states,
            cache.unbaked_blocks.join(", ")
        );
    }
    Ok(cache)
}

pub struct Assets {
    pub cache: Arc<ModelCache>,
    pub resources: Arc<ResourceManager>,
    pub scene: Option<Arc<SceneAssets>>,
    maps: ColorMaps,
    pub builtin: BuiltinData,
    /// The tables the models were baked for.
    pub data: &'static Version,
    jar: PathBuf,
}

impl std::fmt::Debug for Assets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Assets").finish_non_exhaustive()
    }
}

impl Assets {
    pub fn load(jar: &Path) -> Result<Self> {
        Self::load_with_mods(jar, &[])
    }

    /// The vanilla jar with the client resources of `mods` (mod jars) stacked on it.
    pub fn load_with_mods(jar: &Path, mods: &[PathBuf]) -> Result<Self> {
        let mut resources = ResourceManager::vanilla(jar)
            .with_context(|| format!("opening client jar {}", jar.display()))?;
        push_mod_jars(&mut resources, mods);
        Self::from_resources(jar, resources)
    }

    /// Bakes everything from `resources`: the vanilla jar plus whatever packs were stacked on it.
    pub fn from_resources(jar: &Path, resources: ResourceManager) -> Result<Self> {
        let data = mcr_data::v26_3();
        let cache = bake(&resources, data)?;
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
            data,
            jar: jar.to_owned(),
        })
    }

    /// The same resources with models baked for `data`, the tables a modded server's registries produced.
    pub fn for_data(&self, data: &'static Version) -> Result<Self> {
        Ok(Self {
            cache: Arc::new(bake(&self.resources, data)?),
            resources: self.resources.clone(),
            scene: self.scene.clone(),
            maps: self.maps.clone(),
            builtin: self.builtin.clone(),
            data,
            jar: self.jar.clone(),
        })
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.resources.read(path).ok().flatten()
    }

    /// Colours per biome in registry order: from `climates` (the server's data, by index) where given,
    /// else the jar's biome data; unknown biomes get plains.
    pub fn biome_colors(
        &self,
        names: &[String],
        climates: &[Option<BiomeClimate>],
    ) -> Arc<BiomeColors> {
        let mut missing = 0;
        let climates: Vec<BiomeClimate> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                climates
                    .get(i)
                    .cloned()
                    .flatten()
                    .or_else(|| {
                        self.builtin
                            .get(BIOME, name)
                            .and_then(|c| BiomeClimate::from_nbt(&c))
                    })
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
