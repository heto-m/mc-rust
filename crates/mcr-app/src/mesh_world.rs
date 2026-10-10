use mcr_render::Camera;
use mcr_world::{ChunkPos, ChunkStore, SectionPos};

/// What the mesh pipeline needs from whatever owns the loaded chunks.
pub trait MeshWorld {
    fn store(&self) -> &ChunkStore;
    fn store_mut(&mut self) -> &mut ChunkStore;
    fn camera(&self) -> &Camera;
    fn render_distance(&self) -> i32;
    /// Whether `pos` is expected to arrive, so meshing next to it should wait.
    fn is_pending(&self, pos: ChunkPos) -> bool;
    fn loaded_columns(&self) -> Vec<ChunkPos>;
    /// Columns removed since the last call.
    fn take_unloaded(&mut self) -> Vec<ChunkPos>;
    /// Names indexed by the biome ids used in the stored columns; may grow while columns arrive.
    fn biome_names(&self) -> Vec<String>;
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
