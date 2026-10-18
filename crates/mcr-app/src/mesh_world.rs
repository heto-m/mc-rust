use mcr_render::Camera;
use mcr_world::{ChunkPos, ChunkStore, SectionPos};

/// What the mesh pipeline needs from whatever owns the loaded chunks.
pub trait MeshWorld {
    fn store(&self) -> &ChunkStore;
    fn store_mut(&mut self) -> &mut ChunkStore;
    fn camera(&self) -> &Camera;
    fn render_distance(&self) -> i32;
    fn loaded_columns(&self) -> Vec<ChunkPos>;
    /// Columns removed since the last call.
    fn take_unloaded(&mut self) -> Vec<ChunkPos>;
    /// Whether the whole world was replaced since the last call.
    fn take_reset(&mut self) -> bool {
        false
    }
    /// Names indexed by the biome ids used in the stored columns; may grow while columns arrive.
    fn biome_names(&self) -> Vec<String>;
    /// The climate the server sent for each biome, by id; `None` (or a short list) falls back to the jar's.
    fn biome_climates(&self) -> Vec<Option<mcr_mesh::BiomeClimate>> {
        Vec::new()
    }
    /// Hashed world seed used for biome zoom.
    fn biome_seed(&self) -> i64 {
        0
    }
    fn streaming_idle(&self) -> bool;
    /// Sections changed by block updates since the last call; they are meshed ahead of streaming.
    fn take_urgent(&mut self) -> Vec<SectionPos> {
        Vec::new()
    }
}
