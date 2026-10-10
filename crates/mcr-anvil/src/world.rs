use std::path::{Path, PathBuf};
use std::sync::Arc;

use mcr_world::{BlockState, ChunkColumn, ChunkPos, WorldConfig};
use rayon::prelude::*;

use crate::chunk::{Converter, ReadOptions, Stats, StatsSnapshot};
use crate::level::LevelSummary;
use crate::region::RegionFile;
use crate::resolve::{BiomeTable, BlockResolver};
use crate::{AnvilError, Result};

const BIOME_LIMIT: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dimension {
    pub id: String,
    pub min_y: i32,
    pub height: u32,
}

impl Dimension {
    pub fn new(id: impl Into<String>, min_y: i32, height: u32) -> Self {
        Self {
            id: id.into(),
            min_y,
            height,
        }
    }

    pub fn overworld() -> Self {
        Self::new("minecraft:overworld", -64, 384)
    }

    pub fn nether() -> Self {
        Self::new("minecraft:the_nether", 0, 256)
    }

    pub fn end() -> Self {
        Self::new("minecraft:the_end", 0, 256)
    }

    /// `dimensions/<namespace>/<path>/region` first, then the pre-26 layout.
    pub fn region_dir(&self, world: &Path) -> Option<PathBuf> {
        let (ns, path) = self.id.split_once(':').unwrap_or(("minecraft", &self.id));
        let modern = world.join("dimensions").join(ns).join(path).join("region");
        if modern.is_dir() {
            return Some(modern);
        }
        let legacy = match self.id.as_str() {
            "minecraft:overworld" => world.join("region"),
            "minecraft:the_nether" => world.join("DIM-1").join("region"),
            "minecraft:the_end" => world.join("DIM1").join("region"),
            _ => return None,
        };
        legacy.is_dir().then_some(legacy)
    }
}

#[derive(Debug)]
pub struct LoadedChunk {
    pub pos: ChunkPos,
    pub column: ChunkColumn,
}

#[derive(Debug)]
pub struct World {
    summary: LevelSummary,
    dimension: Dimension,
    region_dir: PathBuf,
    converter: Converter,
    biomes: Arc<BiomeTable>,
    stats: Arc<Stats>,
}

