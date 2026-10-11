use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use mcr_mesh::{ATTR_SOLID_RENDER, MeshInput, MeshPool, MeshResult};
use mcr_render::{Camera, FrameInputs, FrameStats, Overlay, Renderer};
use mcr_world::{ChunkPos, ChunkStore, SectionPos};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::assets::Assets;
use crate::mesh_world::MeshWorld;

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

/// Turns dirty sections of a [`MeshWorld`] into GPU geometry and keeps the renderer in step with it.
pub struct RenderSystem {
    pub renderer: Renderer,
    assets: Arc<Assets>,
    pool: MeshPool,
    mesh_threads: usize,
    known_biomes: usize,
    last_biome_check: Instant,
    waiting: FxHashMap<ChunkPos, u128>,
    touched: FxHashSet<ChunkPos>,
    urgent: FxHashSet<SectionPos>,
    urgent_inflight: FxHashSet<SectionPos>,
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

/// Sections beyond the 128th of a (hostile) tall dimension are not meshed.
const MAX_SECTIONS: i32 = 128;
const URGENT_PER_FRAME: usize = 64;

fn section_bit(index: i32) -> u128 {
    if (0..MAX_SECTIONS).contains(&index) {
        1u128 << index
    } else {
        0
    }
}

fn all_sections(count: i32) -> u128 {
    match count {
        n if n <= 0 => 0,
        n if n >= MAX_SECTIONS => u128::MAX,
        n => (1u128 << n) - 1,
    }
}

/// Queues dirty sections, dropping columns whose eight neighbours are not all loaded.
fn queue_dirty(
    waiting: &mut FxHashMap<ChunkPos, u128>,
    touched: &mut FxHashSet<ChunkPos>,
    store: &mut ChunkStore,
    skip: &FxHashSet<SectionPos>,
    mut unrender: impl FnMut(ChunkPos, u128),
) {
    let base = store.config().min_y() >> 4;
    let count = (store.config().height() >> 4) as i32;
    touched.clear();
    for pos in store.drain_dirty() {
        let index = pos.y - base;
        if skip.contains(&pos) || !(0..count).contains(&index) {
            continue;
        }
        *waiting.entry(pos.chunk()).or_default() |= section_bit(index);
        touched.insert(pos.chunk());
    }
    // Loads and unloads dirty the whole 3x3, so only touched columns change readiness.
    for pos in touched.iter() {
        if !store.neighbourhood_loaded(*pos)
            && let Some(mask) = waiting.remove(pos)
        {
            unrender(*pos, mask);
        }
    }
}

impl RenderSystem {
    pub fn new(mut renderer: Renderer, assets: Arc<Assets>, world: &dyn MeshWorld) -> Self {
        renderer.set_texture_groups(&assets.cache.texture_groups());
        renderer.set_tint_palette(&assets.cache.palette());
        renderer.set_animations(assets.cache.animations());
        let mesh_threads = std::thread::available_parallelism()
            .map_or(4, usize::from)
            .saturating_sub(2)
            .max(2);
        let names = world.biome_names();
        let mut pool = MeshPool::new(
            assets.cache.clone(),
            assets.biome_colors(&names),
            mesh_threads,
            mcr_mesh::DEFAULT_BLEND_RADIUS,
        );
        pool.set_biome_seed(world.biome_seed());
        Self {
            renderer,
            assets,
            pool,
            mesh_threads,
            known_biomes: names.len(),
            last_biome_check: Instant::now(),
            waiting: FxHashMap::default(),
            touched: FxHashSet::default(),
            urgent: FxHashSet::default(),
            urgent_inflight: FxHashSet::default(),
            results: Vec::new(),
            ready: VecDeque::new(),
            meshed: 0,
            uploaded: 0,
            arena_full_reported: false,
        }
    }

