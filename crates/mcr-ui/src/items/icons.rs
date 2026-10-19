use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use mcr_assets::{
    BakedItemModel, DisplayContext, Identifier, ItemBaker, ItemContext, ItemLayer, ItemModels,
    ItemProps, LayerKind, Mat4, ResourceManager, Sprite, SpriteId as TextureId, decode_sprite,
};
use rustc_hash::{FxHashMap, FxHasher};

use super::raster::{Glint, LayerDraw, Raster};
use super::special;
use crate::error::Result;
use crate::sprites::SpriteId;
use crate::ui::UiFrame;

const GLINT_TEXTURE: &str = "assets/minecraft/textures/misc/enchanted_glint_item.png";
const GLINT_STRENGTH: f32 = 0.75;
const GLINT_STEP_MS: u64 = 16;
const SLOT: i32 = 16;

/// What the GUI needs to know about one stack, independent of the protocol crate.
pub struct ItemView<'a> {
    pub item: &'a str,
    pub props: &'a dyn ItemProps,
    pub glint: bool,
    pub cooldown: f32,
    /// A bar other than durability, such as a bundle's fullness: width and ARGB colour.
    pub bar: Option<(i32, u32)>,
}

impl std::fmt::Debug for ItemView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemView")
            .field("item", &self.item)
            .field("count", &self.props.count())
            .finish()
    }
}

struct Icon {
    sprite: Option<SpriteId>,
    glint_step: u64,
    used: u64,
}

const MAX_ICONS: usize = 1024;
const ICON_EVICT_SLACK: usize = 128;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IconStats {
    pub rasterized: usize,
    pub cached: usize,
}

/// Item model definitions baked once, and a cache of rasterised icons living in the UI sprite atlas.
pub struct ItemIcons {
    models: ItemModels,
    baked: HashMap<Identifier, Arc<BakedItemModel>>,
    specials: special::Specials,
    images: HashMap<TextureId, Arc<Sprite>>,
    glint: Arc<Sprite>,
    icons: FxHashMap<u64, Icon>,
    icon_scale: u32,
    millis: u64,
    context: ItemContext,
    stats: IconStats,
}

impl std::fmt::Debug for ItemIcons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemIcons")
            .field("definitions", &self.models.len())
            .field("icons", &self.icons.len())
            .finish()
    }
}

impl ItemIcons {
    /// Parses and bakes every item definition; drop and reload this on a resource reload.
    pub fn load(resources: &ResourceManager) -> Result<Self> {
        let models = ItemModels::load(resources)?;
        let mut baker = ItemBaker::new();
        let mut baked = HashMap::new();
        let mut wanted: Vec<Identifier> = models
            .ids()
            .filter_map(|id| models.get(id))
            .flat_map(|def| def.models())
            .collect();
        wanted.sort();
        wanted.dedup();
        for id in &wanted {
            match baker.bake(resources, id) {
                Ok(model) => {
                    baked.insert(id.clone(), model);
                }
                Err(e) => tracing::warn!("item model {id}: {e}"),
            }
        }
        let specials = special::bake_all(resources, &mut baker, &models, &baked)?;
        let mut images = HashMap::new();
        let ids: Vec<TextureId> = baker.sprites().iter().map(|(id, _)| id).collect();
        for id in ids {
            images.insert(id, baker.image(resources, id)?);
        }
        let glint_png = resources
            .read(GLINT_TEXTURE)?
            .ok_or_else(|| mcr_assets::AssetError::Missing(GLINT_TEXTURE.into()))?;
        let glint = Arc::new(decode_sprite(GLINT_TEXTURE, &glint_png, None)?);
        Ok(Self {
            models,
            baked,
            specials,
            images,
            glint,
            icons: FxHashMap::default(),
            icon_scale: 0,
            millis: 0,
            context: ItemContext::default(),
            stats: IconStats::default(),
        })
    }

    pub fn models(&self) -> &ItemModels {
        &self.models
    }

