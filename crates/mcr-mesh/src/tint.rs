//! Tint kinds, constant tint palette and biome colour tables.

use std::path::Path;

use mcr_assets::{ResourceManager, decode_sprite};
use mcr_render::{TINT_TABLE_WORDS, TintTable};

use crate::biome_zoom::zoom_biome;
use crate::input::{MeshInput, quart_index};
use crate::simplex::Simplex;

pub const TINT_NONE: u8 = 0;
pub const TINT_GRASS: u8 = 1;
pub const TINT_FOLIAGE: u8 = 2;
pub const TINT_DRY_FOLIAGE: u8 = 3;
pub const TINT_WATER: u8 = 4;
pub const TINT_CONST_BASE: u8 = 8;

const CONST_SPRUCE: u8 = 0;
const CONST_BIRCH: u8 = 1;
const CONST_LILY: u8 = 2;
const CONST_ATTACHED_STEM: u8 = 3;
const CONST_STEM: u8 = 4;
const CONST_REDSTONE: u8 = 12;

pub const PALETTE_LEN: usize = 28;

pub const DEFAULT_BLEND_RADIUS: usize = 2;
pub const MAX_BLEND_RADIUS: usize = 7;

/// Tint tables one section keeps at most, for biomes that change with height.
pub const MAX_TINT_SLOTS: usize = 4;

const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | g << 8 | b << 16
}

fn from_hex(hex: u32) -> u32 {
    rgb(hex >> 16 & 255, hex >> 8 & 255, hex & 255)
}

/// Constant tint colours (`r | g << 8 | b << 16`), indexed `kind - TINT_CONST_BASE`.
pub fn constant_palette() -> [u32; PALETTE_LEN] {
    let mut p = [0u32; PALETTE_LEN];
    p[CONST_SPRUCE as usize] = from_hex(0x619961);
    p[CONST_BIRCH as usize] = from_hex(0x80a755);
    p[CONST_LILY as usize] = from_hex(0x208030);
    p[CONST_ATTACHED_STEM as usize] = from_hex(0xe0c71c);
    for age in 0..8u32 {
        p[CONST_STEM as usize + age as usize] = rgb(age * 32, 255 - age * 8, age * 4);
    }
    for power in 0..16 {
        let t = power as f32 / 15.0;
        let red = t * 0.6 + if power > 0 { 0.4 } else { 0.3 };
        let green = (t * t * 0.7 - 0.5).clamp(0.0, 1.0);
        let blue = (t * t * 0.6 - 0.7).clamp(0.0, 1.0);
        let q = |c: f32| (c * 255.0).round() as u32;
        p[CONST_REDSTONE as usize + power] = rgb(q(red), q(green), q(blue));
    }
    p
}

const fn constant(index: u8) -> u8 {
    TINT_CONST_BASE + index
}

fn prop<'a>(props: &[(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    props.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

