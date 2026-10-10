use mcr_buf::BlockPos;
use std::time::{Duration, Instant};

use mcr_client::{Client, ClientEvent, PlayerInput, Result};
use mcr_render::Camera;
use mcr_world::{ChunkPos, ChunkStore, SectionPos};
use rustc_hash::FxHashSet;

use crate::mesh_world::MeshWorld;

const BIOME: &str = "minecraft:worldgen/biome";
/// Per-tick bound; sections beyond it still remesh through the normal dirty path.
const MAX_URGENT: usize = 4096;
const QUIET_AFTER_CHUNK: Duration = Duration::from_millis(400);

/// The chunks of a connected [`Client`], exposed to the mesh pipeline without copying columns.
pub struct NetworkWorld {
    pub client: Client,
    pub camera: Camera,
    render_distance: i32,
    loaded: FxHashSet<ChunkPos>,
    unloaded: Vec<ChunkPos>,
    urgent: Vec<SectionPos>,
    events: Vec<ClientEvent>,
    last_chunk: Instant,
}

impl std::fmt::Debug for NetworkWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkWorld")
            .field("loaded", &self.loaded.len())
            .field("render_distance", &self.render_distance)
            .finish_non_exhaustive()
    }
}

impl NetworkWorld {
    pub fn new(client: Client, render_distance: i32) -> Self {
        Self {
            client,
            camera: Camera::default(),
            render_distance,
            loaded: FxHashSet::default(),
            unloaded: Vec::new(),
            urgent: Vec::new(),
            events: Vec::new(),
            last_chunk: Instant::now(),
        }
    }

    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }

    /// Runs one client tick; chunk events are folded into the world, the rest is left in `events`.
    pub fn tick(&mut self, input: &PlayerInput) -> Result<()> {
        let result = self.client.tick(input);
        self.collect_events();
        result
    }

    pub fn take_events(&mut self) -> Vec<ClientEvent> {
        std::mem::take(&mut self.events)
    }

    fn collect_events(&mut self) {
        let mut drained: Vec<ClientEvent> = self.client.drain_events().collect();
        for event in drained.drain(..) {
            match event {
                ClientEvent::ChunkLoaded(pos) => {
                    self.loaded.insert(pos);
                    self.last_chunk = Instant::now();
                }
                ClientEvent::ChunkUnloaded(pos) => {
                    self.loaded.remove(&pos);
                    self.unloaded.push(pos);
                }
                ClientEvent::Respawned => {
                    self.unloaded.extend(self.loaded.drain());
                    self.events.push(event);
                }
                ClientEvent::BlockChanged { pos, .. } => {
                    self.mark_urgent(pos);
                    self.events.push(event);
                }
                other => self.events.push(other),
            }
        }
    }

    fn mark_urgent(&mut self, pos: BlockPos) {
        if self.urgent.len() >= MAX_URGENT {
            return;
        }
        let (sx, sy, sz) = (pos.x >> 4, pos.y >> 4, pos.z >> 4);
        let edge = |v: i32| match v & 15 {
            0 => -1,
            15 => 1,
            _ => 0,
        };
        let (ex, ey, ez) = (edge(pos.x), edge(pos.y), edge(pos.z));
        self.urgent.push(SectionPos::new(sx, sy, sz));
        for (dx, dy, dz) in [(ex, 0, 0), (0, ey, 0), (0, 0, ez)] {
            if (dx, dy, dz) != (0, 0, 0) {
                self.urgent.push(SectionPos::new(sx + dx, sy + dy, sz + dz));
            }
        }
    }
}

impl MeshWorld for NetworkWorld {
    fn store(&self) -> &ChunkStore {
        &self.client.state.chunks
    }

    fn store_mut(&mut self) -> &mut ChunkStore {
        &mut self.client.state.chunks
    }

    fn camera(&self) -> &Camera {
        &self.camera
    }

    fn render_distance(&self) -> i32 {
        self.render_distance
            .min(self.client.state.world.view_distance.max(2))
    }

    fn is_pending(&self, pos: ChunkPos) -> bool {
        let state = &self.client.state;
        let (cx, cz) = state.world.chunk_center;
        let r = state.world.view_distance.max(2);
        (pos.x - cx).abs() <= r && (pos.z - cz).abs() <= r && !self.loaded.contains(&pos)
    }

    fn loaded_columns(&self) -> Vec<ChunkPos> {
        self.loaded.iter().copied().collect()
    }

    fn take_unloaded(&mut self) -> Vec<ChunkPos> {
        std::mem::take(&mut self.unloaded)
    }

    fn biome_names(&self) -> Vec<String> {
        self.client
            .state
            .registries
            .get(BIOME)
            .map(|r| r.entries.iter().map(|e| e.id.as_str().to_owned()).collect())
            .unwrap_or_default()
    }

    fn streaming_idle(&self) -> bool {
        self.client.state.player.loaded && self.last_chunk.elapsed() > QUIET_AFTER_CHUNK
    }

    fn biome_seed(&self) -> i64 {
        self.client.state.dimension.seed
    }

    fn take_urgent(&mut self) -> Vec<SectionPos> {
        std::mem::take(&mut self.urgent)
    }
}
