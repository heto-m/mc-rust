use std::collections::VecDeque;
use std::time::{Duration, Instant};

use mcr_render::Camera;
use mcr_world::{ChunkPos, ChunkStore};
use rustc_hash::FxHashMap;

use crate::mesh_world::MeshWorld;
use crate::source::{SourceEvent, WorldSource};

pub const UNLOAD_MARGIN: i32 = 2;
const INSERT_BUDGET: Duration = Duration::from_millis(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChunkState {
    Requested,
    Loaded,
    Missing,
}

pub fn chunk_of(camera: &Camera) -> ChunkPos {
    ChunkPos::from_block(
        camera.position.x.floor() as i32,
        camera.position.z.floor() as i32,
    )
}

fn dist_sq(a: ChunkPos, b: ChunkPos) -> i64 {
    let dx = i64::from(a.x) - i64::from(b.x);
    let dz = i64::from(a.z) - i64::from(b.z);
    dx * dx + dz * dz
}

pub fn in_distance(a: ChunkPos, b: ChunkPos, distance: i32) -> bool {
    let d = i64::from(distance);
    dist_sq(a, b) <= d * d + d
}

/// Within `distance` or bordering it: like vanilla's server, one extra ring so edge columns can mesh.
fn near_distance(a: ChunkPos, b: ChunkPos, distance: i32) -> bool {
    let toward = |v: i64| v - v.signum();
    let dx = toward(i64::from(a.x) - i64::from(b.x));
    let dz = toward(i64::from(a.z) - i64::from(b.z));
    let d = i64::from(distance);
    dx * dx + dz * dz <= d * d + d
}

/// World state independent of rendering: loaded columns, the viewpoint and the source feeding them.
pub struct Game {
    pub store: ChunkStore,
    pub camera: Camera,
    source: Box<dyn WorldSource>,
    render_distance: i32,
    chunks: FxHashMap<ChunkPos, ChunkState>,
    anchor: Option<ChunkPos>,
    arrived: VecDeque<SourceEvent>,
    scratch: Vec<SourceEvent>,
    unloaded: Vec<ChunkPos>,
}

impl std::fmt::Debug for Game {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Game")
            .field("camera", &self.camera)
            .field("render_distance", &self.render_distance)
            .field("columns", &self.store.len())
            .finish()
    }
}

impl MeshWorld for Game {
    fn store(&self) -> &ChunkStore {
        &self.store
    }

    fn store_mut(&mut self) -> &mut ChunkStore {
        &mut self.store
    }

    fn camera(&self) -> &Camera {
        &self.camera
    }

    fn render_distance(&self) -> i32 {
        self.render_distance
    }

    fn loaded_columns(&self) -> Vec<ChunkPos> {
        self.chunks
            .iter()
            .filter(|(_, s)| **s == ChunkState::Loaded)
            .map(|(p, _)| *p)
            .collect()
    }

    fn take_unloaded(&mut self) -> Vec<ChunkPos> {
        std::mem::take(&mut self.unloaded)
    }

    fn biome_names(&self) -> Vec<String> {
        self.source.biome_names()
    }

    fn streaming_idle(&self) -> bool {
        self.arrived.is_empty() && self.source.in_flight() == 0
    }
}

impl Game {
    pub fn new(source: Box<dyn WorldSource>, camera: Camera, render_distance: i32) -> Self {
        Self {
            store: ChunkStore::new(source.config()),
            camera,
            source,
            render_distance,
            chunks: FxHashMap::default(),
            anchor: None,
            arrived: VecDeque::new(),
            scratch: Vec::new(),
            unloaded: Vec::new(),
        }
    }

    pub fn update(&mut self) {
        let here = chunk_of(&self.camera);
        if self.anchor != Some(here) {
            self.anchor = Some(here);
            self.unload_far(here);
            self.request_near(here);
        }
        self.scratch.clear();
        self.source.poll(&mut self.scratch);
        self.arrived.extend(self.scratch.drain(..));
        let start = Instant::now();
        while let Some(event) = self.arrived.pop_front() {
            self.apply(event);
            if start.elapsed() >= INSERT_BUDGET {
                break;
            }
        }
    }

    fn apply(&mut self, event: SourceEvent) {
        match event {
            SourceEvent::Loaded(pos, column) => {
                if self.chunks.get(&pos) == Some(&ChunkState::Requested) {
                    self.store.insert(pos, *column);
                    self.chunks.insert(pos, ChunkState::Loaded);
                }
            }
            SourceEvent::Missing(pos) => {
                if let Some(state) = self.chunks.get_mut(&pos) {
                    *state = ChunkState::Missing;
                }
            }
        }
    }

    fn request_near(&mut self, here: ChunkPos) {
        let r = self.render_distance;
        for dz in -r - 1..=r + 1 {
            for dx in -r - 1..=r + 1 {
                let pos = ChunkPos::new(here.x + dx, here.z + dz);
                if near_distance(pos, here, r) && !self.chunks.contains_key(&pos) {
                    self.chunks.insert(pos, ChunkState::Requested);
                    self.source
                        .request(pos, u32::try_from(dist_sq(pos, here)).unwrap_or(u32::MAX));
                }
            }
        }
    }

