#[path = "../tests/common/mod.rs"]
mod common;

use std::hint::black_box;
use std::time::{Duration, Instant};

use common::{fixture, state_of};
use criterion::{Criterion, criterion_group, criterion_main};
use mcr_mesh::testing::{TerrainBlocks, fill_terrain_input, terrain_state};
use mcr_mesh::{MeshInput, MeshPool, Mesher};
use mcr_render::SectionMesh;
use mcr_world::{
    BiomeContainer, BlockContainer, ChunkColumn, ChunkData, ChunkPos, ChunkSection, ChunkStore,
    SectionPos, WorldConfig,
};

const SEED: u32 = 5;
const SIDE: i32 = 6;

fn blocks() -> TerrainBlocks {
    TerrainBlocks {
        stone: state_of("minecraft:stone", &[]),
        dirt: state_of("minecraft:dirt", &[]),
        grass_block: state_of("minecraft:grass_block", &[("snowy", "false")]),
        sand: state_of("minecraft:sand", &[]),
        water: state_of("minecraft:water", &[("level", "0")]),
        coal_ore: state_of("minecraft:coal_ore", &[]),
        iron_ore: state_of("minecraft:iron_ore", &[]),
        log: state_of("minecraft:oak_log", &[("axis", "y")]),
        leaves: state_of(
            "minecraft:oak_leaves",
            &[("persistent", "true"), ("waterlogged", "false")],
        ),
        short_grass: state_of("minecraft:short_grass", &[]),
    }
}

/// Sections that straddle the terrain surface, like the bulk of what a client meshes.
fn surface_inputs(b: &TerrainBlocks) -> Vec<MeshInput> {
    let mut out = Vec::new();
    for sz in 0..SIDE {
        for sx in 0..SIDE {
            for sy in 2..=6 {
                let mut input = MeshInput::blank(SectionPos::new(sx, sy, sz), 2);
                fill_terrain_input(&mut input, b, SEED, |_, _| 0);
                input.camera = Some([48.0, 72.0, 48.0]);
                if !input.is_all_air() && input.states.contains(&0) {
                    out.push(input);
                }
            }
        }
    }
    out
}

fn world_store(b: &TerrainBlocks) -> ChunkStore {
    let cfg = WorldConfig::new(35723, 80, -64, 384).unwrap();
    let mut store = ChunkStore::new(cfg);
    for cz in 0..SIDE {
        for cx in 0..SIDE {
            let sections = (0..cfg.section_count())
                .map(|i| {
                    let sy = i as i32 - 4;
                    let mut values = [0u32; 4096];
                    for (n, v) in values.iter_mut().enumerate() {
                        let (x, y, z) = ((n & 15) as i32, (n >> 8) as i32, (n >> 4 & 15) as i32);
                        *v = terrain_state(b, SEED, cx * 16 + x, sy * 16 + y, cz * 16 + z);
                    }
                    ChunkSection::from_containers(
                        BlockContainer::from_values(&values, cfg.block_limits()),
                        BiomeContainer::filled(0, cfg.biome_limits()),
                        values.iter().filter(|&&v| v != 0).count() as u16,
                        0,
                    )
                })
                .collect();
            let data = ChunkData {
                heightmaps: Default::default(),
                sections,
                block_entities: vec![],
            };
            store.insert(ChunkPos::new(cx, cz), ChunkColumn::from_data(data, &cfg));
        }
    }
    store
}