/// Tint kinds for model tint indices 0 and 1 of a block state.
pub fn tint_kinds(block: &str, props: &[(&str, &str)]) -> [u8; 2] {
    let name = block.strip_prefix("minecraft:").unwrap_or(block);
    let number = |key: &str| prop(props, key).and_then(|v| v.parse::<u8>().ok());
    match name {
        "tall_grass" | "large_fern" | "fern" | "short_grass" | "potted_fern" | "bush"
        | "grass_block" | "sugar_cane" => [TINT_GRASS, TINT_NONE],
        "pink_petals" | "wildflowers" => [TINT_NONE, TINT_GRASS],
        "spruce_leaves" => [constant(CONST_SPRUCE), TINT_NONE],
        "birch_leaves" => [constant(CONST_BIRCH), TINT_NONE],
        "oak_leaves" | "jungle_leaves" | "acacia_leaves" | "dark_oak_leaves" | "vine"
        | "mangrove_leaves" => [TINT_FOLIAGE, TINT_NONE],
        "leaf_litter" => [TINT_DRY_FOLIAGE, TINT_NONE],
        "water_cauldron" | "water" | "bubble_column" => [TINT_WATER, TINT_NONE],
        "redstone_wire" => [
            constant(CONST_REDSTONE + number("power").unwrap_or(0).min(15)),
            TINT_NONE,
        ],
        "attached_melon_stem" | "attached_pumpkin_stem" => {
            [constant(CONST_ATTACHED_STEM), TINT_NONE]
        }
        "melon_stem" | "pumpkin_stem" => [
            constant(CONST_STEM + number("age").unwrap_or(0).min(7)),
            TINT_NONE,
        ],
        "lily_pad" => [constant(CONST_LILY), TINT_NONE],
        _ => [TINT_NONE; 2],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GrassModifier {
    #[default]
    None,
    DarkForest,
    Swamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BiomeClimate {
    pub temperature: f32,
    pub downfall: f32,
    pub water: u32,
    pub grass: Option<u32>,
    pub foliage: Option<u32>,
    pub dry_foliage: Option<u32>,
    pub modifier: GrassModifier,
}

impl Default for BiomeClimate {
    fn default() -> Self {
        Self {
            temperature: 0.8,
            downfall: 0.4,
            water: 0x3f76e4,
            grass: None,
            foliage: None,
            dry_foliage: None,
            modifier: GrassModifier::None,
        }
    }
}

fn parse_hex(v: &serde_json::Value) -> Option<u32> {
    match v {
        serde_json::Value::String(s) => u32::from_str_radix(s.trim_start_matches('#'), 16).ok(),
        serde_json::Value::Number(n) => n.as_u64().map(|n| n as u32 & 0xff_ffff),
        _ => None,
    }
}

impl BiomeClimate {
    /// Parses a worldgen biome JSON document (`temperature`, `downfall`, `effects`).
    pub fn from_json(json: &str) -> Option<Self> {
        let root: serde_json::Value = serde_json::from_str(json).ok()?;
        let effects = root.get("effects")?;
        let modifier = match effects.get("grass_color_modifier").and_then(|v| v.as_str()) {
            Some("dark_forest") => GrassModifier::DarkForest,
            Some("swamp") => GrassModifier::Swamp,
            _ => GrassModifier::None,
        };
        Some(Self {
            temperature: root.get("temperature")?.as_f64()? as f32,
            downfall: root.get("downfall")?.as_f64()? as f32,
            water: effects.get("water_color").and_then(parse_hex)?,
            grass: effects.get("grass_color").and_then(parse_hex),
            foliage: effects.get("foliage_color").and_then(parse_hex),
            dry_foliage: effects.get("dry_foliage_color").and_then(parse_hex),
            modifier,
        })
    }
}

fn nbt_hex(v: &mcr_nbt::Value) -> Option<u32> {
    match v {
        mcr_nbt::Value::String(s) => u32::from_str_radix(s.trim_start_matches('#'), 16).ok(),
        other => other.as_i64().map(|n| n as u32 & 0xff_ffff),
    }
}

impl BiomeClimate {
    /// A biome registry entry, as the server sends it or as filled from the client jar.
    pub fn from_nbt(root: &mcr_nbt::Compound) -> Option<Self> {
        let effects = root.get_compound("effects")?;
        let modifier = match effects.get_str("grass_color_modifier") {
            Some("dark_forest") => GrassModifier::DarkForest,
            Some("swamp") => GrassModifier::Swamp,
            _ => GrassModifier::None,
        };
        let color = |key: &str| effects.get(key).and_then(nbt_hex);
        Some(Self {
            temperature: root.get_f32("temperature")?,
            downfall: root.get_f32("downfall")?,
            water: color("water_color")?,
            grass: color("grass_color"),
            foliage: color("foliage_color"),
            dry_foliage: color("dry_foliage_color"),
            modifier,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ColorMaps {
    grass: Vec<u32>,
    foliage: Vec<u32>,
    dry_foliage: Vec<u32>,
}

fn load_map(resources: &ResourceManager, name: &str, fallback: u32) -> Vec<u32> {
    let path = format!("assets/minecraft/textures/colormap/{name}.png");
    let decoded = resources
        .read(&path)
        .ok()
        .flatten()
        .and_then(|bytes| decode_sprite(&path, &bytes, None).ok())
        .filter(|s| s.width == 256 && s.height == 256);
    match decoded {
        Some(sprite) => sprite.frames[0]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| rgb(p[0].into(), p[1].into(), p[2].into()))
            .collect(),
        None => {
            tracing::warn!(%path, "colormap unavailable, using a flat colour");
            vec![fallback; 65536]
        }
    }
}

fn sample(map: &[u32], temp: f32, rain: f32) -> u32 {
    let temp = f64::from(temp.clamp(0.0, 1.0));
    let rain = f64::from(rain.clamp(0.0, 1.0)) * temp;
    let x = ((1.0 - temp) * 255.0) as usize;
    let y = ((1.0 - rain) * 255.0) as usize;
    map.get(y << 8 | x).copied().unwrap_or(map[0])
}

impl ColorMaps {
    pub fn load(resources: &ResourceManager) -> Self {
        Self {
            grass: load_map(resources, "grass", from_hex(0x91bd59)),
            foliage: load_map(resources, "foliage", from_hex(0x77ab2f)),
            dry_foliage: load_map(resources, "dry_foliage", from_hex(0xa0a05a)),
        }
    }

    pub fn flat(grass: u32, foliage: u32, dry_foliage: u32) -> Self {
        Self {
            grass: vec![grass; 65536],
            foliage: vec![foliage; 65536],
            dry_foliage: vec![dry_foliage; 65536],
        }
    }
}

/// Per-biome colours (`r | g << 8 | b << 16`) for the biome tint kinds, indexed by biome id.
#[derive(Debug, Clone)]
pub struct BiomeColors {
    grass: Vec<u32>,
    foliage: Vec<u32>,
    dry_foliage: Vec<u32>,
    water: Vec<u32>,
    swamp: Vec<bool>,
}

fn swamp_noise() -> &'static Simplex {
    static NOISE: std::sync::OnceLock<Simplex> = std::sync::OnceLock::new();
    NOISE.get_or_init(|| Simplex::new(2345))
}

fn modify_grass(modifier: GrassModifier, base: u32) -> u32 {
    let (r, g, b) = (base & 255, base >> 8 & 255, base >> 16 & 255);
    let hex = r << 16 | g << 8 | b;
    match modifier {
        GrassModifier::None => base,
        GrassModifier::DarkForest => from_hex(((hex & 0xfefefe) + 0x28340a) >> 1),
        GrassModifier::Swamp => from_hex(0x6a7039),
    }
}

impl BiomeColors {
    /// An empty list yields a single plains entry, so quads of not-yet-known biomes still get tinted.
    pub fn from_climates(climates: &[BiomeClimate], maps: &ColorMaps) -> Self {
        let plains = [BiomeClimate::default()];
        let climates = if climates.is_empty() {
            &plains[..]
        } else {
            climates
        };
        let mut out = Self {
            grass: Vec::new(),
            foliage: Vec::new(),
            dry_foliage: Vec::new(),
            water: Vec::new(),
            swamp: Vec::new(),
        };
        for c in climates {
            out.swamp.push(c.modifier == GrassModifier::Swamp);
            let grass = c
                .grass
                .map_or_else(|| sample(&maps.grass, c.temperature, c.downfall), from_hex);
            out.grass.push(modify_grass(c.modifier, grass));
            out.foliage.push(c.foliage.map_or_else(
                || sample(&maps.foliage, c.temperature, c.downfall),
                from_hex,
            ));
            out.dry_foliage.push(c.dry_foliage.map_or_else(
                || sample(&maps.dry_foliage, c.temperature, c.downfall),
                from_hex,
            ));
            out.water.push(from_hex(c.water));
        }
        out
    }

    /// Loads `<dir>/<name>.json` for each biome name in registry order; unreadable ones use plains.
    pub fn from_json_dir(dir: &Path, names: &[&str], maps: &ColorMaps) -> Self {
        let climates: Vec<BiomeClimate> = names
            .iter()
            .map(|name| {
                let file = name.strip_prefix("minecraft:").unwrap_or(name);
                std::fs::read_to_string(dir.join(format!("{file}.json")))
                    .ok()
                    .and_then(|s| BiomeClimate::from_json(&s))
                    .unwrap_or_else(|| {
                        tracing::warn!(biome = %name, "biome json unavailable, using plains colours");
                        BiomeClimate::default()
                    })
            })
            .collect();
        Self::from_climates(&climates, maps)
    }

    /// The unblended foliage colour of a biome as `0xRRGGBB`.
    pub fn foliage_rgb(&self, biome: usize) -> u32 {
        let c = self
            .foliage
            .get(biome)
            .or(self.foliage.first())
            .copied()
            .unwrap_or(0x48b518);
        (c & 255) << 16 | (c >> 8 & 255) << 8 | (c >> 16 & 255)
    }

    pub fn len(&self) -> usize {
        self.grass.len()
    }

    pub fn is_empty(&self) -> bool {
        self.grass.is_empty()
    }

    fn colors(&self, kind: u8) -> &[u32] {
        match kind {
            TINT_GRASS => &self.grass,
            TINT_FOLIAGE => &self.foliage,
            TINT_DRY_FOLIAGE => &self.dry_foliage,
            _ => &self.water,
        }
    }

    fn color(&self, kind: u8, biome: u16) -> u32 {
        let colors = self.colors(kind);
        colors
            .get(usize::from(biome))
            .or(colors.first())
            .copied()
            .unwrap_or(0xffffff)
    }

    fn blur(colors: &[u32], radius: usize, out: &mut [u32]) {
        let side = 16 + 2 * radius;
        let span = 2 * radius + 1;
        let count = (span * span) as u32;
        let mut rows = vec![[0u32; 3]; 16 * side];
        for z in 0..side {
            for x in 0..16 {
                let mut sum = [0u32; 3];
                for c in &colors[z * side + x..z * side + x + span] {
                    sum[0] += c & 255;
                    sum[1] += c >> 8 & 255;
                    sum[2] += c >> 16 & 255;
                }
                rows[z * 16 + x] = sum;
            }
        }
        for z in 0..16 {
            for x in 0..16 {
                let mut sum = [0u32; 3];
                for dz in 0..span {
                    let r = rows[(z + dz) * 16 + x];
                    sum[0] += r[0];
                    sum[1] += r[1];
                    sum[2] += r[2];
                }
                out[z * 16 + x] = rgb(sum[0] / count, sum[1] / count, sum[2] / count);
            }
        }
    }

    /// Fills `table` for a 16x16 area from a biome id window of side `16 + 2 * radius`, averaging the
    /// colours over `(2 * radius + 1)^2` columns like vanilla's biome blend.
    pub fn fill_table(&self, window: &[u16], radius: usize, table: &mut TintTable) {
        let side = 16 + 2 * radius;
        debug_assert_eq!(window.len(), side * side);
        let uniform = window.iter().all(|&b| b == window[0]);
        for kind in TINT_GRASS..=TINT_WATER {
            let out = &mut table[(kind as usize - 1) * 256..][..256];
            if uniform {
                out.fill(self.color(kind, window[0]));
                continue;
            }
            let colors: Vec<u32> = window.iter().map(|&b| self.color(kind, b)).collect();
            Self::blur(&colors, radius, out);
        }
    }

    fn grass_at(&self, biome: u16, wx: i32, wz: i32) -> u32 {
        if self.swamp.get(usize::from(biome)).copied().unwrap_or(false) {
            let noise = swamp_noise().get(f64::from(wx) * 0.0225, f64::from(wz) * 0.0225);
            return if noise < -0.1 {
                rgb(0x4c, 0x3a, 0x3c)
            } else {
                rgb(0x6a, 0x70, 0x39)
            };
        }
        self.color(TINT_GRASS, biome)
    }

    /// The tint tables of a section, like vanilla's `calculateBlockTint` at each tinted block's own height:
    /// one [`TintTable`] per distinct biome window among the y levels set in `levels` (bit `y` of each
    /// column, section-local), at most [`MAX_TINT_SLOTS`]. The top byte of word `y` of the first table
    /// holds the table index for level `y`.
    pub fn fill_tables(&self, input: &MeshInput, levels: &[u16; 256]) -> Box<[u32]> {
        let radius = input.blend_radius;
        let side = 16 + 2 * radius;
        let uniform = input.biomes.iter().all(|&b| b == input.biomes[0]);
        let swamp_any = input
            .biomes
            .iter()
            .any(|&b| self.swamp.get(usize::from(b)).copied().unwrap_or(false));
        if uniform && !swamp_any {
            let mut table = vec![0u32; TINT_TABLE_WORDS];
            for kind in TINT_GRASS..=TINT_WATER {
                table[(kind as usize - 1) * 256..][..256].fill(self.color(kind, input.biomes[0]));
            }
            return table.into_boxed_slice();
        }
        let origin = [input.pos.x * 16, input.pos.y * 16, input.pos.z * 16];
        let used = levels.iter().fold(0u16, |a, &m| a | m);
        let used = if used == 0 { 1 << 8 } else { used };
        let mut windows: Vec<Vec<u16>> = Vec::new();
        let mut slot_of_level = [0u8; 16];
        let mut last = 0u8;
        for y in (0..16).filter(|y| used >> y & 1 != 0) {
            let ids: Vec<u16> = (0..side * side)
                .map(|i| {
                    if uniform {
                        return input.biomes[0];
                    }
                    let wx = (i % side) as i32 - radius as i32;
                    let wz = (i / side) as i32 - radius as i32;
                    zoom_biome(
                        input.biome_seed,
                        origin[0] + wx,
                        origin[1] + y,
                        origin[2] + wz,
                        |qx, qy, qz| {
                            input.biomes[quart_index(
                                qx - input.pos.x * 4,
                                qy - input.pos.y * 4,
                                qz - input.pos.z * 4,
                            )]
                        },
                    )
                })
                .collect();
            last = match windows.iter().position(|w| *w == ids) {
                Some(s) => s as u8,
                None if windows.len() < MAX_TINT_SLOTS => {
                    windows.push(ids);
                    (windows.len() - 1) as u8
                }
                None => last,
            };
            slot_of_level[y as usize] = last;
        }
        let mut tables = vec![0u32; windows.len() * TINT_TABLE_WORDS];
        for (slot, ids) in windows.iter().enumerate() {
            for kind in TINT_GRASS..=TINT_WATER {
                let colors: Vec<u32> = ids
                    .iter()
                    .enumerate()
                    .map(|(i, &b)| {
                        if kind == TINT_GRASS {
                            let wx = origin[0] + (i % side) as i32 - radius as i32;
                            let wz = origin[2] + (i / side) as i32 - radius as i32;
                            self.grass_at(b, wx, wz)
                        } else {
                            self.color(kind, b)
                        }
                    })
                    .collect();
                let at = slot * TINT_TABLE_WORDS + (kind as usize - 1) * 256;
                Self::blur(&colors, radius, &mut tables[at..at + 256]);
            }
        }
        for (y, &slot) in slot_of_level.iter().enumerate() {
            tables[y] |= u32::from(slot) << 24;
        }
        tables.into_boxed_slice()
    }

    pub fn new_table(&self) -> Box<TintTable> {
        Box::new([0; TINT_TABLE_WORDS])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_biomes() -> BiomeColors {
        let a = BiomeClimate {
            grass: Some(0xff0000),
            ..BiomeClimate::default()
        };
        let b = BiomeClimate {
            grass: Some(0x0000ff),
            ..BiomeClimate::default()
        };
        BiomeColors::from_climates(&[a, b], &ColorMaps::flat(0, 0, 0))
    }

    fn window(radius: usize, f: impl Fn(usize, usize) -> u16) -> Vec<u16> {
        let side = 16 + 2 * radius;
        (0..side * side).map(|i| f(i % side, i / side)).collect()
    }

    #[test]
    fn radius_zero_uses_own_biome() {
        let colors = two_biomes();
        let mut t = colors.new_table();
        colors.fill_table(&window(0, |x, _| u16::from(x >= 8)), 0, &mut t);
        assert_eq!(t[0], rgb(255, 0, 0));
        assert_eq!(t[8], rgb(0, 0, 255));
    }

    #[test]
    fn blend_averages_neighbouring_columns() {
        let colors = two_biomes();
        let mut t = colors.new_table();
        let r = 2;
        colors.fill_table(&window(r, |x, _| u16::from(x >= r + 8)), r, &mut t);
        assert_eq!(t[0], rgb(255, 0, 0));
        assert_eq!(t[15], rgb(0, 0, 255));
        let edge = t[7];
        assert_eq!(edge, rgb(255 * 3 / 5, 0, 255 * 2 / 5));
    }

    #[test]
    fn json_and_modifiers() {
        let swamp = BiomeClimate::from_json(
            r##"{"temperature":0.8,"downfall":0.9,"effects":{"water_color":"#617b64","foliage_color":"#6a7039","grass_color_modifier":"swamp"}}"##,
        )
        .unwrap();
        assert_eq!(swamp.modifier, GrassModifier::Swamp);
        assert_eq!(swamp.water, 0x617b64);
        let colors = BiomeColors::from_climates(&[swamp], &ColorMaps::flat(1, 2, 3));
        assert_eq!(colors.grass[0], from_hex(0x6a7039));
        assert_eq!(colors.foliage[0], from_hex(0x6a7039));
    }

    #[test]
    fn nbt_matches_json_and_reads_network_ints() {
        let doc = r##"{"temperature":0.8,"downfall":0.9,"effects":{"water_color":"#617b64","foliage_color":"#6a7039","grass_color_modifier":"swamp"}}"##;
        let from_json = BiomeClimate::from_json(doc).unwrap();
        let nbt = mcr_nbt::json::compound_from_slice(doc.as_bytes()).unwrap();
        assert_eq!(BiomeClimate::from_nbt(&nbt), Some(from_json));
        let mut effects = mcr_nbt::Compound::new();
        effects.insert("water_color", 0x3f76e4i32);
        let mut network = mcr_nbt::Compound::new();
        network.insert("temperature", 0.5f32);
        network.insert("downfall", 0.5f32);
        network.insert("effects", effects);
        let climate = BiomeClimate::from_nbt(&network).unwrap();
        assert_eq!(climate.water, 0x3f76e4);
        assert_eq!(climate.grass, None);
    }

    #[test]
    fn palette_and_kinds() {
        let p = constant_palette();
        assert_eq!(p[usize::from(CONST_SPRUCE)], from_hex(0x619961));
        assert_eq!(p[CONST_REDSTONE as usize + 15], rgb(255, 51, 0));
        let kinds = tint_kinds("minecraft:redstone_wire", &[("power", "15")]);
        assert_eq!(kinds[0], constant(CONST_REDSTONE + 15));
        assert_eq!(tint_kinds("minecraft:grass_block", &[])[0], TINT_GRASS);
        assert_eq!(tint_kinds("minecraft:stone", &[]), [0, 0]);
    }

    fn split_input(radius: usize) -> MeshInput {
        let mut input = MeshInput::blank(mcr_world::SectionPos::new(0, 4, 0), radius);
        input.biome_seed = 99;
        for qy in 0..crate::input::QUART_SPAN_Y as i32 {
            for qz in 0..crate::input::QUART_SPAN as i32 {
                for qx in 0..crate::input::QUART_SPAN as i32 {
                    let quart = qx + crate::input::QUART_ORIGIN_XZ;
                    input.biomes[(qy as usize * crate::input::QUART_SPAN + qz as usize)
                        * crate::input::QUART_SPAN
                        + qx as usize] = u16::from(quart >= 2);
                }
            }
        }
        input
    }

    #[test]
    fn zoomed_columns_follow_the_seeded_biome_border() {
        let colors = two_biomes();
        let input = split_input(0);
        let ys = [1u16 << 8; 256];
        let a = colors.fill_tables(&input, &ys);
        let b = colors.fill_tables(&input, &ys);
        assert_eq!(a[..256], b[..256]);
        assert_eq!(a[0], rgb(255, 0, 0));
        assert_eq!(a[15], rgb(0, 0, 255));
        let reds = (0..16).filter(|&x| a[x] == rgb(255, 0, 0)).count();
        assert!(
            (6..=10).contains(&reds),
            "border near x = 8, got {reds} red columns"
        );
    }

    #[test]
    fn tint_follows_each_blocks_height() {
        let colors = two_biomes();
        let mut input = MeshInput::blank(mcr_world::SectionPos::new(0, 4, 0), 0);
        let qy0 = -crate::input::QUART_ORIGIN_Y;
        for qy in 0..crate::input::QUART_SPAN_Y as i32 {
            for q in 0..crate::input::QUART_SPAN * crate::input::QUART_SPAN {
                input.biomes
                    [qy as usize * crate::input::QUART_SPAN * crate::input::QUART_SPAN + q] =
                    u16::from(qy >= qy0 + 2);
            }
        }
        let t = colors.fill_tables(&input, &[1 << 1 | 1 << 14; 256]);
        assert_eq!(t.len(), 2 * TINT_TABLE_WORDS);
        let at = |y: usize, col: usize| {
            let slot = (t[y] >> 24) as usize;
            t[slot * TINT_TABLE_WORDS + col] & 0xff_ffff
        };
        assert_eq!(at(1, 37), rgb(255, 0, 0), "low grass takes the lower biome");
        assert_eq!(
            at(14, 37),
            rgb(0, 0, 255),
            "high grass takes the upper biome"
        );
        let flat = colors.fill_tables(&input, &[1 << 1; 256]);
        assert_eq!(flat.len(), TINT_TABLE_WORDS);
    }

    #[test]
    fn swamp_grass_uses_position_noise() {
        let swamp = BiomeClimate {
            modifier: GrassModifier::Swamp,
            ..BiomeClimate::default()
        };
        let colors = BiomeColors::from_climates(&[swamp], &ColorMaps::flat(1, 2, 3));
        let mut input = MeshInput::blank(mcr_world::SectionPos::new(7, 4, -3), 0);
        input.biome_seed = 1;
        let t = colors.fill_tables(&input, &[1 << 8; 256]);
        let distinct: std::collections::HashSet<u32> =
            t[..256].iter().map(|c| c & 0xff_ffff).collect();
        assert!(
            distinct
                .iter()
                .all(|&c| c == rgb(0x6a, 0x70, 0x39) || c == rgb(0x4c, 0x3a, 0x3c))
        );
    }
}
