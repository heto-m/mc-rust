use mcr_client::ClientState;
use mcr_data::Version;
use mcr_ui::DebugOverlay;
use mcr_world::ChunkPos;

use crate::render_system::PipelineStats;
use crate::target::Target;

pub struct DebugInput<'a> {
    pub state: &'a ClientState,
    pub data: &'static Version,
    pub feet: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub target: Option<Target>,
    pub fps: f32,
    pub frame_ms: f32,
    pub pipeline: PipelineStats,
    pub tick: u64,
    pub adapter: &'a str,
    pub size: (u32, u32),
    pub memory_mb: u64,
    pub meshed_columns: usize,
}

pub fn facing_name(yaw: f32) -> (&'static str, &'static str) {
    match (yaw.rem_euclid(360.0) + 45.0) as u32 / 90 % 4 {
        0 => ("south", "Towards positive Z"),
        1 => ("west", "Towards negative X"),
        2 => ("north", "Towards negative Z"),
        _ => ("east", "Towards positive X"),
    }
}

pub fn resident_mb() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map_or(0, |pages| (pages * 4096) >> 20)
}

pub fn build(input: &DebugInput<'_>) -> DebugOverlay {
    let state = input.state;
    let [x, y, z] = input.feet;
    let block = [x.floor() as i32, y.floor() as i32, z.floor() as i32];
    let chunk = ChunkPos::from_block(block[0], block[2]);
    let (dir, towards) = facing_name(input.yaw);
    let mut left = vec![
        "mc-rust 26.3 (vanilla protocol)".to_owned(),
        format!(
            "{:.0} fps ({:.1} ms) T: {}",
            input.fps, input.frame_ms, input.tick
        ),
        format!(
            "C: {} sections meshed, {} queued, {:.2}M quads",
            input.pipeline.uploaded,
            input.pipeline.queued,
            f64::from(input.pipeline.quads) / 1e6
        ),
        format!(
            "Chunks: {} / {} (view {})",
            state.chunks.len(),
            input.meshed_columns,
            state.world.view_distance
        ),
        String::new(),
        format!("XYZ: {x:.3} / {y:.5} / {z:.3}"),
        format!("Block: {} {} {}", block[0], block[1], block[2]),
        format!(
            "Chunk: {} {} {} in {} {} {}",
            block[0] & 15,
            block[1] & 15,
            block[2] & 15,
            chunk.x,
            block[1] >> 4,
            chunk.z
        ),
        format!(
            "Facing: {dir} ({towards}) ({:.1} / {:.1})",
            wrap_yaw(input.yaw),
            input.pitch
        ),
    ];
    if let Some(column) = state.chunks.column(chunk) {
        let lx = (block[0] & 15) as usize;
        let lz = (block[2] & 15) as usize;
        let sky = column.sky_light_or_above(lx, block[1], lz);
        let blk = column.block_light(lx, block[1], lz);
        left.push(format!(
            "Client Light: {} ({} sky, {} block)",
            sky.unwrap_or(0).max(blk.unwrap_or(0)),
            sky.map_or("?".into(), |v| v.to_string()),
            blk.map_or("?".into(), |v| v.to_string())
        ));
        let index = ((block[1] - column.min_y()) >> 4).max(0) as usize;
        if let Some(section) = column.section(index) {
            let biome = section
                .biome(
                    (lx >> 2) & 3,
                    (block[1].rem_euclid(16) as usize >> 2) & 3,
                    (lz >> 2) & 3,
                )
                .0;
            let name = state
                .registries
                .get("minecraft:worldgen/biome")
                .and_then(|r| r.get(i32::from(biome)))
                .map_or("?", |e| e.id.as_str());
            left.push(format!("Biome: {name}"));
        }
    }
    left.push(format!(
        "Dimension: {}  Game time: {}",
        state.dimension.name.as_str(),
        state.world.game_time
    ));
    let mut right = vec![
        format!("Rust {}", env!("CARGO_PKG_VERSION")),
        format!("Mem: {} MB", input.memory_mb),
        String::new(),
        format!(
            "Display: {}x{} ({})",
            input.size.0, input.size.1, input.adapter
        ),
        format!(
            "Server: {}",
            state.server.brand.as_deref().unwrap_or("unknown")
        ),
        format!("Players: {}", state.players.len()),
        format!("Entities: {}", state.entities.len()),
        String::new(),
    ];
    match input.target {
        Some(Target::Block(hit)) => {
            right.push(format!(
                "Targeted Block: {}, {}, {}",
                hit.pos.x, hit.pos.y, hit.pos.z
            ));
            if let Some(name) = input.data.state_block_name(hit.state) {
                right.push(name.to_owned());
            }
            for (k, v) in input.data.state_properties(hit.state) {
                right.push(format!("{k}: {v}"));
            }
        }
        Some(Target::Entity { id, .. }) => {
            right.push(format!("Targeted Entity: {id}"));
        }
        None => {}
    }
    DebugOverlay { left, right }
}

fn wrap_yaw(yaw: f32) -> f32 {
    let y = yaw.rem_euclid(360.0);
    if y >= 180.0 { y - 360.0 } else { y }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facing_follows_vanilla_yaw() {
        assert_eq!(facing_name(0.0).0, "south");
        assert_eq!(facing_name(90.0).0, "west");
        assert_eq!(facing_name(180.0).0, "north");
        assert_eq!(facing_name(-90.0).0, "east");
        assert_eq!(facing_name(44.0).0, "south");
    }
}
