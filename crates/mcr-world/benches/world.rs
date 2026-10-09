use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use mcr_world::{
    BLOCK_ENTRIES, BlockContainer, ChunkData, ChunkSection, Heightmap, HeightmapKind, Heightmaps,
    WorldConfig,
};

fn config() -> WorldConfig {
    WorldConfig::new(35723, 80, -64, 384).unwrap()
}

fn terrain(distinct: usize, seed: usize) -> [u16; BLOCK_ENTRIES] {
    let mut v = [0u16; BLOCK_ENTRIES];
    for (i, slot) in v.iter_mut().enumerate() {
        let layer = i >> 8;
        *slot = ((layer * 3 + (i * seed) % 5) % distinct) as u16 * 5;
    }
    v
}

fn column(cfg: &WorldConfig) -> Vec<u8> {
    let sections = (0..cfg.section_count())
        .map(|s| {
            let values = terrain(if s < 16 { 24 } else { 2 }, s + 1);
            let blocks = BlockContainer::from_values(&values, cfg.block_limits());
            let biomes = mcr_world::BiomeContainer::from_values(
                &std::array::from_fn(|i| (i % 3) as u16),
                cfg.biome_limits(),
            );
            ChunkSection::from_containers(blocks, biomes, 4096, 0)
        })
        .collect();
    let mut heightmaps = Heightmaps::default();
    for kind in [HeightmapKind::WorldSurface, HeightmapKind::MotionBlocking] {
        heightmaps.set(kind, Heightmap::empty(cfg));
    }
    let mut bytes = Vec::new();
    ChunkData {
        heightmaps,
        sections,
        block_entities: Vec::new(),
    }
    .encode(&mut bytes);
    bytes
}

fn bench(c: &mut Criterion) {
    let cfg = config();
    let limits = cfg.block_limits();
    let mut group = c.benchmark_group("container");
    for (name, distinct) in [("indirect4", 12), ("indirect8", 200), ("direct", 3000)] {
        let values = terrain(distinct, 7);
        let container = BlockContainer::from_values(&values, limits);
        group.bench_function(format!("get_all/{name}"), |b| {
            b.iter(|| {
                let mut acc = 0u32;
                for i in 0..BLOCK_ENTRIES {
                    acc += container.get(black_box(i)) as u32;
                }
                acc
            })
        });
        group.bench_function(format!("unpack/{name}"), |b| {
            let mut out = [0u16; BLOCK_ENTRIES];
            b.iter(|| {
                container.unpack_into(&mut out);
                black_box(&out);
            })
        });
        group.bench_function(format!("set_existing/{name}"), |b| {
            let mut c = container.clone();
            let value = values[0];
            b.iter(|| {
                for i in (0..BLOCK_ENTRIES).step_by(16) {
                    c.set(black_box(i), value);
                }
            })
        });
    }
    group.bench_function("set_growing", |b| {
        b.iter(|| {
            let mut c = BlockContainer::filled(0, limits);
            for i in 0..BLOCK_ENTRIES {
                c.set(i, (i % 300) as u16);
            }
            c
        })
    });
    group.finish();

    let bytes = column(&cfg);
    c.bench_function("decode_column_24_sections", |b| {
        b.iter(|| {
            let mut input = bytes.as_slice();
            ChunkData::decode(black_box(&mut input), &cfg).unwrap()
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
