use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use mcr_modbridge::sound::SoundLink;
use mcr_modbridge::{
    Bridge, BridgeConfig, BridgeEvent, ComposeStats, ModFrames, NetLink, SHUTDOWN_TIMEOUT,
};
use mcr_render::Renderer;

use crate::cli::Options;

pub const CACHE_DIR: &str = ".cache";

/// The headless Fabric boot running next to the native client; Java mirrors every connection.
pub struct ModHost {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Bridge>,
    net: ModNet,
}

/// What the shell needs from the mod host: hooks for each connection and Java's join requests.
#[derive(Debug, Clone)]
pub struct ModNet {
    pub link: NetLink,
    /// Java's sound output, played by the shell's mixer.
    pub sound: Option<SoundLink>,
    /// Java's frames in lockstep with the renderer; its GUI replaces the native one while shown.
    pub frames: Option<ModFrames>,
    connects: Arc<Mutex<Vec<String>>>,
}

impl ModNet {
    /// Addresses Java's own `ConnectScreen` asked Rust to join, oldest first.
    pub fn take_connect_requests(&self) -> Vec<String> {
        std::mem::take(&mut *self.connects.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

pub fn mod_jars(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut jars: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading mods folder {}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "jar"))
        .collect();
    jars.sort();
    Ok(jars)
}

impl ModHost {
    /// Java records its GPU work on `renderer`'s device and its GUI is composed into every frame.
    pub fn start(opts: &Options, dir: &Path, renderer: &mut Renderer) -> Result<Option<Self>> {
        let mods = mod_jars(dir)?;
        if mods.is_empty() {
            eprintln!(
                "[mods] {} has no mod jars; not starting Java",
                dir.display()
            );
            return Ok(None);
        }
        let mut config = BridgeConfig::new(CACHE_DIR.into(), opts.game_dir.join("java"), mods);
        config.username = opts.name.clone();
        config.uuid = mcr_auth::offline::offline_uuid(&opts.name)
            .unwrap_or_default()
            .hyphenated()
            .to_string();
        (config.width, config.height) = renderer.size();
        config.compose = true;
        mcr_modbridge::gpu::install(
            renderer.device().clone(),
            renderer.queue().clone(),
            renderer.adapter_info().clone(),
        )
        .context("sharing the GPU device with Java")?;
        let mut bridge = Bridge::start(config).context("starting the Fabric bridge")?;
        let frames = bridge.frames();
        if let Some(f) = &frames {
            renderer.set_external(Some(Box::new(f.clone())));
        }
        let net = ModNet {
            link: bridge.net(),
            sound: SoundLink::get().ok(),
            frames,
            connects: Arc::new(Mutex::new(Vec::new())),
        };
        let connects = net.connects.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("mcr-mods-log".into())
            .spawn(move || {
                while !flag.load(Ordering::Acquire) {
                    if let Some(event) = bridge.next_event(Duration::from_millis(100)) {
                        eprintln!("[mods] {event:?}");
                        if let BridgeEvent::ConnectRequested { address, .. } = event {
                            connects
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .push(address);
                        }
                    }
                }
                bridge
            })?;
        Ok(Some(Self { stop, thread, net }))
    }

    pub fn net(&self) -> ModNet {
        self.net.clone()
    }

    /// A Java side that exited or crashed is frozen for good, so the process exits here.
    pub fn finish(self) {
        if let Some(frames) = &self.net.frames {
            let s = frames.lock().stats();
            let (w50, w99, wmax) = ComposeStats::percentiles(&s.waits_us);
            let (j50, j99, jmax) = ComposeStats::percentiles(&s.java_us);
            eprintln!(
                "[mods] composed {} frames ({} late): render-thread wait p50 {w50} us p99 {w99} us max {wmax} us; java frame p50 {j50} us p99 {j99} us max {jmax} us; ticks java {} rust {}",
                s.composed, s.late, s.java_ticks, s.rust_ticks
            );
        }
        self.stop.store(true, Ordering::Release);
        let Ok(bridge) = self.thread.join() else {
            return;
        };
        let outcome = bridge.shutdown(SHUTDOWN_TIMEOUT);
        eprintln!("[mods] shutdown: {outcome:?}");
        if outcome.needs_process_exit() {
            std::process::exit(0);
        }
    }
}
