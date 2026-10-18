use crate::container::Limits;
use crate::{Result, WorldError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BlockState(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Biome(pub u16);

impl BlockState {
    pub const AIR: Self = Self(0);
}

/// Registry sizes and dimension shape; everything version- or dimension-specific lives here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldConfig {
    block_states: u32,
    biomes: u32,
    min_y: i32,
    height: u32,
    air: [BlockState; 3],
}

impl WorldConfig {
    pub const MAX_IDS: u32 = 1 << 16;
    /// Modded registries go past 65536 block states (a mod adding a property to every waterloggable block).
    pub const MAX_BLOCK_STATES: u32 = 1 << 20;

    pub fn new(block_states: u32, biomes: u32, min_y: i32, height: u32) -> Result<Self> {
        if block_states == 0 || block_states > Self::MAX_BLOCK_STATES {
            return Err(WorldError::InvalidConfig(
                "block state count must be 1..=1048576",
            ));
        }
        if biomes == 0 || biomes > Self::MAX_IDS {
            return Err(WorldError::InvalidConfig("biome count must be 1..=65536"));
        }
        if min_y.rem_euclid(16) != 0 || height == 0 || !height.is_multiple_of(16) || height > 4096 {
            return Err(WorldError::InvalidConfig(
                "min_y and height must be multiples of 16, height 16..=4096",
            ));
        }
        Ok(Self {
            block_states,
            biomes,
            min_y,
            height,
            air: [BlockState::AIR; 3],
        })
    }

    /// Air, cave air and void air; they do not count towards a section's non-empty total.
    pub fn with_air(mut self, air: [BlockState; 3]) -> Self {
        self.air = air;
        self
    }

    pub fn block_state_count(&self) -> u32 {
        self.block_states
    }

    pub fn biome_count(&self) -> u32 {
        self.biomes
    }

    pub fn min_y(&self) -> i32 {
        self.min_y
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn section_count(&self) -> usize {
        (self.height / 16) as usize
    }

    pub fn light_section_count(&self) -> usize {
        self.section_count() + 2
    }

    pub fn is_air(&self, state: BlockState) -> bool {
        self.air.contains(&state)
    }

    pub fn block_limits(&self) -> Limits {
        Limits::new(self.block_states, 4, 8)
    }

    pub fn biome_limits(&self) -> Limits {
        Limits::new(self.biomes, 1, 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_registries_past_the_limits() {
        assert!(WorldConfig::new(WorldConfig::MAX_BLOCK_STATES + 1, 10, -64, 384).is_err());
        assert!(WorldConfig::new(70472, 10, -64, 384).is_ok());
        assert!(WorldConfig::new(10, 65537, -64, 384).is_err());
        assert!(WorldConfig::new(0, 10, -64, 384).is_err());
    }

    #[test]
    fn rejects_unaligned_dimensions() {
        assert!(WorldConfig::new(35723, 80, -60, 384).is_err());
        assert!(WorldConfig::new(35723, 80, -64, 380).is_err());
    }
}
