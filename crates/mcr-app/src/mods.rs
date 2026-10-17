use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use mcr_modbridge::ModInput;
use mcr_modbridge::manager::{Manager, ModStatus, Report, SessionStore, SessionTracker};
use mcr_modbridge::sound::SoundLink;
use mcr_modbridge::world::WorldRequest;
use mcr_modbridge::{
    Bridge, BridgeConfig, BridgeEvent, ComposeStats, ModFrames, NetLink, SHUTDOWN_TIMEOUT,
};
use mcr_render::Renderer;

use crate::cli::Options;
use crate::mods_ui::ModsApp;

pub const CACHE_DIR: &str = ".cache";
static CACHE_ABS: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
/// The JVM's AOT caches, under the game dir.
pub const AOT_DIR: &str = "cache/jvm-aot";

/// The headless Fabric boot running next to the native client; Java mirrors every connection.
pub struct ModHost {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Bridge>,
    net: ModNet,
    tracker: Arc<SessionTracker>,
    recording: bool,
}

fn aot_line(plan: &mcr_modbridge::aot::AotPlan) -> String {
    use mcr_modbridge::aot::AotPlan;
    match plan {
        AotPlan::Use(path) => format!("JVM AOT cache: {}", path.display()),
        AotPlan::Record(_) => {
            "JVM AOT cache: recording this session; it is built after you quit".to_owned()
        }
        AotPlan::Building => "JVM AOT cache: still being built, starting without it".to_owned(),
        AotPlan::Off(why) => format!("JVM AOT cache off: {why}"),
    }
}

/// What the shell needs from the mod host: hooks for each connection and Java's join requests.
#[derive(Debug, Clone)]
pub struct ModNet {
    pub link: NetLink,
    /// Java's sound output, played by the shell's mixer.
    pub sound: Option<SoundLink>,
    /// Java's frames in lockstep with the renderer; its GUI replaces the native one while shown.
    pub frames: Option<ModFrames>,
    /// Java's input while its GUI is composed.
    pub input: Option<ModInput>,
    connects: Arc<Mutex<Vec<String>>>,
    keys: Arc<Mutex<Vec<(String, String)>>>,
    worlds: Arc<Mutex<Vec<WorldRequest>>>,
}

