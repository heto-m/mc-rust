use std::io::{Cursor, Read};

use mcr_assets::{Identifier, ResourceManager, decode_sprite};
use rustc_hash::FxHashMap;
use serde_json::Value;

use crate::atlas::Atlas;
use crate::error::{Result, UiError};
use crate::style::FontId;

const GLYPH_PAGE_SIZE: u32 = 512;
const MAX_REFERENCE_DEPTH: u32 = 8;
const UV_NUDGE: f32 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphInfo {
    pub advance: f32,
    pub shadow_offset: f32,
    pub bold_offset: f32,
}

impl GlyphInfo {
    pub fn simple(advance: f32) -> Self {
        Self {
            advance,
            shadow_offset: 1.0,
            bold_offset: 1.0,
        }
    }

    pub fn advance(&self, bold: bool) -> f32 {
        self.advance + if bold { self.bold_offset } else { 0.0 }
    }

    fn fishy(&self) -> bool {
        let ok = |a: f32| (0.0..=32.0).contains(&a);
        !ok(self.advance) || !ok(self.advance(true))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FontOptions {
    pub uniform: bool,
    pub jp: bool,
}

impl FontOptions {
    fn passes(&self, filter: Option<&Value>) -> bool {
        let Some(map) = filter.and_then(Value::as_object) else {
            return true;
        };
        map.iter().all(|(key, want)| {
            let have = match key.as_str() {
                "uniform" => self.uniform,
                "jp" => self.jp,
                _ => return true,
            };
            want.as_bool().is_none_or(|w| w == have)
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct BitmapCell {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    scale: f32,
    ascent: f32,
}

#[derive(Debug)]
struct Sheet {
    width: u32,
    rgba: Vec<u8>,
}

#[derive(Debug)]
struct Bitmap {
    sheet: Sheet,
    glyphs: FxHashMap<u32, (GlyphInfo, BitmapCell)>,
}

#[derive(Debug)]
struct Unihex {
    files: Vec<Vec<u8>>,
    index: FxHashMap<u32, (u32, u32, u16)>,
    overrides: Vec<(u32, u32, i32, i32)>,
}

struct HexGlyph {
    lines: [u32; 16],
    left: i32,
    right: i32,
}

impl Unihex {
    fn decode(&self, cp: u32) -> Option<HexGlyph> {
        let &(file, off, len) = self.index.get(&cp)?;
        let text = &self.files[file as usize][off as usize..off as usize + len as usize];
        let digits_per_line = text.len() / 16;
        let mut lines = [0u32; 16];
        let mut mask = 0u32;
        for (i, line) in lines.iter_mut().enumerate() {
            let mut v = 0u32;
            for &d in &text[i * digits_per_line..(i + 1) * digits_per_line] {
                v = v << 4 | (d as char).to_digit(16)?;
            }
            *line = v << (32 - digits_per_line * 4);
            mask |= *line;
        }
        let bit_width = digits_per_line as i32 * 4;
        let (left, right) = match self.overrides.iter().find(|r| (r.0..=r.1).contains(&cp)) {
            Some(r) => (r.2, r.3),
            None if mask == 0 => (0, bit_width),
            None => (
                mask.leading_zeros() as i32,
                31 - mask.trailing_zeros() as i32,
            ),
        };
        Some(HexGlyph { lines, left, right })
    }

    fn info(glyph: &HexGlyph) -> GlyphInfo {
        let width = glyph.right - glyph.left + 1;
        GlyphInfo {
            advance: (width / 2 + 1) as f32,
            shadow_offset: 0.5,
            bold_offset: 0.5,
        }
    }
}

#[derive(Debug)]
enum Provider {
    Bitmap(Bitmap),
    Space(FxHashMap<u32, f32>),
    Unihex(Unihex),
}

#[derive(Clone, Copy, Debug)]
enum Source {
    Missing,
    Empty,
    Bitmap {
        set: u8,
        provider: u16,
        cell: BitmapCell,
    },
    Unihex {
        set: u8,
        provider: u16,
        cp: u32,
    },
}

impl Provider {
    fn get(&self, set: u8, provider: u16, cp: u32) -> Option<(GlyphInfo, Source)> {
        match self {
            Self::Bitmap(b) => b.glyphs.get(&cp).map(|&(info, cell)| {
                (
                    info,
                    Source::Bitmap {
                        set,
                        provider,
                        cell,
                    },
                )
            }),
            Self::Space(map) => map
                .get(&cp)
                .map(|&advance| (GlyphInfo::simple(advance), Source::Empty)),
            Self::Unihex(u) => u
                .decode(cp)
                .map(|g| (Unihex::info(&g), Source::Unihex { set, provider, cp })),
        }
    }

    fn supported(&self, out: &mut Vec<u32>) {
        match self {
            Self::Bitmap(b) => out.extend(b.glyphs.keys()),
            Self::Space(m) => out.extend(m.keys()),
            Self::Unihex(u) => out.extend(u.index.keys()),
        }
    }
}

#[derive(Debug, Default)]
struct FontSet {
    providers: Vec<Provider>,
    cache: FxHashMap<u32, u32>,
    by_width: Option<FxHashMap<u32, Vec<u32>>>,
}

impl FontSet {
    fn select(&self, set: u8, cp: u32) -> Option<(GlyphInfo, Source)> {
        self.providers
            .iter()
            .enumerate()
            .find_map(|(i, p)| p.get(set, i as u16, cp).filter(|(info, _)| !info.fishy()))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Baked {
    pub page: u32,
    pub uv: [f32; 4],
    pub left: f32,
    pub right: f32,
    pub up: f32,
    pub down: f32,
    pub empty: bool,
}

#[derive(Debug)]
struct Glyph {
    info: GlyphInfo,
    source: Source,
    baked: Option<Baked>,
}

#[derive(Debug)]
pub struct Fonts {
    sets: Vec<FontSet>,
    glyphs: Vec<Glyph>,
    atlas: Atlas,
    missing: u32,
}

impl Fonts {
    pub fn load(resources: &ResourceManager, options: FontOptions) -> Result<Self> {
        let mut sets = Vec::new();
        for id in 0..FontId::COUNT as u8 {
            let mut set = FontSet::default();
            let name = Identifier::new("minecraft", FontId(id).path());
            collect_providers(resources, &name, options, &mut set.providers, 0)?;
            sets.push(set);
        }
        let missing = Glyph {
            info: GlyphInfo::simple(6.0),
            source: Source::Missing,
            baked: None,
        };
        Ok(Self {
            sets,
            glyphs: vec![missing],
            atlas: Atlas::new(GLYPH_PAGE_SIZE),
            missing: 0,
        })
    }

    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    pub fn atlas_mut(&mut self) -> &mut Atlas {
        &mut self.atlas
    }

    pub fn glyph(&mut self, font: FontId, ch: char) -> u32 {
        if font.is_missing() {
            return self.missing;
        }
        let set_index = font.0 as usize % self.sets.len();
        let cp = ch as u32;
        if let Some(&idx) = self.sets[set_index].cache.get(&cp) {
            return idx;
        }
        let idx = match self.sets[set_index].select(set_index as u8, cp) {
            Some((info, source)) => {
                self.glyphs.push(Glyph {
                    info,
                    source,
                    baked: None,
                });
                (self.glyphs.len() - 1) as u32
            }
            None => self.missing,
        };
        self.sets[set_index].cache.insert(cp, idx);
        idx
    }

    pub fn info(&self, glyph: u32) -> GlyphInfo {
        self.glyphs[glyph as usize].info
    }

    pub fn advance(&mut self, font: FontId, ch: char, bold: bool) -> f32 {
        let g = self.glyph(font, ch);
        self.info(g).advance(bold)
    }

    /// Picks a deterministic pseudo-random glyph whose advance rounds up to the same width.
    pub fn random_glyph(&mut self, font: FontId, ch: char, rng: &mut u32) -> u32 {
        let base = self.glyph(font, ch);
        if font.is_missing() {
            return base;
        }
        let width = self.info(base).advance.ceil() as u32;
        let set_index = font.0 as usize % self.sets.len();
        if self.sets[set_index].by_width.is_none() {
            let mut cps = Vec::new();
            self.sets[set_index]
                .providers
                .iter()
                .for_each(|p| p.supported(&mut cps));
            cps.sort_unstable();
            cps.dedup();
            let mut map: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
            for cp in cps {
                if let Some((info, _)) = self.sets[set_index].select(set_index as u8, cp) {
                    map.entry(info.advance.ceil() as u32).or_default().push(cp);
                }
            }
            self.sets[set_index].by_width = Some(map);
        }
        *rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let pick = self.sets[set_index]
            .by_width
            .as_ref()
            .and_then(|m| m.get(&width))
            .filter(|l| !l.is_empty())
            .map(|l| l[(*rng >> 8) as usize % l.len()]);
        match pick.and_then(char::from_u32) {
            Some(c) => self.glyph(font, c),
            None => self.missing,
        }
    }

    pub fn baked(&mut self, glyph: u32) -> Baked {
        if let Some(b) = self.glyphs[glyph as usize].baked {
            return b;
        }
        let source = self.glyphs[glyph as usize].source;
        let baked = self.bake(source);
        self.glyphs[glyph as usize].baked = Some(baked);
        baked
    }

    fn bake(&mut self, source: Source) -> Baked {
        let empty = Baked {
            page: 0,
            uv: [0.0; 4],
            left: 0.0,
            right: 0.0,
            up: 0.0,
            down: 0.0,
            empty: true,
        };
        let (w, h, rgba, oversample, bearing_top) = match source {
            Source::Empty => return empty,
            Source::Missing => {
                let (w, h) = (5u32, 8u32);
                let mut px = vec![0u8; (w * h * 4) as usize];
                for y in 0..h {
                    for x in 0..w {
                        if x == 0 || x + 1 == w || y == 0 || y + 1 == h {
                            let i = ((y * w + x) * 4) as usize;
                            px[i..i + 4].copy_from_slice(&[255; 4]);
                        }
                    }
                }
                (w, h, px, 1.0, 7.0)
            }
            Source::Bitmap {
                set,
                provider,
                cell,
            } => {
                let Provider::Bitmap(b) = &self.sets[set as usize].providers[provider as usize]
                else {
                    return empty;
                };
                let mut px = Vec::with_capacity((cell.w * cell.h * 4) as usize);
                for row in 0..cell.h {
                    let start = (((cell.y + row) * b.sheet.width + cell.x) * 4) as usize;
                    px.extend_from_slice(&b.sheet.rgba[start..start + cell.w as usize * 4]);
                }
                (cell.w, cell.h, px, 1.0 / cell.scale, cell.ascent)
            }
            Source::Unihex { set, provider, cp } => {
                let Provider::Unihex(u) = &self.sets[set as usize].providers[provider as usize]
                else {
                    return empty;
                };
                let Some(g) = u.decode(cp) else { return empty };
                let w = (g.right - g.left + 1) as u32;
                let mut px = vec![0u8; (w * 16 * 4) as usize];
                for (row, line) in g.lines.iter().enumerate() {
                    for col in 0..w {
                        let bit = 31 - (g.left + col as i32);
                        if (0..32).contains(&bit) && (line >> bit) & 1 != 0 {
                            let i = (row as u32 * w + col) as usize * 4;
                            px[i..i + 4].copy_from_slice(&[255; 4]);
                        }
                    }
                }
                (w, 16, px, 2.0, 7.0)
            }
        };
        let Some(slot) = self.atlas.alloc(w, h) else {
            return empty;
        };
        self.atlas.write(slot, w, h, &rgba);
        let size = self.atlas.size() as f32;
        let (sx, sy) = (slot.x as f32, slot.y as f32);
        let up = 7.0 - bearing_top;
        Baked {
            page: slot.page,
            uv: [
                (sx + UV_NUDGE) / size,
                (sy + UV_NUDGE) / size,
                (sx - UV_NUDGE + w as f32) / size,
                (sy - UV_NUDGE + h as f32) / size,
            ],
            left: 0.0,
            right: w as f32 / oversample,
            up,
            down: up + h as f32 / oversample,
            empty: false,
        }
    }
}

fn codepoints(s: &str) -> Vec<u32> {
    s.chars().map(|c| c as u32).collect()
}

fn one_codepoint(v: &Value) -> Option<u32> {
    match v {
        Value::String(s) => s.chars().next().map(|c| c as u32),
        Value::Number(n) => n.as_u64().map(|n| n as u32),
        _ => None,
    }
}

fn collect_providers(
    resources: &ResourceManager,
    font: &Identifier,
    options: FontOptions,
    out: &mut Vec<Provider>,
    depth: u32,
) -> Result<()> {
    if depth > MAX_REFERENCE_DEPTH {
        return Err(UiError::Invalid(format!("font reference cycle at {font}")));
    }
    let path = font.resource_path("font", ".json");
    let Some(bytes) = resources.read(&path)? else {
        tracing::warn!("missing font definition {path}");
        return Ok(());
    };
    let json: Value = serde_json::from_slice(&bytes).map_err(|source| UiError::Json {
        path: path.clone(),
        source,
    })?;
    let defs = json
        .get("providers")
        .and_then(Value::as_array)
        .ok_or_else(|| UiError::Invalid(format!("{path}: no providers")))?;
    for def in defs {
        if !options.passes(def.get("filter")) {
            continue;
        }
        let kind = def.get("type").and_then(Value::as_str).unwrap_or_default();
        match kind {
            "reference" => {
                let id = def.get("id").and_then(Value::as_str).unwrap_or_default();
                collect_providers(resources, &Identifier::parse(id), options, out, depth + 1)?;
            }
            "space" => out.push(parse_space(def)),
            "bitmap" => {
                if let Some(p) = parse_bitmap(resources, def)? {
                    out.push(p);
                }
            }
            "unihex" => {
                if let Some(p) = parse_unihex(resources, def)? {
                    out.push(p);
                }
            }
            other => tracing::warn!("{path}: unsupported glyph provider type {other:?}"),
        }
    }
    Ok(())
}

fn parse_space(def: &Value) -> Provider {
    let mut map = FxHashMap::default();
    if let Some(advances) = def.get("advances").and_then(Value::as_object) {
        for (key, v) in advances {
            if let (Some(c), Some(a)) = (key.chars().next(), v.as_f64()) {
                map.insert(c as u32, a as f32);
            }
        }
    }
    Provider::Space(map)
}

fn parse_bitmap(resources: &ResourceManager, def: &Value) -> Result<Option<Provider>> {
    let file = def.get("file").and_then(Value::as_str).unwrap_or_default();
    let height = def.get("height").and_then(Value::as_f64).unwrap_or(8.0) as f32;
    let ascent = def.get("ascent").and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let grid: Vec<Vec<u32>> = def
        .get("chars")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_str)
                .map(codepoints)
                .collect()
        })
        .unwrap_or_default();
    if grid.is_empty() || grid[0].is_empty() || grid.iter().any(|r| r.len() != grid[0].len()) {
        return Err(UiError::Invalid(format!("bitmap {file}: bad chars grid")));
    }
    let id = Identifier::parse(file);
    let path = id.resource_path("textures", "");
    let Some(png) = resources.read(&path)? else {
        tracing::warn!("missing font texture {path}");
        return Ok(None);
    };
    let sprite = decode_sprite(&path, &png, None)?;
    let rgba = sprite.frames.into_iter().next().unwrap_or_default();
    let (cols, rows) = (grid[0].len() as u32, grid.len() as u32);
    let (cw, ch) = (sprite.width / cols, sprite.height / rows);
    let scale = height / ch as f32;
    let mut glyphs = FxHashMap::default();
    for (row, line) in grid.iter().enumerate() {
        for (col, &cp) in line.iter().enumerate() {
            if cp == 0 {
                continue;
            }
            let (x, y) = (col as u32 * cw, row as u32 * ch);
            let used = (0..cw)
                .rev()
                .find(|&dx| {
                    (0..ch)
                        .any(|dy| rgba[(((y + dy) * sprite.width + x + dx) * 4 + 3) as usize] != 0)
                })
                .map_or(0, |dx| dx + 1);
            let advance = (0.5 + used as f32 * scale) as i32 + 1;
            glyphs.insert(
                cp,
                (
                    GlyphInfo::simple(advance as f32),
                    BitmapCell {
                        x,
                        y,
                        w: cw,
                        h: ch,
                        scale,
                        ascent,
                    },
                ),
            );
        }
    }
    Ok(Some(Provider::Bitmap(Bitmap {
        sheet: Sheet {
            width: sprite.width,
            rgba,
        },
        glyphs,
    })))
}

fn parse_unihex(resources: &ResourceManager, def: &Value) -> Result<Option<Provider>> {
    let file = def
        .get("hex_file")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let id = Identifier::parse(file);
    let path = format!("assets/{}/{}", id.namespace(), id.path());
    let Some(zip_bytes) = resources.read(&path)? else {
        tracing::warn!("missing unihex archive {path}");
        return Ok(None);
    };
    let invalid = |e: &dyn std::fmt::Display| UiError::Invalid(format!("{path}: {e}"));
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes)).map_err(|e| invalid(&e))?;
    let mut files = Vec::new();
    let mut index = FxHashMap::default();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| invalid(&e))?;
        if !entry.name().map_err(|e| invalid(&e))?.ends_with(".hex") {
            continue;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data).map_err(|e| invalid(&e))?;
        index_hex(&data, files.len() as u32, &mut index);
        files.push(data);
    }
    let overrides = def
        .get("size_overrides")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|o| {
                    Some((
                        one_codepoint(o.get("from")?)?,
                        one_codepoint(o.get("to")?)?,
                        o.get("left")?.as_i64()? as i32,
                        o.get("right")?.as_i64()? as i32,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(Provider::Unihex(Unihex {
        files,
        index,
        overrides,
    })))
}

fn index_hex(data: &[u8], file: u32, index: &mut FxHashMap<u32, (u32, u32, u16)>) {
    let mut pos = 0;
    while pos < data.len() {
        let end = data[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(data.len(), |n| pos + n);
        let line = &data[pos..end];
        if let Some(colon) = line.iter().position(|&b| b == b':') {
            let digits = line[colon + 1..]
                .strip_suffix(b"\r")
                .unwrap_or(&line[colon + 1..]);
            let cp = line[..colon]
                .iter()
                .try_fold(0u32, |v, &d| Some(v << 4 | (d as char).to_digit(16)?));
            if let (Some(cp), true) = (cp, matches!(digits.len(), 32 | 64 | 96 | 128)) {
                index.insert(cp, (file, (pos + colon + 1) as u32, digits.len() as u16));
            }
        }
        pos = end + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph_from_hex(hex: &str, overrides: Vec<(u32, u32, i32, i32)>) -> (Unihex, HexGlyph) {
        let data = format!("0041:{hex}\n").into_bytes();
        let mut index = FxHashMap::default();
        index_hex(&data, 0, &mut index);
        let u = Unihex {
            files: vec![data],
            index,
            overrides,
        };
        let g = u.decode(0x41).unwrap();
        (u, g)
    }

    #[test]
    fn unihex_8_wide_glyph_width_and_advance() {
        let hex = "00".repeat(4) + "181818" + &"00".repeat(9);
        let (_, g) = glyph_from_hex(&hex, vec![]);
        assert_eq!((g.left, g.right), (3, 4));
        assert_eq!(Unihex::info(&g).advance, 2.0);
    }

    #[test]
    fn unihex_16_wide_glyph_and_override() {
        let hex = "0001".repeat(8) + &"8000".repeat(8);
        let (_, g) = glyph_from_hex(&hex, vec![]);
        assert_eq!((g.left, g.right), (0, 15));
        assert_eq!(Unihex::info(&g).advance, 9.0);
        let (_, g) = glyph_from_hex(&hex, vec![(0x40, 0x50, 1, 15)]);
        assert_eq!((g.left, g.right), (1, 15));
        assert_eq!(Unihex::info(&g).advance, 8.0);
    }

    #[test]
    fn hex_index_skips_malformed_lines() {
        let mut index = FxHashMap::default();
        index_hex(
            b"0041:00\n0042:0000000000000000000000000000000F\n",
            0,
            &mut index,
        );
        assert_eq!(index.len(), 1);
        assert!(index.contains_key(&0x42));
    }
}
