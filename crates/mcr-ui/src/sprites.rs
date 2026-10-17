use mcr_assets::{ResourceManager, decode_sprite};
use rustc_hash::FxHashMap;
use serde_json::Value;

use crate::atlas::Atlas;
use crate::error::Result;

const SPRITE_ROOT: &str = "assets/minecraft/textures/gui/sprites/";
const SPRITE_PAGE_SIZE: u32 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scaling {
    Stretch,
    Tile {
        width: u32,
        height: u32,
    },
    NineSlice {
        width: u32,
        height: u32,
        border: [u32; 4],
        stretch_inner: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpriteId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteInfo {
    pub page: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub scaling: Scaling,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteQuad {
    pub dst: [f32; 4],
    pub uv: [f32; 4],
}

impl SpriteInfo {
    fn quad(
        &self,
        atlas_size: f32,
        nominal: (f32, f32),
        src: [f32; 4],
        dst: [f32; 4],
    ) -> SpriteQuad {
        let u = |sx: f32| (self.x as f32 + sx / nominal.0 * self.width as f32) / atlas_size;
        let v = |sy: f32| (self.y as f32 + sy / nominal.1 * self.height as f32) / atlas_size;
        SpriteQuad {
            dst,
            uv: [u(src[0]), v(src[1]), u(src[2]), v(src[3])],
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tiled(
        &self,
        atlas_size: f32,
        nominal: (f32, f32),
        src: [f32; 4],
        dst: [f32; 4],
        emit: &mut impl FnMut(SpriteQuad),
    ) {
        let (tile_w, tile_h) = (src[2] - src[0], src[3] - src[1]);
        let (w, h) = (dst[2] - dst[0], dst[3] - dst[1]);
        if w <= 0.0 || h <= 0.0 || tile_w <= 0.0 || tile_h <= 0.0 {
            return;
        }
        let mut ty = 0.0;
        while ty < h {
            let ch = tile_h.min(h - ty);
            let mut tx = 0.0;
            while tx < w {
                let cw = tile_w.min(w - tx);
                let cell_src = [src[0], src[1], src[0] + cw, src[1] + ch];
                let cell_dst = [dst[0] + tx, dst[1] + ty, dst[0] + tx + cw, dst[1] + ty + ch];
                emit(self.quad(atlas_size, nominal, cell_src, cell_dst));
                tx += tile_w;
            }
            ty += tile_h;
        }
    }

    /// Emits the quads that draw this sprite into the destination rectangle.
    pub fn layout(
        &self,
        atlas_size: u32,
        (x, y, w, h): (i32, i32, i32, i32),
        mut emit: impl FnMut(SpriteQuad),
    ) {
        let size = atlas_size as f32;
        let dst = [x as f32, y as f32, (x + w) as f32, (y + h) as f32];
        match self.scaling {
            Scaling::Stretch => {
                let nominal = (self.width as f32, self.height as f32);
                emit(self.quad(size, nominal, [0.0, 0.0, nominal.0, nominal.1], dst));
            }
            Scaling::Tile { width, height } => {
                let nominal = (width as f32, height as f32);
                self.tiled(
                    size,
                    nominal,
                    [0.0, 0.0, nominal.0, nominal.1],
                    dst,
                    &mut emit,
                );
            }
            Scaling::NineSlice {
                width,
                height,
                border,
                stretch_inner,
            } => {
                let [bl, bt, br, bb] = [
                    border[0].min(w as u32 / 2) as i32,
                    border[1].min(h as u32 / 2) as i32,
                    border[2].min(w as u32 / 2) as i32,
                    border[3].min(h as u32 / 2) as i32,
                ];
                let (nw, nh) = (width as i32, height as i32);
                let nominal = (width as f32, height as f32);
                let cols = [
                    (0, bl, x, bl),
                    (bl, nw - bl - br, x + bl, w - bl - br),
                    (nw - br, br, x + w - br, br),
                ];
                let rows = [
                    (0, bt, y, bt),
                    (bt, nh - bt - bb, y + bt, h - bt - bb),
                    (nh - bb, bb, y + h - bb, bb),
                ];
                for (ri, &(sy, sh, dy, dh)) in rows.iter().enumerate() {
                    for (ci, &(sx, sw, dx, dw)) in cols.iter().enumerate() {
                        if dw <= 0 || dh <= 0 || sw <= 0 || sh <= 0 {
                            continue;
                        }
                        let src = [sx as f32, sy as f32, (sx + sw) as f32, (sy + sh) as f32];
                        let cell = [dx as f32, dy as f32, (dx + dw) as f32, (dy + dh) as f32];
                        let corner = ri != 1 && ci != 1;
                        if corner || stretch_inner {
                            emit(self.quad(size, nominal, src, cell));
                        } else {
                            self.tiled(size, nominal, src, cell, &mut emit);
                        }
                    }
                }
            }
        }
    }

    /// Draws the `w`x`h` window of the sprite starting at pixel `(tx, ty)` at 1:1.
    pub fn region(
        &self,
        atlas_size: u32,
        (tx, ty): (i32, i32),
        (x, y, w, h): (i32, i32, i32, i32),
    ) -> SpriteQuad {
        let nominal = (self.width as f32, self.height as f32);
        let src = [tx as f32, ty as f32, (tx + w) as f32, (ty + h) as f32];
        let dst = [x as f32, y as f32, (x + w) as f32, (y + h) as f32];
        self.quad(atlas_size as f32, nominal, src, dst)
    }

    /// Stretches the `sw`x`sh` window starting at pixel `(tx, ty)` over the destination rectangle.
    pub fn region_scaled(
        &self,
        atlas_size: u32,
        (tx, ty, sw, sh): (i32, i32, i32, i32),
        (x, y, w, h): (i32, i32, i32, i32),
    ) -> SpriteQuad {
        let nominal = (self.width as f32, self.height as f32);
        let src = [tx as f32, ty as f32, (tx + sw) as f32, (ty + sh) as f32];
        let dst = [x as f32, y as f32, (x + w) as f32, (y + h) as f32];
        self.quad(atlas_size as f32, nominal, src, dst)
    }
}

/// Hands runtime sprites back from owners that are dropped without access to [`Sprites`].
#[derive(Debug, Clone, Default)]
pub struct SpriteReleaser(std::sync::Arc<std::sync::Mutex<Vec<SpriteId>>>);

impl SpriteReleaser {
    pub fn release(&self, ids: impl IntoIterator<Item = SpriteId>) {
        if let Ok(mut queue) = self.0.lock() {
            queue.extend(ids);
        }
    }
}

/// Runtime sprites that go back to their atlas when this set is dropped.
#[derive(Debug, Default)]
pub struct OwnedSprites {
    releaser: Option<SpriteReleaser>,
    ids: Vec<SpriteId>,
}

impl OwnedSprites {
    pub fn add(
        &mut self,
        sprites: &mut Sprites,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Option<SpriteId> {
        self.releaser.get_or_insert_with(|| sprites.releaser());
        let id = sprites.add_raw(width, height, rgba)?;
        self.ids.push(id);
        Some(id)
    }

    pub fn free(&mut self, sprites: &mut Sprites, id: SpriteId) {
        if let Some(at) = self.ids.iter().position(|&i| i == id) {
            self.ids.swap_remove(at);
            sprites.free_raw(id);
        }
    }

    /// Drops the ids without freeing them, after the atlas they belong to was rebuilt.
    pub fn forget(&mut self) {
        self.ids.clear();
        self.releaser = None;
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

impl Drop for OwnedSprites {
    fn drop(&mut self) {
        if let Some(releaser) = &self.releaser {
            releaser.release(self.ids.drain(..));
        }
    }
}

#[derive(Debug)]
pub struct Sprites {
    atlas: Atlas,
    infos: Vec<SpriteInfo>,
    names: FxHashMap<String, SpriteId>,
    loaded: u32,
    freed: FxHashMap<(u32, u32), Vec<SpriteId>>,
    freed_ids: rustc_hash::FxHashSet<u32>,
    releaser: SpriteReleaser,
}

impl Sprites {
    pub fn load(resources: &ResourceManager) -> Result<Self> {
        let mut paths: Vec<&str> = resources
            .list_prefix(SPRITE_ROOT)
            .filter(|p| p.ends_with(".png"))
            .collect();
        paths.sort_unstable();
        let mut atlas = Atlas::new(SPRITE_PAGE_SIZE);
        let mut infos = Vec::with_capacity(paths.len());
        let mut names = FxHashMap::default();
        for path in paths {
            let Some(png) = resources.read(path)? else {
                continue;
            };
            let meta_path = format!("{path}.mcmeta");
            let meta = resources.read(&meta_path)?;
            let sprite = match decode_sprite(path, &png, meta.as_deref()) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("skipping sprite {path}: {e}");
                    continue;
                }
            };
            let Some(slot) = atlas.alloc(sprite.width, sprite.height) else {
                tracing::warn!("sprite {path} does not fit an atlas page");
                continue;
            };
            atlas.write(slot, sprite.width, sprite.height, &sprite.frames[0]);
            let scaling = meta
                .as_deref()
                .and_then(|m| serde_json::from_slice::<Value>(m).ok())
                .and_then(|v| parse_scaling(v.get("gui")?.get("scaling")?))
                .unwrap_or(Scaling::Stretch);
            let name = path[SPRITE_ROOT.len()..path.len() - 4].to_owned();
            names.insert(name, SpriteId(infos.len() as u32));
            infos.push(SpriteInfo {
                page: slot.page,
                x: slot.x,
                y: slot.y,
                width: sprite.width,
                height: sprite.height,
                scaling,
            });
        }
        Ok(Self {
            atlas,
            loaded: infos.len() as u32,
            infos,
            names,
            freed: FxHashMap::default(),
            freed_ids: Default::default(),
            releaser: SpriteReleaser::default(),
        })
    }

    /// Ids only mean something to this `Sprites`, so a handle kept across a reload frees nothing.
    pub fn releaser(&self) -> SpriteReleaser {
        self.releaser.clone()
    }

    fn collect_released(&mut self) {
        let ids = match self.releaser.0.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(_) => return,
        };
        for id in ids {
            self.free_raw(id);
        }
    }

    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    pub fn atlas_mut(&mut self) -> &mut Atlas {
        &mut self.atlas
    }

    pub fn len(&self) -> usize {
        self.infos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.infos.is_empty()
    }

    pub fn id(&self, name: &str) -> Option<SpriteId> {
        self.names
            .get(name.strip_prefix("minecraft:").unwrap_or(name))
            .copied()
    }

    pub fn info(&self, id: SpriteId) -> &SpriteInfo {
        &self.infos[id.0 as usize]
    }

    /// Adds an unnamed straight-alpha RGBA image at runtime; `None` when it exceeds an atlas page.
    pub fn add_raw(&mut self, width: u32, height: u32, rgba: &[u8]) -> Option<SpriteId> {
        self.collect_released();
        if let Some(id) = self.freed.get_mut(&(width, height)).and_then(Vec::pop) {
            self.freed_ids.remove(&id.0);
            self.rewrite(id, rgba);
            return Some(id);
        }
        let slot = self.atlas.alloc(width, height)?;
        self.atlas.write(slot, width, height, rgba);
        let id = SpriteId(self.infos.len() as u32);
        self.infos.push(SpriteInfo {
            page: slot.page,
            x: slot.x,
            y: slot.y,
            width,
            height,
            scaling: Scaling::Stretch,
        });
        Some(id)
    }

    /// Releases a sprite from [`Sprites::add_raw`]; the next `add_raw` of the same size reuses its slot.
    /// The caller must stop drawing `id`. Loaded sprites and repeated frees are ignored.
    pub fn free_raw(&mut self, id: SpriteId) {
        if id.0 < self.loaded || id.0 as usize >= self.infos.len() || !self.freed_ids.insert(id.0) {
            return;
        }
        let info = self.infos[id.0 as usize];
        self.freed
            .entry((info.width, info.height))
            .or_default()
            .push(id);
    }

    /// Overwrites the pixels of a sprite from [`Sprites::add_raw`] with an image of the same size.
    pub fn rewrite(&mut self, id: SpriteId, rgba: &[u8]) {
        let info = self.infos[id.0 as usize];
        let slot = crate::atlas::Slot {
            page: info.page,
            x: info.x,
            y: info.y,
        };
        self.atlas.write(slot, info.width, info.height, rgba);
    }
}

fn parse_scaling(v: &Value) -> Option<Scaling> {
    let int = |key: &str| v.get(key)?.as_u64().map(|n| n as u32);
    match v.get("type")?.as_str()? {
        "stretch" => Some(Scaling::Stretch),
        "tile" => Some(Scaling::Tile {
            width: int("width")?,
            height: int("height")?,
        }),
        "nine_slice" => {
            let b = v.get("border")?;
            let border = match b.as_u64() {
                Some(n) => [n as u32; 4],
                None => {
                    let side = |k: &str| b.get(k)?.as_u64().map(|n| n as u32);
                    [side("left")?, side("top")?, side("right")?, side("bottom")?]
                }
            };
            Some(Scaling::NineSlice {
                width: int("width")?,
                height: int("height")?,
                border,
                stretch_inner: v
                    .get("stretch_inner")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(scaling: Scaling, w: u32, h: u32) -> SpriteInfo {
        SpriteInfo {
            page: 0,
            x: 100,
            y: 200,
            width: w,
            height: h,
            scaling,
        }
    }

    fn quads(i: &SpriteInfo, rect: (i32, i32, i32, i32)) -> Vec<SpriteQuad> {
        let mut out = Vec::new();
        i.layout(1000, rect, |q| out.push(q));
        out
    }

    fn covered_area(q: &[SpriteQuad]) -> f32 {
        q.iter()
            .map(|q| (q.dst[2] - q.dst[0]) * (q.dst[3] - q.dst[1]))
            .sum()
    }

    #[test]
    fn stretch_is_one_quad_covering_the_sprite() {
        let q = quads(&info(Scaling::Stretch, 20, 10), (5, 6, 40, 30));
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].dst, [5.0, 6.0, 45.0, 36.0]);
        assert_eq!(q[0].uv, [0.1, 0.2, 0.12, 0.21]);
    }

    #[test]
    fn nine_slice_keeps_corners_and_covers_the_rect() {
        let s = Scaling::NineSlice {
            width: 20,
            height: 20,
            border: [3, 3, 3, 3],
            stretch_inner: true,
        };
        let q = quads(&info(s, 20, 20), (0, 0, 100, 50));
        assert_eq!(q.len(), 9);
        assert_eq!(covered_area(&q), 5000.0);
        assert_eq!(q[0].dst, [0.0, 0.0, 3.0, 3.0]);
        assert_eq!(q[8].dst, [97.0, 47.0, 100.0, 50.0]);
    }

    #[test]
    fn nine_slice_tiles_edges_and_crops_the_last_tile() {
        let s = Scaling::NineSlice {
            width: 10,
            height: 10,
            border: [2, 2, 2, 2],
            stretch_inner: false,
        };
        let q = quads(&info(s, 10, 10), (0, 0, 20, 10));
        assert_eq!(covered_area(&q), 200.0);
        let top_mid: Vec<_> = q
            .iter()
            .filter(|q| q.dst[1] == 0.0 && q.dst[0] >= 2.0 && q.dst[2] <= 18.0)
            .collect();
        assert_eq!(top_mid.len(), 3);
        let last = top_mid[2];
        assert_eq!(last.dst[2] - last.dst[0], 4.0);
        assert!((last.uv[2] - last.uv[0] - 4.0 / 1000.0).abs() < 1e-6);
    }

    #[test]
    fn nine_slice_border_clamps_on_tiny_destinations() {
        let s = Scaling::NineSlice {
            width: 20,
            height: 20,
            border: [8, 8, 8, 8],
            stretch_inner: false,
        };
        let q = quads(&info(s, 20, 20), (0, 0, 6, 6));
        assert_eq!(covered_area(&q), 36.0);
    }

    #[test]
    fn asymmetric_border_parses() {
        let v: Value = serde_json::from_str(
            r#"{"type":"nine_slice","width":130,"height":24,"border":{"left":2,"top":2,"right":2,"bottom":0}}"#,
        )
        .unwrap();
        assert_eq!(
            parse_scaling(&v),
            Some(Scaling::NineSlice {
                width: 130,
                height: 24,
                border: [2, 2, 2, 0],
                stretch_inner: false
            })
        );
    }

    #[test]
    fn tile_repeats_from_the_top_left() {
        let q = quads(
            &info(
                Scaling::Tile {
                    width: 16,
                    height: 16,
                },
                16,
                16,
            ),
            (0, 0, 40, 16),
        );
        assert_eq!(q.len(), 3);
        assert_eq!(q[2].dst, [32.0, 0.0, 40.0, 16.0]);
    }

    #[test]
    fn region_maps_pixels_one_to_one() {
        let q = info(Scaling::Stretch, 182, 5).region(1000, (0, 0), (10, 20, 91, 5));
        assert_eq!(q.dst, [10.0, 20.0, 101.0, 25.0]);
        assert!((q.uv[2] - 0.191).abs() < 1e-6);
    }
}

#[cfg(test)]
mod raw_tests {
    use super::*;

    fn empty() -> Sprites {
        Sprites {
            atlas: Atlas::new(64),
            infos: Vec::new(),
            names: FxHashMap::default(),
            loaded: 0,
            freed: FxHashMap::default(),
            freed_ids: Default::default(),
            releaser: SpriteReleaser::default(),
        }
    }

    #[test]
    fn owned_sprites_return_to_the_atlas_when_dropped() {
        let mut sprites = empty();
        let pixels = vec![1u8; 16 * 16 * 4];
        for _ in 0..2_000 {
            let mut owned = OwnedSprites::default();
            let a = owned.add(&mut sprites, 16, 16, &pixels).expect("fits");
            owned.add(&mut sprites, 16, 16, &pixels).expect("fits");
            owned.free(&mut sprites, a);
            assert_eq!(owned.len(), 1);
        }
        assert!(sprites.len() <= 3);
        assert_eq!(sprites.atlas().pages().len(), 1);
    }

    #[test]
    fn released_sprites_are_reused_after_their_owner_drops() {
        let mut sprites = empty();
        let pixels = vec![1u8; 16 * 16 * 4];
        for _ in 0..5_000 {
            let owned: Vec<SpriteId> = (0..4)
                .filter_map(|_| sprites.add_raw(16, 16, &pixels))
                .collect();
            sprites.releaser().release(owned);
        }
        assert_eq!(sprites.len(), 4);
        assert_eq!(sprites.atlas().pages().len(), 1);
    }

    #[test]
    fn freed_slots_are_reused_so_pages_stay_bounded() {
        let mut sprites = empty();
        let pixels = vec![255u8; 16 * 16 * 4];
        for _ in 0..10_000 {
            let id = sprites.add_raw(16, 16, &pixels).expect("fits");
            sprites.free_raw(id);
            sprites.free_raw(id);
        }
        assert_eq!(sprites.atlas().pages().len(), 1);
        assert_eq!(sprites.len(), 1);
    }

    #[test]
    fn double_free_does_not_hand_one_slot_to_two_sprites() {
        let mut sprites = empty();
        let pixels = vec![0u8; 8 * 8 * 4];
        let id = sprites.add_raw(8, 8, &pixels).expect("fits");
        sprites.free_raw(id);
        sprites.free_raw(id);
        let a = sprites.add_raw(8, 8, &pixels).expect("fits");
        let b = sprites.add_raw(8, 8, &pixels).expect("fits");
        assert_ne!(a, b);
    }
}
