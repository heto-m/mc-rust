//! Runs the stock client under Fabric or NeoForge inside mcr's embedded JVM and paces it from Rust.

mod abi;
pub mod compose;
mod events;
pub mod frame;
pub mod gpu;
pub mod input;
mod launch;
mod loader;
pub mod manager;
pub mod net;
mod pacer;
pub mod profile;
pub mod proxy;
mod sections;
pub mod sound;
pub mod sync;
pub mod world;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mcr_jvm::{DestroyOutcome, HomeSearch, JavaVm, JvmConfig, JvmError, RingError, VmOutcome};

pub use abi::{
    BRIDGE_VERSION, COMPOSE_PROPERTY, ENTRIES, FrameBegin, MOVE_BACKWARD, MOVE_FORWARD, MOVE_JUMP,
    MOVE_LEFT, MOVE_RIGHT, MOVE_SNEAK, MOVE_SPRINT, TABLE_PROPERTY,
};
pub use compose::{ComposeStats, FrameLink, ModFrames};
pub use events::BridgeEvent;
pub use frame::{Perspective, Pipelining};
pub use input::ModInput;
pub use launch::{
    AotSettings, BOOT_JAR, BRIDGE_JAR, BridgeConfig, FML_CONFIG, LAUNCH_MANIFEST, LaunchManifest,
    LaunchPlan, MOD_FOLDERS_PROPERTY, plan, prepare_fml_config, prepare_game_dir,
};
pub use loader::{Loader, MixedLoaders};
pub use mcr_jvm::{aot, preload};
pub use net::NetLink;
pub use pacer::{FRAME_RATE, TICK_RATE, TickClock};

pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
/// `DestroyJavaVM` writes an AOT recording first, which takes seconds for a modded session.
pub const RECORDING_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("{} missing: {hint}", path.display())]
    Missing { path: PathBuf, hint: String },
    #[error("launch manifest: {0}")]
    Manifest(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Jvm(#[from] JvmError),
    #[error(transparent)]
    Ring(#[from] RingError),
    #[error("the bridge control ring already has a writer")]
    Busy,
    #[error(transparent)]
    Gpu(#[from] gpu::GpuError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShutdownOutcome {
    /// The boot call returned and the VM was destroyed.
    Clean {
        elapsed: Duration,
    },
    /// The VM ended, but the boot call failed or `DestroyJavaVM` did not finish cleanly.
    Unclean {
        boot_ok: bool,
        destroyed: bool,
    },
    /// Java called `System.exit`: the VM is frozen and mcr must exit the process to end it.
    Exited {
        code: i32,
    },
    /// Java crashed and parked its thread; the VM cannot stop, so mcr must exit the process.
    Crashed,
    TimedOut,
}

impl ShutdownOutcome {
    /// The JVM is still in the process and can only end with it.
    pub fn needs_process_exit(&self) -> bool {
        matches!(self, Self::Exited { .. } | Self::Crashed | Self::TimedOut)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VmState {
    Running,
    Exited(i32),
    Finished { boot_ok: bool, destroyed: bool },
}

/// The running Java side. One per process, because HotSpot allows one VM per process.
#[derive(Debug)]
pub struct Bridge {
    events: Receiver<BridgeEvent>,
    controls: Arc<pacer::Controls>,
    pacer: Option<JoinHandle<()>>,
    watcher: Option<JoinHandle<()>>,
    vm: VmState,
    frames: Option<ModFrames>,
    _proxy: proxy::HttpProxy,
    input: ModInput,
    aot: Option<Warmup>,
}

#[derive(Debug)]
struct Warmup {
    cache: aot::AotCache,
    plan: aot::AotPlan,
    helper: PathBuf,
    classes: preload::ClassList,
    preload: preload::PreloadPlan,
    preload_failed: bool,
}

/// Java loads `<marker><k>` when it reaches preload milestone `k` (`McrPreload`).
pub const PRELOAD_MARKER: &str = "dev.mcr.bridge.McrPreload$Mark";
pub const PRELOAD_PROPERTY: &str = "mcr.preload";
/// Whether sessions preload when `MCR_JVM_PRELOAD` is not set; lists are recorded either way.
pub const PRELOAD_BY_DEFAULT: bool = true;

impl Bridge {
    /// Moves the process into `config.game_dir`: mods open `config/…` through java.io, which
    /// ignores `user.dir`.
    pub fn start(mut config: BridgeConfig) -> Result<Self, BridgeError> {
        config.make_absolute()?;
        let manifest = LaunchManifest::load_for(&config.cache_dir, config.loader)?;
        for jar in [launch::BOOT_JAR, launch::BRIDGE_JAR] {
            let path = config.cache_dir.join(jar);
            if !path.is_file() {
                return Err(BridgeError::Missing {
                    path,
                    hint: "run `cargo xtask java-build`".into(),
                });
            }
        }
        if config.loader == Loader::NeoForge {
            launch::check_neoforge_bridge(&config.cache_dir.join(launch::BRIDGE_JAR))?;
        }
        launch::prepare_game_dir(&config.game_dir)?;
        if config.loader == Loader::NeoForge {
            launch::prepare_fml_config(&config.game_dir)?;
        }
        let home = HomeSearch::from_env(Some(&config.cache_dir)).locate()?;
        let table = abi::table();
        let http = proxy::HttpProxy::start(proxy::ProxyConfig {
            hosts: config.http_hosts.clone(),
            ..proxy::ProxyConfig::default()
        })?;
        config.proxy_port = Some(http.port());
        let plan = launch::plan(&manifest, &config, table.address())?;

        let shared = abi::shared()?;
        if config.compose {
            compose::activate()?;
        }
        let (tx, events) = abi::event_queue();
        *shared.events.lock().unwrap_or_else(PoisonError::into_inner) = Some(tx);
        let writer = Arc::new(Mutex::new(
            shared.control.writer().ok_or(BridgeError::Busy)?,
        ));

        let mut options = plan.jvm_options();
        let warmup = config
            .aot
            .as_ref()
            .filter(|_| config.loader == Loader::Fabric)
            .and_then(|settings| warmup(settings, &home, &plan));
        if let Some(w) = &warmup {
            options.extra.extend(w.plan.options());
            options.extra.extend(w.preload.options());
            if let Some(list) = w.preload.list() {
                options
                    .extra
                    .push(format!("-D{PRELOAD_PROPERTY}={}", list.display()));
            }
        }
        let mut jvm = JvmConfig::new(home, options);
        jvm.args = plan.args.clone();
        if warmup.as_ref().is_some_and(|w| w.plan.records()) {
            jvm.destroy_timeout = RECORDING_SHUTDOWN_TIMEOUT;
        }
        tracing::info!(
            mods = config.mods.len(),
            loader = %config.loader,
            version = manifest.loader_version,
            "starting the bridge"
        );
        std::env::set_current_dir(&config.game_dir)?;
        let vm = JavaVm::start(jvm)?;
        if let Some(Warmup {
            cache,
            plan: aot::AotPlan::Use(path),
            ..
        }) = &warmup
            && aot::mapped(path) == Some(false)
        {
            tracing::warn!("the JVM refused its AOT cache; the next start records a new one");
            if let Err(e) = cache.reject() {
                tracing::warn!("removing the AOT cache: {e}");
            }
        }
        let watcher = std::thread::Builder::new()
            .name("mcr-bridge-vm".into())
            .spawn(move || {
                let event = match vm.join() {
                    VmOutcome::Exited { code } => BridgeEvent::VmExited { code },
                    VmOutcome::Finished { boot, destroy } => {
                        if let Err(e) = &boot {
                            tracing::error!("bridge boot call failed: {e}");
                        }
                        BridgeEvent::VmFinished {
                            boot_ok: boot.is_ok(),
                            destroyed: matches!(destroy, DestroyOutcome::Destroyed),
                        }
                    }
                };
                event.log();
                if let Ok(shared) = abi::shared() {
                    shared.net.detach();
                    shared.frame.java_gone();
                }
                abi::emit(event);
            })?;
        let controls = pacer::Controls::new(config.width, config.height);
        controls
            .external
            .store(config.compose, std::sync::atomic::Ordering::Release);
        let input = ModInput::new(&shared.input);
        let frames = config
            .compose
            .then(|| ModFrames::new(FrameLink::new(writer.clone(), shared, input.clone())));
        let pacer = pacer::spawn(writer, shared, controls.clone())?;
        Ok(Self {
            events,
            controls,
            pacer: Some(pacer),
            watcher: Some(watcher),
            vm: VmState::Running,
            frames,
            _proxy: http,
            input,
            aot: warmup,
        })
    }

    /// The render thread's frame link when started with [`BridgeConfig::compose`]; install it
    /// on the renderer with `set_external`.
    pub fn frames(&self) -> Option<ModFrames> {
        self.frames.clone()
    }

    /// What this session does with the JVM's AOT cache, when one is configured.
    pub fn aot_plan(&self) -> Option<&aot::AotPlan> {
        self.aot.as_ref().map(|w| &w.plan)
    }

    /// Whether this session records and preloads Java's classes, when warm-up is configured.
    pub fn preload_plan(&self) -> Option<&preload::PreloadPlan> {
        self.aot.as_ref().map(|w| &w.preload)
    }

    /// The packet bridge; one per process like the VM.
    pub fn net(&self) -> NetLink {
        NetLink::new(
            &abi::shared()
                .expect("the bridge started with its shared state")
                .net,
        )
    }

    /// Java's input; one per process like the VM. Events reach Java with its next composed frame.
    pub fn input(&self) -> ModInput {
        self.input.clone()
    }

    /// Stops posting frames (Java stays blocked in its wait) or resumes.
    pub fn set_paused(&self, paused: bool) {
        self.controls.paused.store(paused, Ordering::Release);
    }

    pub fn next_event(&mut self, timeout: Duration) -> Option<BridgeEvent> {
        match self.events.recv_timeout(timeout) {
            Ok(event) => {
                self.observe(&event);
                Some(event)
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => None,
        }
    }

    pub fn try_event(&mut self) -> Option<BridgeEvent> {
        let event = self.events.try_recv().ok()?;
        self.observe(&event);
        Some(event)
    }

    fn observe(&mut self, event: &BridgeEvent) {
        match *event {
            BridgeEvent::Info { ref key, .. } if key == "preload_stopped" => {
                if let Some(w) = &mut self.aot {
                    w.preload_failed = true;
                }
            }
            BridgeEvent::VmExited { code } => self.vm = VmState::Exited(code),
            BridgeEvent::VmFinished { boot_ok, destroyed } => {
                self.vm = VmState::Finished { boot_ok, destroyed };
            }
            _ => {}
        }
    }

    pub fn crashed(&self) -> bool {
        abi::shared().is_ok_and(|s| s.crashed.load(Ordering::Acquire))
    }

    pub fn dropped_events(&self) -> u64 {
        abi::shared().map_or(0, |s| s.dropped_events.load(Ordering::Relaxed))
    }

    /// An exited or crashed VM is frozen for good; the outcome then asks the caller to end the process.
    pub fn shutdown(mut self, timeout: Duration) -> ShutdownOutcome {
        let outcome = self.stop_vm(timeout);
        if let Some(w) = self.aot.take() {
            w.finish(matches!(outcome, ShutdownOutcome::Clean { .. }));
        }
        outcome
    }

    fn stop_vm(&mut self, timeout: Duration) -> ShutdownOutcome {
        let start = Instant::now();
        let timeout = if self.aot.as_ref().is_some_and(|w| w.plan.records()) {
            timeout.max(RECORDING_SHUTDOWN_TIMEOUT)
        } else {
            timeout
        };
        if self.frames.take().is_some()
            && let Ok(backend) = gpu::backend()
        {
            backend.slots.deactivate(backend.gpu);
        }
        self.controls.stop.store(true, Ordering::Release);
        if let Some(p) = self.pacer.take() {
            let _ = p.join();
        }
        let deadline = start + timeout;
        loop {
            while self.try_event().is_some() {}
            match self.vm {
                VmState::Exited(code) => return ShutdownOutcome::Exited { code },
                VmState::Finished {
                    boot_ok: true,
                    destroyed: true,
                } => {
                    if let Some(w) = self.watcher.take() {
                        let _ = w.join();
                    }
                    return ShutdownOutcome::Clean {
                        elapsed: start.elapsed(),
                    };
                }
                VmState::Finished { boot_ok, destroyed } => {
                    return ShutdownOutcome::Unclean { boot_ok, destroyed };
                }
                VmState::Running if self.crashed() => return ShutdownOutcome::Crashed,
                VmState::Running => {}
            }
            let now = Instant::now();
            if now >= deadline {
                return ShutdownOutcome::TimedOut;
            }
            if let Some(event) = self.next_event((deadline - now).min(Duration::from_millis(50))) {
                drop(event);
            }
        }
    }
}

/// Picks this session's AOT plan; any failure only means starting without the cache.
fn warmup(settings: &AotSettings, home: &mcr_jvm::JavaHome, plan: &LaunchPlan) -> Option<Warmup> {
    let started = Instant::now();
    let options: Vec<String> = match plan.jvm_options().to_strings() {
        Ok(all) => all.into_iter().filter(|o| !o.starts_with("-D")).collect(),
        Err(e) => {
            tracing::warn!("AOT cache off: {e}");
            return None;
        }
    };
    let inputs = aot::KeyInputs {
        java_home: home.path(),
        options: &options,
        class_path: &plan.options_class_path,
        jars: &plan.mod_jars,
        extra: &format!("bridge {BRIDGE_VERSION}"),
    };
    let result = aot::AotCache::new(&settings.dir, &inputs).and_then(|cache| {
        let plan_now = cache.plan();
        cache.begin(&plan_now)?;
        if plan_now.records() {
            cache.write_job(&home.java(), &options, &plan.options_class_path)?;
        }
        Ok((cache, plan_now))
    });
    match result {
        Ok((cache, aot_plan)) => {
            let classes = preload::ClassList::new(&settings.dir, cache.key(), PRELOAD_MARKER);
            let mut preload = classes.plan(PRELOAD_BY_DEFAULT);
            if let Err(e) = classes.begin(&preload) {
                tracing::warn!("class preloading off: {e}");
                preload = preload::PreloadPlan::Off("class list directory unavailable");
            }
            tracing::info!(
                key = cache.key(),
                plan = ?aot_plan,
                ?preload,
                ms = started.elapsed().as_millis() as u64,
                "JVM AOT cache"
            );
            Some(Warmup {
                cache,
                plan: aot_plan,
                helper: settings.helper.clone(),
                classes,
                preload,
                preload_failed: false,
            })
        }
        Err(e) => {
            tracing::warn!("AOT cache off: {e}");
            None
        }
    }
}

impl Warmup {
    /// Merges the class list, keeps a finished recording and starts the helper that builds the cache from it.
    fn finish(self, clean: bool) {
        match self
            .classes
            .finish(&self.preload, clean && !self.preload_failed)
        {
            Ok(n) if n > 0 => tracing::info!(classes = n, "class list for preloading updated"),
            Ok(_) => {}
            Err(e) => tracing::warn!("class list for preloading: {e}"),
        }
        if self.plan.records() {
            match self.cache.end_recording(clean) {
                Ok(true) => {
                    tracing::info!("AOT recording kept; building the cache in the background")
                }
                Ok(false) => {
                    tracing::info!("AOT recording dropped: the session did not end cleanly")
                }
                Err(e) => tracing::warn!("AOT recording: {e}"),
            }
        }
        if self.cache.needs_create()
            && let Err(e) = self.cache.spawn_create(&self.helper)
        {
            tracing::warn!("starting the AOT cache build: {e}");
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.controls.stop.store(true, Ordering::Release);
    }
}