fn bench_mesh(c: &mut Criterion) {
    let Some(f) = fixture() else { return };
    let b = blocks();
    let inputs = surface_inputs(&b);
    println!("{} surface sections", inputs.len());
    let mut mesher = Mesher::new(f.cache.clone(), f.colors.clone());
    let mut out = SectionMesh::default();

    let start = Instant::now();
    let rounds = 20usize;
    let mut quads = 0;
    for _ in 0..rounds {
        for input in &inputs {
            mesher.mesh(black_box(input), &mut out);
            quads += out.cube_count() + out.model.len() + out.translucent.len();
        }
    }
    let per = start.elapsed() / (rounds * inputs.len()) as u32;
    println!(
        "single thread: {per:?} per section, {} quads per section on average",
        quads / (rounds * inputs.len())
    );

    c.bench_function("mesh_surface_sections_single_thread", |bench| {
        bench.iter(|| {
            for input in &inputs {
                mesher.mesh(black_box(input), &mut out);
            }
        })
    });

    let threads = std::thread::available_parallelism().map_or(4, usize::from);
    let mut pool = MeshPool::new(f.cache.clone(), f.colors.clone(), threads, 2);
    let rounds = 20u32;
    let mut sink = Vec::new();
    let mut total = Duration::ZERO;
    for _ in 0..rounds {
        let batch: Vec<MeshInput> = inputs.to_vec();
        let start = Instant::now();
        for (i, input) in batch.into_iter().enumerate() {
            pool.submit(input, i as u32);
        }
        pool.wait_idle(&mut sink);
        total += start.elapsed();
        sink.clear();
    }
    let per_round = total / rounds;
    println!(
        "{threads} threads: {per_round:?} per {} sections = {:.0} sections/s",
        inputs.len(),
        inputs.len() as f64 / per_round.as_secs_f64()
    );

    let store = world_store(&b);
    let positions: Vec<SectionPos> = inputs.iter().map(|i| i.pos).collect();
    let start = Instant::now();
    for pos in &positions {
        black_box(MeshInput::snapshot(&store, *pos, 2));
    }
    println!(
        "snapshot: {:?} per section",
        start.elapsed() / positions.len() as u32
    );
    c.bench_function("snapshot_section", |bench| {
        bench.iter(|| {
            for pos in &positions {
                black_box(MeshInput::snapshot(&store, *pos, 2));
            }
        })
    });
}

/// Back-to-front sorting of translucent quads: water surfaces and a checkerboard of stained glass, the
/// worst case for one section.
fn bench_translucent_sort(c: &mut Criterion) {
    let Some(f) = fixture() else { return };
    let b = blocks();
    let mut mesher = Mesher::new(f.cache.clone(), f.colors.clone());
    let mut water = Vec::new();
    for input in surface_inputs(&b) {
        let mut out = SectionMesh::default();
        mesher.mesh(&input, &mut out);
        if !out.translucent.is_empty() {
            water.push((out.translucent.clone(), input.pos));
        }
    }
    let glass = state_of("minecraft:red_stained_glass", &[]);
    let mut input = MeshInput::blank(SectionPos::new(0, 4, 0), 2);
    for y in 0..16 {
        for z in 0..16 {
            for x in 0..16 {
                if (x + y + z) % 2 == 0 {
                    input.set_state(x, y, z, glass);
                }
            }
        }
    }
    let mut checker = SectionMesh::default();
    mesher.mesh(&input, &mut checker);
    let quads: usize = water.iter().map(|w| w.0.len()).sum();
    println!(
        "{} water sections, {quads} translucent quads; checkerboard {} quads",
        water.len(),
        checker.translucent.len()
    );
    let mut scratch = Vec::new();
    let mut work = Vec::new();
    c.bench_function("translucent_sort_water_sections", |bench| {
        bench.iter(|| {
            for (q, pos) in &water {
                work.clone_from(q);
                let eye = [
                    48.0 - pos.x as f32 * 16.0,
                    72.0 - pos.y as f32 * 16.0,
                    48.0 - pos.z as f32 * 16.0,
                ];
                mcr_render::sort_back_to_front(&mut work, eye, &mut scratch);
            }
            black_box(&work);
        })
    });
    c.bench_function("translucent_sort_glass_checkerboard", |bench| {
        bench.iter(|| {
            work.clone_from(&checker.translucent);
            mcr_render::sort_back_to_front(&mut work, [8.0, 30.0, -4.0], &mut scratch);
            black_box(&work);
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(20);
    targets = bench_mesh, bench_translucent_sort
}
criterion_main!(benches);
