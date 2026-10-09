use mcr_world::{ChunkPos, ChunkStore};

pub trait CollisionWorld {
    /// Unloaded positions must report air (state 0).
    fn block_state(&self, x: i32, y: i32, z: i32) -> u32;

    fn is_loaded(&self, _x: i32, _z: i32) -> bool {
        true
    }

    fn min_y(&self) -> i32 {
        -64
    }
}

impl CollisionWorld for ChunkStore {
    #[inline]
    fn block_state(&self, x: i32, y: i32, z: i32) -> u32 {
        self.block(x, y, z).map_or(0, |s| u32::from(s.0))
    }

    fn is_loaded(&self, x: i32, z: i32) -> bool {
        self.column(ChunkPos::from_block(x, z)).is_some()
    }

    fn min_y(&self) -> i32 {
        self.config().min_y()
    }
}
