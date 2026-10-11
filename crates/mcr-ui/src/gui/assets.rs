use mcr_assets::{ResourceManager, decode_sprite};
use rustc_hash::FxHashMap;

use crate::error::Result;
use crate::sprites::SpriteId;
use crate::ui::Ui;

const GUI_ROOT: &str = "assets/minecraft/textures/gui/";
const EFFECT_ROOT: &str = "assets/minecraft/textures/mob_effect/";
const DIRS: [&str; 5] = [
    "container/",
    "container/creative_inventory/",
    "title/",
    "signs/",
    "hanging_signs/",
];
const FILES: [&str; 7] = [
    "menu_background",
    "menu_list_background",
    "header_separator",
    "footer_separator",
    "inworld_header_separator",
    "inworld_footer_separator",
    "book",
];

/// Whole-image GUI textures (container backgrounds, title logo, menu backgrounds) placed in the sprite atlas.
#[derive(Debug, Default)]
pub struct ScreenAssets {
    textures: FxHashMap<String, (SpriteId, u32, u32)>,
}

impl ScreenAssets {
    pub fn load(ui: &mut Ui, resources: &ResourceManager) -> Result<Self> {
        let mut paths: Vec<&str> = resources
            .list_prefix(GUI_ROOT)
            .filter(|p| p.ends_with(".png"))
            .filter(|p| {
                let rel = &p[GUI_ROOT.len()..p.len() - 4];
                DIRS.iter()
                    .any(|d| rel.strip_prefix(d).is_some_and(|r| !r.contains('/')))
                    || FILES.contains(&rel)
            })
            .collect();
        paths.sort_unstable();
        let mut textures = FxHashMap::default();
        paths.extend(
            resources
                .list_prefix(EFFECT_ROOT)
                .filter(|p| p.ends_with(".png")),
        );
        for path in paths {
            let Some(png) = resources.read(path)? else {
                continue;
            };
            let sprite = match decode_sprite(path, &png, None) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("skipping texture {path}: {e}");
                    continue;
                }
            };
            let (width, height, pixels) = shrink(sprite.width, sprite.height, &sprite.frames[0]);
            let Some(id) = ui.sprites_mut().add_raw(width, height, &pixels) else {
                tracing::warn!("texture {path} does not fit an atlas page");
                continue;
            };
            let name = match path.strip_prefix(GUI_ROOT) {
                Some(rel) => rel[..rel.len() - 4].to_owned(),
                None => format!("mob_effect/{}", &path[EFFECT_ROOT.len()..path.len() - 4]),
            };
            textures.insert(name, (id, width, height));
        }
        Ok(Self { textures })
    }

    pub fn texture(&self, name: &str) -> Option<SpriteId> {
        self.textures.get(name).map(|t| t.0)
    }

    pub fn size(&self, name: &str) -> Option<(u32, u32)> {
        self.textures.get(name).map(|t| (t.1, t.2))
    }
}

const MAX_TEXTURE: u32 = 512;

/// Box-filters oversized images down by whole factors so they fit an atlas page.
fn shrink(width: u32, height: u32, rgba: &[u8]) -> (u32, u32, std::borrow::Cow<'_, [u8]>) {
    let factor = width.max(height).div_ceil(MAX_TEXTURE).max(1);
    if factor == 1 {
        return (width, height, rgba.into());
    }
    let (w, h) = (width / factor, height / factor);
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 4];
            for dy in 0..factor {
                for dx in 0..factor {
                    let i = (((y * factor + dy) * width + x * factor + dx) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += u32::from(rgba[i + c]);
                    }
                }
            }
            let o = ((y * w + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (sum[c] / (factor * factor)) as u8;
            }
        }
    }
    (w, h, out.into())
}