    /// Swaps in freshly baked assets and remeshes every loaded section in place.
    pub fn reload_assets(&mut self, assets: Arc<Assets>, world: &dyn MeshWorld) {
        self.renderer
            .set_texture_groups(&assets.cache.texture_groups());
        self.renderer.set_tint_palette(&assets.cache.palette());
        self.renderer.set_animations(assets.cache.animations());
        let names = world.biome_names();
        self.known_biomes = names.len();
        self.pool = MeshPool::new(
            assets.cache.clone(),
            assets.biome_colors(&names),
            self.mesh_threads,
            mcr_mesh::DEFAULT_BLEND_RADIUS,
        );
        self.pool.set_biome_seed(world.biome_seed());
        self.assets = assets;
        self.ready.clear();
        self.urgent.clear();
        self.urgent_inflight.clear();
        self.requeue_all(world);
    }

    fn requeue_all(&mut self, world: &dyn MeshWorld) {
        let all = all_sections((world.store().config().height() >> 4) as i32);
        for pos in world.loaded_columns() {
            if world.store().neighbourhood_loaded(pos) {
                self.waiting.insert(pos, all);
            }
        }
    }

    pub fn settled(&self, world: &dyn MeshWorld) -> bool {
        world.streaming_idle()
            && world.store().dirty_count() == 0
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

    pub fn update(&mut self, world: &mut dyn MeshWorld) -> Result<()> {
        self.remove_unloaded(world);
        self.refresh_biomes(world);
        self.pool.set_biome_seed(world.biome_seed());
        self.urgent.extend(world.take_urgent());
        self.collect_dirty(world);
        self.submit_urgent(world);
        self.submit(world);
        self.upload(world);
        self.sync_camera(world);
        Ok(())
    }

    /// Tells the renderer whether the camera sits inside an opaque block.
    pub fn sync_camera(&mut self, world: &dyn MeshWorld) {
        let eye = world.camera().block_pos();
        let in_opaque = world
            .store()
            .block(eye.x, eye.y, eye.z)
            .is_some_and(|b| self.assets.cache.attr(b.0) & ATTR_SOLID_RENDER != 0);
        self.renderer.set_camera_in_opaque_block(in_opaque);
    }

    pub fn draw(&mut self, camera: &Camera, sky_light: f32) -> Option<FrameStats> {
        self.draw_with(camera, sky_light, None)
    }

    pub fn draw_with(
        &mut self,
        camera: &Camera,
        sky_light: f32,
        overlay: Option<&mut dyn Overlay>,
    ) -> Option<FrameStats> {
        self.renderer
            .render_with_overlay(&FrameInputs { camera, sky_light }, overlay)
            .ok()
    }

    fn min_section(world: &dyn MeshWorld) -> i32 {
        world.store().config().min_y() >> 4
    }

    /// Drops every section of `world` from the GPU and returns the renderer for the next session.
    pub fn into_parts(mut self, world: &dyn MeshWorld) -> (Renderer, Arc<Assets>) {
        let base = Self::min_section(world);
        let count = (world.store().config().height() >> 4) as i32;
        for pos in world.loaded_columns() {
            for y in base..base + count {
                self.renderer.remove_section([pos.x, y, pos.z]);
            }
        }
        (self.renderer, self.assets)
    }

    fn remove_unloaded(&mut self, world: &mut dyn MeshWorld) {
        let base = Self::min_section(world);
        let count = (world.store().config().height() >> 4) as i32;
        for pos in world.take_unloaded() {
            self.waiting.remove(&pos);
            for y in base..base + count {
                self.renderer.remove_section([pos.x, y, pos.z]);
            }
        }
    }

    fn refresh_biomes(&mut self, world: &dyn MeshWorld) {
        if self.last_biome_check.elapsed() < BIOME_CHECK_INTERVAL {
            return;
        }
        self.last_biome_check = Instant::now();
        let names = world.biome_names();
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
        self.requeue_all(world);
    }

    fn collect_dirty(&mut self, world: &mut dyn MeshWorld) {
        let base = Self::min_section(world);
        let renderer = &mut self.renderer;
        queue_dirty(
            &mut self.waiting,
            &mut self.touched,
            world.store_mut(),
            &self.urgent,
            |pos, mask| {
                for index in 0..MAX_SECTIONS {
                    if mask & section_bit(index) != 0 {
                        renderer.remove_section([pos.x, base + index, pos.z]);
                    }
                }
            },
        );
    }

    fn submit_urgent(&mut self, world: &dyn MeshWorld) {
        let base = Self::min_section(world);
        let count = (world.store().config().height() >> 4) as i32;
        let start = Instant::now();
        let batch: Vec<SectionPos> = self.urgent.iter().copied().take(URGENT_PER_FRAME).collect();
        for pos in batch {
            let index = pos.y - base;
            if !(0..count).contains(&index) {
                self.urgent.remove(&pos);
                continue;
            }
            if start.elapsed() >= SNAPSHOT_BUDGET {
                break;
            }
            self.urgent.remove(&pos);
            if let Some(mask) = self.waiting.get_mut(&pos.chunk()) {
                *mask &= !section_bit(index);
            }
            if !world.store().neighbourhood_loaded(pos.chunk()) {
                self.renderer.remove_section([pos.x, pos.y, pos.z]);
                continue;
            }
            let Some(column) = world.store().column(pos.chunk()) else {
                continue;
            };
            if column.section(index as usize).is_none_or(|s| s.is_empty()) {
                self.renderer.remove_section([pos.x, pos.y, pos.z]);
            } else if let Some(input) =
                MeshInput::snapshot(world.store(), pos, self.pool.blend_radius())
            {
                self.pool.submit(input, 0);
                self.urgent_inflight.insert(pos);
            }
        }
        if self.urgent.len() > URGENT_PER_FRAME * 16 {
            for pos in std::mem::take(&mut self.urgent) {
                let index = pos.y - base;
                if (0..count).contains(&index) {
                    *self.waiting.entry(pos.chunk()).or_default() |= section_bit(index);
                }
            }
        }
    }

    fn submit(&mut self, world: &dyn MeshWorld) {
        let cap = self.mesh_threads * 6;
        if self.waiting.is_empty() || self.pool.pending() >= cap {
            return;
        }
        let start = Instant::now();
        let camera = world.camera();
        let cam = camera.position;
        let fwd = camera.forward();
        let mut order: Vec<(f32, ChunkPos)> = self
            .waiting
            .keys()
            .map(|p| {
                let dx = (f64::from(p.x) * 16.0 + 8.0 - cam.x) as f32;
                let dz = (f64::from(p.z) * 16.0 + 8.0 - cam.z) as f32;
                let behind = dx * fwd.x + dz * fwd.z < 0.0;
                let d2 = dx * dx + dz * dz;
                (if behind && d2 > 1024.0 { d2 * 4.0 } else { d2 }, *p)
            })
            .collect();
        order.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));

