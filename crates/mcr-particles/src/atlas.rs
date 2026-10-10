use std::collections::HashMap;

use mcr_render::EffectImage;

use crate::names::PARTICLE_NAMES;

const ATLAS_WIDTH: u32 = 1024;
const MAX_ATLAS_HEIGHT: u32 = 2048;
const MAX_SPRITES_PER_SET: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Uv {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

impl Uv {
    pub const ZERO: Uv = Uv {
        u0: 0.0,
        v0: 0.0,
        u1: 1.0,
        v1: 1.0,
    };
}

/// Every particle sprite packed into one texture, plus the ordered sprite list of each
/// `particles/<name>.json`.
#[derive(Debug)]
pub struct ParticleAtlas {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub(crate) sprites: Vec<Uv>,
    pub(crate) sets: Vec<Vec<u16>>,
}

impl ParticleAtlas {
    /// `read` returns the bytes of a jar resource path such as `assets/minecraft/particles/flame.json`.
    pub fn load(read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Self {
        let mut images: Vec<EffectImage> = Vec::new();
        let mut by_texture: HashMap<String, u16> = HashMap::new();
        let mut sets: Vec<Vec<u16>> = Vec::with_capacity(PARTICLE_NAMES.len());
        for name in PARTICLE_NAMES {
            let mut set = Vec::new();
            let textures = read(&format!("assets/minecraft/particles/{name}.json"))
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .and_then(|v| v.get("textures").cloned())
                .and_then(|t| t.as_array().cloned())
                .unwrap_or_default();
            for tex in textures
                .iter()
                .filter_map(|t| t.as_str())
                .take(MAX_SPRITES_PER_SET)
            {
                let tex = tex.strip_prefix("minecraft:").unwrap_or(tex).to_string();
                let id = match by_texture.get(&tex) {
                    Some(&id) => id,
                    None => {
                        let Some(img) =
                            read(&format!("assets/minecraft/textures/particle/{tex}.png"))
                                .and_then(|b| EffectImage::decode(&b))
                        else {
                            continue;
                        };
                        let Ok(id) = u16::try_from(images.len()) else {
                            continue;
                        };
                        images.push(img);
                        by_texture.insert(tex, id);
                        id
                    }
                };
                set.push(id);
            }
            sets.push(set);
        }
        Self::pack(images, sets)
    }

    pub(crate) fn pack(images: Vec<EffectImage>, sets: Vec<Vec<u16>>) -> Self {
        let mut order: Vec<usize> = (0..images.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(images[i].height));
        let mut placed = vec![(0u32, 0u32); images.len()];
        let (mut x, mut y, mut row_h) = (1u32, 1u32, 0u32);
        for &i in &order {
            let img = &images[i];
            if x + img.width + 1 > ATLAS_WIDTH {
                x = 1;
                y += row_h + 1;
                row_h = 0;
            }
            placed[i] = (x, y);
            x += img.width + 1;
            row_h = row_h.max(img.height);
        }
        let height = (y + row_h + 1)
            .next_power_of_two()
            .clamp(2, MAX_ATLAS_HEIGHT);
        let mut rgba = vec![0u8; (ATLAS_WIDTH * height * 4) as usize];
        let mut sprites = vec![Uv::ZERO; images.len()];
        for (i, img) in images.iter().enumerate() {
            let (px, py) = placed[i];
            if py + img.height > height {
                continue;
            }
            for row in 0..img.height {
                let src = (row * img.width * 4) as usize;
                let dst = (((py + row) * ATLAS_WIDTH + px) * 4) as usize;
                let n = (img.width * 4) as usize;
                rgba[dst..dst + n].copy_from_slice(&img.rgba[src..src + n]);
            }
            let (w, h) = (ATLAS_WIDTH as f32, height as f32);
            sprites[i] = Uv {
                u0: px as f32 / w,
                v0: py as f32 / h,
                u1: (px + img.width) as f32 / w,
                v1: (py + img.height) as f32 / h,
            };
        }
        Self {
            width: ATLAS_WIDTH,
            height,
            rgba,
            sprites,
            sets,
        }
    }

    pub fn sprite_count(&self) -> usize {
        self.sprites.len()
    }

    pub fn set(&self, type_id: usize) -> &[u16] {
        self.sets.get(type_id).map_or(&[], Vec::as_slice)
    }

    pub fn uv(&self, sprite: u16) -> Uv {
        self.sprites
            .get(usize::from(sprite))
            .copied()
            .unwrap_or(Uv::ZERO)
    }
}

pub const SLOT: u32 = 32;
pub const DYNAMIC_SIZE: u32 = 1024;
pub const DYNAMIC_SLOTS: u32 = (DYNAMIC_SIZE / SLOT) * (DYNAMIC_SIZE / SLOT);

/// Tracks block and item textures copied into the dynamic atlas, one `SLOT` square each.
#[derive(Debug, Default)]
pub(crate) struct DynamicSlots {
    keys: HashMap<u64, u32>,
    next: u32,
}

fn slot_uv(slot: u32) -> Uv {
    let per_row = DYNAMIC_SIZE / SLOT;
    let (x, y) = ((slot % per_row) * SLOT, (slot / per_row) * SLOT);
    let s = DYNAMIC_SIZE as f32;
    Uv {
        u0: x as f32 / s,
        v0: y as f32 / s,
        u1: (x + SLOT) as f32 / s,
        v1: (y + SLOT) as f32 / s,
    }
}

impl DynamicSlots {
    pub(crate) fn lookup(&self, key: u64) -> Option<Uv> {
        self.keys.get(&key).map(|&s| slot_uv(s))
    }

    /// Reserves a slot for a key that `lookup` did not find.
    pub(crate) fn allocate(&mut self, key: u64) -> (u32, Uv) {
        if self.next >= DYNAMIC_SLOTS {
            self.keys.clear();
            self.next = 0;
        }
        let slot = self.next;
        self.next += 1;
        self.keys.insert(key, slot);
        (slot, slot_uv(slot))
    }

    pub(crate) fn origin(slot: u32) -> (u32, u32) {
        let per_row = DYNAMIC_SIZE / SLOT;
        ((slot % per_row) * SLOT, (slot / per_row) * SLOT)
    }
}

/// Nearest-neighbour resample into a `SLOT` square.
pub(crate) fn to_slot(image: &EffectImage) -> Vec<u8> {
    let mut out = vec![0u8; (SLOT * SLOT * 4) as usize];
    if image.width == 0 || image.height == 0 {
        return out;
    }
    let size = image.width.min(image.height);
    for y in 0..SLOT {
        for x in 0..SLOT {
            let sx = x * size / SLOT;
            let sy = y * size / SLOT;
            let src = ((sy * image.width + sx) * 4) as usize;
            let dst = ((y * SLOT + x) * 4) as usize;
            if let Some(px) = image.rgba.get(src..src + 4) {
                out[dst..dst + 4].copy_from_slice(px);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32) -> EffectImage {
        EffectImage {
            width: w,
            height: h,
            rgba: vec![200; (w * h * 4) as usize],
        }
    }

    #[test]
    fn packing_never_overlaps() {
        let images: Vec<_> = (0..300)
            .map(|i| img(16 + (i % 3) * 16, 16 + (i % 2) * 16))
            .collect();
        let atlas = ParticleAtlas::pack(images, vec![]);
        let rects: Vec<_> = atlas.sprites.iter().collect();
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let overlap = a.u0 < b.u1 && b.u0 < a.u1 && a.v0 < b.v1 && b.v0 < a.v1;
                assert!(!overlap);
            }
        }
    }

    #[test]
    fn slots_wrap_instead_of_growing() {
        let mut d = DynamicSlots::default();
        for k in 0..u64::from(DYNAMIC_SLOTS) * 3 {
            d.allocate(k);
        }
        assert!(d.next <= DYNAMIC_SLOTS);
    }

    #[test]
    fn missing_json_yields_empty_sets() {
        let atlas = ParticleAtlas::load(&|_| None);
        assert_eq!(atlas.sprite_count(), 0);
        assert!(atlas.set(39).is_empty());
    }
}
