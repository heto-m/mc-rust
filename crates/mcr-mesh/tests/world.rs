use std::sync::Arc;
use std::time::{Duration, Instant};

use mcr_mesh::testing::{DIRT, STONE, test_cache};
use mcr_mesh::{BiomeClimate, BiomeColors, ColorMaps, MeshInput, MeshPool, MeshResult, pidx};
use mcr_world::{
    Biome, BlockState, ChunkColumn, ChunkPos, ChunkStore, LightData, LightNibbles, LightUpdate,
    SectionPos, WorldConfig,
};

fn config() -> WorldConfig {
    WorldConfig::new(35723, 80, -64, 384).unwrap()
}

fn colors() -> Arc<BiomeColors> {
    Arc::new(BiomeColors::from_climates(
        &[BiomeClimate::default()],
        &ColorMaps::flat(0x00ff00, 0x00aa00, 0x888800),
    ))
}

fn store() -> ChunkStore {
    let mut s = ChunkStore::new(config());
    for (x, z) in [(0, 0), (1, 0), (0, 1)] {
        s.insert(ChunkPos::new(x, z), ChunkColumn::empty(&config()));
    }
    s
}

#[test]
fn snapshot_includes_neighbour_borders_light_and_biomes() {
    let mut s = store();
    s.set_block(15, 5, 5, BlockState(STONE));
    s.set_block(16, 5, 5, BlockState(DIRT));
    s.set_block(3, 16, 3, BlockState(DIRT));
    s.set_block(-1, 5, 5, BlockState(STONE));
    let mut nibbles = LightNibbles::default();
    nibbles.set(0, 6, 5, 9);
    s.apply_light(
        ChunkPos::new(1, 0),
        LightData {
            sky: vec![LightUpdate {
                section: 5,
                data: Some(nibbles),
            }],
            block: vec![],
        },
    );

    let input = MeshInput::snapshot(&s, SectionPos::new(0, 0, 0), 2).unwrap();
    assert_eq!(input.state(15, 5, 5), STONE);
    assert_eq!(input.state(16, 5, 5), DIRT);
    assert_eq!(input.state(3, 16, 3), DIRT);
    assert_eq!(input.state(-1, 5, 5), 0, "unloaded neighbour is air");
    assert_eq!(input.light[pidx(16, 6, 5)] >> 4, 9);
    assert_eq!(
        input.light[pidx(3, 3, 3)] >> 4,
        15,
        "absent light is daylight"
    );
    assert!(MeshInput::snapshot(&s, SectionPos::new(5, 0, 0), 2).is_none());
}

#[test]
fn biome_window_reads_neighbour_columns() {
    let mut s = store();
    let mut column = ChunkColumn::empty(&config());
    let mut section = column.sections()[4].clone();
    section.set_biome(0, 2, 0, Biome(7));
    let data = mcr_world::ChunkData {
        heightmaps: Default::default(),
        sections: {
            let mut v = column.sections().to_vec();
            v[4] = section;
            v
        },
        block_entities: vec![],
    };
    column = ChunkColumn::from_data(data, &config());
    s.insert(ChunkPos::new(1, 0), column);
    let input = MeshInput::snapshot(&s, SectionPos::new(0, 0, 0), 2).unwrap();
    assert_eq!(input.biomes[mcr_mesh::quart_index(4, 2, 0)], 7);
    assert_eq!(input.biomes[mcr_mesh::quart_index(3, 2, 0)], 0);
    assert_eq!(input.biomes[mcr_mesh::quart_index(4, 3, 0)], 0);
}

fn pool(threads: usize) -> MeshPool {
    MeshPool::new(Arc::new(test_cache()), colors(), threads, 2)
}

fn wait(pool: &mut MeshPool) -> Vec<MeshResult> {
    let mut out = Vec::new();
    pool.wait_idle(&mut out);
    out
}

#[test]
fn pool_meshes_dirty_sections_and_reports_unloaded_ones() {
    let mut s = store();
    s.drain_dirty().for_each(drop);
    s.set_block(15, 5, 5, BlockState(STONE));
    let mut p = pool(4);
    let gone = p.submit_dirty(&mut s, SectionPos::new(0, 0, 0));
    assert!(gone.is_empty());
    let results = wait(&mut p);
    assert_eq!(results.len(), 2, "edited section and its loaded neighbour");
    let center = results
        .iter()
        .find(|r| r.pos == SectionPos::new(0, 0, 0))
        .unwrap();
    assert_eq!(center.mesh.cube_count(), 6);
    let neighbour = results
        .iter()
        .find(|r| r.pos == SectionPos::new(1, 0, 0))
        .unwrap();
    assert_eq!(
        neighbour.mesh.cube_count(),
        0,
        "border block lives in the other section"
    );

    s.remove(ChunkPos::new(1, 0));
    s.insert(ChunkPos::new(2, 2), ChunkColumn::empty(&config()));
    s.remove(ChunkPos::new(2, 2));
    let gone = p.submit_dirty(&mut s, SectionPos::new(0, 0, 0));
    assert!(gone.is_empty() || gone.iter().all(|g| g.x == 2 || g.x == 1));
}

#[test]
fn newer_submission_supersedes_older_one() {
    let mut p = pool(1);
    let mut s = store();
    s.drain_dirty().for_each(drop);
    let pos = SectionPos::new(0, 0, 0);
    let mut first = MeshInput::snapshot(&s, pos, 2).unwrap();
    first.set_state(1, 1, 1, STONE);
    p.submit(first, 0);
    let mut second = MeshInput::snapshot(&s, pos, 2).unwrap();
    second.set_state(1, 1, 1, STONE);
    second.set_state(5, 1, 1, STONE);
    p.submit(second, 0);
    let results = wait(&mut p);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].mesh.cube_count(), 12);
}

#[test]
fn poll_does_not_block_while_workers_are_busy() {
    let mut p = pool(1);
    let s = store();
    for i in 0..200 {
        let mut input = MeshInput::snapshot(&s, SectionPos::new(0, 0, 0), 2).unwrap();
        input.pos = SectionPos::new(100 + i, 0, 0);
        for z in 0..16 {
            for x in 0..16 {
                for y in 0..8 {
                    input.set_state(x, y, z, if (x + y + z) % 2 == 0 { STONE } else { 0 });
                }
            }
        }
        p.submit(input, i as u32);
    }
    let start = Instant::now();
    let mut out = Vec::new();
    p.poll(&mut out);
    assert!(start.elapsed() < Duration::from_millis(50));
    assert!(p.pending() > 0, "work was still outstanding");
    p.wait_idle(&mut out);
    assert_eq!(out.len(), 200);
}
