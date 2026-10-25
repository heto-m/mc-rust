use std::sync::OnceLock;

use mcr_buf::BlockPos;
use mcr_client::ClientState;
use mcr_data::Version;
use mcr_render::{PostChain, wgpu::AdapterInfo};
use mcr_settings::options::Options;
use mcr_settings::value::{CloudStatus, TextureFiltering};
use mcr_ui::{Category, DebugOverlay, ENTRIES};
use mcr_world::{ChunkPos, HeightmapKind};

use crate::render_system::PipelineStats;
use crate::target::Target;

pub struct SoundDebug {
    pub channels: String,
    pub mood: f32,
    pub cache: (usize, usize),
}

pub struct DebugInput<'a> {
    pub state: &'a ClientState,
    pub data: &'static Version,
    pub options: &'a Options,
    pub feet: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub target: Option<Target>,
    pub fluid_target: Option<(BlockPos, u32)>,
    pub speed: f64,
    pub fps: f32,
    pub frame_ms: f32,
    pub gpu_percent: Option<f32>,
    pub pipeline: PipelineStats,
    pub tick: u64,
    pub adapter: &'a AdapterInfo,
    pub size: (u32, u32),
    pub memory_mb: u64,
    pub meshed_columns: usize,
    pub particles: usize,
    pub post_effects: &'a [PostChain],
    pub sound: Option<SoundDebug>,
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

/// `DebugScreenDisplayer`: what the entries write, assembled the way `DebugScreenOverlay` does.
#[derive(Default)]
struct Displayer {
    left: Vec<String>,
    right: Vec<String>,
    regular: Vec<String>,
    groups: Vec<(&'static str, Vec<String>)>,
}

impl Displayer {
    fn priority(&mut self, line: String) {
        if self.left.len() > self.right.len() {
            self.right.push(line);
        } else {
            self.left.push(line);
        }
    }

    fn line(&mut self, line: String) {
        self.regular.push(line);
    }

    fn group(&mut self, name: &'static str, lines: impl IntoIterator<Item = String>) {
        match self.groups.iter_mut().find(|(n, _)| *n == name) {
            Some((_, existing)) => existing.extend(lines),
            None => self.groups.push((name, lines.into_iter().collect())),
        }
    }

    fn finish(self) -> (Vec<String>, Vec<String>) {
        let Self {
            mut left,
            mut right,
            regular,
            groups,
        } = self;
        if !left.is_empty() {
            left.push(String::new());
        }
        if !right.is_empty() {
            right.push(String::new());
        }
        if !regular.is_empty() {
            let mid = regular.len().div_ceil(2);
            left.extend(regular[..mid].iter().cloned());
            right.extend(regular[mid..].iter().cloned());
            left.push(String::new());
            if mid < regular.len() {
                right.push(String::new());
            }
        }
        let mid = groups.len().div_ceil(2);
        for (i, (_, lines)) in groups.into_iter().enumerate() {
            if lines.is_empty() {
                continue;
            }
            let side = if i < mid { &mut left } else { &mut right };
            side.extend(lines);
            side.push(String::new());
        }
        (left, right)
    }
}

fn cpu_info() -> &'static str {
    static CPU: OnceLock<String> = OnceLock::new();
    CPU.get_or_init(|| {
        let text = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let threads = text.lines().filter(|l| l.starts_with("processor")).count();
        let model = text
            .lines()
            .find_map(|l| l.strip_prefix("model name"))
            .and_then(|l| l.split_once(':'))
            .map_or("<unknown>", |(_, m)| m.trim());
        format!(
            "{threads}x {}",
            model.split_whitespace().collect::<Vec<_>>().join(" ")
        )
    })
}

fn heap_line(kind: &str, used: u64, committed: u64, max: u64) -> String {
    format!("Memory ({kind}): i=000MiB u={used:03}MiB c={committed:03}MiB m={max:03}MiB")
}

fn proc_kib(file: &str, key: &str) -> u64 {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
        .unwrap_or(0)
}

fn detailed_memory() -> [String; 2] {
    let status_kib = |key: &str| proc_kib("/proc/self/status", key);
    let total = proc_kib("/proc/meminfo", "MemTotal");
    let mib = |kib: u64| kib >> 10;
    [
        heap_line(
            "heap",
            mib(status_kib("RssAnon")),
            mib(status_kib("VmData")),
            mib(total),
        ),
        heap_line(
            "non-heap",
            mib(status_kib("RssFile")),
            mib(status_kib("VmExe") + status_kib("VmLib")),
            mib(total),
        ),
    ]
}

