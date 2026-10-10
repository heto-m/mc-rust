use crate::bits;
use crate::payload::{ChunkData, LightData, LightNibbles};
use crate::{BlockState, ChunkSection, Result, WorldConfig, WorldError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum HeightmapKind {
    WorldSurfaceWorldgen = 0,
    WorldSurface = 1,
    OceanFloorWorldgen = 2,
    OceanFloor = 3,
    MotionBlocking = 4,
    MotionBlockingNoLeaves = 5,
}

impl HeightmapKind {
    pub const ALL: [Self; 6] = [
        Self::WorldSurfaceWorldgen,
        Self::WorldSurface,
        Self::OceanFloorWorldgen,
        Self::OceanFloor,
        Self::MotionBlocking,
        Self::MotionBlockingNoLeaves,
    ];

    pub fn from_id(id: u8) -> Option<Self> {
        Self::ALL.get(id as usize).copied()
    }
}

/// 256 packed column heights, stored relative to the dimension's `min_y`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heightmap {
    data: Box<[u64]>,
    bits: u8,
}

impl Heightmap {
    pub fn empty(cfg: &WorldConfig) -> Self {
        let bits = Self::bits_for(cfg);
        Self {
            data: vec![0; bits::long_count(bits, 256)].into(),
            bits,
        }
    }

    pub fn from_raw(data: Box<[u64]>, cfg: &WorldConfig) -> Result<Self> {
        let bits = Self::bits_for(cfg);
        let expected = bits::long_count(bits, 256);
        if data.len() != expected {
            return Err(WorldError::BadLength {
                what: "heightmap longs",
                expected,
                got: data.len(),
            });
        }
        Ok(Self { data, bits })
    }

    fn bits_for(cfg: &WorldConfig) -> u8 {
        bits::ceil_log2(cfg.height() + 1)
    }

    pub fn raw(&self) -> &[u64] {
        &self.data
    }

    /// Y of the first free block above the column's top block.
    #[inline]
    pub fn first_available(&self, x: usize, z: usize, min_y: i32) -> i32 {
        bits::get(&self.data, self.bits, (z & 15) * 16 + (x & 15)) as i32 + min_y
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Heightmaps([Option<Heightmap>; 6]);

impl Heightmaps {
    pub fn get(&self, kind: HeightmapKind) -> Option<&Heightmap> {
        self.0[kind as usize].as_ref()
    }

    pub fn set(&mut self, kind: HeightmapKind, map: Heightmap) {
        self.0[kind as usize] = Some(map);
    }
}

/// Block entity as sent on the wire; `nbt` holds the raw network-NBT bytes, empty when absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockEntityData {
    pub packed_xz: u8,
    pub y: i16,
    pub type_id: u32,
    pub nbt: Box<[u8]>,
}

impl BlockEntityData {
    pub fn x(&self) -> usize {
        (self.packed_xz >> 4) as usize
    }

