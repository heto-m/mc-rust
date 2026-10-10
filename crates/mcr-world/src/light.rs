//! Client-side light propagation for block changes (sky and block light), like vanilla's `LevelLightEngine`.

use std::collections::VecDeque;

use rustc_hash::FxHashMap;

use crate::{BlockState, ChunkColumn, ChunkPos, ChunkStore};

const DIRS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];
const ALL: u8 = 0b11_1111;
const DOWN: usize = 0;
#[cfg(test)]
const UP: usize = 1;

/// Per-state light properties: emission, dampening and the faces whose light-occlusion shape is full.
#[derive(Debug, Clone)]
pub struct LightTable {
    packed: Vec<u16>,
}

impl LightTable {
    /// `props(state)` returns `(emission, dampening, occluding_faces)`; faces are bits in
    /// `Direction` order (down, up, north, south, west, east) and only for states that use their
    /// shape for light occlusion.
    pub fn new(states: usize, mut props: impl FnMut(u32) -> (u8, u8, u8)) -> Self {
        let packed = (0..states as u32)
            .map(|s| {
                let (emission, dampening, faces) = props(s);
                u16::from(emission.min(15))
                    | u16::from(dampening.min(15)) << 4
                    | u16::from(faces & ALL) << 8
            })
            .collect();
        Self { packed }
    }

    fn entry(&self, state: BlockState) -> u16 {
        self.packed
            .get(usize::from(state.0))
            .copied()
            .unwrap_or(15 << 4)
    }

    fn emission(&self, state: BlockState) -> u8 {
        (self.entry(state) & 15) as u8
    }

    fn dampening(&self, state: BlockState) -> u8 {
        (self.entry(state) >> 4 & 15) as u8
    }

    fn faces(&self, state: BlockState) -> u8 {
        (self.entry(state) >> 8) as u8
    }

    fn occluded(&self, from: BlockState, to: BlockState, dir: usize) -> bool {
        self.faces(from) >> dir & 1 != 0 || self.faces(to) >> (dir ^ 1) & 1 != 0
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Block,
    Sky,
}

#[derive(Clone, Copy)]
struct Entry {
    pos: [i32; 3],
    level: u8,
    dirs: u8,
    emission: bool,
}

struct Engine<'a> {
    columns: &'a mut FxHashMap<ChunkPos, ChunkColumn>,
    table: &'a LightTable,
    layer: Layer,
    decrease: VecDeque<Entry>,
    increase: VecDeque<Entry>,
    changed: Vec<[i32; 3]>,
}

fn offset(p: [i32; 3], d: usize) -> [i32; 3] {
    [p[0] + DIRS[d][0], p[1] + DIRS[d][1], p[2] + DIRS[d][2]]
}

