use std::time::Instant;

use mcr_anvil::World;

#[test]
#[ignore = "needs MCR_TEST_WORLD pointing at a vanilla 26.3 world"]
fn loads_a_vanilla_world() {
    let path = std::env::var("MCR_TEST_WORLD").expect("MCR_TEST_WORLD");
    let world = World::open(&path).unwrap();
    let [sx, _, sz] = world.summary().spawn.unwrap_or_default();

    let started = Instant::now();
    let chunks = world.load_chunks_around(sx >> 4, sz >> 4, 16).unwrap();
    let elapsed = started.elapsed();

    let stats = world.stats();
    println!(
        "chunks {} in {elapsed:?}, stats {stats:?}, biomes {}",
        chunks.len(),
        world.biome_names().len()
    );
    assert!(!chunks.is_empty());
    assert_eq!(stats.unknown_blocks, 0);
    assert_eq!(stats.bad_properties, 0);
    assert_eq!(stats.unknown_biomes, 0);
    assert_eq!(stats.chunks_corrupt, 0);
    assert_eq!(stats.wrong_data_version, 0);
    let solid = chunks
        .iter()
        .any(|c| c.column.sections().iter().any(|s| s.non_empty_count() > 0));
    assert!(solid);
}
