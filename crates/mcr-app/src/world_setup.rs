use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, anyhow};
use mcr_render::RendererConfig;

use crate::assets::Assets;
use crate::cli::Options;
use crate::engine::Engine;
use crate::source::WorldSource;

/// Arenas start sized for render distance 16 over typical terrain and grow on demand.
pub fn renderer_config(opts: &Options, base: RendererConfig) -> RendererConfig {
    RendererConfig {
        vsync: opts.vsync,
        occlusion_culling: opts.occlusion,
        visibility_graph: opts.visibility_graph,
        cube_arena_quads: 4 << 20,
        model_arena_quads: 1 << 20,
        tint_tables: 8192,
        ..base
    }
}

pub fn open(opts: &Options) -> Result<(Arc<Assets>, Box<dyn WorldSource>)> {
    let path = opts.world.as_deref().ok_or_else(|| anyhow!("no --world"))?;
    let start = Instant::now();
    let assets = Arc::new(Assets::load(&opts.jar)?);
    println!("assets baked in {:.2}s", start.elapsed().as_secs_f32());
    let source = Engine::open_anvil(path)?;
    Ok((assets, source))
}

pub fn peak_rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("VmHWM:"))
                .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        })
        .map_or(0, |kb| kb >> 10)
}