impl Engine<'_> {
    fn column(&self, x: i32, z: i32) -> Option<&ChunkColumn> {
        self.columns.get(&ChunkPos::from_block(x, z))
    }

    fn state(&self, p: [i32; 3]) -> Option<BlockState> {
        self.column(p[0], p[2])?
            .block((p[0] & 15) as usize, p[1], (p[2] & 15) as usize)
    }

    fn level(&self, p: [i32; 3]) -> Option<u8> {
        let column = self.column(p[0], p[2])?;
        let (x, z) = ((p[0] & 15) as usize, (p[2] & 15) as usize);
        match self.layer {
            Layer::Block => column.block_light(x, p[1], z),
            Layer::Sky => column.sky_light(x, p[1], z),
        }
    }

    fn set_level(&mut self, p: [i32; 3], level: u8) {
        let Some(column) = self.columns.get_mut(&ChunkPos::from_block(p[0], p[2])) else {
            return;
        };
        let (x, z) = ((p[0] & 15) as usize, (p[2] & 15) as usize);
        let stored = match self.layer {
            Layer::Block => column.set_block_light(x, p[1], z, level),
            Layer::Sky => column.set_sky_light(x, p[1], z, level),
        };
        if stored {
            self.changed.push(p);
        }
    }

    /// Lowest `y` from which every cell up to the sky is a full-strength sky source.
    fn lowest_source(&self, x: i32, z: i32) -> i32 {
        let Some(column) = self.column(x, z) else {
            return i32::MAX;
        };
        let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
        let mut top = BlockState(0);
        for (index, section) in column.sections().iter().enumerate().rev() {
            if section.is_empty() {
                top = BlockState(0);
                continue;
            }
            let base = column.min_y() + index as i32 * 16;
            for ly in (0..16).rev() {
                let bottom = section.block(lx, ly, lz);
                if self.table.dampening(bottom) != 0 || self.table.occluded(top, bottom, DOWN) {
                    return base + ly as i32 + 1;
                }
                top = bottom;
            }
        }
        i32::MIN
    }

    fn update_sky_column(&mut self, x: i32, z: i32, lowest: i32) {
        let Some(column) = self.column(x, z) else {
            return;
        };
        let floor = column.min_y() - 16;
        let ceiling = column.max_y() + 15;
        if lowest != i32::MIN {
            let mut y = (lowest - 1).min(ceiling);
            while y >= floor {
                match self.level([x, y, z]) {
                    Some(15) => {
                        self.set_level([x, y, z], 0);
                        self.decrease.push_back(Entry {
                            pos: [x, y, z],
                            level: 15,
                            dirs: ALL,
                            emission: false,
                        });
                    }
                    Some(_) => break,
                    None => {}
                }
                y -= 1;
            }
        }
        for y in lowest.max(floor)..=ceiling {
            match self.level([x, y, z]) {
                Some(15) => break,
                Some(_) => {
                    self.set_level([x, y, z], 15);
                    self.increase.push_back(Entry {
                        pos: [x, y, z],
                        level: 15,
                        dirs: ALL,
                        emission: false,
                    });
                }
                None => {}
            }
        }
    }

    fn check(&mut self, p: [i32; 3], lowest: i32) {
        let (Some(old), Some(state)) = (self.level(p), self.state(p)) else {
            return;
        };
        let emission = match self.layer {
            Layer::Block => self.table.emission(state),
            Layer::Sky if p[1] >= lowest => return,
            Layer::Sky => 0,
        };
        if emission < old {
            self.set_level(p, 0);
            self.decrease.push_back(Entry {
                pos: p,
                level: old,
                dirs: ALL,
                emission: false,
            });
        } else {
            self.decrease.push_back(Entry {
                pos: p,
                level: 1,
                dirs: ALL,
                emission: false,
            });
        }
        if emission > 0 {
            self.increase.push_back(Entry {
                pos: p,
                level: emission,
                dirs: ALL,
                emission: true,
            });
        }
    }

    fn propagate_increase(&mut self, e: Entry) {
        let Some(stored) = self.level(e.pos) else {
            return;
        };
        if e.emission && stored < e.level {
            self.set_level(e.pos, e.level);
        } else if stored != e.level {
            return;
        }
        let from_level = i32::from(e.level);
        let mut from_state = None;
        for d in 0..6 {
            if e.dirs >> d & 1 == 0 {
                continue;
            }
            let to = offset(e.pos, d);
            let Some(to_level) = self.level(to).map(i32::from) else {
                continue;
            };
            if from_level - 1 <= to_level {
                continue;
            }
            let Some(to_state) = self.state(to) else {
                continue;
            };
            let new = from_level - i32::from(self.table.dampening(to_state).max(1));
            if new <= to_level {
                continue;
            }
            let from_state = *from_state.get_or_insert_with(|| self.state(e.pos));
            if from_state.is_none_or(|f| self.table.occluded(f, to_state, d)) {
                continue;
            }
            self.set_level(to, new as u8);
            if new > 1 {
                self.increase.push_back(Entry {
                    pos: to,
                    level: new as u8,
                    dirs: ALL & !(1 << (d ^ 1)),
                    emission: false,
                });
            }
        }
    }

    fn propagate_decrease(&mut self, e: Entry) {
        for d in 0..6 {
            if e.dirs >> d & 1 == 0 {
                continue;
            }
            let to = offset(e.pos, d);
            let Some(to_level) = self.level(to).filter(|&l| l != 0) else {
                continue;
            };
            if i32::from(to_level) < i32::from(e.level) {
                let emission = match self.layer {
                    Layer::Block => self.state(to).map_or(0, |s| self.table.emission(s)),
                    Layer::Sky => 0,
                };
                self.set_level(to, 0);
                if emission < to_level {
                    self.decrease.push_back(Entry {
                        pos: to,
                        level: to_level,
                        dirs: ALL & !(1 << (d ^ 1)),
                        emission: false,
                    });
                }
                if emission > 0 {
                    self.increase.push_back(Entry {
                        pos: to,
                        level: emission,
                        dirs: ALL,
                        emission: true,
                    });
                }
            } else {
                self.increase.push_back(Entry {
                    pos: to,
                    level: to_level,
                    dirs: 1 << (d ^ 1),
                    emission: false,
                });
            }
        }
    }

    fn run(&mut self) {
        while let Some(e) = self.decrease.pop_front() {
            self.propagate_decrease(e);
        }
        while let Some(e) = self.increase.pop_front() {
            self.propagate_increase(e);
        }
    }
}

