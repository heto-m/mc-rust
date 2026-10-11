use std::collections::hash_set::Drain;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{BlockState, ChunkColumn, WorldConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub const fn from_block(x: i32, z: i32) -> Self {
        Self::new(x >> 4, z >> 4)
    }
}

/// `y` is the section coordinate (block y divided by 16, rounded down).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SectionPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl SectionPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub const fn chunk(&self) -> ChunkPos {
        ChunkPos::new(self.x, self.z)
    }
}

#[derive(Debug)]
pub struct ChunkStore {
    cfg: WorldConfig,
    pub(crate) columns: FxHashMap<ChunkPos, ChunkColumn>,
    dirty: FxHashSet<SectionPos>,
    pub(crate) light_table: Option<std::sync::Arc<crate::LightTable>>,
    pub(crate) pending_light: FxHashSet<[i32; 3]>,
}

impl ChunkStore {
    pub fn new(cfg: WorldConfig) -> Self {
        Self {
            cfg,
            columns: FxHashMap::default(),
            dirty: FxHashSet::default(),
            light_table: None,
            pending_light: FxHashSet::default(),
        }
    }

    /// Enables the local light engine: block changes then relight like vanilla's client does.
    pub fn set_light_table(&mut self, table: std::sync::Arc<crate::LightTable>) {
        self.light_table = Some(table);
    }

    pub fn config(&self) -> &WorldConfig {
        &self.cfg
    }

    pub fn len(&self) -> usize {
        self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    pub fn positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.columns.keys().copied()
    }

    pub fn column(&self, pos: ChunkPos) -> Option<&ChunkColumn> {
        self.columns.get(&pos)
    }

    pub fn column_mut(&mut self, pos: ChunkPos) -> Option<&mut ChunkColumn> {
        self.columns.get_mut(&pos)
    }

    /// Replaces any existing column; its sections and those of the loaded 3x3 around it become dirty.
    pub fn insert(&mut self, pos: ChunkPos, column: ChunkColumn) {
        let sections = column.sections().len() as i32;
        let base = column.min_y() >> 4;
        self.columns.insert(pos, column);
        self.mark_column(pos, base, sections, &RING);
    }

    /// The loaded 3x3 around the removed column becomes dirty.
    pub fn remove(&mut self, pos: ChunkPos) -> Option<ChunkColumn> {
        let column = self.columns.remove(&pos)?;
        let base = column.min_y() >> 4;
        let sections = column.sections().len() as i32;
        self.dirty.retain(|s| s.chunk() != pos);
        self.mark_neighbours(pos, base, sections, &RING);
        Some(column)
    }

    /// Whether `pos` and its eight neighbours are loaded, as vanilla requires before meshing.
    pub fn neighbourhood_loaded(&self, pos: ChunkPos) -> bool {
        self.columns.contains_key(&pos)
            && RING.iter().all(|(dx, dz)| {
                self.columns
                    .contains_key(&ChunkPos::new(pos.x + dx, pos.z + dz))
            })
    }

    pub fn apply_light(&mut self, pos: ChunkPos, light: crate::LightData) -> bool {
        let Some(column) = self.columns.get_mut(&pos) else {
            return false;
        };
        column.apply_light(light);
        let base = column.min_y() >> 4;
        let sections = column.sections().len() as i32;
        self.mark_column(pos, base, sections, &NEIGHBOURS);
        true
    }

    /// Returns `Ok(false)` when the chunk is not loaded.
    pub fn apply_biomes(&mut self, pos: ChunkPos, buffer: &[u8]) -> crate::Result<bool> {
        let Some(column) = self.columns.get_mut(&pos) else {
            return Ok(false);
        };
        let mut input = buffer;
        column.replace_biomes(&mut input, &self.cfg)?;
        let base = column.min_y() >> 4;
        let sections = column.sections().len() as i32;
        self.mark_column(pos, base, sections, &NEIGHBOURS);
        Ok(true)
    }

    fn mark_column(&mut self, pos: ChunkPos, base: i32, sections: i32, around: &[(i32, i32)]) {
        for y in base..base + sections {
            self.dirty.insert(SectionPos::new(pos.x, y, pos.z));
        }
        self.mark_neighbours(pos, base, sections, around);
    }

