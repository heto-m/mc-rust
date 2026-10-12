//! Player skins from the `textures` profile property, fetched off-thread, plus the 8x8 heads the tab list shows.

use std::cell::RefCell;
use std::sync::mpsc::{Receiver, Sender, channel};

use mcr_auth::textures::{SkinSource, fetch_skin, parse_textures};
use mcr_client::PlayerList;
use mcr_entity_model::{Skin, default_skin};
use rustc_hash::{FxHashMap, FxHashSet};
use uuid::Uuid;

use crate::status::decode_png_rgba;

const MAX_SKINS: usize = 512;
const MAX_INFLIGHT: usize = 4;
const MAX_FAILED: usize = 256;

pub const HEAD_SIZE: u32 = 8;
pub type AssetReader = Box<dyn Fn(&str) -> Option<Vec<u8>>>;
pub type HeadPixels = [u8; (HEAD_SIZE * HEAD_SIZE * 4) as usize];

type Reply = (Uuid, String, Option<Skin>);

pub struct SkinStore {
    read: AssetReader,
    skins: FxHashMap<Uuid, Skin>,
    requested: FxHashMap<Uuid, String>,
    failed: FxHashSet<u64>,
    defaults: RefCell<FxHashMap<(&'static str, bool), Option<HeadPixels>>>,
    inflight: usize,
    tx: Sender<Reply>,
    rx: Receiver<Reply>,
}

impl std::fmt::Debug for SkinStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkinStore")
            .field("skins", &self.skins.len())
            .finish_non_exhaustive()
    }
}

impl SkinStore {
    pub fn new(read: AssetReader) -> Self {
        let (tx, rx) = channel();
        Self {
            read,
            skins: FxHashMap::default(),
            requested: FxHashMap::default(),
            failed: FxHashSet::default(),
            defaults: RefCell::default(),
            inflight: 0,
            tx,
            rx,
        }
    }

    /// Starts downloads for listed players whose profile names a skin, and collects finished ones.
    pub fn update(&mut self, players: &PlayerList) {
        let wanted: Vec<(Uuid, Option<&str>)> = players
            .iter()
            .map(|p| (p.uuid, p.textures.as_deref()))
            .collect();
        self.sync(&wanted);
    }

    fn mark_failed(&mut self, value: &str) {
        if self.failed.len() >= MAX_FAILED {
            self.failed.clear();
        }
        self.failed.insert(fx_hash(value));
    }

    fn sync(&mut self, wanted: &[(Uuid, Option<&str>)]) {
        while let Ok((uuid, value, skin)) = self.rx.try_recv() {
            self.inflight = self.inflight.saturating_sub(1);
            match skin {
                Some(skin) if self.requested.get(&uuid) == Some(&value) => {
                    self.skins.insert(uuid, skin);
                }
                Some(_) => {}
                None => self.mark_failed(&value),
            }
        }
        let present: FxHashSet<Uuid> = wanted.iter().map(|(u, _)| *u).collect();
        self.requested.retain(|u, _| present.contains(u));
        self.skins.retain(|u, _| present.contains(u));
        for &(uuid, value) in wanted {
            if self.inflight >= MAX_INFLIGHT || self.requested.len() >= MAX_SKINS {
                break;
            }
            let Some(value) = value else { continue };
            if self.requested.get(&uuid).is_some_and(|v| v == value)
                || self.failed.contains(&fx_hash(value))
            {
                continue;
            }
            let Some(source) = parse_textures(value) else {
                self.mark_failed(value);
                continue;
            };
            self.requested.insert(uuid, value.to_owned());
            self.inflight += 1;
            let (tx, value) = (self.tx.clone(), value.to_owned());
            std::thread::spawn(move || {
                let skin = std::panic::catch_unwind(|| download(&source)).unwrap_or(None);
                let _ = tx.send((uuid, value, skin));
            });
        }
    }

    pub fn get(&self, uuid: Uuid) -> Option<&Skin> {
        self.skins.get(&uuid)
    }

    /// A cache key and the head of a player's skin, or of the default skin their UUID picks.
    pub fn head(&self, uuid: Uuid, hat: bool) -> Option<(String, HeadPixels)> {
        if let Some(skin) = self.skins.get(&uuid) {
            let key = format!("{}:{hat}", skin.key);
            return head_of(skin.width, &skin.rgba, hat).map(|p| (key, p));
        }
        let (name, slim) = default_skin(uuid);
        let key = format!("default:{name}:{slim}:{hat}");
        let pixels = *self
            .defaults
            .borrow_mut()
            .entry((name, hat))
            .or_insert_with(|| {
                let dir = if slim { "slim" } else { "wide" };
                let bytes = (self.read)(&format!(
                    "assets/minecraft/textures/entity/player/{dir}/{name}.png"
                ))?;
                let (w, _, rgba) = decode_png_rgba(&bytes, valid_skin_size)?;
                head_of(w, &rgba, hat)
            });
        pixels.map(|p| (key, p))
    }
}

