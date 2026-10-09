use std::collections::HashMap;
use std::time::Instant;

use mcr_anvil::World;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: inspect <world dir> [radius]")?;
    let radius: u32 = args.next().map_or(Ok(16), |r| r.parse())?;

    let world = World::open(&path)?;
    let s = world.summary();
    println!("{} ({})", s.name, s.dir.display());
    println!(
        "version {:?} data {:?} gamemode {} hardcore {} commands {} last played {}",
        s.version_name, s.data_version, s.game_type, s.hardcore, s.allow_commands, s.last_played_ms
    );
    println!("spawn {:?} player {:?}", s.spawn, s.player_pos);

    let [sx, _, sz] = s.spawn.unwrap_or_default();
    let started = Instant::now();
    let chunks = world.load_chunks_around(sx >> 4, sz >> 4, radius)?;
    println!(
        "loaded {} chunks within {radius} of spawn in {:?}",
        chunks.len(),
        started.elapsed()
    );

    let version = mcr_data::v26_3();
    let mut histogram: HashMap<u16, u64> = HashMap::new();
    let mut flat = [0u16; 4096];
    for chunk in &chunks {
        for section in chunk.column.sections() {
            section.unpack_blocks(&mut flat);
            for &state in &flat {
                *histogram.entry(state).or_default() += 1;
            }
        }
    }
    let mut by_block: HashMap<&str, u64> = HashMap::new();
    for (state, n) in histogram {
        let name = version.state_block_name(state as u32).unwrap_or("?");
        *by_block.entry(name).or_default() += n;
    }
    let mut rows: Vec<_> = by_block.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (name, n) in rows.iter().take(25) {
        println!("{n:>12}  {name}");
    }
    println!("stats {:?}", world.stats());
    println!("biomes {:?}", world.biome_names());
    Ok(())
}