    pub fn z(&self) -> usize {
        (self.packed_xz & 15) as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LightSlot {
    #[default]
    Absent,
    Dark,
    Data(LightNibbles),
}

impl LightSlot {
    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        match self {
            Self::Absent => None,
            Self::Dark => Some(0),
            Self::Data(n) => Some(n.get(x, y, z)),
        }
    }

    fn set(&mut self, (x, y, z): (usize, usize, usize), level: u8) -> bool {
        match self {
            Self::Absent => return false,
            Self::Dark => {
                if level == 0 {
                    return true;
                }
                *self = Self::Data(LightNibbles::default());
            }
            Self::Data(_) => {}
        }
        if let Self::Data(n) = self {
            n.set(x, y, z, level);
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkColumn {
    min_y: i32,
    sections: Vec<ChunkSection>,
    heightmaps: Heightmaps,
    block_entities: Vec<BlockEntityData>,
    sky: Vec<LightSlot>,
    block: Vec<LightSlot>,
}

impl ChunkColumn {
    pub fn empty(cfg: &WorldConfig) -> Self {
        Self {
            min_y: cfg.min_y(),
            sections: (0..cfg.section_count())
                .map(|_| ChunkSection::empty(cfg))
                .collect(),
            heightmaps: Heightmaps::default(),
            block_entities: Vec::new(),
            sky: vec![LightSlot::Absent; cfg.light_section_count()],
            block: vec![LightSlot::Absent; cfg.light_section_count()],
        }
    }

    pub fn from_data(data: ChunkData, cfg: &WorldConfig) -> Self {
        let mut column = Self::empty(cfg);
        column.sections = data.sections;
        column.heightmaps = data.heightmaps;
        column.block_entities = data.block_entities;
        column
    }

    pub fn apply_light(&mut self, light: LightData) {
        for (slots, updates) in [(&mut self.sky, light.sky), (&mut self.block, light.block)] {
            for update in updates {
                if let Some(slot) = slots.get_mut(update.section as usize) {
                    *slot = update.data.map_or(LightSlot::Dark, LightSlot::Data);
                }
            }
        }
    }

    /// Decodes one biome container per section (the chunks-biomes packet layout).
    pub fn replace_biomes(&mut self, input: &mut &[u8], cfg: &WorldConfig) -> crate::Result<()> {
        let mut decoded = Vec::with_capacity(self.sections.len());
        for _ in 0..self.sections.len() {
            decoded.push(crate::BiomeContainer::decode(input, cfg.biome_limits())?);
        }
        for (section, biomes) in self.sections.iter_mut().zip(decoded) {
            section.replace_biomes(biomes);
        }
        Ok(())
    }

    pub fn min_y(&self) -> i32 {
        self.min_y
    }

    pub fn max_y(&self) -> i32 {
        self.min_y + self.sections.len() as i32 * 16
    }

    pub fn sections(&self) -> &[ChunkSection] {
        &self.sections
    }

    pub fn section(&self, index: usize) -> Option<&ChunkSection> {
        self.sections.get(index)
    }

    pub fn heightmaps(&self) -> &Heightmaps {
        &self.heightmaps
    }

    pub fn block_entities(&self) -> &[BlockEntityData] {
        &self.block_entities
    }

    pub fn sky_light_section(&self, index: usize) -> Option<&LightSlot> {
        self.sky.get(index)
    }

    pub fn block_light_section(&self, index: usize) -> Option<&LightSlot> {
        self.block.get(index)
    }

    #[inline]
    fn section_index(&self, y: i32) -> Option<usize> {
        let rel = y.checked_sub(self.min_y)?;
        let index = usize::try_from(rel >> 4).ok()?;
        (index < self.sections.len()).then_some(index)
    }

    /// `x` and `z` are chunk-local; `y` is a world height.
    #[inline]
    pub fn block(&self, x: usize, y: i32, z: usize) -> Option<BlockState> {
        let section = &self.sections[self.section_index(y)?];
        Some(section.block(x, (y & 15) as usize, z))
    }

    pub fn set_block(
        &mut self,
        x: usize,
        y: i32,
        z: usize,
        state: BlockState,
        cfg: &WorldConfig,
    ) -> Option<BlockState> {
        let index = self.section_index(y)?;
        Some(self.sections[index].set_block(x, (y & 15) as usize, z, state, cfg))
    }

    pub fn sky_light(&self, x: usize, y: i32, z: usize) -> Option<u8> {
        let slot = self.sky.get(self.light_index(y)?)?;
        slot.get(x, (y & 15) as usize, z)
    }

    pub fn block_light(&self, x: usize, y: i32, z: usize) -> Option<u8> {
        let slot = self.block.get(self.light_index(y)?)?;
        slot.get(x, (y & 15) as usize, z)
    }

    /// Writes one nibble; `false` when the light section has not arrived.
    pub fn set_sky_light(&mut self, x: usize, y: i32, z: usize, level: u8) -> bool {
        let Some(index) = self.light_index(y) else {
            return false;
        };
        self.sky
            .get_mut(index)
            .is_some_and(|s| s.set((x, (y & 15) as usize, z), level))
    }

    pub fn set_block_light(&mut self, x: usize, y: i32, z: usize, level: u8) -> bool {
        let Some(index) = self.light_index(y) else {
            return false;
        };
        self.block
            .get_mut(index)
            .is_some_and(|s| s.set((x, (y & 15) as usize, z), level))
    }

    fn light_index(&self, y: i32) -> Option<usize> {
        let rel = y.checked_sub(self.min_y)?;
        usize::try_from((rel >> 4) + 1).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::LightUpdate;

    fn cfg() -> WorldConfig {
        WorldConfig::new(35723, 80, -64, 384).unwrap()
    }

    #[test]
    fn world_y_maps_to_sections_including_negatives() {
        let cfg = cfg();
        let mut c = ChunkColumn::empty(&cfg);
        assert_eq!(
            c.set_block(2, -64, 3, BlockState(5), &cfg),
            Some(BlockState(0))
        );
        assert_eq!(
            c.set_block(2, 319, 3, BlockState(6), &cfg),
            Some(BlockState(0))
        );
        assert_eq!(c.block(2, -64, 3), Some(BlockState(5)));
        assert_eq!(c.block(2, 319, 3), Some(BlockState(6)));
        assert_eq!(c.block(2, -65, 3), None);
        assert_eq!(c.block(2, 320, 3), None);
        assert_eq!(c.max_y(), 320);
        assert_eq!(c.sections()[0].block(2, 0, 3), BlockState(5));
        assert_eq!(c.sections()[23].block(2, 15, 3), BlockState(6));
    }

    #[test]
    fn light_covers_one_extra_section_each_side() {
        let cfg = cfg();
        let mut c = ChunkColumn::empty(&cfg);
        assert_eq!(c.sky_light(0, -70, 0), None);
        let mut nib = LightNibbles::default();
        nib.set(1, 15, 2, 12);
        c.apply_light(LightData {
            sky: vec![
                LightUpdate {
                    section: 0,
                    data: Some(nib),
                },
                LightUpdate {
                    section: 25,
                    data: None,
                },
            ],
            block: vec![],
        });
        assert_eq!(c.sky_light(1, -65, 2), Some(12));
        assert_eq!(c.sky_light(0, 330, 0), Some(0));
        assert_eq!(c.sky_light(0, 0, 0), None);
        assert_eq!(c.block_light(0, 0, 0), None);
        assert_eq!(c.sky_light(0, -81, 0), None);
    }

    #[test]
    fn heightmap_validates_length() {
        let cfg = cfg();
        assert_eq!(Heightmap::empty(&cfg).raw().len(), 37);
        assert!(Heightmap::from_raw(vec![0; 36].into(), &cfg).is_err());
        let mut hm = Heightmap::empty(&cfg);
        hm.data[0] = 5;
        assert_eq!(hm.first_available(0, 0, -64), -59);
        assert_eq!(hm.first_available(1, 0, -64), -64);
    }
}