impl ModNet {
    /// Addresses Java's own `ConnectScreen` asked Rust to join, oldest first.
    pub fn take_connect_requests(&self) -> Vec<String> {
        std::mem::take(&mut *self.connects.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// What Java's world screens asked for, oldest first.
    pub fn take_world_requests(&self) -> Vec<WorldRequest> {
        std::mem::take(&mut *self.worlds.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Vanilla mappings Java rebound, oldest first, as `(mapping, key)` names.
    pub fn take_key_changes(&self) -> Vec<(String, String)> {
        std::mem::take(&mut *self.keys.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// Java's rebinds waiting for the shell; a stuck shell cannot grow it without bound.
const MAX_KEY_CHANGES: usize = 1024;
/// Java's world requests waiting for the shell.
const MAX_WORLD_REQUESTS: usize = 16;

/// The mod mode decision and the Mods screen's model, made once at startup.
pub struct Startup {
    pub boot: Option<BootPlan>,
    pub app: ModsApp,
}

/// What Java starts with, once the renderer exists.
#[derive(Debug)]
pub struct BootPlan {
    jars: Vec<PathBuf>,
    active_ids: Vec<String>,
    heap_mib: u32,
    session: SessionStore,
    explicit: bool,
    http_hosts: Vec<String>,
}

/// Plans a Java boot only when at least one mod is enabled and resolvable, and the last session did
/// not crash or hang (that case shows the fallback prompt instead).
/// The repository `.cache` as an absolute path, resolved before Java moves the process into its game dir.
pub fn cache_dir() -> &'static Path {
    CACHE_ABS.get_or_init(|| std::path::absolute(CACHE_DIR).unwrap_or_else(|_| CACHE_DIR.into()))
}

pub fn startup(opts: &Options) -> Result<Startup> {
    cache_dir();
    let mods_dir = opts
        .mods
        .clone()
        .unwrap_or_else(|| opts.game_dir.join("mods"));
    let manager = Manager::new(&opts.game_dir, &mods_dir);
    let settings = manager.load_settings();
    let session = manager.session();
    let previous = session.load();
    let report = manager.scan(&settings);
    for line in report_lines(&report) {
        eprintln!("[mods] {line}");
    }
    let jars = report.jars();
    let would_boot = !jars.is_empty();
    let prompt = would_boot && previous.needs_fallback_prompt();
    if !would_boot {
        eprintln!(
            "[mods] no enabled mods in {}; not starting Java",
            mods_dir.display()
        );
        if previous.needs_fallback_prompt() {
            session.reset();
        }
    }
    let boot = (would_boot && !prompt).then(|| BootPlan {
        jars,
        active_ids: report.active_ids(),
        heap_mib: settings.heap_mib(),
        session,
        explicit: opts.mods.is_some(),
        http_hosts: manager.policy.http_hosts().to_vec(),
    });
    let app = ModsApp::new(manager, settings, previous, prompt);
    Ok(Startup { boot, app })
}

/// One line per mod that is not loaded, or loaded with a policy caveat.
fn report_lines(report: &Report) -> Vec<String> {
    report
        .entries
        .iter()
        .filter_map(|e| {
            let why = match (&e.status, &e.policy) {
                (ModStatus::Active, Some(p)) => format!("{:?}: {}", p.status, p.reason),
                (ModStatus::Active, None) => return None,
                (ModStatus::Disabled, _) => "disabled".to_owned(),
                (ModStatus::Blocked(r), _) => format!("blocked: {r}"),
                (ModStatus::Excluded(r), _) => format!("excluded: {r}"),
                (ModStatus::ServerOnly, _) => "server-only".to_owned(),
                (ModStatus::Unsupported(r), _) => format!("not loaded: {r}"),
            };
            Some(format!("{} {}: {why}", e.info.id, e.info.version))
        })
        .collect()
}

impl BootPlan {
    /// Starts Java; a failure is fatal only for an explicit `--mods`, otherwise mcr runs natively.
    pub fn start(
        self,
        opts: &Options,
        renderer: &mut Renderer,
        app: &ModsApp,
    ) -> Result<Option<ModHost>> {
        let explicit = self.explicit;
        let session = self.session.clone();
        let ids = self.active_ids.clone();
        match ModHost::start(opts, self, renderer) {
            Ok(host) => {
                app.set_running(ids, host.tracker.handle());
                Ok(Some(host))
            }
            Err(e) if explicit => Err(e),
            Err(e) => {
                eprintln!("[mods] Java did not start, continuing without mods: {e:#}");
                session.reset();
                Ok(None)
            }
        }
    }
}

impl ModHost {
    /// Java records its GPU work on `renderer`'s device and its GUI is composed into every frame.
    fn start(opts: &Options, plan: BootPlan, renderer: &mut Renderer) -> Result<Self> {
        let mut config = BridgeConfig::new(
            cache_dir().to_path_buf(),
            opts.game_dir.join("java"),
            plan.jars.clone(),
        );
        config.username = opts.name.clone();
        config.uuid = mcr_auth::offline::offline_uuid(&opts.name)
            .unwrap_or_default()
            .hyphenated()
            .to_string();
        (config.width, config.height) = renderer.size();
        config.compose = true;
        config.http_hosts.extend(plan.http_hosts.iter().cloned());
        config.max_heap = format!("{}m", plan.heap_mib);
        config.saves_dir = Some(opts.game_dir.join("saves"));
        config.aot = std::env::current_exe()
            .ok()
            .map(|helper| mcr_modbridge::AotSettings {
                dir: opts.game_dir.join(AOT_DIR),
                helper,
            });
        let session = &plan.session;
        let tracker = Arc::new(SessionTracker::begin(session.clone()));
        let fail = |e: anyhow::Error| {
            session.reset();
            e
        };
        mcr_modbridge::gpu::install(
            renderer.device().clone(),
            renderer.queue().clone(),
            renderer.adapter_info().clone(),
        )
        .context("sharing the GPU device with Java")
        .map_err(fail)?;
        let mut bridge = Bridge::start(config)
            .context("starting the Fabric bridge")
            .map_err(fail)?;
        let recording = bridge.aot_plan().is_some_and(|p| p.records());
        if let Some(line) = bridge.aot_plan().map(aot_line) {
            eprintln!("[mods] {line}");
        }
        let frames = bridge.frames();
        if let Some(f) = &frames {
            renderer.set_external(Some(Box::new(f.clone())));
        }
        let input = frames.is_some().then(|| {
            let input = bridge.input();
            input.set_clipboard(crate::mod_input::SystemClipboard::open());
            let (caps, num) = crate::mod_input::keyboard_locks();
            input.set_locks(caps, num);
            input
        });
        let net = ModNet {
            link: bridge.net(),
            sound: SoundLink::get().ok(),
            frames,
            input,
            connects: Arc::new(Mutex::new(Vec::new())),
            keys: Arc::new(Mutex::new(Vec::new())),
            worlds: Arc::new(Mutex::new(Vec::new())),
        };
        let connects = net.connects.clone();
        let keys = net.keys.clone();
        let worlds = net.worlds.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let watcher = tracker.clone();
        let thread = std::thread::Builder::new()
            .name("mcr-mods-log".into())
            .spawn(move || {
                while !flag.load(Ordering::Acquire) {
                    watcher.check_hang();
                    if let Some(event) = bridge.next_event(Duration::from_millis(100)) {
                        eprintln!("[mods] {event:?}");
                        watcher.observe(&event);
                        match event {
                            BridgeEvent::ConnectRequested { address, .. } => connects
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .push(address),
                            BridgeEvent::KeysChanged(changes) => {
                                let mut queue = keys.lock().unwrap_or_else(PoisonError::into_inner);
                                let room = MAX_KEY_CHANGES.saturating_sub(queue.len());
                                queue.extend(changes.into_iter().take(room));
                            }
                            BridgeEvent::World(request) => {
                                let mut queue =
                                    worlds.lock().unwrap_or_else(PoisonError::into_inner);
                                if queue.len() < MAX_WORLD_REQUESTS {
                                    queue.push(request);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                bridge
            })?;
        Ok(Self {
            stop,
            thread,
            net,
            tracker,
            recording,
        })
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
        self.tracker.finish(&outcome);
        eprintln!("[mods] shutdown: {outcome:?}");
        if self.recording {
            let kept = matches!(outcome, mcr_modbridge::ShutdownOutcome::Clean { .. });
            eprintln!(
                "[mods] JVM AOT recording {}",
                if kept {
                    "kept; building the cache in the background"
                } else {
                    "dropped: Java did not stop cleanly"
                }
            );
        }
        if outcome.needs_process_exit() {
            std::process::exit(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use mcr_modbridge::manager::{SessionRecord, SessionState};

    use super::*;
    use crate::mods_ui::testing::{manifest, temp, write_jar};

    fn options(dir: &std::path::Path) -> Options {
        Options {
            game_dir: dir.to_owned(),
            ..Options::default()
        }
    }

    fn java_thread() -> bool {
        std::fs::read_dir("/proc/self/task")
            .map(|d| {
                d.flatten().any(|e| {
                    std::fs::read_to_string(e.path().join("comm"))
                        .is_ok_and(|c| c.trim() == "mcr-java")
                })
            })
            .unwrap_or(false)
    }

    #[test]
    fn nothing_enabled_starts_no_jvm() {
        let dir = temp("startup-none");
        write_jar(&dir, "a.jar", &manifest("alpha", ""));
        write_jar(&dir, "s.jar", &manifest("sodium", ""));
        let mut settings = mcr_settings::ModSettings::default();
        settings.set_enabled("alpha", false);
        settings
            .save(&dir.join(mcr_settings::mods::FILE_NAME))
            .unwrap();
        let started = startup(&options(&dir)).unwrap();
        assert!(started.boot.is_none());
        assert!(!java_thread());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn startup_names_mods_left_out_and_caveats() {
        let dir = temp("startup-lines");
        write_jar(&dir, "a.jar", &manifest("alpha", ""));
        write_jar(&dir, "s.jar", &manifest("sodium", ""));
        write_jar(&dir, "c.jar", &manifest("continuity", ""));
        write_jar(
            &dir,
            "b.jar",
            &manifest("beta", r#","depends":{"missing_lib":"*"}"#),
        );
        let manager = Manager::new(&dir, &dir.join("mods"));
        let lines = report_lines(&manager.scan(&manager.load_settings()));
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("sodium 1.0.0: blocked: ")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("beta 1.0.0: excluded: ") && l.contains("missing_lib")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("continuity 1.0.0: Partial: ")),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.starts_with("alpha")), "{lines:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_or_missing_folder_starts_no_jvm() {
        let dir = std::env::temp_dir().join(format!("mcr-startup-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(startup(&options(&dir)).unwrap().boot.is_none());
        assert!(!java_thread());
    }

    #[test]
    fn a_crashed_session_prompts_instead_of_booting() {
        let dir = temp("startup-crash");
        write_jar(&dir, "a.jar", &manifest("alpha", ""));
        let store = Manager::new(&dir, &dir.join("mods")).session();
        store.save(&SessionRecord {
            state: SessionState::Crashed,
            crash_report: None,
        });
        let started = startup(&options(&dir)).unwrap();
        assert!(started.boot.is_none());
        assert!(started.app.take_crash_prompt().is_some());
        assert!(!java_thread());
        assert_eq!(
            store.load().state,
            SessionState::Crashed,
            "the prompt decides, not startup"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stale_crash_with_no_mods_left_is_forgotten() {
        let dir = temp("startup-stale");
        let store = Manager::new(&dir, &dir.join("mods")).session();
        store.save(&SessionRecord {
            state: SessionState::Hung,
            crash_report: None,
        });
        let started = startup(&options(&dir)).unwrap();
        assert!(started.boot.is_none() && started.app.take_crash_prompt().is_none());
        assert_eq!(store.load(), SessionRecord::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disabled_all_wins_over_enabled_mods() {
        let dir = temp("startup-disabled-all");
        write_jar(&dir, "a.jar", &manifest("alpha", ""));
        let mut settings = mcr_settings::ModSettings::default();
        settings.disabled_all = true;
        settings
            .save(&dir.join(mcr_settings::mods::FILE_NAME))
            .unwrap();
        let started = startup(&options(&dir)).unwrap();
        assert!(started.boot.is_none());
        assert!(!java_thread());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
