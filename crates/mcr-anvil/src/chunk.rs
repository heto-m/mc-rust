use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use mcr_nbt::{Compound, List, Value};
use mcr_world::{
    BIOME_ENTRIES, BLOCK_ENTRIES, BiomeContainer, BlockContainer, BlockEntityData, ChunkColumn,
    ChunkData, ChunkPos, ChunkSection, Heightmap, HeightmapKind, Heightmaps, LIGHT_BYTES,
    LightData, LightNibbles, LightUpdate, WorldConfig,
};

use crate::resolve::{BiomeTable, BlockResolver, Resolved};
use crate::{AnvilError, Result};

pub const SUPPORTED_DATA_VERSION: i32 = 5023;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChunkStatus {
    Empty,
    StructureStarts,
    StructureReferences,
    Biomes,
    Terrain,
    Features,
    InitializeLight,
    Light,
    Spawn,
    Full,
}

impl ChunkStatus {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name.strip_prefix("minecraft:").unwrap_or(name) {
            "empty" => Self::Empty,
            "structure_starts" => Self::StructureStarts,
            "structure_references" => Self::StructureReferences,
            "biomes" => Self::Biomes,
            "terrain" => Self::Terrain,
            "features" => Self::Features,
            "initialize_light" => Self::InitializeLight,
            "light" => Self::Light,
            "spawn" => Self::Spawn,
            "full" => Self::Full,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReadOptions {
    pub min_status: ChunkStatus,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            min_status: ChunkStatus::Full,
        }
    }
}

#[derive(Debug, Default)]
pub struct Stats {
    pub chunks_loaded: AtomicU64,
    pub chunks_skipped_status: AtomicU64,
    pub chunks_corrupt: AtomicU64,
    pub unknown_blocks: AtomicU64,
    pub bad_properties: AtomicU64,
    pub unknown_biomes: AtomicU64,
    pub unknown_block_entities: AtomicU64,
    pub wrong_data_version: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatsSnapshot {
    pub chunks_loaded: u64,
    pub chunks_skipped_status: u64,
    pub chunks_corrupt: u64,
    pub unknown_blocks: u64,
    pub bad_properties: u64,
    pub unknown_biomes: u64,
    pub unknown_block_entities: u64,
    pub wrong_data_version: u64,
}

impl Stats {
    pub fn snapshot(&self) -> StatsSnapshot {
        let get = |a: &AtomicU64| a.load(Ordering::Relaxed);
        StatsSnapshot {
            chunks_loaded: get(&self.chunks_loaded),
            chunks_skipped_status: get(&self.chunks_skipped_status),
            chunks_corrupt: get(&self.chunks_corrupt),
            unknown_blocks: get(&self.unknown_blocks),
            bad_properties: get(&self.bad_properties),
            unknown_biomes: get(&self.unknown_biomes),
            unknown_block_entities: get(&self.unknown_block_entities),
            wrong_data_version: get(&self.wrong_data_version),
        }
    }
}

fn bump(counter: &AtomicU64) -> u64 {
    counter.fetch_add(1, Ordering::Relaxed)
}

fn ceil_log2(n: usize) -> u8 {
    (usize::BITS - n.saturating_sub(1).leading_zeros()) as u8
}

/// Disk packing never spans longs: each holds `64 / bits` whole entries.
fn unpack<const N: usize>(data: &[i64], bits: u8, palette_len: usize) -> Result<[u16; N]> {
    let per_long = 64 / bits as usize;
    if data.len() < N.div_ceil(per_long) {
        return Err(AnvilError::Malformed("palette data too short"));
    }
    let mask = (1u64 << bits) - 1;
    let mut out = [0u16; N];
    for (i, slot) in out.iter_mut().enumerate() {
        let word = data[i / per_long] as u64;
        let v = (word >> ((i % per_long) * bits as usize)) & mask;
        if v as usize >= palette_len {
            return Err(AnvilError::Malformed("palette index out of range"));
        }
        *slot = v as u16;
    }
    Ok(out)
}

const HEIGHTMAP_KEYS: [(HeightmapKind, &str); 6] = [
    (HeightmapKind::WorldSurfaceWorldgen, "WORLD_SURFACE_WG"),
    (HeightmapKind::WorldSurface, "WORLD_SURFACE"),
    (HeightmapKind::OceanFloorWorldgen, "OCEAN_FLOOR_WG"),
    (HeightmapKind::OceanFloor, "OCEAN_FLOOR"),
    (HeightmapKind::MotionBlocking, "MOTION_BLOCKING"),
    (
        HeightmapKind::MotionBlockingNoLeaves,
        "MOTION_BLOCKING_NO_LEAVES",
    ),
];

/// Turns chunk NBT into columns; cheap to share between threads.
#[derive(Debug, Clone)]
pub struct Converter {
    cfg: WorldConfig,
    resolver: Arc<BlockResolver>,
    biomes: Arc<BiomeTable>,
    options: ReadOptions,
    stats: Arc<Stats>,
}

impl Converter {
    pub fn new(
        cfg: WorldConfig,
        resolver: Arc<BlockResolver>,
        biomes: Arc<BiomeTable>,
        options: ReadOptions,
        stats: Arc<Stats>,
    ) -> Self {
        Self {
            cfg,
            resolver,
            biomes,
            options,
            stats,
        }
    }

