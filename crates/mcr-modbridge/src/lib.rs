//! Runs the stock client under Fabric inside mcr's embedded JVM and paces it from Rust.

mod abi;
pub mod compose;
mod events;
pub mod frame;
pub mod gpu;
mod launch;
pub mod net;
mod pacer;
pub mod sound;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mcr_jvm::{DestroyOutcome, HomeSearch, JavaVm, JvmConfig, JvmError, RingError, VmOutcome};

pub use abi::{BRIDGE_VERSION, COMPOSE_PROPERTY, ENTRIES, FrameBegin, TABLE_PROPERTY};
pub use compose::{ComposeStats, FrameLink, ModFrames};
pub use events::BridgeEvent;
pub use launch::{
    BOOT_JAR, BRIDGE_JAR, BridgeConfig, LAUNCH_MANIFEST, LaunchManifest, LaunchPlan, plan,
    prepare_game_dir,
};
pub use net::NetLink;
pub use pacer::{FRAME_RATE, TICK_RATE, TickClock};

pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

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
}

impl Bridge {
    pub fn start(config: BridgeConfig) -> Result<Self, BridgeError> {
        let manifest = LaunchManifest::load(&config.cache_dir)?;
        for jar in [launch::BOOT_JAR, launch::BRIDGE_JAR] {
            let path = config.cache_dir.join(jar);
            if !path.is_file() {
                return Err(BridgeError::Missing {
                    path,
                    hint: "run `cargo xtask java-build`".into(),
                });
            }
        }
        launch::prepare_game_dir(&config.game_dir)?;
        let home = HomeSearch::from_env(Some(&config.cache_dir)).locate()?;
        let table = abi::table();
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

        let mut jvm = JvmConfig::new(home, plan.jvm_options());
        jvm.args = plan.args.clone();
        tracing::info!(
            mods = config.mods.len(),
            loader = manifest.loader_version,
            "starting the Fabric bridge"
        );
        let vm = JavaVm::start(jvm)?;
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
        let frames = config
            .compose
            .then(|| ModFrames::new(FrameLink::new(writer.clone(), shared)));
        let pacer = pacer::spawn(writer, shared, controls.clone())?;
        Ok(Self {
            events,
            controls,
            pacer: Some(pacer),
            watcher: Some(watcher),
            vm: VmState::Running,
            frames,
        })
    }

    /// The render thread's frame link when started with [`BridgeConfig::compose`]; install it
    /// on the renderer with `set_external`.
    pub fn frames(&self) -> Option<ModFrames> {
        self.frames.clone()
    }

    /// The packet bridge; one per process like the VM.
    pub fn net(&self) -> NetLink {
        NetLink::new(
            &abi::shared()
                .expect("the bridge started with its shared state")
                .net,
        )
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
        let start = Instant::now();
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

impl Drop for Bridge {
    fn drop(&mut self) {
        self.controls.stop.store(true, Ordering::Release);
    }
}
