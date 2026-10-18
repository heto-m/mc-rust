//! Java's baked fluid and block models for Rust's terrain (the bridge's render report), and the
//! title-time prebake that has a modded pack's model cache ready before the join.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use mcr_assets::ResourceManager;
use mcr_data::Version;
use mcr_mesh::ModelCache;
use mcr_mesh::java::JavaModels;
use mcr_modbridge::ModFrames;

/// A join waits at most this long for a prebake still running from the title screen.
const PREBAKE_WAIT: Duration = Duration::from_secs(10);
/// Prebakes per title report: the first, and again after Java answered its model request.
const MAX_PREBAKES: u32 = 3;
/// The first prebake waits this long for Java's fluid models to arrive after its registries.
const FLUIDS_WAIT: Duration = Duration::from_secs(5);

struct Prebaked {
    version: Version,
    cache: ModelCache,
    models: u64,
}

type Job = Receiver<Option<Prebaked>>;

#[derive(Default)]
pub struct JavaRender {
    models: Arc<JavaModels>,
    /// Render reports merged into `models`.
    models_gen: u64,
    render_seen: u64,
    registries_seen: u64,
    title_report: Option<(Arc<Vec<u8>>, Instant)>,
    prebakes: u32,
    job: Option<(Job, u64)>,
    prebaked: Option<Prebaked>,
    requested: HashSet<String>,
    /// An in-game rebake after Java answered a request made at the join.
    rebake: Option<Receiver<Option<ModelCache>>>,
    /// `models_gen` the live cache was baked with.
    live_models: u64,
    user_packs: (u64, Vec<std::path::PathBuf>),
}

impl JavaRender {
    pub fn models(&self) -> Arc<JavaModels> {
        self.models.clone()
    }

    /// Java's selected packs that exist inside `game_dir`; others are left out.
    pub fn user_packs(&mut self, game_dir: &std::path::Path) -> &[std::path::PathBuf] {
        if self.user_packs.0 != self.models_gen {
            let root = game_dir.canonicalize().ok();
            let packs = self
                .models
                .packs
                .iter()
                .filter_map(|p| std::path::Path::new(p).canonicalize().ok())
                .filter(|p| root.as_ref().is_some_and(|r| p.starts_with(r)))
                .collect();
            self.user_packs = (self.models_gen, packs);
        }
        &self.user_packs.1
    }

    /// The resources changed: what was prebaked from the old ones is dropped and baked again.
    pub fn resources_changed(&mut self) {
        self.job = None;
        self.prebaked = None;
        self.prebakes = 0;
        self.live_models = self.models_gen;
    }

