use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;

use anyhow::{Context, Result};
use mcr_anvil::{RegionFile, World};
use mcr_world::{ChunkColumn, ChunkPos, WorldConfig};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug)]
pub enum SourceEvent {
    Loaded(ChunkPos, Box<ChunkColumn>),
    Missing(ChunkPos),
}

pub trait WorldSource {
    fn config(&self) -> WorldConfig;
    /// Names indexed by the biome ids used in delivered columns; may grow while columns arrive.
    fn biome_names(&self) -> Vec<String>;
    fn spawn(&self) -> Option<[f64; 3]> {
        None
    }
    /// Lower `priority` is served first. Requesting a chunk already in flight is a no-op.
    fn request(&mut self, pos: ChunkPos, priority: u32);
    /// Drops a request not yet delivered; its event, if already produced, is discarded.
    fn cancel(&mut self, pos: ChunkPos);
    fn poll(&mut self, out: &mut Vec<SourceEvent>);
    fn in_flight(&self) -> usize;
}

type Key = (i32, i32);

#[derive(Default)]
struct Queue {
    heap: BinaryHeap<Reverse<(u32, u64, Key)>>,
    wanted: FxHashSet<Key>,
    seq: u64,
    shutdown: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
}

type RegionSlot = Arc<OnceLock<Option<Arc<RegionFile>>>>;

const MAX_CACHED_REGIONS: usize = 24;

pub struct AnvilSource {
    world: Arc<World>,
    shared: Arc<Shared>,
    events: Receiver<SourceEvent>,
    workers: Vec<JoinHandle<()>>,
    outstanding: FxHashSet<ChunkPos>,
}

impl std::fmt::Debug for AnvilSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnvilSource")
            .field("workers", &self.workers.len())
            .field("outstanding", &self.outstanding.len())
            .finish()
    }
}

fn region_for(
    world: &World,
    cache: &Mutex<FxHashMap<Key, RegionSlot>>,
    key: Key,
) -> Option<Arc<RegionFile>> {
    let slot = {
        let mut map = cache.lock().expect("region cache");
        if map.len() >= MAX_CACHED_REGIONS && !map.contains_key(&key) {
            map.clear();
        }
        map.entry(key).or_default().clone()
    };
    slot.get_or_init(|| {
        world
            .open_region(key.0, key.1)
            .unwrap_or_else(|e| {
                tracing::warn!(rx = key.0, rz = key.1, error = %e, "unreadable region");
                None
            })
            .map(Arc::new)
    })
    .clone()
}

fn worker(
    world: Arc<World>,
    shared: Arc<Shared>,
    cache: Arc<Mutex<FxHashMap<Key, RegionSlot>>>,
    tx: Sender<SourceEvent>,
) {
    loop {
        let (cx, cz) = {
            let mut q = shared.queue.lock().expect("queue lock");
            loop {
                if q.shutdown {
                    return;
                }
                match q.heap.pop() {
                    Some(Reverse((_, _, key))) if q.wanted.remove(&key) => break key,
                    Some(_) => {}
                    None => q = shared.wake.wait(q).expect("queue wait"),
                }
            }
        };
        let pos = ChunkPos::new(cx, cz);
        let loaded = region_for(&world, &cache, (cx >> 5, cz >> 5))
            .and_then(|region| world.load_chunk_in(&region, cx, cz));
        let event = match loaded {
            Some(chunk) => SourceEvent::Loaded(pos, Box::new(chunk.column)),
            None => SourceEvent::Missing(pos),
        };
        if tx.send(event).is_err() {
            return;
        }
    }
}

impl AnvilSource {
    pub fn open(path: &Path, threads: usize) -> Result<Self> {
        let world = Arc::new(World::open(path).with_context(|| format!("opening {path:?}"))?);
        let shared = Arc::new(Shared {
            queue: Mutex::default(),
            wake: Condvar::new(),
        });
        let (tx, events) = channel();
        let cache = Arc::new(Mutex::default());
        let workers = (0..threads.max(1))
            .map(|i| {
                let (world, shared, cache, tx) =
                    (world.clone(), shared.clone(), cache.clone(), tx.clone());
                std::thread::Builder::new()
                    .name(format!("anvil-{i}"))
                    .spawn(move || worker(world, shared, cache, tx))
                    .expect("spawn anvil worker")
            })
            .collect();
        Ok(Self {
            world,
            shared,
            events,
            workers,
            outstanding: FxHashSet::default(),
        })
    }
}

impl WorldSource for AnvilSource {
    fn config(&self) -> WorldConfig {
        *self.world.config()
    }

    fn biome_names(&self) -> Vec<String> {
        self.world.biome_names()
    }

    fn spawn(&self) -> Option<[f64; 3]> {
        let s = self.world.summary();
        s.player_pos
            .or_else(|| s.spawn.map(|p| p.map(|v| f64::from(v) + 0.5)))
    }

    fn request(&mut self, pos: ChunkPos, priority: u32) {
        if !self.outstanding.insert(pos) {
            return;
        }
        let mut q = self.shared.queue.lock().expect("queue lock");
        q.seq += 1;
        let seq = q.seq;
        q.wanted.insert((pos.x, pos.z));
        q.heap.push(Reverse((priority, seq, (pos.x, pos.z))));
        drop(q);
        self.shared.wake.notify_one();
    }

    fn cancel(&mut self, pos: ChunkPos) {
        if self.outstanding.remove(&pos) {
            self.shared
                .queue
                .lock()
                .expect("queue lock")
                .wanted
                .remove(&(pos.x, pos.z));
        }
    }

    fn poll(&mut self, out: &mut Vec<SourceEvent>) {
        while let Ok(event) = self.events.try_recv() {
            let pos = match &event {
                SourceEvent::Loaded(p, _) | SourceEvent::Missing(p) => *p,
            };
            if self.outstanding.remove(&pos) {
                out.push(event);
            }
        }
    }

    fn in_flight(&self) -> usize {
        self.outstanding.len()
    }
}

impl Drop for AnvilSource {
    fn drop(&mut self) {
        self.shared.queue.lock().expect("queue lock").shutdown = true;
        self.shared.wake.notify_all();
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}
