use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use mcr_modbridge::{Bridge, BridgeConfig, BridgeEvent, NetLink, SHUTDOWN_TIMEOUT};

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
    pub fn start(opts: &Options, dir: &Path) -> Result<Option<Self>> {
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
        config.width = opts.size.0;
        config.height = opts.size.1;
        let mut bridge = Bridge::start(config).context("starting the Fabric bridge")?;
        let net = ModNet {
            link: bridge.net(),
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