    pub fn config(&self) -> &WorldConfig {
        &self.cfg
    }

    pub fn stats(&self) -> &Arc<Stats> {
        &self.stats
    }

    /// `Ok(None)` when the chunk is below the minimum generation status.
    pub fn convert(&self, chunk: &Compound) -> Result<Option<(ChunkPos, ChunkColumn)>> {
        let status = match chunk.get_str("Status") {
            None => ChunkStatus::Full,
            Some(s) => match ChunkStatus::from_name(s) {
                Some(s) => s,
                None => {
                    bump(&self.stats.chunks_skipped_status);
                    return Ok(None);
                }
            },
        };
        if status < self.options.min_status {
            bump(&self.stats.chunks_skipped_status);
            return Ok(None);
        }
        if let Some(v) = chunk.get_i32("DataVersion")
            && v != SUPPORTED_DATA_VERSION
            && bump(&self.stats.wrong_data_version) == 0
        {
            tracing::warn!(
                found = v,
                expected = SUPPORTED_DATA_VERSION,
                "chunk data version differs from the supported one"
            );
        }
        let (Some(x), Some(z)) = (chunk.get_i32("xPos"), chunk.get_i32("zPos")) else {
            return Err(AnvilError::Malformed("missing chunk position"));
        };

        let min_section = self.cfg.min_y() >> 4;
        let count = self.cfg.section_count();
        let mut sections: Vec<ChunkSection> =
            (0..count).map(|_| ChunkSection::empty(&self.cfg)).collect();
        let mut light = LightData {
            sky: Vec::new(),
            block: Vec::new(),
        };
        if let Some(List::Compound(list)) = chunk.get_list("sections") {
            for entry in list {
                let Some(y) = entry.get_i8("Y").map(i32::from) else {
                    return Err(AnvilError::Malformed("section without Y"));
                };
                let index = y - min_section;
                if (-1..=count as i32).contains(&index) {
                    let slot = (index + 1) as u16;
                    for (key, out) in [
                        ("SkyLight", &mut light.sky),
                        ("BlockLight", &mut light.block),
                    ] {
                        if let Some(nibbles) = nibbles(entry, key) {
                            out.push(LightUpdate {
                                section: slot,
                                data: Some(nibbles),
                            });
                        }
                    }
                }
                if (0..count as i32).contains(&index) {
                    sections[index as usize] = self.section(entry)?;
                }
            }
        }

        let mut heightmaps = Heightmaps::default();
        if let Some(maps) = chunk.get_compound("Heightmaps") {
            for (kind, key) in HEIGHTMAP_KEYS {
                if let Some(longs) = maps.get_long_array(key) {
                    let raw: Box<[u64]> = longs.iter().map(|&l| l as u64).collect();
                    if let Ok(map) = Heightmap::from_raw(raw, &self.cfg) {
                        heightmaps.set(kind, map);
                    }
                }
            }
        }

        let block_entities = self.block_entities(chunk);
        let mut column = ChunkColumn::from_data(
            ChunkData {
                heightmaps,
                sections,
                block_entities,
            },
            &self.cfg,
        );
        column.apply_light(light);
        bump(&self.stats.chunks_loaded);
        Ok(Some((ChunkPos::new(x, z), column)))
    }

