use mcr_buf::take;

use crate::container::{
    BIOME_ENTRIES, BLOCK_ENTRIES, BiomeContainer, BlockContainer, biome_index, block_index,
};
use crate::{Biome, BlockState, Result, WorldConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkSection {
    blocks: BlockContainer,
    biomes: BiomeContainer,
    non_empty: u16,
    fluid_count: u16,
}

impl ChunkSection {
    pub fn empty(cfg: &WorldConfig) -> Self {
        Self {
            blocks: BlockContainer::filled(BlockState::AIR.0, cfg.block_limits()),
            biomes: BiomeContainer::filled(0, cfg.biome_limits()),
            non_empty: 0,
            fluid_count: 0,
        }
    }

    pub fn from_containers(
        blocks: BlockContainer,
        biomes: BiomeContainer,
        non_empty: u16,
        fluid_count: u16,
    ) -> Self {
        Self {
            blocks,
            biomes,
            non_empty,
            fluid_count,
        }
    }

    pub fn blocks(&self) -> &BlockContainer {
        &self.blocks
    }

    pub fn biomes(&self) -> &BiomeContainer {
        &self.biomes
    }

    /// Count of non-air blocks as last reported by the server, adjusted by local edits.
    pub fn non_empty_count(&self) -> u16 {
        self.non_empty
    }

    /// As sent by the server; local edits do not maintain it.
    pub fn fluid_count(&self) -> u16 {
        self.fluid_count
    }

    pub fn is_empty(&self) -> bool {
        self.non_empty == 0
    }

    pub fn replace_biomes(&mut self, biomes: BiomeContainer) {
        self.biomes = biomes;
    }

    #[inline]
    pub fn block(&self, x: usize, y: usize, z: usize) -> BlockState {
        BlockState(self.blocks.get(block_index(x, y, z)))
    }

    pub fn set_block(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        state: BlockState,
        cfg: &WorldConfig,
    ) -> BlockState {
        let old = BlockState(self.blocks.set(block_index(x, y, z), state.0));
        match (cfg.is_air(old), cfg.is_air(state)) {
            (true, false) => self.non_empty = self.non_empty.saturating_add(1),
            (false, true) => self.non_empty = self.non_empty.saturating_sub(1),
            _ => {}
        }
        old
    }

    #[inline]
    pub fn biome(&self, x: usize, y: usize, z: usize) -> Biome {
        Biome(self.biomes.get(biome_index(x, y, z)) as u16)
    }

    pub fn set_biome(&mut self, x: usize, y: usize, z: usize, biome: Biome) -> Biome {
        Biome(self.biomes.set(biome_index(x, y, z), u32::from(biome.0)) as u16)
    }

    pub fn unpack_blocks(&self, out: &mut [u32; BLOCK_ENTRIES]) {
        self.blocks.unpack_into(out);
    }

    pub fn unpack_biomes(&self, out: &mut [u32; BIOME_ENTRIES]) {
        self.biomes.unpack_into(out);
    }

    pub fn decode(input: &mut &[u8], cfg: &WorldConfig) -> Result<Self> {
        let head = take(input, 4)?;
        let non_empty = u16::from_be_bytes([head[0], head[1]]);
        let fluid_count = u16::from_be_bytes([head[2], head[3]]);
        let blocks = BlockContainer::decode(input, cfg.block_limits())?;
        let biomes = BiomeContainer::decode(input, cfg.biome_limits())?;
        Ok(Self {
            blocks,
            biomes,
            non_empty,
            fluid_count,
        })
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.non_empty.to_be_bytes());
        out.extend_from_slice(&self.fluid_count.to_be_bytes());
        self.blocks.encode(out);
        self.biomes.encode(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> WorldConfig {
        WorldConfig::new(35723, 80, -64, 384).unwrap().with_air([
            BlockState(0),
            BlockState(18650),
            BlockState(18649),
        ])
    }

    #[test]
    fn set_tracks_non_empty() {
        let cfg = cfg();
        let mut s = ChunkSection::empty(&cfg);
        assert!(s.is_empty());
        s.set_block(1, 2, 3, BlockState(1), &cfg);
        s.set_block(1, 2, 4, BlockState(18650), &cfg);
        assert_eq!(s.non_empty_count(), 1);
        s.set_block(1, 2, 3, BlockState(18649), &cfg);
        assert!(s.is_empty());
        s.set_block(1, 2, 3, BlockState(0), &cfg);
        assert_eq!(s.non_empty_count(), 0);
    }

    #[test]
    fn coordinates_address_y_z_x() {
        let cfg = cfg();
        let mut s = ChunkSection::empty(&cfg);
        s.set_block(3, 9, 5, BlockState(77), &cfg);
        let mut flat = [0u32; BLOCK_ENTRIES];
        s.unpack_blocks(&mut flat);
        assert_eq!(flat[9 * 256 + 5 * 16 + 3], 77);
        assert_eq!(flat.iter().filter(|&&v| v != 0).count(), 1);
        s.set_biome(1, 2, 3, Biome(7));
        let mut b = [0u32; BIOME_ENTRIES];
        s.unpack_biomes(&mut b);
        assert_eq!(b[2 * 16 + 3 * 4 + 1], 7);
    }

    #[test]
    fn encode_decode_round_trip() {
        let cfg = cfg();
        let mut s = ChunkSection::empty(&cfg);
        for i in 0..40 {
            s.set_block(i % 16, i / 16, i % 7, BlockState(i as u32 + 1), &cfg);
        }
        s.set_biome(0, 0, 0, Biome(3));
        let mut bytes = Vec::new();
        s.encode(&mut bytes);
        let mut input = bytes.as_slice();
        assert_eq!(ChunkSection::decode(&mut input, &cfg).unwrap(), s);
        assert!(input.is_empty());
    }
}
