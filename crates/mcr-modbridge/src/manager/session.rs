use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::{BridgeEvent, ShutdownOutcome};

pub const SESSION_FILE: &str = "last-session.json";
/// A boot that reports nothing for this long counts as hung.
pub const HANG_AFTER: Duration = Duration::from_secs(180);
const MAX_FILE: u64 = 64 << 10;
const MAX_PATH: usize = 1024;
const SUMMARY_READ: u64 = 16 << 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Booting,
    Running,
    Clean,
    Crashed,
    Hung,
}

impl SessionState {
    fn name(self) -> &'static str {
        match self {
            Self::Booting => "booting",
            Self::Running => "running",
            Self::Clean => "clean",
            Self::Crashed => "crashed",
            Self::Hung => "hung",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "booting" => Self::Booting,
            "running" => Self::Running,
            "crashed" => Self::Crashed,
            "hung" => Self::Hung,
            _ => Self::Clean,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub state: SessionState,
    pub crash_report: Option<PathBuf>,
}

impl Default for SessionRecord {
    fn default() -> Self {
        Self {
            state: SessionState::Clean,
            crash_report: None,
        }
    }
}

impl SessionRecord {
    /// A boot that never finished, a crash or a hang. A `running` record means the process
    /// was killed from outside, which is not Java's fault.
    pub fn needs_fallback_prompt(&self) -> bool {
        matches!(
            self.state,
            SessionState::Booting | SessionState::Crashed | SessionState::Hung
        )
    }

    pub fn parse(text: &str) -> Self {
        let Ok(root) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        Self {
            state: SessionState::parse(root["state"].as_str().unwrap_or_default()),
            crash_report: root["crash_report"]
                .as_str()
                .filter(|p| !p.is_empty() && p.len() <= MAX_PATH)
                .map(PathBuf::from),
        }
    }

    pub fn to_text(&self) -> String {
        json!({
            "state": self.state.name(),
            "crash_report": self.crash_report.as_ref().map(|p| p.to_string_lossy()),
        })
        .to_string()
    }

    /// One line from the report (its `Description:` if it has one) for the Mods screen.
    pub fn crash_summary(&self) -> Option<String> {
        let path = self.crash_report.as_ref()?;
        let mut text = String::new();
        std::fs::File::open(path)
            .ok()?
            .take(SUMMARY_READ)
            .read_to_string(&mut text)
            .ok()?;
        let line = text
            .lines()
            .map(str::trim)
            .find_map(|l| l.strip_prefix("Description:").map(str::trim))
            .or_else(|| {
                text.lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty() && !l.starts_with(['-', '#', '/']))
            })?;
        Some(super::scan::clean(line, 160))
    }
}

/// `<data>/java/last-session.json`.
#[derive(Debug, Clone)]
pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("java").join(SESSION_FILE),
        }
    }

    pub fn load(&self) -> SessionRecord {
        let mut text = String::new();
        match std::fs::File::open(&self.path)
            .and_then(|f| f.take(MAX_FILE).read_to_string(&mut text))
        {
            Ok(_) => SessionRecord::parse(&text),
            Err(_) => SessionRecord::default(),
        }
    }

    pub fn save(&self, record: &SessionRecord) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&self.path, record.to_text()) {
            tracing::warn!(path = %self.path.display(), "cannot write the session record: {e}");
        }
    }

    pub fn reset(&self) {
        self.save(&SessionRecord::default());
    }
}

/// Follows one Java session and keeps `last-session.json` current.
#[derive(Debug)]
pub struct SessionTracker {
    store: SessionStore,
    started: Instant,
    shared: Arc<Mutex<SessionRecord>>,
}

impl SessionTracker {
    /// Records `booting` before the VM starts, so a boot that dies leaves its mark.
    pub fn begin(store: SessionStore) -> Self {
        let record = SessionRecord {
            state: SessionState::Booting,
            crash_report: None,
        };
        store.save(&record);
        Self {
            store,
            started: Instant::now(),
            shared: Arc::new(Mutex::new(record)),
        }
    }

    pub fn handle(&self) -> Arc<Mutex<SessionRecord>> {
        self.shared.clone()
    }

    fn update(&self, edit: impl FnOnce(&mut SessionRecord) -> bool) {
        let mut record = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if edit(&mut record) {
            self.store.save(&record);
        }
    }

