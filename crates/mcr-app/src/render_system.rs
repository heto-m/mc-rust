use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use mcr_mesh::{MeshInput, MeshPool, MeshResult};
use mcr_render::{Camera, FrameInputs, FrameStats, Renderer};
use mcr_world::{ChunkPos, SectionPos};
use rustc_hash::FxHashMap;

use crate::assets::Assets;
use crate::game::{Game, chunk_of, in_distance};

const SNAPSHOT_BUDGET: Duration = Duration::from_millis(2);
const UPLOAD_BUDGET: Duration = Duration::from_millis(3);
const BIOME_CHECK_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Default, Clone, Copy)]
pub struct PipelineStats {
    pub meshed: u64,
    pub uploaded: u64,
    pub queued: usize,
    pub quads: u32,
}

/// Turns dirty sections of a [`Game`] into GPU geometry and keeps the renderer in step with it.
pub struct RenderSystem {
    pub renderer: Renderer,
    assets: Arc<Assets>,
    pool: MeshPool,
    mesh_threads: usize,
    known_biomes: usize,
    last_biome_check: Instant,
    waiting: FxHashMap<ChunkPos, u64>,
    results: Vec<MeshResult>,
    ready: VecDeque<MeshResult>,
    meshed: u64,
    uploaded: u64,
    arena_full_reported: bool,
}

impl std::fmt::Debug for RenderSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderSystem")
            .field("pool", &self.pool)
            .field("waiting_columns", &self.waiting.len())
            .finish_non_exhaustive()
    }
}

fn section_bit(index: i32) -> u64 {
    1u64 << index
}

impl RenderSystem {
    pub fn new(mut renderer: Renderer, assets: Arc<Assets>, game: &Game) -> Self {
        renderer.set_texture_groups(&assets.cache.texture_groups());
        renderer.set_tint_palette(&assets.cache.palette());
        let mesh_threads = std::thread::available_parallelism()
            .map_or(4, usize::from)
            .saturating_sub(2)
            .max(2);
        let names = game.source().biome_names();
        let pool = MeshPool::new(
            assets.cache.clone(),
            assets.biome_colors(&names),
            mesh_threads,
            mcr_mesh::DEFAULT_BLEND_RADIUS,
        );
        Self {
            renderer,
            assets,
            pool,
            mesh_threads,
            known_biomes: names.len(),
            last_biome_check: Instant::now(),
            waiting: FxHashMap::default(),
            results: Vec::new(),
            ready: VecDeque::new(),
            meshed: 0,
            uploaded: 0,
            arena_full_reported: false,
        }
    }

    pub fn settled(&self, game: &Game) -> bool {
        game.streaming_idle()
            && game.store.dirty_count() == 0
            && self.waiting.is_empty()
            && self.ready.is_empty()
            && self.pool.pending() == 0
            && self.renderer.pending_upload_bytes() == 0
    }

    pub fn stats(&self) -> PipelineStats {
        PipelineStats {
            meshed: self.meshed,
            uploaded: self.uploaded,
            queued: self.pool.pending()
                + self.ready.len()
                + self
                    .waiting
                    .values()
                    .map(|m| m.count_ones() as usize)
                    .sum::<usize>(),
            quads: self.renderer.cube_quads_used() + self.renderer.model_quads_used(),
        }
    }

    pub fn update(&mut self, game: &mut Game) -> Result<()> {
        self.remove_unloaded(game);
        self.refresh_biomes(game);
        self.collect_dirty(game);
        self.submit(game);
        self.upload(game);
        Ok(())
    }

    pub fn draw(&mut self, camera: &Camera) -> Option<FrameStats> {
        self.renderer
            .render(&FrameInputs {
                camera,
                sky_light: 1.0,
            })
            .ok()
    }

    fn min_section(game: &Game) -> i32 {
        game.store.config().min_y() >> 4
    }

    fn remove_unloaded(&mut self, game: &mut Game) {
        let base = Self::min_section(game);
        let count = (game.store.config().height() >> 4) as i32;
        for pos in game.take_unloaded() {
            self.waiting.remove(&pos);
            for y in base..base + count {
                self.renderer.remove_section([pos.x, y, pos.z]);
            }
        }
    }