    pub fn context(&self) -> &ItemContext {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut ItemContext {
        &mut self.context
    }

    /// Wall-clock milliseconds driving the glint animation.
    pub fn set_time(&mut self, millis: u64) {
        self.millis = millis;
    }

    pub fn stats(&self) -> IconStats {
        self.stats
    }

    pub fn layers(&self, item: &str, props: &dyn ItemProps) -> Vec<ItemLayer> {
        let mut ctx = self.context.clone();
        ctx.display = DisplayContext::Gui;
        self.models.evaluate(&Identifier::parse(item), props, &ctx)
    }

    fn resolve<'a>(&'a self, layers: &'a [ItemLayer]) -> Vec<LayerDraw<'a>> {
        let mut out = Vec::with_capacity(layers.len());
        for layer in layers {
            let model_id = match &layer.kind {
                LayerKind::Model(id) => id,
                LayerKind::Special { kind, .. } => {
                    if let Some(draw) = special::resolve(kind, &self.specials) {
                        out.push(draw);
                    }
                    continue;
                }
                LayerKind::BundleSelectedItem => continue,
            };
            if let Some(model) = self.baked.get(model_id) {
                out.push(LayerDraw {
                    model,
                    tints: &layer.tints,
                    local: layer.transform,
                });
            }
        }
        out
    }

    fn key(layers: &[ItemLayer], glint: bool, scale: u32) -> u64 {
        let mut h = FxHasher::default();
        (glint, scale).hash(&mut h);
        for layer in layers {
            match &layer.kind {
                LayerKind::Model(id) => (0u8, id).hash(&mut h),
                LayerKind::Special { base, kind } => (1u8, base, format!("{kind:?}")).hash(&mut h),
                LayerKind::BundleSelectedItem => 2u8.hash(&mut h),
            }
            layer.tints.hash(&mut h);
            transform_bits(&layer.transform).hash(&mut h);
        }
        h.finish()
    }

    fn render(&self, layers: &[ItemLayer], glint: bool, size: usize) -> Vec<u8> {
        let draws = self.resolve(layers);
        let glint = glint.then(|| Glint {
            texture: self.glint.clone(),
            millis: self.millis,
            strength: GLINT_STRENGTH,
        });
        Raster {
            size,
            images: &self.images,
            glint: glint.as_ref(),
        }
        .render(&draws)
    }

    /// Draws the item at the 16x16 slot whose top-left is `(x, y)` in GUI pixels.
    pub fn draw(&mut self, frame: &mut UiFrame<'_>, view: &ItemView, x: i32, y: i32) {
        self.draw_sized(frame, view, x, y, SLOT, SLOT);
    }

    /// Like [`Self::draw`] but stretches the icon to `w` by `h` GUI pixels.
    pub fn draw_sized(
        &mut self,
        frame: &mut UiFrame<'_>,
        view: &ItemView,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) {
        if let Some(id) = self.icon(frame, view) {
            frame.sprite(id, x, y, w, h);
        }
    }

    /// The slot at `(x, y)` scaled by `(sx, sy)` around `(x + 8, y + 12)`, like the hotbar pop.
    pub fn draw_squeezed(
        &mut self,
        frame: &mut UiFrame<'_>,
        view: &ItemView,
        x: i32,
        y: i32,
        sx: f32,
        sy: f32,
    ) {
        if let Some(id) = self.icon(frame, view) {
            let (cx, cy) = (x as f32 + 8.0, y as f32 + 12.0);
            let dst = [cx - 8.0 * sx, cy - 12.0 * sy, cx + 8.0 * sx, cy + 4.0 * sy];
            frame.sprite_f(id, dst, 0xFFFF_FFFF);
        }
    }

    fn icon(&mut self, frame: &mut UiFrame<'_>, view: &ItemView) -> Option<SpriteId> {
        let scale = frame.scale();
        if scale != self.icon_scale {
            for icon in std::mem::take(&mut self.icons).into_values() {
                if let Some(id) = icon.sprite {
                    frame.ui().sprites_mut().free_raw(id);
                }
            }
            self.icon_scale = scale;
        }
        let layers = self.layers(view.item, view.props);
        if layers.is_empty() {
            return None;
        }
        let key = Self::key(&layers, view.glint, scale);
        let size = SLOT as usize * scale as usize;
        let step = self.millis / GLINT_STEP_MS;
        let now = frame.number();
        if let Some(icon) = self.icons.get_mut(&key) {
            icon.used = now;
        } else if self.icons.len() >= MAX_ICONS {
            self.evict_icons(frame, now);
        }
        match self.icons.get(&key) {
            Some(icon) if !view.glint || icon.glint_step == step => self.stats.cached += 1,
            Some(_) => {
                let pixels = self.render(&layers, view.glint, size);
                let icon = self.icons.get_mut(&key).expect("checked above");
                icon.glint_step = step;
                if let Some(id) = icon.sprite {
                    frame.ui().sprites_mut().rewrite(id, &pixels);
                }
                self.stats.rasterized += 1;
            }
            None => {
                let pixels = self.render(&layers, view.glint, size);
                let sprite = frame
                    .ui()
                    .sprites_mut()
                    .add_raw(size as u32, size as u32, &pixels);
                self.icons.insert(
                    key,
                    Icon {
                        sprite,
                        glint_step: step,
                        used: now,
                    },
                );
                self.stats.rasterized += 1;
            }
        }
        self.icons[&key].sprite
    }

    fn evict_icons(&mut self, frame: &mut UiFrame<'_>, now: u64) {
        let mut idle: Vec<(u64, u64)> = self
            .icons
            .iter()
            .filter(|(_, icon)| icon.used < now)
            .map(|(&key, icon)| (icon.used, key))
            .collect();
        idle.sort_unstable();
        let excess = (self.icons.len() + ICON_EVICT_SLACK).saturating_sub(MAX_ICONS);
        for (_, key) in idle.into_iter().take(excess) {
            if let Some(icon) = self.icons.remove(&key)
                && let Some(id) = icon.sprite
            {
                frame.ui().sprites_mut().free_raw(id);
            }
        }
    }

    /// Count, durability bar and cooldown overlay.
    pub fn decorations(&self, frame: &mut UiFrame<'_>, view: &ItemView, x: i32, y: i32) {
        let props = view.props;
        if let Some((width, color)) = durability_bar(props).or(view.bar) {
            frame.fill_rect(x + 2, y + 13, x + 2 + 13, y + 15, 0xFF00_0000);
            frame.fill_rect(x + 2, y + 13, x + 2 + width, y + 14, color);
        }
        if view.cooldown > 0.0 {
            let top = y + (16.0 * (1.0 - view.cooldown)).floor() as i32;
            let bottom = top + (16.0 * view.cooldown).ceil() as i32;
            frame.fill_rect(x, top, x + 16, bottom, 0x7FFF_FFFF);
        }
        if props.count() != 1 {
            let text = props.count().to_string();
            let width = frame.ui().text_width(&text);
            frame.text(
                &text,
                (x + 19 - 2) as f32 - width,
                (y + 6 + 3) as f32,
                0xFFFF_FFFF,
                true,
            );
        }
    }

    pub fn draw_with_decorations(
        &mut self,
        frame: &mut UiFrame<'_>,
        view: &ItemView,
        x: i32,
        y: i32,
    ) {
        self.draw(frame, view, x, y);
        self.decorations(frame, view, x, y);
    }
}

fn transform_bits(m: &Mat4) -> [u32; 16] {
    std::array::from_fn(|i| m[i / 4][i % 4].to_bits())
}

/// Bar width in pixels and its ARGB colour, when the item is damaged and breakable.
pub fn durability_bar(props: &dyn ItemProps) -> Option<(i32, u32)> {
    let (damage, max) = (props.damage(), props.max_damage());
    if max <= 0 || damage <= 0 || props.unbreakable() {
        return None;
    }
    let width = (13.0 - damage as f32 * 13.0 / max as f32)
        .round()
        .clamp(0.0, 13.0) as i32;
    let hue = ((max - damage) as f32 / max as f32).max(0.0) / 3.0;
    Some((width, 0xFF00_0000 | hsv_to_rgb(hue, 1.0, 1.0)))
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> u32 {
    let i = (hue * 6.0) as i32 % 6;
    let f = hue * 6.0 - (hue * 6.0).floor();
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - f * saturation);
    let t = value * (1.0 - (1.0 - f) * saturation);
    let (r, g, b) = match i {
        0 => (value, t, p),
        1 => (q, value, p),
        2 => (p, value, t),
        3 => (p, q, value),
        4 => (t, p, value),
        _ => (value, p, q),
    };
    let byte = |c: f32| ((c * 255.0 + 0.5) as u32).min(255);
    byte(r) << 16 | byte(g) << 8 | byte(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Damaged(i32, i32, bool);

    impl ItemProps for Damaged {
        fn damage(&self) -> i32 {
            self.0
        }
        fn max_damage(&self) -> i32 {
            self.1
        }
        fn unbreakable(&self) -> bool {
            self.2
        }
    }

    #[test]
    fn bar_follows_vanilla_formula() {
        assert_eq!(durability_bar(&Damaged(0, 100, false)), None);
        assert_eq!(durability_bar(&Damaged(10, 100, true)), None);
        assert_eq!(durability_bar(&Damaged(10, 0, false)), None);
        let (width, color) = durability_bar(&Damaged(50, 100, false)).unwrap();
        assert_eq!(width, 7);
        assert_eq!(color, 0xFFFF_FF00);
        let (width, color) = durability_bar(&Damaged(1, 1561, false)).unwrap();
        assert_eq!(width, 13);
        assert_eq!(color >> 24, 0xFF);
        assert!((color >> 8) & 0xFF == 0xFF && (color >> 16) & 0xFF < 8);
        let (width, color) = durability_bar(&Damaged(99, 100, false)).unwrap();
        assert_eq!(width, 0);
        assert_eq!(color, 0xFFFF_0000 | 0x0500);
    }
}