impl ChunkStore {
    /// Relights around every block changed since the last run and remeshes sections whose light changed.
    pub fn run_light_updates(&mut self) {
        if self.pending_light.is_empty() {
            return;
        }
        let Some(table) = self.light_table.clone() else {
            self.pending_light.clear();
            return;
        };
        let cells: Vec<[i32; 3]> = self.pending_light.drain().collect();
        let mut changed = Vec::new();
        for layer in [Layer::Block, Layer::Sky] {
            let mut engine = Engine {
                columns: &mut self.columns,
                table: &table,
                layer,
                decrease: VecDeque::new(),
                increase: VecDeque::new(),
                changed: Vec::new(),
            };
            let mut lowest: FxHashMap<(i32, i32), i32> = FxHashMap::default();
            if layer == Layer::Sky {
                for c in &cells {
                    lowest.entry((c[0], c[2])).or_insert_with(|| {
                        let y = engine.lowest_source(c[0], c[2]);
                        engine.update_sky_column(c[0], c[2], y);
                        y
                    });
                }
            }
            for c in &cells {
                let source = lowest.get(&(c[0], c[2])).copied().unwrap_or(i32::MAX);
                engine.check(*c, source);
            }
            engine.run();
            changed.append(&mut engine.changed);
        }
        for c in changed {
            self.mark_block_dirty(c[0], c[1], c[2]);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{LightData, LightNibbles, LightUpdate, WorldConfig};

    const AIR: BlockState = BlockState(0);
    const STONE: BlockState = BlockState(1);
    const TORCH: BlockState = BlockState(2);
    const LEAVES: BlockState = BlockState(3);
    const SLAB: BlockState = BlockState(4);

    fn cfg() -> WorldConfig {
        WorldConfig::new(16, 80, 0, 64).unwrap()
    }

    fn table() -> LightTable {
        LightTable::new(16, |s| match s {
            1 => (0, 15, 0),
            2 => (14, 0, 0),
            3 => (0, 1, 0),
            4 => (0, 0, 1 << UP),
            _ => (0, 0, 0),
        })
    }

    fn full_sky() -> LightData {
        let mut nib = LightNibbles::default();
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    nib.set(x, y, z, 15);
                }
            }
        }
        LightData {
            sky: (0..6)
                .map(|section| LightUpdate {
                    section,
                    data: Some(nib.clone()),
                })
                .collect(),
            block: (0..6)
                .map(|section| LightUpdate {
                    section,
                    data: None,
                })
                .collect(),
        }
    }

    fn open_world(chunks: &[(i32, i32)]) -> ChunkStore {
        let mut store = ChunkStore::new(cfg());
        for &(x, z) in chunks {
            let mut column = ChunkColumn::empty(&cfg());
            column.apply_light(full_sky());
            store.insert(ChunkPos::new(x, z), column);
        }
        store.set_light_table(Arc::new(table()));
        store.drain_dirty().for_each(drop);
        store
    }

    fn sky(s: &ChunkStore, x: i32, y: i32, z: i32) -> u8 {
        s.column(ChunkPos::from_block(x, z))
            .unwrap()
            .sky_light((x & 15) as usize, y, (z & 15) as usize)
            .unwrap()
    }

    fn block(s: &ChunkStore, x: i32, y: i32, z: i32) -> u8 {
        s.column(ChunkPos::from_block(x, z))
            .unwrap()
            .block_light((x & 15) as usize, y, (z & 15) as usize)
            .unwrap()
    }

    #[test]
    fn torch_lights_and_unlights_with_distance_falloff() {
        let mut s = open_world(&[(0, 0)]);
        s.set_block(8, 20, 8, TORCH);
        assert_eq!(block(&s, 8, 20, 8), 14);
        assert_eq!(block(&s, 8, 20, 9), 13);
        assert_eq!(block(&s, 10, 21, 8), 11);
        assert_eq!(block(&s, 8, 20, 15), 7);
        s.set_block(8, 20, 8, AIR);
        for (x, y, z) in [(8, 20, 8), (8, 20, 9), (10, 21, 8)] {
            assert_eq!(block(&s, x, y, z), 0);
        }
    }

    #[test]
    fn torch_light_is_blocked_by_stone_and_dirties_neighbour_chunks() {
        let mut s = open_world(&[(0, 0), (1, 0)]);
        s.set_block(10, 20, 8, STONE);
        s.set_block(9, 20, 8, TORCH);
        assert_eq!(block(&s, 11, 20, 8), 10);
        s.drain_dirty().for_each(drop);
        s.set_block(13, 20, 8, TORCH);
        assert_eq!(block(&s, 17, 20, 8), 10);
        let dirty: Vec<_> = s.drain_dirty().collect();
        assert!(dirty.contains(&crate::SectionPos::new(1, 1, 0)));
    }

    #[test]
    fn roof_darkens_and_hole_relights_with_horizontal_spread() {
        let mut s = open_world(&[(0, 0)]);
        for x in 4..13 {
            for z in 4..13 {
                s.set_block_deferred(x, 30, z, STONE);
            }
        }
        s.run_light_updates();
        assert_eq!(sky(&s, 8, 29, 8), 10);
        assert_eq!(sky(&s, 8, 20, 8), 10);
        assert_eq!(sky(&s, 3, 29, 8), 15);
        assert_eq!(sky(&s, 4, 29, 8), 14);
        s.set_block(8, 30, 8, AIR);
        assert_eq!(sky(&s, 8, 30, 8), 15);
        assert_eq!(sky(&s, 8, 5, 8), 15);
        assert_eq!(sky(&s, 9, 25, 8), 14);
        assert_eq!(sky(&s, 11, 25, 8), 13);
        assert_eq!(sky(&s, 6, 25, 6), 12);
        s.set_block(8, 30, 8, STONE);
        assert_eq!(sky(&s, 8, 5, 8), 10);
        assert_eq!(sky(&s, 9, 25, 8), 11);
    }

    #[test]
    fn breaking_the_surface_block_lights_the_hole() {
        let mut s = open_world(&[(0, 0)]);
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..=30 {
                    s.set_block_deferred(x, y, z, STONE);
                }
            }
        }
        s.run_light_updates();
        assert_eq!(sky(&s, 8, 30, 8), 0);
        assert_eq!(sky(&s, 8, 31, 8), 15);
        s.set_block(8, 30, 8, AIR);
        assert_eq!(sky(&s, 8, 30, 8), 15);
        s.set_block(8, 29, 8, AIR);
        assert_eq!(sky(&s, 8, 29, 8), 15);
        s.set_block(9, 29, 8, AIR);
        assert_eq!(sky(&s, 9, 29, 8), 14);
    }

    #[test]
    fn leaves_dampen_light_by_one_per_block() {
        let mut s = open_world(&[(0, 0)]);
        s.set_block(8, 30, 8, LEAVES);
        s.set_block(8, 29, 8, LEAVES);
        assert_eq!(sky(&s, 8, 30, 8), 14);
        assert_eq!(sky(&s, 8, 29, 8), 14);
        assert_eq!(sky(&s, 8, 28, 8), 14);
    }

    #[test]
    fn absent_light_sections_are_left_alone() {
        let mut s = ChunkStore::new(cfg());
        s.insert(ChunkPos::new(0, 0), ChunkColumn::empty(&cfg()));
        s.set_light_table(Arc::new(table()));
        s.set_block(8, 20, 8, TORCH);
        let c = s.column(ChunkPos::new(0, 0)).unwrap();
        assert_eq!(c.block_light(8, 20, 8), None);
    }

    #[test]
    fn upward_slab_face_stops_downward_skylight() {
        let mut s = open_world(&[(0, 0)]);
        s.set_block(8, 30, 8, SLAB);
        assert_eq!(sky(&s, 8, 30, 8), 14);
        assert_eq!(sky(&s, 8, 29, 8), 14);
    }

    fn reference(s: &ChunkStore, layer: Layer, table: &LightTable) -> FxHashMap<[i32; 3], u8> {
        let mut columns = s.columns.clone();
        let engine = Engine {
            columns: &mut columns,
            table,
            layer,
            decrease: VecDeque::new(),
            increase: VecDeque::new(),
            changed: Vec::new(),
        };
        let mut levels = FxHashMap::default();
        for x in 0..32 {
            for z in 0..16 {
                let lowest = engine.lowest_source(x, z);
                for y in -16..80 {
                    let p = [x, y, z];
                    if engine.level(p).is_none() {
                        continue;
                    }
                    let state = engine.state(p).unwrap_or(AIR);
                    let source = match layer {
                        Layer::Block => table.emission(state),
                        Layer::Sky if y >= lowest => 15,
                        Layer::Sky => 0,
                    };
                    levels.insert(p, source);
                }
            }
        }
        loop {
            let mut changed = false;
            let keys: Vec<_> = levels.keys().copied().collect();
            for p in keys {
                let Some(state) = engine.state(p) else {
                    continue;
                };
                let mut best = levels[&p];
                for d in 0..6 {
                    let n = offset(p, d);
                    let (Some(&nl), Some(ns)) = (levels.get(&n), engine.state(n)) else {
                        continue;
                    };
                    if table.occluded(ns, state, d ^ 1) {
                        continue;
                    }
                    let cand = i32::from(nl) - i32::from(table.dampening(state).max(1));
                    best = best.max(cand.max(0) as u8);
                }
                if best != levels[&p] {
                    levels.insert(p, best);
                    changed = true;
                }
            }
            if !changed {
                return levels;
            }
        }
    }

    #[test]
    fn incremental_updates_match_a_full_relight() {
        let table = table();
        let mut s = open_world(&[(0, 0), (1, 0)]);
        let mut seed = 0x2545_f491_u64;
        let mut next = move |n: u64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) % n
        };
        for round in 0..40 {
            let batch = 1 + next(6);
            for _ in 0..batch {
                let (x, z) = (next(32) as i32, next(16) as i32);
                let y = 8 + next(40) as i32;
                let state = [AIR, AIR, STONE, STONE, TORCH, LEAVES, SLAB][next(7) as usize];
                s.set_block_deferred(x, y, z, state);
            }
            s.run_light_updates();
            for layer in [Layer::Block, Layer::Sky] {
                let expected = reference(&s, layer, &table);
                let engine = Engine {
                    columns: &mut s.columns,
                    table: &table,
                    layer,
                    decrease: VecDeque::new(),
                    increase: VecDeque::new(),
                    changed: Vec::new(),
                };
                for (p, want) in expected {
                    let got = engine.level(p).unwrap();
                    assert_eq!(
                        got,
                        want,
                        "round {round} layer sky={} at {p:?}",
                        layer == Layer::Sky
                    );
                }
            }
        }
    }
}