    fn section(&self, entry: &Compound) -> Result<ChunkSection> {
        let version = mcr_data::v26_3();
        let states = entry
            .get_compound("block_states")
            .ok_or(AnvilError::Malformed("section without block_states"))?;
        let entries: Vec<(&str, Option<&Compound>)> = match states.get_list("palette") {
            Some(List::Compound(p)) if !p.is_empty() => p
                .iter()
                .map(|e| {
                    (
                        e.get_str("Name").unwrap_or("minecraft:air"),
                        e.get_compound("Properties"),
                    )
                })
                .collect(),
            Some(List::String(p)) if !p.is_empty() => {
                p.iter().map(|n| (n.as_str(), None)).collect()
            }
            _ => return Err(AnvilError::Malformed("empty block palette")),
        };
        let mapped: Vec<u16> = entries
            .iter()
            .map(|&(name, props)| match self.resolver.resolve(name, props) {
                Resolved::Exact(s) => s,
                Resolved::BadProperty(s) => {
                    bump(&self.stats.bad_properties);
                    s
                }
                Resolved::UnknownBlock => {
                    bump(&self.stats.unknown_blocks);
                    0
                }
            })
            .collect();
        let infos: Vec<_> = mapped
            .iter()
            .map(|&s| version.block_state(s as u32))
            .collect();
        let is_air = |p: usize| infos[p].is_none_or(|i| i.is_air());
        let has_fluid = |p: usize| infos[p].is_some_and(|i| i.fluid.is_some());

        let (blocks, non_empty, fluids) = if mapped.len() == 1 {
            let n = if is_air(0) { 0 } else { BLOCK_ENTRIES as u16 };
            let f = if has_fluid(0) {
                BLOCK_ENTRIES as u16
            } else {
                0
            };
            (
                BlockContainer::filled(mapped[0], self.cfg.block_limits()),
                n,
                f,
            )
        } else {
            let data = states.get_long_array("data").unwrap_or(&[]);
            let bits = ceil_log2(mapped.len()).max(4);
            let idx = unpack::<BLOCK_ENTRIES>(data, bits, mapped.len())?;
            let mut values = [0u16; BLOCK_ENTRIES];
            let (mut non_empty, mut fluids) = (0u16, 0u16);
            for (out, &i) in values.iter_mut().zip(&idx) {
                *out = mapped[i as usize];
                non_empty += u16::from(!is_air(i as usize));
                fluids += u16::from(has_fluid(i as usize));
            }
            (
                BlockContainer::from_values(&values, self.cfg.block_limits()),
                non_empty,
                fluids,
            )
        };

        let biomes = self.biome_container(entry.get_compound("biomes"))?;
        Ok(ChunkSection::from_containers(
            blocks, biomes, non_empty, fluids,
        ))
    }

    fn biome_container(&self, biomes: Option<&Compound>) -> Result<BiomeContainer> {
        let limits = self.cfg.biome_limits();
        let Some(biomes) = biomes else {
            return Ok(BiomeContainer::filled(0, limits));
        };
        let names = match biomes.get_list("palette") {
            Some(List::String(p)) if !p.is_empty() => p,
            _ => return Err(AnvilError::Malformed("empty biome palette")),
        };
        let ids: Vec<u16> = names
            .iter()
            .map(|n| {
                self.biomes.intern(n).unwrap_or_else(|| {
                    bump(&self.stats.unknown_biomes);
                    0
                })
            })
            .collect();
        if ids.len() == 1 {
            return Ok(BiomeContainer::filled(ids[0], limits));
        }
        let data = biomes.get_long_array("data").unwrap_or(&[]);
        let idx = unpack::<BIOME_ENTRIES>(data, ceil_log2(ids.len()), ids.len())?;
        let mut values = [0u16; BIOME_ENTRIES];
        for (out, &i) in values.iter_mut().zip(&idx) {
            *out = ids[i as usize];
        }
        Ok(BiomeContainer::from_values(&values, limits))
    }

