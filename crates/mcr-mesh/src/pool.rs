//! Background meshing: snapshot on the caller's thread, mesh on workers, poll results without blocking.

use std::collections::{BinaryHeap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use mcr_render::SectionMesh;
use mcr_world::{ChunkStore, SectionPos};

use crate::cache::ModelCache;
use crate::input::MeshInput;
use crate::mesher::{MeshOptions, Mesher};
use crate::tint::BiomeColors;

#[derive(Debug)]
pub struct MeshResult {
    pub pos: SectionPos,
    pub mesh: SectionMesh,
}

struct Job {
    priority: u32,
    seq: u64,
    generation: u64,
    input: MeshInput,
}

impl PartialEq for Job {
    fn eq(&self, other: &Self) -> bool {
        (self.priority, self.seq) == (other.priority, other.seq)
    }
}
impl Eq for Job {}
impl PartialOrd for Job {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Job {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (other.priority, other.seq).cmp(&(self.priority, self.seq))
    }
}

struct Shared {
    queue: Mutex<BinaryHeap<Job>>,
    wake: Condvar,
    shutdown: AtomicBool,
    in_flight: AtomicUsize,
}

pub struct MeshPool {
    shared: Arc<Shared>,
    results: Receiver<(u64, MeshResult)>,
    workers: Vec<JoinHandle<()>>,
    latest: HashMap<SectionPos, u64>,
    next_generation: u64,
    seq: u64,
    blend_radius: usize,
    biome_seed: i64,
}

impl std::fmt::Debug for MeshPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeshPool")
            .field("workers", &self.workers.len())
            .field("pending", &self.pending())
            .finish()
    }
}

fn worker(shared: Arc<Shared>, tx: Sender<(u64, MeshResult)>, mut mesher: Mesher) {
    loop {
        let job = {
            let mut queue = shared.queue.lock().expect("queue lock");
            loop {
                if shared.shutdown.load(Ordering::Acquire) {
                    return;
                }
                if let Some(job) = queue.pop() {
                    break job;
                }
                queue = shared.wake.wait(queue).expect("queue wait");
            }
        };
        let mut mesh = SectionMesh::default();
        mesher.mesh(&job.input, &mut mesh);
        let result = MeshResult {
            pos: job.input.pos,
            mesh,
        };
        let _ = tx.send((job.generation, result));
        shared.in_flight.fetch_sub(1, Ordering::AcqRel);
    }
}

impl MeshPool {
    pub fn new(
        cache: Arc<ModelCache>,
        colors: Arc<BiomeColors>,
        threads: usize,
        blend_radius: usize,
    ) -> Self {
        Self::with_options(cache, colors, threads, blend_radius, MeshOptions::default())
    }

    pub fn with_options(
        cache: Arc<ModelCache>,
        colors: Arc<BiomeColors>,
        threads: usize,
        blend_radius: usize,
        options: MeshOptions,
    ) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(BinaryHeap::new()),
            wake: Condvar::new(),
            shutdown: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
        });
        let (tx, results) = channel();
        let workers = (0..threads.max(1))
            .map(|i| {
                let mesher = Mesher::with_options(cache.clone(), colors.clone(), options);
                let (shared, tx) = (shared.clone(), tx.clone());
                std::thread::Builder::new()
                    .name(format!("mesh-{i}"))
                    .spawn(move || worker(shared, tx, mesher))
                    .expect("spawn mesh worker")
            })
            .collect();
        Self {
            shared,
            results,
            workers,
            latest: HashMap::new(),
            next_generation: 0,
            seq: 0,
            blend_radius,
            biome_seed: 0,
        }
    }

    /// The hashed world seed biome zoom uses; applies to jobs submitted afterwards.
    pub fn set_biome_seed(&mut self, seed: i64) {
        self.biome_seed = seed;
    }

    pub fn blend_radius(&self) -> usize {
        self.blend_radius
    }

    /// Queues `input`; lower `priority` values mesh first. Older queued or running jobs for the same
    /// section are superseded and their results dropped by `poll`.
    pub fn submit(&mut self, mut input: MeshInput, priority: u32) {
        input.biome_seed = self.biome_seed;
        self.next_generation += 1;
        self.seq += 1;
        self.latest.insert(input.pos, self.next_generation);
        self.shared.in_flight.fetch_add(1, Ordering::AcqRel);
        self.shared.queue.lock().expect("queue lock").push(Job {
            priority,
            seq: self.seq,
            generation: self.next_generation,
            input,
        });
        self.shared.wake.notify_one();
    }

    /// Drains every dirty section of `store`, snapshots it and queues it, nearest to `camera` first.
    /// Returns sections that are no longer loaded, whose meshes should be removed.
    pub fn submit_dirty(&mut self, store: &mut ChunkStore, camera: SectionPos) -> Vec<SectionPos> {
        let dirty: Vec<SectionPos> = store.drain_dirty().collect();
        let mut gone = Vec::new();
        for pos in dirty {
            match MeshInput::snapshot(store, pos, self.blend_radius) {
                Some(input) => {
                    let d = |a: i32, b: i32| (a - b).unsigned_abs();
                    let priority = d(pos.x, camera.x).pow(2)
                        + d(pos.y, camera.y).pow(2)
                        + d(pos.z, camera.z).pow(2);
                    self.submit(input, priority);
                }
                None => {
                    self.latest.remove(&pos);
                    gone.push(pos);
                }
            }
        }
        gone
    }

    /// Jobs queued or running.
    pub fn pending(&self) -> usize {
        self.shared.in_flight.load(Ordering::Acquire)
    }

    /// Returns finished, non-stale results without blocking.
    pub fn poll(&mut self, out: &mut Vec<MeshResult>) {
        while let Ok((generation, result)) = self.results.try_recv() {
            if self.latest.get(&result.pos) == Some(&generation) {
                self.latest.remove(&result.pos);
                out.push(result);
            }
        }
    }

    /// Blocks until every submitted job has finished, then collects the results.
    pub fn wait_idle(&mut self, out: &mut Vec<MeshResult>) {
        while self.pending() > 0 {
            std::thread::yield_now();
        }
        self.poll(out);
    }
}

impl Drop for MeshPool {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        self.shared.wake.notify_all();
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}