fn colored_value(name: &str, value: &str) -> String {
    let color = match value {
        "true" => "§a",
        "false" => "§c",
        _ => "",
    };
    format!("{name}: {color}{value}")
}

fn tag_lines(names: Vec<&str>) -> impl Iterator<Item = String> {
    names.into_iter().map(|t| format!("#{t}"))
}

fn region_local(chunk: i32) -> i32 {
    chunk & 31
}

fn target_name(data: &Version, kind: i32) -> Option<&str> {
    data.entity_types().name_of(u32::try_from(kind).ok()?)
}

/// The F3 text for the entries `on` says are showing, written in entry id order.
pub fn build(input: &DebugInput<'_>, on: &dyn Fn(&str) -> bool) -> DebugOverlay {
    let state = input.state;
    let data = input.data;
    let [x, y, z] = input.feet;
    let block = [x.floor() as i32, y.floor() as i32, z.floor() as i32];
    let chunk = ChunkPos::from_block(block[0], block[2]);
    let (dir, towards) = facing_name(input.yaw);
    let column = state.chunks.column(chunk);
    let mut out = Displayer::default();
    for def in ENTRIES.iter().filter(|e| e.category == Category::Text) {
        if !on(def.id) {
            continue;
        }
        match def.id {
            "biome" => {
                let index = column.map(|c| ((block[1] - c.min_y()) >> 4).max(0) as usize);
                if let (Some(column), Some(index)) = (column, index)
                    && let Some(section) = column.section(index)
                {
                    let lx = (block[0] & 15) as usize;
                    let lz = (block[2] & 15) as usize;
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
                    out.line(format!("Biome: {name}"));
                }
            }
            "chunk_render_stats" => out.line(format!(
                "C: {} sections meshed, {} queued, {:.2}M quads",
                input.pipeline.uploaded,
                input.pipeline.queued,
                f64::from(input.pipeline.quads) / 1e6
            )),
            "chunk_source_stats" => out.line(format!(
                "Chunks: {} / {} (view {})",
                state.chunks.len(),
                input.meshed_columns,
                state.world.view_distance
            )),
            "day_count" => {
                if let Some(time) = state.world.day_time() {
                    out.line(format!("Day #{}", time.div_euclid(24_000)));
                }
            }
            "detailed_memory" => out.group("memory", detailed_memory()),
            "entity_render_stats" => out.line(format!("E: {}", state.entities.len())),
            "fps" => out.priority(format!("{:.0} fps ({:.1} ms)", input.fps, input.frame_ms)),
            "game_version" => out.priority("mc-rust 26.3 (vanilla protocol)".to_owned()),
            "gpu_utilization" => {
                if let Some(percent) = input.gpu_percent {
                    out.line(if percent > 100.0 {
                        "GPU: §c100%".to_owned()
                    } else {
                        format!("GPU: {}%", percent.round())
                    });
                }
            }
            "heightmap" => {
                if let Some(column) = column {
                    let height = |kind| {
                        column.heightmaps().get(kind).map_or_else(
                            || "?".to_owned(),
                            |map| {
                                (map.first_available(
                                    (block[0] & 15) as usize,
                                    (block[2] & 15) as usize,
                                    column.min_y(),
                                ) - 1)
                                    .to_string()
                            },
                        )
                    };
                    out.group(
                        "heightmaps",
                        [
                            format!(
                                "CH S: {} M: {} ML: {}",
                                height(HeightmapKind::WorldSurface),
                                height(HeightmapKind::MotionBlocking),
                                height(HeightmapKind::MotionBlockingNoLeaves)
                            ),
                            "SH S: ?? O: ?? M: ?? ML: ??".to_owned(),
                        ],
                    );
                }
            }
            "light_levels" => {
                if let Some(column) = column {
                    let lx = (block[0] & 15) as usize;
                    let lz = (block[2] & 15) as usize;
                    let sky = column.sky_light_or_above(lx, block[1], lz);
                    let blk = column.block_light(lx, block[1], lz);
                    out.group(
                        "light",
                        [format!(
                            "Client Light: {} ({} sky, {} block)",
                            sky.unwrap_or(0).max(blk.unwrap_or(0)),
                            sky.map_or("?".into(), |v| v.to_string()),
                            blk.map_or("?".into(), |v| v.to_string())
                        )],
                    );
                }
            }
            "looking_at_block_state" => {
                let mut lines = Vec::new();
                if let Some(Target::Block(hit)) = input.target {
                    lines.push(format!(
                        "§nTargeted Block: {}, {}, {}",
                        hit.pos.x, hit.pos.y, hit.pos.z
                    ));
                    lines.extend(data.state_block_name(hit.state).map(str::to_owned));
                    lines.extend(
                        data.state_properties(hit.state)
                            .map(|(k, v)| colored_value(k, v)),
                    );
                }
                out.group("looking_at_block", lines);
            }
            "looking_at_block_tags" => {
                let mut lines = Vec::new();
                if let Some(Target::Block(hit)) = input.target
                    && let Some(info) = data.block_state(hit.state)
                {
                    lines.extend(tag_lines(
                        state
                            .registries
                            .tags_containing("minecraft:block", info.block as i32),
                    ));
                }
                out.group("looking_at_block", lines);
            }
            "looking_at_entity" => {
                let mut lines = Vec::new();
                if let Some(Target::Entity { id, .. }) = input.target
                    && let Some(entity) = state.entities.get(id)
                {
                    lines.push("§nTargeted Entity".to_owned());
                    lines.push(target_name(data, entity.kind).unwrap_or("?").to_owned());
                }
                out.group("looking_at_entity", lines);
            }
            "looking_at_entity_tags" => {
                let mut lines = Vec::new();
                if let Some(Target::Entity { id, .. }) = input.target
                    && let Some(entity) = state.entities.get(id)
                {
                    lines.extend(tag_lines(
                        state
                            .registries
                            .tags_containing("minecraft:entity_type", entity.kind),
                    ));
                }
                out.group("looking_at_entity", lines);
            }
            "looking_at_fluid_state" => {
                let mut lines = Vec::new();
                if let Some((pos, block_state)) = input.fluid_target {
                    lines.push(format!("§nTargeted Fluid: {}, {}, {}", pos.x, pos.y, pos.z));
                    let fluid = data.block_state(block_state).and_then(|i| i.fluid);
                    lines.push(
                        fluid
                            .and_then(|f| data.fluid_name(f.fluid))
                            .unwrap_or("minecraft:empty")
                            .to_owned(),
                    );
                    if let Some(f) = fluid {
                        lines.push(colored_value("falling", &f.falling.to_string()));
                        if !f.source {
                            lines.push(format!("level: {}", f.amount));
                        }
                    }
                }
                out.group("looking_at_fluid", lines);
            }
            "looking_at_fluid_tags" => {
                let mut lines = Vec::new();
                if let Some((_, block_state)) = input.fluid_target {
                    let fluid = data.block_state(block_state).and_then(|i| i.fluid);
                    if let Some(f) = fluid {
                        lines.extend(tag_lines(
                            state
                                .registries
                                .tags_containing("minecraft:fluid", f.fluid as i32),
                        ));
                    }
                }
                out.group("looking_at_fluid", lines);
            }
            "memory" => out.group("memory", [format!("Mem: {} MB", input.memory_mb)]),
            "particle_render_stats" => out.line(format!("P: {}", input.particles)),
            "player_position" => out.group(
                "position",
                [
                    format!("XYZ: {x:.3} / {y:.5} / {z:.3}"),
                    format!("Block: {} {} {}", block[0], block[1], block[2]),
                    format!(
                        "Chunk: {} {} {} [{} {} in r.{}.{}.mca]",
                        chunk.x,
                        block[1] >> 4,
                        chunk.z,
                        region_local(chunk.x),
                        region_local(chunk.z),
                        chunk.x >> 5,
                        chunk.z >> 5
                    ),
                    format!(
                        "Facing: {dir} ({towards}) ({:.1} / {:.1})",
                        wrap_yaw(input.yaw),
                        wrap_yaw(input.pitch)
                    ),
                    format!("{} FC: 0", state.dimension.name.as_str()),
                ],
            ),
            "player_section_position" => out.group(
                "position",
                [format!(
                    "Section-relative: {:02} {:02} {:02}",
                    block[0] & 15,
                    block[1] & 15,
                    block[2] & 15
                )],
            ),
            "player_speed" => out.group(
                "position",
                [format!("Speed: {:.3} blocks/tick", input.speed)],
            ),
            "post_effects" => {
                if !input.post_effects.is_empty() {
                    let names: Vec<String> = input
                        .post_effects
                        .iter()
                        .map(|c| format!("minecraft:{}", c.name()))
                        .collect();
                    out.line(format!("Post: {}", names.join(", ")));
                }
            }
            "simple_performance_impactors" => {
                let o = input.options;
                out.line(format!(
                    "{}{}B: {}",
                    if o.improved_transparency {
                        "improved-transparency "
                    } else {
                        ""
                    },
                    match o.render_clouds {
                        CloudStatus::Off => "",
                        CloudStatus::Fast => "fast-clouds ",
                        CloudStatus::Fancy => "fancy-clouds ",
                    },
                    o.biome_blend_radius
                ));
                out.line(match o.texture_filtering {
                    TextureFiltering::Anisotropic => format!(
                        "Filtering: Anisotropic {}x",
                        1 << o.max_anisotropy_bit.clamp(1, 3)
                    ),
                    TextureFiltering::Rgss => "Filtering: RGSS".to_owned(),
                    TextureFiltering::None => "Filtering: None".to_owned(),
                });
                out.line("Terrain Rendering: multidrawindirect".to_owned());
            }
            "sound_cache" => {
                if let Some(sound) = &input.sound {
                    out.line(format!(
                        "Sound cache: {} buffers, {} MiB",
                        sound.cache.0,
                        sound.cache.1.div_ceil(1 << 20)
                    ));
                }
            }
            "sound_mood" => {
                if let Some(sound) = &input.sound {
                    out.line(format!(
                        "{} (Mood {}%)",
                        sound.channels,
                        (sound.mood * 100.0).round()
                    ));
                }
            }
            "system_specs" => out.group(
                "system",
                [
                    format!("Rust {}", env!("CARGO_PKG_VERSION")),
                    format!("CPU: {}", cpu_info()),
                    format!("Display: {}x{}", input.size.0, input.size.1),
                    format!("{}{}", input.adapter.name, device_kind(input.adapter)),
                    format!(
                        "{:?} {}",
                        input.adapter.backend,
                        input.adapter.driver_info.lines().next().unwrap_or("")
                    ),
                ],
            ),
            "tps" => {
                out.line(format!("T: {}", input.tick));
                out.line(format!(
                    "Server: {}, {} players",
                    state.server.brand.as_deref().unwrap_or("unknown"),
                    state.players.len()
                ));
            }
            _ => {}
        }
    }
    let (left, right) = out.finish();
    DebugOverlay {
        left,
        right,
        charts: Vec::new(),
        lightmap: None,
        pie: None,
    }
}