    fn mark_neighbours(&mut self, pos: ChunkPos, base: i32, sections: i32, around: &[(i32, i32)]) {
        for &(dx, dz) in around {
            let n = ChunkPos::new(pos.x + dx, pos.z + dz);
            if self.columns.contains_key(&n) {
                for y in base..base + sections {
                    self.dirty.insert(SectionPos::new(n.x, y, n.z));
                }
            }
        }
    }

    #[inline]
    pub fn block(&self, x: i32, y: i32, z: i32) -> Option<BlockState> {
        self.columns.get(&ChunkPos::from_block(x, z))?.block(
            (x & 15) as usize,
            y,
            (z & 15) as usize,
        )
    }

    /// Returns the previous state, or `None` if the position is not loaded.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, state: BlockState) -> Option<BlockState> {
        let old = self.set_block_deferred(x, y, z, state);
        self.run_light_updates();
        old
    }

    /// Like [`set_block`](Self::set_block) but leaves the light update to [`run_light_updates`](Self::run_light_updates).
    pub fn set_block_deferred(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        state: BlockState,
    ) -> Option<BlockState> {
        let pos = ChunkPos::from_block(x, z);
        let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
        let old = self
            .columns
            .get_mut(&pos)?
            .set_block(lx, y, lz, state, &self.cfg)?;
        if old != state {
            self.mark_block_dirty(x, y, z);
            if self.light_table.is_some() {
                self.pending_light.insert([x, y, z]);
            }
        }
        Some(old)
    }

    /// Applies section-blocks-update entries: `state << 12 | x << 8 | z << 4 | y` (section-local).
    pub fn apply_section_updates(
        &mut self,
        section: SectionPos,
        entries: impl IntoIterator<Item = u64>,
    ) -> usize {
        let mut applied = 0;
        for entry in entries {
            let state = BlockState((entry >> 12) as u16);
            let (lx, lz, ly) = (
                ((entry >> 8) & 15) as i32,
                ((entry >> 4) & 15) as i32,
                (entry & 15) as i32,
            );
            let (x, y, z) = (
                section.x * 16 + lx,
                section.y * 16 + ly,
                section.z * 16 + lz,
            );
            if self.set_block_deferred(x, y, z, state).is_some() {
                applied += 1;
            }
        }
        self.run_light_updates();
        applied
    }

    pub(crate) fn mark_block_dirty(&mut self, x: i32, y: i32, z: i32) {
        let (sx, sy, sz) = (x >> 4, y >> 4, z >> 4);
        self.dirty.insert(SectionPos::new(sx, sy, sz));
        let edge = |v: i32| match v & 15 {
            0 => -1,
            15 => 1,
            _ => 0,
        };
        let (ex, ey, ez) = (edge(x), edge(y), edge(z));
        for mask in 1..8 {
            let (dx, dy, dz) = (
                if mask & 1 != 0 { ex } else { 0 },
                if mask & 2 != 0 { ey } else { 0 },
                if mask & 4 != 0 { ez } else { 0 },
            );
            if (dx, dy, dz) == (0, 0, 0) {
                continue;
            }
            let n = SectionPos::new(sx + dx, sy + dy, sz + dz);
            if self.has_section(n) {
                self.dirty.insert(n);
            }
        }
    }

    fn has_section(&self, pos: SectionPos) -> bool {
        self.columns.get(&pos.chunk()).is_some_and(|c| {
            let rel = pos.y - (c.min_y() >> 4);
            rel >= 0 && (rel as usize) < c.sections().len()
        })
    }

    pub fn dirty_count(&self) -> usize {
        self.dirty.len()
    }

    /// Yields and clears every section needing a remesh.
    pub fn drain_dirty(&mut self) -> Drain<'_, SectionPos> {
        self.dirty.drain()
    }
}