        let base = Self::min_section(world);
        for (rank, (_, pos)) in order.into_iter().enumerate() {
            if self.pool.pending() >= cap || start.elapsed() >= SNAPSHOT_BUDGET {
                break;
            }
            let Some(mask) = self.waiting.remove(&pos) else {
                continue;
            };
            if !world.store().neighbourhood_loaded(pos) {
                continue;
            }
            let Some(column) = world.store().column(pos) else {
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
                    MeshInput::snapshot(world.store(), section, self.pool.blend_radius())
                {
                    self.pool
                        .submit(input, ((rank as u32) << 5 | index as u32) + 1);
                }
            }
        }
    }

    fn upload(&mut self, world: &dyn MeshWorld) {
        self.results.clear();
        self.pool.poll(&mut self.results);
        self.meshed += self.results.len() as u64;
        for result in self.results.drain(..) {
            if self.urgent_inflight.remove(&result.pos) {
                self.ready.push_front(result);
            } else {
                self.ready.push_back(result);
            }
        }
        let start = Instant::now();
        while let Some(MeshResult { pos, mesh }) = self.ready.pop_front() {
            if !world.store().neighbourhood_loaded(pos.chunk()) {
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

#[cfg(test)]
mod tests {
    use mcr_world::{ChunkColumn, WorldConfig};

    use super::*;

    fn cfg() -> WorldConfig {
        WorldConfig::new(35723, 80, -64, 384).unwrap()
    }

    #[derive(Default)]
    struct Queue {
        waiting: FxHashMap<ChunkPos, u128>,
        touched: FxHashSet<ChunkPos>,
        unrendered: Vec<ChunkPos>,
    }

    impl Queue {
        fn collect(&mut self, store: &mut ChunkStore) {
            let unrendered = &mut self.unrendered;
            queue_dirty(
                &mut self.waiting,
                &mut self.touched,
                store,
                &FxHashSet::default(),
                |pos, _| unrendered.push(pos),
            );
        }

        fn take(&mut self, pos: ChunkPos) -> Option<u128> {
            self.waiting.remove(&pos)
        }
    }

    fn load(store: &mut ChunkStore, x: i32, z: i32) {
        store.insert(ChunkPos::new(x, z), ChunkColumn::empty(&cfg()));
    }

    fn ring(store: &mut ChunkStore, skip: (i32, i32)) {
        for z in -1..=1 {
            for x in -1..=1 {
                if (x, z) != skip {
                    load(store, x, z);
                }
            }
        }
    }

    #[test]
    fn column_without_all_neighbours_is_not_queued() {
        let mut store = ChunkStore::new(cfg());
        let mut q = Queue::default();
        ring(&mut store, (1, 1));
        q.collect(&mut store);
        assert!(q.waiting.is_empty());
        assert!(q.unrendered.contains(&ChunkPos::new(0, 0)));
    }

    #[test]
    fn last_diagonal_neighbour_queues_the_whole_column() {
        let mut store = ChunkStore::new(cfg());
        let mut q = Queue::default();
        ring(&mut store, (1, 1));
        q.collect(&mut store);
        load(&mut store, 1, 1);
        q.collect(&mut store);
        assert_eq!(q.take(ChunkPos::new(0, 0)), Some(all_sections(24)));
        assert!(q.waiting.is_empty(), "edge columns stay unmeshed");
    }

    #[test]
    fn unloading_a_neighbour_unrenders_the_column() {
        let mut store = ChunkStore::new(cfg());
        let mut q = Queue::default();
        ring(&mut store, (9, 9));
        q.collect(&mut store);
        assert!(q.take(ChunkPos::new(0, 0)).is_some());
        q.unrendered.clear();
        store.remove(ChunkPos::new(-1, -1));
        q.collect(&mut store);
        assert!(q.waiting.is_empty());
        assert!(q.unrendered.contains(&ChunkPos::new(0, 0)));
    }

    #[test]
    fn border_column_meshes_once_its_outer_ring_arrives() {
        let mut store = ChunkStore::new(cfg());
        let mut q = Queue::default();
        ring(&mut store, (9, 9));
        q.collect(&mut store);
        q.waiting.clear();
        for z in -1..=1 {
            load(&mut store, 2, z);
        }
        q.collect(&mut store);
        assert!(q.waiting.contains_key(&ChunkPos::new(1, 0)));
        assert!(!q.waiting.contains_key(&ChunkPos::new(1, 1)));
        q.waiting.clear();
        for x in 0..=2 {
            load(&mut store, x, 2);
        }
        q.collect(&mut store);
        assert!(q.waiting.contains_key(&ChunkPos::new(1, 1)));
    }

    #[test]
    fn hostile_section_indices_are_ignored() {
        assert_eq!(section_bit(-1), 0);
        assert_eq!(section_bit(MAX_SECTIONS), 0);
        assert_eq!(section_bit(i32::MIN), 0);
        assert_eq!(section_bit(127), 1u128 << 127);
        assert_eq!(all_sections(i32::MAX), u128::MAX);
        assert_eq!(all_sections(-5), 0);
        assert_eq!(all_sections(24), (1 << 24) - 1);
    }
}