    fn refresh_biomes(&mut self, game: &Game) {
        if self.last_biome_check.elapsed() < BIOME_CHECK_INTERVAL {
            return;
        }
        self.last_biome_check = Instant::now();
        let names = game.source().biome_names();
        if names.len() == self.known_biomes {
            return;
        }
        self.known_biomes = names.len();
        self.pool = MeshPool::new(
            self.assets.cache.clone(),
            self.assets.biome_colors(&names),
            self.mesh_threads,
            mcr_mesh::DEFAULT_BLEND_RADIUS,
        );
        self.ready.clear();
        let all = (1u64 << (game.store.config().height() >> 4)) - 1;
        for pos in game.loaded_columns() {
            self.waiting.insert(pos, all);
        }
    }

    fn collect_dirty(&mut self, game: &mut Game) {
        let base = Self::min_section(game);
        let here = chunk_of(&game.camera);
        let distance = game.render_distance();
        let dirty: Vec<SectionPos> = game.store.drain_dirty().collect();
        for pos in dirty {
            if in_distance(pos.chunk(), here, distance) {
                *self.waiting.entry(pos.chunk()).or_default() |= section_bit(pos.y - base);
            }
        }
    }

    fn submit(&mut self, game: &Game) {
        let cap = self.mesh_threads * 6;
        if self.waiting.is_empty() || self.pool.pending() >= cap {
            return;
        }
        let start = Instant::now();
        let cam = game.camera.position;
        let fwd = game.camera.forward();
        let mut order: Vec<(f32, ChunkPos)> = self
            .waiting
            .keys()
            .filter(|p| self.neighbours_settled(game, **p))
            .map(|p| {
                let dx = (f64::from(p.x) * 16.0 + 8.0 - cam.x) as f32;
                let dz = (f64::from(p.z) * 16.0 + 8.0 - cam.z) as f32;
                let behind = dx * fwd.x + dz * fwd.z < 0.0;
                let d2 = dx * dx + dz * dz;
                (if behind && d2 > 1024.0 { d2 * 4.0 } else { d2 }, *p)
            })
            .collect();
        order.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));

        let base = Self::min_section(game);
        for (rank, (_, pos)) in order.into_iter().enumerate() {
            if self.pool.pending() >= cap || start.elapsed() >= SNAPSHOT_BUDGET {
                break;
            }
            let Some(mask) = self.waiting.remove(&pos) else {
                continue;
            };
            let Some(column) = game.store.column(pos) else {
                continue;
            };
            for index in 0..column.sections().len() as i32 {
                if mask & section_bit(index) == 0 {
                    continue;
                }
                let section = SectionPos::new(pos.x, base + index, pos.z);
                if column.section(index as usize).is_none_or(|s| s.is_empty()) {
                    self.renderer
                        .remove_section([section.x, section.y, section.z]);
                } else if let Some(input) =
                    MeshInput::snapshot(&game.store, section, self.pool.blend_radius())
                {
                    self.pool.submit(input, (rank as u32) << 5 | index as u32);
                }
            }
        }
    }

    fn neighbours_settled(&self, game: &Game, pos: ChunkPos) -> bool {
        (-1..=1)
            .all(|dz| (-1..=1).all(|dx| !game.is_pending(ChunkPos::new(pos.x + dx, pos.z + dz))))
    }

    fn upload(&mut self, game: &Game) {
        self.results.clear();
        self.pool.poll(&mut self.results);
        self.meshed += self.results.len() as u64;
        self.ready.extend(self.results.drain(..));
        let start = Instant::now();
        while let Some(MeshResult { pos, mesh }) = self.ready.pop_front() {
            if game.store.column(pos.chunk()).is_none() {
                continue;
            }
            match self.renderer.set_section([pos.x, pos.y, pos.z], &mesh) {
                Ok(()) => self.uploaded += 1,
                Err(e) if !self.arena_full_reported => {
                    self.arena_full_reported = true;
                    eprintln!("upload failed: {e}");
                }
                Err(_) => {}
            }
            if start.elapsed() >= UPLOAD_BUDGET {
                break;
            }
        }
    }
}
