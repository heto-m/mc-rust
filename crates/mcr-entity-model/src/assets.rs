use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use mcr_assets::{ItemModels, ResourceManager};
use serde::Deserialize;

use crate::Error;
use crate::data::{AnimationSet, LayerSet};

#[derive(Debug, Clone)]
pub struct ImageRgba {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl ImageRgba {
    pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        ImageRgba {
            width,
            height,
            rgba: rgba.repeat((width * height) as usize),
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    pub fn set(&mut self, x: u32, y: u32, p: [u8; 4]) {
        let i = ((y * self.width + x) * 4) as usize;
        self.rgba[i..i + 4].copy_from_slice(&p);
    }
}

/// The checkerboard shown for textures that cannot be found.
pub fn missing_image() -> ImageRgba {
    let mut img = ImageRgba::solid(16, 16, [0, 0, 0, 255]);
    for y in 0..16 {
        for x in 0..16 {
            if (x / 8 + y / 8) % 2 == 0 {
                img.set(x, y, [248, 0, 248, 255]);
            }
        }
    }
    img
}

/// Reads `assets/minecraft/<texture_path>` (e.g. `textures/entity/pig/pig_temperate.png`) from a jar.
pub fn load_jar_texture(jar: impl AsRef<Path>, texture_path: &str) -> Result<ImageRgba, Error> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
    let mut bytes = Vec::new();
    archive
        .by_name(&format!("assets/minecraft/{texture_path}"))?
        .read_to_end(&mut bytes)?;
    decode_png(&bytes)
}

pub(crate) fn decode_png(bytes: &[u8]) -> Result<ImageRgba, Error> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buf)?;
    let data = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        other => return Err(Error::PngFormat(other)),
    };
    Ok(ImageRgba {
        width: info.width,
        height: info.height,
        rgba,
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct Equippable {
    pub slot: Option<String>,
    pub asset_id: Option<String>,
    #[serde(default)]
    pub glider: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct DyeableDef {
    color_when_undyed: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
struct LayerDefJson {
    texture: String,
    dyeable: Option<DyeableDef>,
    #[serde(default)]
    use_player_texture: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct TrimOverrideWhen {
    material: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TrimOverrideJson {
    palette: Option<String>,
    texture: Option<String>,
    when: TrimOverrideWhen,
}

#[derive(Debug, Clone, Deserialize)]
struct EquipmentJson {
    #[serde(default)]
    layers: HashMap<String, Vec<LayerDefJson>>,
    #[serde(default)]
    trim_overrides: Vec<TrimOverrideJson>,
}

#[derive(Debug, Clone)]
pub struct EquipmentLayer {
    pub texture: String,
    pub dyeable: bool,
    pub undyed_color: Option<i32>,
    pub use_player_texture: bool,
}

#[derive(Debug, Clone)]
pub struct TrimOverride {
    pub material: Option<String>,
    pub palette: Option<String>,
    pub texture: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EquipmentInfo {
    pub layers: HashMap<String, Vec<EquipmentLayer>>,
    pub trim_overrides: Vec<TrimOverride>,
}

impl EquipmentInfo {
    fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let json: EquipmentJson = serde_json::from_slice(bytes)?;
        Ok(EquipmentInfo {
            layers: json
                .layers
                .into_iter()
                .map(|(k, v)| {
                    (
                        k,
                        v.into_iter()
                            .map(|l| EquipmentLayer {
                                texture: l.texture,
                                dyeable: l.dyeable.is_some(),
                                undyed_color: l.dyeable.and_then(|d| d.color_when_undyed),
                                use_player_texture: l.use_player_texture,
                            })
                            .collect(),
                    )
                })
                .collect(),
            trim_overrides: json
                .trim_overrides
                .into_iter()
                .map(|o| TrimOverride {
                    material: o.when.material,
                    palette: o.palette,
                    texture: o.texture,
                })
                .collect(),
        })
    }
}

/// Everything read-only the scene needs: resources, extracted model data and item definitions.
#[derive(Debug)]
pub struct SceneAssets {
    pub resources: Arc<ResourceManager>,
    pub layers: LayerSet,
    pub animations: AnimationSet,
    pub renderers: crate::data::RendererSet,
    pub items: Arc<ItemModels>,
    pub equippables: HashMap<String, Equippable>,
    equipment: std::sync::Mutex<HashMap<String, Option<Arc<EquipmentInfo>>>>,
}

impl SceneAssets {
    /// `extracted` is the version's extracted-data directory (entity_models.json and friends).
    pub fn load(resources: Arc<ResourceManager>, extracted: &Path) -> Result<Self, Error> {
        let items =
            Arc::new(ItemModels::load(&resources).map_err(|e| Error::Assets(e.to_string()))?);
        Self::with_items(resources, extracted, items)
    }

    pub fn with_items(
        resources: Arc<ResourceManager>,
        extracted: &Path,
        items: Arc<ItemModels>,
    ) -> Result<Self, Error> {
        let equippables = match std::fs::read(extracted.join("item_components.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(_) => HashMap::new(),
        };
        Ok(SceneAssets {
            resources,
            layers: LayerSet::load(extracted.join("entity_models.json"))?,
            animations: AnimationSet::load(extracted.join("animations.json"))?,
            renderers: crate::data::RendererSet::load(extracted.join("entity_renderers.json"))
                .unwrap_or_else(|_| crate::data::RendererSet(HashMap::new())),
            items,
            equippables,
            equipment: Default::default(),
        })
    }

    /// Parsed `equipment/<name>.json`; `None` when the pack has no such asset.
    pub fn equipment(&self, asset: &str) -> Option<Arc<EquipmentInfo>> {
        let name = asset.strip_prefix("minecraft:").unwrap_or(asset);
        let mut cache = self.equipment.lock().ok()?;
        if let Some(hit) = cache.get(name) {
            return hit.clone();
        }
        let parsed = self
            .resources
            .read(&format!("assets/minecraft/equipment/{name}.json"))
            .ok()
            .flatten()
            .and_then(|b| EquipmentInfo::parse(&b).ok())
            .map(Arc::new);
        cache.insert(name.to_owned(), parsed.clone());
        parsed
    }

    pub fn read_texture(&self, key: &str) -> Option<ImageRgba> {
        let bytes = self
            .resources
            .read(&format!("assets/minecraft/textures/{key}.png"))
            .ok()??;
        decode_png(&bytes).ok()
    }
}