fn fx_hash(value: &str) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault};
    BuildHasherDefault::<rustc_hash::FxHasher>::default().hash_one(value)
}

fn valid_skin_size(w: u32, h: u32) -> bool {
    w == 64 && (h == 64 || h == 32)
}

fn download(source: &SkinSource) -> Option<Skin> {
    let bytes = fetch_skin(source).ok()?;
    let (width, height, rgba) = decode_png_rgba(&bytes, valid_skin_size)?;
    Some(Skin {
        key: source.url.clone(),
        width,
        height,
        rgba,
        slim: source.slim,
    })
}

/// The face, with the hat layer blended over it when worn, from a 64-wide skin.
pub fn head_of(width: u32, rgba: &[u8], hat: bool) -> Option<HeadPixels> {
    if width != 64 || rgba.len() < 64 * 16 * 4 {
        return None;
    }
    let px = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * width + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    let mut out = [0u8; (HEAD_SIZE * HEAD_SIZE * 4) as usize];
    for y in 0..HEAD_SIZE {
        for x in 0..HEAD_SIZE {
            let face = px(8 + x, 8 + y);
            let layer = if hat { px(40 + x, 8 + y) } else { [0; 4] };
            let a = u32::from(layer[3]);
            let o = ((y * HEAD_SIZE + x) * 4) as usize;
            for c in 0..3 {
                out[o + c] =
                    ((u32::from(layer[c]) * a + u32::from(face[c]) * (255 - a)) / 255) as u8;
            }
            out[o + 3] = 255;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skin_with(face: [u8; 4], hat: [u8; 4]) -> Vec<u8> {
        let mut rgba = vec![0u8; 64 * 64 * 4];
        for y in 0..8 {
            for x in 0..8 {
                for (ox, c) in [(8, face), (40, hat)] {
                    let i = (((8 + y) * 64 + ox + x) * 4) as usize;
                    rgba[i..i + 4].copy_from_slice(&c);
                }
            }
        }
        rgba
    }

    #[test]
    fn head_blends_the_hat_over_the_face() {
        let plain = skin_with([200, 100, 50, 255], [0, 0, 0, 0]);
        assert_eq!(
            &head_of(64, &plain, true).unwrap()[..4],
            &[200, 100, 50, 255]
        );
        let hatted = skin_with([200, 100, 50, 255], [10, 20, 30, 255]);
        assert_eq!(
            &head_of(64, &hatted, true).unwrap()[..4],
            &[10, 20, 30, 255]
        );
        assert_eq!(
            &head_of(64, &hatted, false).unwrap()[..4],
            &[200, 100, 50, 255]
        );
        let half = head_of(64, &skin_with([200, 0, 0, 255], [0, 0, 200, 128]), true).unwrap();
        assert!(half[0] < 110 && half[2] > 90);
    }

    #[test]
    fn cycling_bad_values_keeps_failed_bounded() {
        let mut store = SkinStore::new(Box::new(|_| None));
        for i in 0..(MAX_FAILED * 4) {
            let value = format!("bad-{i}");
            store.sync(&[(Uuid::from_u128(1), Some(&value))]);
            assert!(store.failed.len() <= MAX_FAILED);
        }
    }

    #[test]
    fn departed_players_free_their_slots() {
        let mut store = SkinStore::new(Box::new(|_| None));
        for i in 0..MAX_SKINS as u128 {
            store.requested.insert(Uuid::from_u128(i), "v".into());
        }
        store.sync(&[(Uuid::from_u128(1_000_000), Some("bad"))]);
        assert!(store.requested.is_empty());
        assert!(store.get(Uuid::from_u128(0)).is_none());
    }

    #[test]
    fn short_or_narrow_images_have_no_head() {
        assert!(head_of(32, &[0; 32 * 32 * 4], true).is_none());
        assert!(head_of(64, &[0; 16], true).is_none());
    }

    #[test]
    fn default_skins_come_from_the_reader() {
        let mut png = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png, 64, 64);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&skin_with([1, 2, 3, 255], [0, 0, 0, 0]))
                .unwrap();
        }
        let store = SkinStore::new(Box::new(move |_| Some(png.clone())));
        let (key, head) = store.head(Uuid::nil(), true).unwrap();
        assert!(key.starts_with("default:"));
        assert_eq!(&head[..4], &[1, 2, 3, 255]);
        let missing = SkinStore::new(Box::new(|_| None));
        assert!(missing.head(Uuid::nil(), true).is_none());
    }
}
