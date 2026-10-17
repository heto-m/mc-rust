use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use mcr_render::{GpuCounters, Renderer, RendererConfig, Target};

use crate::cli::Options;
use crate::shell::Shell;
use crate::world_setup;

const STATS_EVERY: Duration = Duration::from_secs(10);
const REJOIN_AFTER: Duration = Duration::from_secs(3);
const FRAME: Duration = Duration::from_millis(16);
const RESPAWN_EVERY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, Default)]
pub struct Stats {
    pub gpu: GpuCounters,
    pub entity_layers: u32,
    pub entities: usize,
    pub chunks: usize,
}

impl Stats {
    pub fn line(&self, secs: u64, joins: u32) -> String {
        let g = &self.gpu;
        format!(
            "soak-stats t={secs} joins={joins} gpu_buffers={} gpu_buffer_bytes={} gpu_textures={} gpu_texture_bytes={} gpu_texture_views={} gpu_bind_groups={} gpu_allocations={} entity_layers={} entities={} chunks={}",
            g.buffers,
            g.buffer_bytes,
            g.textures,
            g.texture_bytes,
            g.texture_views,
            g.bind_groups,
            g.allocations,
            self.entity_layers,
            self.entities,
            self.chunks,
        )
    }
}

/// Plays offscreen for `duration`, rejoining after every kick, and prints a `soak-stats` line every 10 s.
pub fn run(opts: &Options, duration: Duration) -> Result<()> {
    let (width, height) = opts.size;
    let mut config = world_setup::renderer_config(opts, RendererConfig::default());
    let started = crate::mods::startup(opts)?;
    if started.boot.is_some() {
        config.scene_texture = true;
        config.optional_features = mcr_modbridge::gpu::context::OPTIONAL_FEATURES;
    }
    let mut renderer =
        Renderer::new(Target::Offscreen { width, height }, config).context("creating renderer")?;
    let mod_host = match started.boot {
        Some(plan) => plan.start(opts, &mut renderer, &started.app)?,
        None => None,
    };
    let config = crate::shell_config(
        opts,
        mod_host.as_ref().map(crate::mods::ModHost::net),
        started.app,
    );
    let result = Shell::new(renderer, config).and_then(|shell| frames(shell, duration));
    if let Some(host) = mod_host {
        host.finish();
    }
    result
}

fn frames(mut shell: Shell, duration: Duration) -> Result<()> {
    let started = Instant::now();
    let mut last = started;
    let mut next_stats = Duration::ZERO;
    let mut next_respawn = Duration::ZERO;
    let mut idle_since: Option<Instant> = None;
    let mut joins = 0u32;
    let mut in_world = false;
    while started.elapsed() < duration && !shell.quit {
        let now = Instant::now();
        shell.advance((now - last).as_secs_f32());
        last = now;
        shell.frame();
        let playing = shell.play.is_some();
        if playing && !in_world {
            joins += 1;
        }
        in_world = playing;
        if shell.idle() {
            let since = *idle_since.get_or_insert(now);
            if since.elapsed() > REJOIN_AFTER && shell.rejoin() {
                idle_since = None;
            }
        } else {
            idle_since = None;
        }
        if started.elapsed() >= next_respawn {
            next_respawn += RESPAWN_EVERY;
            shell.respawn_if_dead();
        }
        if started.elapsed() >= next_stats {
            next_stats += STATS_EVERY;
            println!(
                "{}",
                shell.soak_stats().line(started.elapsed().as_secs(), joins)
            );
        }
        if let Some(rest) = FRAME.checked_sub(now.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    println!(
        "soak done: {joins} joins, peak rss {} MB",
        world_setup::peak_rss_mb()
    );
    Ok(())
}