    fn unload_far(&mut self, here: ChunkPos) {
        let limit = self.render_distance + UNLOAD_MARGIN;
        let far: Vec<(ChunkPos, ChunkState)> = self
            .chunks
            .iter()
            .filter(|(p, _)| !in_distance(**p, here, limit))
            .map(|(p, s)| (*p, *s))
            .collect();
        for (pos, state) in far {
            self.chunks.remove(&pos);
            match state {
                ChunkState::Loaded => {
                    self.store.remove(pos);
                    self.unloaded.push(pos);
                }
                ChunkState::Requested => self.source.cancel(pos),
                ChunkState::Missing => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mcr_world::{ChunkColumn, WorldConfig};

    use super::*;

    #[test]
    fn distances_between_extreme_chunks_do_not_overflow() {
        let far = ChunkPos::new(i32::MAX, i32::MIN);
        let near = ChunkPos::new(0, 0);
        assert!(!in_distance(far, near, 32));
        assert!(!near_distance(far, near, 32));
        assert!(in_distance(far, far, 0));
        assert!(near_distance(ChunkPos::new(33, 0), near, 32));
    }

    #[derive(Default)]
    struct Fake {
        requested: Vec<ChunkPos>,
        cancelled: Vec<ChunkPos>,
        ready: Vec<SourceEvent>,
    }

    struct FakeSource(std::rc::Rc<std::cell::RefCell<Fake>>);

    fn cfg() -> WorldConfig {
        WorldConfig::new(35723, 80, -64, 384).unwrap()
    }

    impl WorldSource for FakeSource {
        fn config(&self) -> WorldConfig {
            cfg()
        }
        fn biome_names(&self) -> Vec<String> {
            Vec::new()
        }
        fn request(&mut self, pos: ChunkPos, _: u32) {
            self.0.borrow_mut().requested.push(pos);
        }
        fn cancel(&mut self, pos: ChunkPos) {
            self.0.borrow_mut().cancelled.push(pos);
        }
        fn poll(&mut self, out: &mut Vec<SourceEvent>) {
            out.append(&mut self.0.borrow_mut().ready);
        }
        fn in_flight(&self) -> usize {
            0
        }
    }

    fn game(distance: i32) -> (Game, std::rc::Rc<std::cell::RefCell<Fake>>) {
        let fake = std::rc::Rc::new(std::cell::RefCell::new(Fake::default()));
        let g = Game::new(
            Box::new(FakeSource(fake.clone())),
            Camera::default(),
            distance,
        );
        (g, fake)
    }

    #[test]
    fn requests_a_disc_around_the_camera() {
        let (mut g, fake) = game(4);
        g.update();
        let requested = &fake.borrow().requested;
        assert!(requested.contains(&ChunkPos::new(0, 0)));
        assert!(requested.contains(&ChunkPos::new(4, 0)));
        assert!(!requested.contains(&ChunkPos::new(5, 5)));
        assert!(!requested.contains(&ChunkPos::new(6, 0)));
    }

    #[test]
    fn every_column_in_range_has_its_neighbours_requested() {
        let (mut g, fake) = game(6);
        g.update();
        let requested: rustc_hash::FxHashSet<ChunkPos> =
            fake.borrow().requested.iter().copied().collect();
        for dz in -6..=6 {
            for dx in -6..=6 {
                let pos = ChunkPos::new(dx, dz);
                if !in_distance(pos, ChunkPos::new(0, 0), 6) {
                    continue;
                }
                for nz in -1..=1 {
                    for nx in -1..=1 {
                        assert!(requested.contains(&ChunkPos::new(dx + nx, dz + nz)));
                    }
                }
            }
        }
        let origin = ChunkPos::new(0, 0);
        assert!(
            requested
                .iter()
                .all(|p| in_distance(*p, origin, 6 + UNLOAD_MARGIN))
        );
    }

    #[test]
    fn unloads_beyond_distance_plus_margin() {
        let (mut g, fake) = game(2);
        g.update();
        fake.borrow_mut().ready.push(SourceEvent::Loaded(
            ChunkPos::new(0, 0),
            Box::new(ChunkColumn::empty(&cfg())),
        ));
        g.update();
        assert_eq!(g.store.len(), 1);
        g.camera.position.x = 16.0 * 5.0;
        g.update();
        assert_eq!(g.store.len(), 0);
        assert_eq!(g.take_unloaded(), [ChunkPos::new(0, 0)]);
        assert!(fake.borrow().cancelled.contains(&ChunkPos::new(-2, 0)));
    }

    #[test]
    fn missing_chunks_are_not_rerequested_while_in_range() {
        let (mut g, fake) = game(2);
        g.update();
        fake.borrow_mut()
            .ready
            .push(SourceEvent::Missing(ChunkPos::new(1, 1)));
        g.update();
        g.camera.position.x = 16.0;
        let before = fake.borrow().requested.len();
        g.update();
        let again = fake.borrow().requested[before..]
            .iter()
            .filter(|p| **p == ChunkPos::new(1, 1))
            .count();
        assert_eq!(again, 0);
    }
}