    pub fn observe(&self, event: &BridgeEvent) {
        match event {
            BridgeEvent::Screen(_) | BridgeEvent::Initialized(_) => self.update(|r| {
                let booting = r.state == SessionState::Booting;
                if booting {
                    r.state = SessionState::Running;
                }
                booting
            }),
            BridgeEvent::Crash { report } => self.update(|r| {
                r.state = SessionState::Crashed;
                r.crash_report.clone_from(report);
                true
            }),
            BridgeEvent::VmExited { code } if *code != 0 => self.update(|r| {
                r.state = SessionState::Crashed;
                true
            }),
            _ => {}
        }
    }

    /// Called from the event loop; marks a silent boot as hung.
    pub fn check_hang(&self) {
        if self.started.elapsed() > HANG_AFTER {
            self.update(|r| {
                let booting = r.state == SessionState::Booting;
                if booting {
                    r.state = SessionState::Hung;
                }
                booting
            });
        }
    }

    pub fn finish(&self, outcome: &ShutdownOutcome) {
        self.update(|r| {
            if matches!(r.state, SessionState::Crashed | SessionState::Hung) {
                return false;
            }
            r.state = match outcome {
                ShutdownOutcome::Clean { .. } | ShutdownOutcome::Exited { code: 0 } => {
                    SessionState::Clean
                }
                ShutdownOutcome::TimedOut => SessionState::Hung,
                _ => SessionState::Crashed,
            };
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mcr-session-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn records_round_trip_and_garbage_is_clean() {
        let r = SessionRecord {
            state: SessionState::Crashed,
            crash_report: Some(PathBuf::from("/x/crash.txt")),
        };
        assert_eq!(SessionRecord::parse(&r.to_text()), r);
        for junk in ["", "{", "[]", r#"{"state":"weird"}"#] {
            assert_eq!(
                SessionRecord::parse(junk),
                SessionRecord::default(),
                "{junk}"
            );
        }
    }

    #[test]
    fn only_boot_crash_and_hang_prompt() {
        for (state, prompt) in [
            (SessionState::Booting, true),
            (SessionState::Crashed, true),
            (SessionState::Hung, true),
            (SessionState::Running, false),
            (SessionState::Clean, false),
        ] {
            let r = SessionRecord {
                state,
                crash_report: None,
            };
            assert_eq!(r.needs_fallback_prompt(), prompt, "{state:?}");
        }
    }

    #[test]
    fn a_boot_that_dies_leaves_booting_on_disk() {
        let dir = temp("boot");
        let store = SessionStore::new(&dir);
        let tracker = SessionTracker::begin(store.clone());
        assert_eq!(store.load().state, SessionState::Booting);
        assert!(store.load().needs_fallback_prompt());
        tracker.observe(&BridgeEvent::Screen("Title".into()));
        assert_eq!(store.load().state, SessionState::Running);
        tracker.finish(&ShutdownOutcome::Clean {
            elapsed: Duration::ZERO,
        });
        assert_eq!(store.load(), SessionRecord::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_crash_keeps_its_report_through_shutdown() {
        let dir = temp("crash");
        let store = SessionStore::new(&dir);
        let tracker = SessionTracker::begin(store.clone());
        tracker.observe(&BridgeEvent::Crash {
            report: Some(PathBuf::from("crash-reports/c.txt")),
        });
        tracker.finish(&ShutdownOutcome::Crashed);
        let record = store.load();
        assert_eq!(record.state, SessionState::Crashed);
        assert_eq!(
            record.crash_report,
            Some(PathBuf::from("crash-reports/c.txt"))
        );
        assert_eq!(*tracker.handle().lock().unwrap(), record);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exit_codes_and_timeouts_map_to_states() {
        for (outcome, state) in [
            (ShutdownOutcome::Exited { code: 0 }, SessionState::Clean),
            (ShutdownOutcome::Exited { code: 7 }, SessionState::Crashed),
            (ShutdownOutcome::TimedOut, SessionState::Hung),
        ] {
            let dir = temp("exit");
            let tracker = SessionTracker::begin(SessionStore::new(&dir));
            tracker.finish(&outcome);
            assert_eq!(tracker.handle().lock().unwrap().state, state);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn crash_summary_prefers_the_description() {
        let dir = temp("summary");
        let report = dir.join("crash.txt");
        std::fs::write(
            &report,
            "---- Minecraft Crash Report ----\n// oops\n\nTime: now\nDescription: Mod `x` failed\x07 in init\n\nstack\n",
        )
        .unwrap();
        let r = SessionRecord {
            state: SessionState::Crashed,
            crash_report: Some(report),
        };
        assert_eq!(r.crash_summary().as_deref(), Some("Mod `x` failed in init"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