const NEIGHBOURS: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
const RING: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> WorldConfig {
        WorldConfig::new(35723, 80, -64, 384).unwrap()
    }

    fn store_with(positions: &[(i32, i32)]) -> ChunkStore {
        let mut s = ChunkStore::new(cfg());
        for &(x, z) in positions {
            s.insert(ChunkPos::new(x, z), ChunkColumn::empty(&cfg()));
        }
        s.drain_dirty().for_each(drop);
        s
    }

    #[test]
    fn world_lookup_handles_negative_coordinates() {
        let mut s = store_with(&[(-1, -1), (0, 0)]);
        assert_eq!(s.set_block(-1, 10, -16, BlockState(4)), Some(BlockState(0)));
        assert_eq!(s.block(-1, 10, -16), Some(BlockState(4)));
        assert_eq!(s.block(15, 10, 15), Some(BlockState(0)));
        assert_eq!(s.block(16, 10, 0), None);
        assert_eq!(s.block(0, 400, 0), None);
        assert!(s.column(ChunkPos::new(-1, -1)).is_some());
    }

    #[test]
    fn interior_edit_dirties_one_section() {
        let mut s = store_with(&[(0, 0)]);
        s.set_block(5, 5, 5, BlockState(1));
        s.set_block(6, 5, 5, BlockState(1));
        let dirty: Vec<_> = s.drain_dirty().collect();
        assert_eq!(dirty, [SectionPos::new(0, 0, 0)]);
        assert_eq!(s.dirty_count(), 0);
    }

    #[test]
    fn border_edit_dirties_loaded_neighbour_only() {
        let mut s = store_with(&[(0, 0), (1, 0)]);
        s.set_block(15, 5, 5, BlockState(1));
        let mut dirty: Vec<_> = s.drain_dirty().collect();
        dirty.sort_by_key(|p| (p.x, p.y, p.z));
        assert_eq!(dirty, [SectionPos::new(0, 0, 0), SectionPos::new(1, 0, 0)]);
        s.set_block(0, 5, 5, BlockState(1));
        assert_eq!(s.drain_dirty().count(), 1);
    }

    #[test]
    fn unchanged_block_is_not_dirty() {
        let mut s = store_with(&[(0, 0)]);
        s.set_block(1, 1, 1, BlockState(0));
        assert_eq!(s.dirty_count(), 0);
    }

    #[test]
    fn vertical_border_stops_at_dimension_edge() {
        let mut s = store_with(&[(0, 0)]);
        s.set_block(1, -64, 1, BlockState(1));
        assert_eq!(s.drain_dirty().count(), 1);
        s.set_block(1, -49, 1, BlockState(1));
        assert_eq!(s.drain_dirty().count(), 2);
    }

    #[test]
    fn insert_dirties_column_and_neighbours() {
        let mut s = store_with(&[(0, 0)]);
        s.insert(ChunkPos::new(1, 0), ChunkColumn::empty(&cfg()));
        assert_eq!(s.dirty_count(), 48);
        s.drain_dirty().for_each(drop);
        s.remove(ChunkPos::new(1, 0));
        assert_eq!(s.dirty_count(), 24);
    }

    #[test]
    fn diagonal_neighbours_are_dirtied_on_load_and_unload() {
        let mut s = store_with(&[(0, 0), (5, 5)]);
        s.insert(ChunkPos::new(1, 1), ChunkColumn::empty(&cfg()));
        let chunks: FxHashSet<ChunkPos> = s.drain_dirty().map(|p| p.chunk()).collect();
        assert_eq!(
            chunks,
            [ChunkPos::new(0, 0), ChunkPos::new(1, 1)]
                .into_iter()
                .collect()
        );
        s.remove(ChunkPos::new(1, 1));
        let chunks: FxHashSet<ChunkPos> = s.drain_dirty().map(|p| p.chunk()).collect();
        assert_eq!(chunks, [ChunkPos::new(0, 0)].into_iter().collect());
    }

    #[test]
    fn neighbourhood_needs_all_eight_columns() {
        let mut ring: Vec<(i32, i32)> = (-1..=1)
            .flat_map(|z| (-1..=1).map(move |x| (x, z)))
            .collect();
        let s = store_with(&ring);
        assert!(s.neighbourhood_loaded(ChunkPos::new(0, 0)));
        assert!(!s.neighbourhood_loaded(ChunkPos::new(1, 0)));
        ring.retain(|&p| p != (1, 1));
        let s = store_with(&ring);
        assert!(!s.neighbourhood_loaded(ChunkPos::new(0, 0)));
        assert!(!store_with(&[]).neighbourhood_loaded(ChunkPos::new(0, 0)));
    }

    #[test]
    fn section_updates_use_packed_local_coordinates() {
        let mut s = store_with(&[(0, 0)]);
        let entry = (42u64 << 12) | (3 << 8) | (4 << 4) | 5;
        let n = s.apply_section_updates(SectionPos::new(0, 2, 0), [entry, entry | (1 << 40)]);
        assert_eq!(n, 2);
        assert_eq!(s.block(3, 37, 4), Some(BlockState(42)));
        let none = s.apply_section_updates(SectionPos::new(5, 2, 0), [entry]);
        assert_eq!(none, 0);
    }
}
