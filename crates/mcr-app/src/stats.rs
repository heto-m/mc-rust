use std::time::{Duration, Instant};

use mcr_render::{FrameStats, MemoryStats};

use crate::render_system::PipelineStats;

#[derive(Debug)]
pub struct StatWindow {
    frames: u32,
    frame_ms: f32,
    cpu_ms: f32,
    gpu_draw_ms: f32,
    gpu_cull_ms: f32,
    gpu_samples: u32,
    draws: u32,
    since: Instant,
}

impl Default for StatWindow {
    fn default() -> Self {
        Self {
            frames: 0,
            frame_ms: 0.0,
            cpu_ms: 0.0,
            gpu_draw_ms: 0.0,
            gpu_cull_ms: 0.0,
            gpu_samples: 0,
            draws: 0,
            since: Instant::now(),
        }
    }
}

impl StatWindow {
    pub fn record(&mut self, dt: f32, s: &FrameStats) {
        self.frames += 1;
        self.frame_ms += dt * 1000.0;
        self.cpu_ms += s.cpu_ms;
        if let Some(g) = s.gpu {
            self.gpu_cull_ms += g.cull_ms;
            self.gpu_draw_ms += g.draw_ms;
            self.gpu_samples += 1;
            self.draws = g.cube_draws + g.model_draws;
        }
    }

    /// One line per second, `None` in between.
    pub fn report(&mut self, pipeline: Option<(PipelineStats, MemoryStats)>) -> Option<String> {
        if self.since.elapsed() < Duration::from_secs(1) || self.frames == 0 {
            return None;
        }
        let n = self.frames as f32;
        let g = self.gpu_samples.max(1) as f32;
        let mut line = format!(
            "{:.0} fps | frame {:.2} ms | cpu {:.2} ms | gpu cull {:.3} ms draw {:.2} ms | draws {}",
            1000.0 / (self.frame_ms / n),
            self.frame_ms / n,
            self.cpu_ms / n,
            self.gpu_cull_ms / g,
            self.gpu_draw_ms / g,
            self.draws,
        );
        if let Some((p, mem)) = pipeline {
            line += &format!(
                " | meshed {} uploaded {} | quads {:.2}M | queue {} | gpu {} MB",
                p.meshed,
                p.uploaded,
                f64::from(p.quads) / 1e6,
                p.queued,
                mem.total_bytes() >> 20,
            );
        }
        *self = Self::default();
        Some(line)
    }
}
