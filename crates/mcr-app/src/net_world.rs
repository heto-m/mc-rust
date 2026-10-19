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
    inbox: Inbox,
}

/// What the mesh pipeline and the game take from the client's events.
#[derive(Debug)]
struct Inbox {
    loaded: FxHashSet<ChunkPos>,
    unloaded: Vec<ChunkPos>,
    urgent: Vec<SectionPos>,
    events: Vec<ClientEvent>,
    last_chunk: Instant,
    reset: bool,
}

/// Events after which the client holds a different world: respawn, reconfiguration and the login after it.
pub fn resets_world(event: &ClientEvent) -> bool {
    matches!(
        event,
        ClientEvent::Respawned | ClientEvent::Reconfiguring | ClientEvent::Joined { .. }
    )
}

impl std::fmt::Debug for NetworkWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkWorld")
            .field("loaded", &self.inbox.loaded.len())
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
            inbox: Inbox::new(),
        }
    }

    pub fn set_render_distance(&mut self, distance: i32) {
        self.render_distance = distance.clamp(2, 64);
    }

    pub fn loaded_count(&self) -> usize {
        self.inbox.loaded.len()
    }

    /// Runs one client tick; chunk events are folded into the world and, like the rest, left in
    /// `events`.
    pub fn tick(&mut self, input: &PlayerInput) -> Result<()> {
        let result = self.client.tick(input);
        for event in self.client.drain_events() {
            self.inbox.absorb(event);
        }
        result
    }

    pub fn take_events(&mut self) -> Vec<ClientEvent> {
        std::mem::take(&mut self.inbox.events)
    }
}

impl Inbox {
    fn new() -> Self {
        Self {
            loaded: FxHashSet::default(),
            unloaded: Vec::new(),
            urgent: Vec::new(),
            events: Vec::new(),
            last_chunk: Instant::now(),
            reset: false,
        }
    }

    fn absorb(&mut self, event: ClientEvent) {
        match event {
            ClientEvent::ChunkLoaded(pos) => {
                self.loaded.insert(pos);
                self.last_chunk = Instant::now();
            }
            ClientEvent::ChunkUnloaded(pos) => {
                self.loaded.remove(&pos);
                self.unloaded.push(pos);
            }
            ref e if resets_world(e) => {
                self.loaded.clear();
                self.unloaded.clear();
                self.urgent.clear();
                self.reset = true;
            }
            ClientEvent::BlockChanged { pos, .. } => self.mark_urgent(pos),
            _ => {}
        }
        self.events.push(event);
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

    fn loaded_columns(&self) -> Vec<ChunkPos> {
        self.inbox.loaded.iter().copied().collect()
    }

    fn take_unloaded(&mut self) -> Vec<ChunkPos> {
        std::mem::take(&mut self.inbox.unloaded)
    }

    fn take_reset(&mut self) -> bool {
        std::mem::take(&mut self.inbox.reset)
    }

    fn biome_names(&self) -> Vec<String> {
        self.client
            .state
            .registries
            .get(BIOME)
            .map(|r| r.entries.iter().map(|e| e.id.as_str().to_owned()).collect())
            .unwrap_or_default()
    }

    fn biome_climates(&self) -> Vec<Option<mcr_mesh::BiomeClimate>> {
        self.client
            .state
            .registries
            .get(BIOME)
            .map(|r| {
                r.entries
                    .iter()
                    .map(|e| e.data.as_ref().and_then(mcr_mesh::BiomeClimate::from_nbt))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn streaming_idle(&self) -> bool {
        self.client.state.player.loaded && self.inbox.last_chunk.elapsed() > QUIET_AFTER_CHUNK
    }

    fn biome_seed(&self) -> i64 {
        self.client.state.dimension.seed
    }

    fn take_urgent(&mut self) -> Vec<SectionPos> {
        std::mem::take(&mut self.inbox.urgent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_events_reach_the_game_too() {
        let mut inbox = Inbox::new();
        let pos = ChunkPos::new(3, -4);
        inbox.absorb(ClientEvent::ChunkLoaded(pos));
        inbox.absorb(ClientEvent::ChunkUnloaded(pos));
        assert!(inbox.loaded.is_empty());
        assert_eq!(inbox.unloaded, [pos]);
        assert!(matches!(
            inbox.events[..],
            [ClientEvent::ChunkLoaded(a), ClientEvent::ChunkUnloaded(b)] if a == pos && b == pos
        ));
    }
}