impl World {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path, Dimension::overworld(), ReadOptions::default())
    }

    pub fn open_with(
        path: impl AsRef<Path>,
        dimension: Dimension,
        options: ReadOptions,
    ) -> Result<Self> {
        let path = path.as_ref();
        let summary = LevelSummary::read(path)?;
        let region_dir = dimension.region_dir(path).ok_or_else(|| {
            AnvilError::NotAWorld(format!("{}: no region folder", path.display()))
        })?;
        let version = mcr_data::v26_3();
        let resolver = Arc::new(BlockResolver::new(version));
        let air = ["minecraft:air", "minecraft:cave_air", "minecraft:void_air"]
            .map(|n| BlockState(resolver.default_state(n).unwrap_or(0)));
        let cfg = WorldConfig::new(
            resolver.state_count(),
            BIOME_LIMIT as u32,
            dimension.min_y,
            dimension.height,
        )?
        .with_air(air);
        let biomes = Arc::new(BiomeTable::new(BIOME_LIMIT));
        let stats = Arc::new(Stats::default());
        let converter = Converter::new(cfg, resolver, biomes.clone(), options, stats.clone());
        Ok(Self {
            summary,
            dimension,
            region_dir,
            converter,
            biomes,
            stats,
        })
    }

    pub fn summary(&self) -> &LevelSummary {
        &self.summary
    }

    pub fn dimension(&self) -> &Dimension {
        &self.dimension
    }

    pub fn config(&self) -> &WorldConfig {
        self.converter.config()
    }

    /// Biome names indexed by the ids used in loaded columns.
    pub fn biome_names(&self) -> Vec<String> {
        self.biomes.names()
    }

    pub fn stats(&self) -> StatsSnapshot {
        self.stats.snapshot()
    }

    /// Region coordinates of every `r.<x>.<z>.mca` file present.
    pub fn regions(&self) -> Result<Vec<(i32, i32)>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.region_dir)? {
            let name = entry?.file_name();
            let name = name.to_string_lossy();
            let Some(rest) = name.strip_prefix("r.").and_then(|r| r.strip_suffix(".mca")) else {
                continue;
            };
            if let Some((x, z)) = rest.split_once('.')
                && let (Ok(x), Ok(z)) = (x.parse(), z.parse())
            {
                out.push((x, z));
            }
        }
        out.sort_unstable();
        Ok(out)
    }

    pub fn open_region(&self, rx: i32, rz: i32) -> Result<Option<RegionFile>> {
        let path = self.region_dir.join(format!("r.{rx}.{rz}.mca"));
        match RegionFile::open(&path, rx, rz) {
            Ok(r) => Ok(Some(r)),
            Err(AnvilError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn load_one(&self, region: &RegionFile, lx: usize, lz: usize) -> Option<LoadedChunk> {
        let result = region
            .read_chunk(lx, lz)
            .and_then(|nbt| nbt.map_or(Ok(None), |nbt| self.converter.convert(&nbt)));
        match result {
            Ok(found) => found.map(|(pos, column)| LoadedChunk { pos, column }),
            Err(e) => {
                self.stats
                    .chunks_corrupt
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tracing::warn!(
                    chunk_x = region.region_x() * 32 + lx as i32,
                    chunk_z = region.region_z() * 32 + lz as i32,
                    error = %e,
                    "skipping unreadable chunk"
                );
                None
            }
        }
    }

    /// Unreadable chunks are skipped and counted in [`Self::stats`].
    pub fn load_region(&self, rx: i32, rz: i32) -> Result<Vec<LoadedChunk>> {
        let Some(region) = self.open_region(rx, rz)? else {
            return Ok(Vec::new());
        };
        Ok((0..1024usize)
            .into_par_iter()
            .filter(|&s| region.has_chunk(s % 32, s / 32))
            .filter_map(|s| self.load_one(&region, s % 32, s / 32))
            .collect())
    }

    /// Like [`Self::load_chunk`] with the region already opened by [`Self::open_region`].
    pub fn load_chunk_in(&self, region: &RegionFile, cx: i32, cz: i32) -> Option<LoadedChunk> {
        self.load_one(region, (cx & 31) as usize, (cz & 31) as usize)
    }

    pub fn load_chunk(&self, cx: i32, cz: i32) -> Result<Option<LoadedChunk>> {
        let Some(region) = self.open_region(cx >> 5, cz >> 5)? else {
            return Ok(None);
        };
        Ok(self.load_one(&region, (cx & 31) as usize, (cz & 31) as usize))
    }

    /// Every existing chunk in the square of half-width `radius` around the centre, in parallel.
    pub fn load_chunks_around(
        &self,
        chunk_x: i32,
        chunk_z: i32,
        radius: u32,
    ) -> Result<Vec<LoadedChunk>> {
        let r = radius as i32;
        let (x0, x1, z0, z1) = (chunk_x - r, chunk_x + r, chunk_z - r, chunk_z + r);
        let regions: Vec<(i32, i32)> = (z0 >> 5..=z1 >> 5)
            .flat_map(|rz| (x0 >> 5..=x1 >> 5).map(move |rx| (rx, rz)))
            .collect();
        let batches = regions
            .into_par_iter()
            .map(|(rx, rz)| {
                let Some(region) = self.open_region(rx, rz)? else {
                    return Ok(Vec::new());
                };
                Ok((0..1024usize)
                    .into_par_iter()
                    .filter(|&s| {
                        let (cx, cz) = (rx * 32 + (s % 32) as i32, rz * 32 + (s / 32) as i32);
                        (x0..=x1).contains(&cx)
                            && (z0..=z1).contains(&cz)
                            && region.has_chunk(s % 32, s / 32)
                    })
                    .filter_map(|s| self.load_one(&region, s % 32, s / 32))
                    .collect::<Vec<_>>())
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(batches.into_iter().flatten().collect())
    }
}

#[cfg(test)]
mod tests {
    use mcr_nbt::{Compound, List, Value};

    use super::*;
    use crate::region::COMPRESSION_ZLIB;
    use crate::region::testutil::RegionWriter;

    fn chunk(x: i32, z: i32, status: &str) -> Compound {
        let mut states = Compound::new();
        let mut stone = Compound::new();
        stone.insert("Name", "minecraft:stone");
        states.insert("palette", List::Compound(vec![stone]));
        let mut biomes = Compound::new();
        biomes.insert("palette", List::String(vec!["minecraft:plains".into()]));
        let mut section = Compound::new();
        section.insert("Y", 0i8);
        section.insert("block_states", states);
        section.insert("biomes", biomes);
        let mut c = Compound::new();
        c.insert("DataVersion", 5023);
        c.insert("xPos", x);
        c.insert("zPos", z);
        c.insert("Status", status);
        c.insert("sections", List::Compound(vec![section]));
        c
    }

    fn make_world(modern: bool, tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "mcr-anvil-world-{}-{modern}-{tag}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let regions = if modern {
            root.join("dimensions/minecraft/overworld/region")
        } else {
            root.join("region")
        };
        std::fs::create_dir_all(&regions).unwrap();
        let mut data = Compound::new();
        data.insert("LevelName", "T");
        let mut level = Compound::new();
        level.insert("Data", Value::Compound(data));
        std::fs::write(
            root.join("level.dat"),
            mcr_nbt::write_disk("", &level, mcr_nbt::Compression::Gzip).unwrap(),
        )
        .unwrap();
        let mut w = RegionWriter::new();
        w.put(0, 0, &chunk(0, 0, "minecraft:full"), COMPRESSION_ZLIB);
        w.put(1, 0, &chunk(1, 0, "minecraft:full"), COMPRESSION_ZLIB);
        w.put(2, 0, &chunk(2, 0, "minecraft:features"), COMPRESSION_ZLIB);
        w.put_raw(3, 0, COMPRESSION_ZLIB, b"garbage");
        std::fs::write(regions.join("r.0.0.mca"), w.finish()).unwrap();
        let mut w = RegionWriter::new();
        w.put(31, 31, &chunk(-1, -1, "minecraft:full"), COMPRESSION_ZLIB);
        std::fs::write(regions.join("r.-1.-1.mca"), w.finish()).unwrap();
        root
    }

    #[test]
    fn both_layouts_load_and_filter() {
        for modern in [true, false] {
            let root = make_world(modern, "layouts");
            let world = World::open(&root).unwrap();
            assert_eq!(world.summary().name, "T");
            assert_eq!(world.regions().unwrap(), [(-1, -1), (0, 0)]);
            let mut got: Vec<_> = world
                .load_chunks_around(0, 0, 3)
                .unwrap()
                .iter()
                .map(|c| (c.pos.x, c.pos.z))
                .collect();
            got.sort();
            assert_eq!(got, [(-1, -1), (0, 0), (1, 0)]);
            let s = world.stats();
            assert_eq!((s.chunks_skipped_status, s.chunks_corrupt), (1, 1));
            assert_eq!(world.load_region(0, 0).unwrap().len(), 2);
            assert!(world.load_region(5, 5).unwrap().is_empty());
            assert!(world.load_chunk(1, 0).unwrap().is_some());
            assert_eq!(world.biome_names(), ["minecraft:plains"]);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn lax_status_includes_features() {
        let root = make_world(true, "lax");
        let opts = ReadOptions {
            min_status: crate::ChunkStatus::Features,
        };
        let world = World::open_with(&root, Dimension::overworld(), opts).unwrap();
        assert_eq!(world.load_chunks_around(1, 0, 1).unwrap().len(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_dimension_is_an_error() {
        let root = make_world(false, "missing");
        assert!(World::open_with(&root, Dimension::nether(), ReadOptions::default()).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