    fn block_entities(&self, chunk: &Compound) -> Vec<BlockEntityData> {
        let Some(List::Compound(list)) = chunk.get_list("block_entities") else {
            return Vec::new();
        };
        let types = mcr_data::v26_3().registry("minecraft:block_entity_type");
        let mut out = Vec::with_capacity(list.len());
        for be in list {
            let (Some(id), Some(x), Some(y), Some(z)) = (
                be.get_str("id"),
                be.get_i32("x"),
                be.get_i32("y"),
                be.get_i32("z"),
            ) else {
                bump(&self.stats.unknown_block_entities);
                continue;
            };
            let Some(type_id) = types.and_then(|t| t.id_of(id)) else {
                bump(&self.stats.unknown_block_entities);
                continue;
            };
            let mut rest = Compound::new();
            for (k, v) in be.iter() {
                if !matches!(k, "id" | "x" | "y" | "z") {
                    rest.insert(k, v.clone());
                }
            }
            let mut nbt = Vec::new();
            mcr_nbt::write_network(&mut nbt, Some(&Value::Compound(rest)));
            out.push(BlockEntityData {
                packed_xz: (((x & 15) << 4) | (z & 15)) as u8,
                y: y as i16,
                type_id,
                nbt: nbt.into(),
            });
        }
        out
    }
}

fn nibbles(entry: &Compound, key: &str) -> Option<LightNibbles> {
    let bytes = entry.get_byte_array(key)?;
    if bytes.len() != LIGHT_BYTES {
        return None;
    }
    let mut boxed = Box::new([0u8; LIGHT_BYTES]);
    for (o, &b) in boxed.iter_mut().zip(bytes) {
        *o = b as u8;
    }
    Some(LightNibbles::new(boxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn pack(indices: &[u16], bits: u8) -> Vec<i64> {
        let per = 64 / bits as usize;
        let mut out = vec![0i64; indices.len().div_ceil(per)];
        for (i, &v) in indices.iter().enumerate() {
            out[i / per] |= (v as i64) << ((i % per) * bits as usize);
        }
        out
    }

    fn converter(min: ChunkStatus) -> Converter {
        let cfg = WorldConfig::new(mcr_data::v26_3().state_count() as u32, 1024, -64, 384).unwrap();
        Converter::new(
            cfg,
            Arc::new(BlockResolver::new(mcr_data::v26_3())),
            Arc::new(BiomeTable::new(1024)),
            ReadOptions { min_status: min },
            Arc::default(),
        )
    }

    fn entry(name: &str, props: &[(&str, &str)]) -> Compound {
        let mut c = Compound::new();
        c.insert("Name", name);
        if !props.is_empty() {
            let mut p = Compound::new();
            for (k, v) in props {
                p.insert(*k, *v);
            }
            c.insert("Properties", p);
        }
        c
    }

    fn section(y: i8, palette: Vec<Compound>, indices: &[u16]) -> Compound {
        let mut states = Compound::new();
        let n = palette.len();
        states.insert("palette", List::Compound(palette));
        if n > 1 {
            states.insert("data", Value::LongArray(pack(indices, ceil_log2(n).max(4))));
        }
        let mut biomes = Compound::new();
        biomes.insert("palette", List::String(vec!["minecraft:plains".into()]));
        let mut s = Compound::new();
        s.insert("Y", y);
        s.insert("block_states", states);
        s.insert("biomes", biomes);
        s
    }

    fn chunk(status: &str, sections: Vec<Compound>) -> Compound {
        let mut c = Compound::new();
        c.insert("DataVersion", SUPPORTED_DATA_VERSION);
        c.insert("xPos", -3);
        c.insert("zPos", 7);
        c.insert("Status", status);
        c.insert("sections", List::Compound(sections));
        c
    }

    #[test]
    fn palette_maps_to_state_ids_and_counts() {
        let conv = converter(ChunkStatus::Full);
        let mut idx = vec![0u16; 4096];
        idx[0] = 1;
        idx[1] = 2;
        idx[2] = 3;
        let pal = vec![
            entry("minecraft:air", &[]),
            entry("minecraft:stone", &[]),
            entry(
                "minecraft:oak_stairs",
                &[("facing", "south"), ("half", "top")],
            ),
            entry("minecraft:not_a_block", &[]),
        ];
        let c = chunk("minecraft:full", vec![section(0, pal, &idx)]);
        let (pos, col) = conv.convert(&c).unwrap().unwrap();
        assert_eq!(pos, ChunkPos::new(-3, 7));
        let stairs = match conv.resolver.resolve(
            "minecraft:oak_stairs",
            entry("x", &[("facing", "south"), ("half", "top")]).get_compound("Properties"),
        ) {
            Resolved::Exact(s) => s,
            r => panic!("{r:?}"),
        };
        assert_eq!(col.block(0, 0, 0).map(|b| b.0), Some(1));
        assert_eq!(col.block(1, 0, 0).map(|b| b.0), Some(stairs));
        assert_eq!(col.block(2, 0, 0).map(|b| b.0), Some(0));
        assert_eq!(col.block(3, 0, 0).map(|b| b.0), Some(0));
        assert_eq!(col.sections()[4].non_empty_count(), 2);
        assert_eq!(conv.stats.snapshot().unknown_blocks, 1);
        assert_eq!(conv.stats.snapshot().chunks_loaded, 1);
    }

    #[test]
    fn single_entry_palette_fills_the_section() {
        let conv = converter(ChunkStatus::Full);
        let c = chunk(
            "minecraft:full",
            vec![section(-4, vec![entry("minecraft:stone", &[])], &[])],
        );
        let (_, col) = conv.convert(&c).unwrap().unwrap();
        assert_eq!(col.block(5, -64, 5).map(|b| b.0), Some(1));
        assert_eq!(col.block(5, -49, 15).map(|b| b.0), Some(1));
        assert_eq!(col.block(5, -48, 5).map(|b| b.0), Some(0));
        assert_eq!(col.sections()[0].non_empty_count(), 4096);
    }

    #[test]
    fn light_and_biomes_land_in_the_right_slots() {
        let conv = converter(ChunkStatus::Full);
        let mut s = section(-1, vec![entry("minecraft:air", &[])], &[]);
        let mut sky = vec![0i8; 2048];
        sky[0] = 0x2f;
        s.insert("SkyLight", Value::ByteArray(sky));
        let mut below = Compound::new();
        below.insert("Y", -5i8);
        below.insert("SkyLight", Value::ByteArray(vec![-1; 2048]));
        let c = chunk("minecraft:full", vec![s, below]);
        let (_, col) = conv.convert(&c).unwrap().unwrap();
        assert_eq!(col.sky_light(0, -16, 0), Some(15));
        assert_eq!(col.sky_light(1, -16, 0), Some(2));
        assert_eq!(col.sky_light(0, -70, 0), Some(15));
        assert_eq!(col.block_light(0, -16, 0), None);
        assert_eq!(conv.biomes.name(0).as_deref(), Some("minecraft:plains"));
    }

    #[test]
    fn status_gate_and_corruption() {
        let strict = converter(ChunkStatus::Full);
        assert!(
            strict
                .convert(&chunk("minecraft:features", vec![]))
                .unwrap()
                .is_none()
        );
        assert!(
            strict
                .convert(&chunk("minecraft:bogus", vec![]))
                .unwrap()
                .is_none()
        );
        let lax = converter(ChunkStatus::Features);
        assert!(
            lax.convert(&chunk("minecraft:features", vec![]))
                .unwrap()
                .is_some()
        );
        let bad = chunk(
            "minecraft:full",
            vec![section(
                0,
                vec![entry("minecraft:air", &[]), entry("minecraft:stone", &[])],
                &[9; 4096],
            )],
        );
        assert!(lax.convert(&bad).is_err());
        let short = section(
            0,
            vec![entry("minecraft:air", &[]), entry("minecraft:stone", &[])],
            &[],
        );
        short.get_compound("block_states").map(|_| ()).unwrap();
        assert!(lax.convert(&chunk("minecraft:full", vec![short])).is_err());
    }

    #[test]
    fn name_only_palettes_are_strings() {
        let conv = converter(ChunkStatus::Full);
        let mut idx = vec![0u16; 4096];
        idx[0] = 1;
        let mut sec = section(0, vec![], &[]);
        let mut states = Compound::new();
        states.insert(
            "palette",
            List::String(vec!["minecraft:air".into(), "minecraft:dirt".into()]),
        );
        states.insert("data", Value::LongArray(pack(&idx, 4)));
        sec.insert("block_states", states);
        let (_, col) = conv
            .convert(&chunk("minecraft:full", vec![sec]))
            .unwrap()
            .unwrap();
        let dirt = conv.resolver.default_state("minecraft:dirt");
        assert_eq!(col.block(0, 0, 0).map(|b| b.0), dirt);
        assert_eq!(col.block(1, 0, 0).map(|b| b.0), Some(0));
        assert_eq!(conv.stats.snapshot().unknown_blocks, 0);
    }

    #[test]
    fn block_entities_become_network_nbt() {
        let conv = converter(ChunkStatus::Full);
        let mut be = Compound::new();
        be.insert("id", "minecraft:chest");
        be.insert("x", -45);
        be.insert("y", 70);
        be.insert("z", 120);
        be.insert("lock", "k");
        let mut unknown = Compound::new();
        unknown.insert("id", "minecraft:nonsense");
        unknown.insert("x", 0);
        unknown.insert("y", 0);
        unknown.insert("z", 0);
        let mut c = chunk("minecraft:full", vec![]);
        c.insert("block_entities", List::Compound(vec![be, unknown]));
        let (_, col) = conv.convert(&c).unwrap().unwrap();
        let [e] = col.block_entities() else { panic!() };
        assert_eq!((e.x(), e.z(), e.y), (3, 8, 70));
        let mut input = &e.nbt[..];
        let v = mcr_nbt::read_network(&mut input, mcr_nbt::Limits::NETWORK)
            .unwrap()
            .unwrap();
        assert_eq!(v.as_compound().unwrap().get_str("lock"), Some("k"));
        assert_eq!(conv.stats.snapshot().unknown_block_entities, 1);
    }
}