    /// Takes in Java's newest reports; at the title screen, prebakes for Java's registries.
    pub fn poll(
        &mut self,
        resources: &Arc<ResourceManager>,
        frames: Option<&ModFrames>,
        at_title: bool,
    ) {
        if let Some((generation, bytes)) = mcr_modbridge::registries::render_latest()
            && generation > self.render_seen
        {
            self.render_seen = generation;
            match JavaModels::parse(&bytes) {
                Ok(report) => {
                    println!(
                        "[mods] Java models: {} fluids, {} blocks",
                        report.fluids.len(),
                        report.blocks.len()
                    );
                    let mut merged = (*self.models).clone();
                    merged.merge(report);
                    self.models = Arc::new(merged);
                    self.models_gen += 1;
                }
                Err(e) => eprintln!("[mods] Java's render report could not be used: {e}"),
            }
        }
        if let Some((generation, bytes)) = mcr_modbridge::registries::latest()
            && generation > self.registries_seen
        {
            self.registries_seen = generation;
            if !mcr_modbridge::registries::is_configuration(&bytes) {
                self.title_report = Some((bytes, Instant::now()));
                self.prebakes = 0;
                self.prebaked = None;
            }
        }
        if let Some((job, models)) = &self.job {
            match job.try_recv() {
                Ok(done) => {
                    let models = *models;
                    self.job = None;
                    if let Some(p) = done {
                        println!(
                            "[mods] prebaked models for {} block states (Java models {}), {} unbaked",
                            p.version.state_count(),
                            p.cache.java_states,
                            p.cache.unbaked_states
                        );
                        self.request(&p.cache, frames);
                        self.prebaked = Some(Prebaked { models, ..p });
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.job = None,
            }
        }
        let stale = self
            .prebaked
            .as_ref()
            .is_none_or(|p| p.models < self.models_gen);
        if stale
            && at_title
            && self.prebakes < MAX_PREBAKES
            && let Some((report, at)) = &self.title_report
            && (self.models_gen > 0 || at.elapsed() > FLUIDS_WAIT)
        {
            self.prebakes += 1;
            self.job = Some((
                spawn_prebake(report.clone(), resources.clone(), self.models.clone()),
                self.models_gen,
            ));
        }
    }

    /// Asks Java for the blocks `cache` could not bake that were not asked for yet.
    pub fn request(&mut self, cache: &ModelCache, frames: Option<&ModFrames>) {
        let Some(frames) = frames else { return };
        let fresh: Vec<&str> = cache
            .java_requests
            .iter()
            .filter(|n| !self.requested.contains(*n))
            .map(String::as_str)
            .collect();
        if fresh.is_empty() {
            return;
        }
        println!(
            "[mods] asking Java for the models of {} blocks",
            fresh.len()
        );
        frames.lock().request_models(fresh.iter().copied());
        self.requested.extend(fresh.into_iter().map(str::to_owned));
    }

    /// The prebaked cache re-indexed for `data` (the server's registries), waiting for a prebake
    /// still running; `None` when there is none or it does not fit.
    pub fn take_cache_for(&mut self, data: &Version) -> Option<ModelCache> {
        if let Some((job, models)) = self.job.take() {
            let started = Instant::now();
            match job.recv_timeout(PREBAKE_WAIT) {
                Ok(Some(p)) => {
                    println!("[mods] waited {:?} for the prebake", started.elapsed());
                    self.prebaked = Some(Prebaked { models, ..p });
                }
                Ok(None) | Err(RecvTimeoutError::Disconnected) => {}
                Err(RecvTimeoutError::Timeout) => {
                    eprintln!("[mods] prebake still running after {PREBAKE_WAIT:?}; baking now");
                    return None;
                }
            }
        }
        self.prebakes = 0;
        let p = self.prebaked.take()?;
        if p.models < self.models_gen {
            return None;
        }
        match p.cache.remap(&p.version, data) {
            Some(cache) => {
                self.live_models = p.models;
                Some(cache)
            }
            None => {
                println!("[mods] the server's block tables differ from the title's; baking now");
                None
            }
        }
    }

    /// The live cache was baked now, with the current models.
    pub fn baked_live(&mut self) {
        self.live_models = self.models_gen;
    }

    /// In game: once Java answered requests the live cache needs, rebakes it in the background;
    /// returns the new cache when ready.
    pub fn poll_rebake(
        &mut self,
        resources: &Arc<ResourceManager>,
        data: &'static Version,
        live: &ModelCache,
    ) -> Option<ModelCache> {
        if let Some(rx) = &self.rebake {
            return match rx.try_recv() {
                Ok(cache) => {
                    self.rebake = None;
                    cache
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.rebake = None;
                    None
                }
            };
        }
        let answered = self.live_models < self.models_gen
            && live.java_requests.iter().any(|b| self.models.has_block(b));
        if answered {
            self.live_models = self.models_gen;
            let (tx, rx) = channel();
            let (resources, models) = (resources.clone(), self.models.clone());
            std::thread::spawn(move || {
                let _ = tx.send(ModelCache::build_with(&resources, data, &models).ok());
            });
            self.rebake = Some(rx);
        }
        None
    }
}

fn spawn_prebake(
    report: Arc<Vec<u8>>,
    resources: Arc<ResourceManager>,
    models: Arc<JavaModels>,
) -> Job {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("mcr-prebake".into())
        .spawn(move || {
            let result = (|| {
                let report = mcr_modbridge::registries::parse(&report).ok()?;
                let version = mcr_data::vanilla_26_3().rebuild(&report.export).ok()?;
                let cache = ModelCache::build_with(&resources, &version, &models).ok()?;
                Some(Prebaked {
                    version,
                    cache,
                    models: 0,
                })
            })();
            let _ = tx.send(result);
        })
        .expect("spawn prebake thread");
    rx
}