fn device_kind(info: &AdapterInfo) -> &'static str {
    use mcr_render::wgpu::DeviceType;
    match info.device_type {
        DeviceType::IntegratedGpu => " (iGPU)",
        DeviceType::DiscreteGpu => " (dGPU)",
        DeviceType::VirtualGpu => " (vGPU)",
        DeviceType::Cpu => " (software)",
        DeviceType::Other => "",
    }
}

fn wrap_yaw(yaw: f32) -> f32 {
    let y = yaw.rem_euclid(360.0);
    if y >= 180.0 { y - 360.0 } else { y }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_lines_alternate_and_groups_split_in_halves() {
        let mut d = Displayer::default();
        d.priority("a".into());
        d.priority("b".into());
        d.line("r1".into());
        d.line("r2".into());
        d.line("r3".into());
        d.group("one", ["g1".to_owned()]);
        d.group("empty", []);
        d.group("two", ["g2".to_owned()]);
        d.group("one", ["g1b".to_owned()]);
        let (left, right) = d.finish();
        assert_eq!(left, ["a", "", "r1", "r2", "", "g1", "g1b", ""]);
        assert_eq!(right, ["b", "", "r3", "", "g2", ""]);
    }

    #[test]
    fn facing_follows_vanilla_yaw() {
        assert_eq!(facing_name(0.0).0, "south");
        assert_eq!(facing_name(90.0).0, "west");
        assert_eq!(facing_name(180.0).0, "north");
        assert_eq!(facing_name(-90.0).0, "east");
        assert_eq!(facing_name(44.0).0, "south");
    }
}
