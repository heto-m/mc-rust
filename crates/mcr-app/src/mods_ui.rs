//! Glue between the mod manager (`mcr-modbridge::manager`) and the native Mods screen.

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Child, Command};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use mcr_modbridge::manager::{
    Icon, Manager, ModEntry, ModStatus, PolicyStatus, Report, SessionRecord, SessionState,
};
use mcr_settings::ModSettings;
use mcr_ui::{
    IconCache, ModIcon, ModRow, ModTone, ModsModel, ModsScreen, PromptAction, Screen, ScreenAction,
    SharedMods, crash_fallback_screen,
};

fn icon_id(icon: &Icon) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for b in icon.rgba.iter().chain(&icon.width.to_le_bytes()) {
        hash = (hash ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn row(entry: &ModEntry) -> ModRow {
    let policy = entry
        .policy
        .as_ref()
        .filter(|p| p.status != PolicyStatus::Blocked);
    let (badge, tone, reason) = match (&entry.status, policy) {
        (ModStatus::Active, Some(p)) if p.status == PolicyStatus::Partial => {
            ("Partial", ModTone::Warn, p.reason.clone())
        }
        (ModStatus::Active, Some(p)) => ("Unsupported", ModTone::Warn, p.reason.clone()),
        (ModStatus::Active, None) => ("Active", ModTone::Good, String::new()),
        (ModStatus::Disabled, Some(p)) if p.status == PolicyStatus::Unsupported => {
            ("Unsupported", ModTone::Warn, p.reason.clone())
        }
        (ModStatus::Disabled, _) => ("Disabled", ModTone::Neutral, String::new()),
        (ModStatus::Blocked(why), _) => ("Blocked", ModTone::Bad, why.clone()),
        (ModStatus::Excluded(why), _) => ("Not loaded", ModTone::Bad, why.clone()),
        (ModStatus::ServerOnly, _) => (
            "Server only",
            ModTone::Neutral,
            "not loaded on the client".to_owned(),
        ),
        (ModStatus::Unsupported(why), _) => {
            let label = if why.starts_with("unreadable") {
                "Invalid"
            } else {
                "Not supported"
            };
            (label, ModTone::Bad, why.clone())
        }
    };
    ModRow {
        id: entry.info.id.clone(),
        name: entry.info.name.clone(),
        version: entry.info.version.clone(),
        description: entry.info.description.clone(),
        badge: badge.to_owned(),
        tone,
        reason,
        enabled: matches!(entry.status, ModStatus::Active | ModStatus::Excluded(_)),
        toggleable: entry.toggleable(),
        icon: entry.info.icon.as_deref().map(|i| {
            Rc::new(ModIcon {
                id: icon_id(i),
                width: i.width,
                height: i.height,
                rgba: i.rgba.clone(),
            })
        }),
    }
}

pub fn open_path(path: &Path) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

#[derive(Debug)]
struct Controller {
    manager: Manager,
    settings: ModSettings,
    report: Report,
    running: Vec<String>,
    running_heap: u32,
    live: Option<Arc<Mutex<SessionRecord>>>,
    previous: SessionRecord,
}

impl Controller {
    fn save(&self) {
        if let Err(e) = std::fs::create_dir_all(&self.manager.data_dir)
            .and_then(|()| self.settings.save(&self.manager.settings_path()))
        {
            eprintln!("[mods] cannot save mod settings: {e}");
        }
    }

    fn session(&self) -> SessionRecord {
        match &self.live {
            Some(live) => live.lock().unwrap_or_else(PoisonError::into_inner).clone(),
            None => self.previous.clone(),
        }
    }
}

impl ModsModel for Controller {
    fn refresh(&mut self) -> Vec<ModRow> {
        self.report = self.manager.scan(&self.settings);
        self.report.entries.iter().map(row).collect()
    }

    fn set_enabled(&mut self, id: &str, on: bool) {
        self.settings.set_enabled(id, on);
        if on {
            self.settings.disabled_all = false;
        }
        self.save();
    }

    fn heap_mib(&self) -> u32 {
        self.settings.heap_mib()
    }

    fn set_heap_mib(&mut self, mib: u32) {
        self.settings.set_heap_mib(mib);
        self.save();
    }

    fn heap_range(&self) -> (u32, u32) {
        (
            mcr_settings::mods::MIN_HEAP_MIB,
            mcr_settings::mods::MAX_HEAP_MIB,
        )
    }

    fn restart_required(&self) -> bool {
        let heap_changed =
            !self.running.is_empty() && self.settings.heap_mib() != self.running_heap;
        self.report.active_ids() != self.running || heap_changed
    }

    fn notice(&self) -> Option<String> {
        let record = self.session();
        if matches!(record.state, SessionState::Crashed | SessionState::Hung) {
            let what = if record.state == SessionState::Hung {
                "Java hung"
            } else {
                "Java crashed"
            };
            let detail = record.crash_summary();
            let path = record
                .crash_report
                .as_ref()
                .map(|p| p.display().to_string());
            return Some(match (detail, path) {
                (Some(d), Some(p)) => format!("{what}: {d} ({p})"),
                (None, Some(p)) => format!("{what}; report: {p}"),
                _ => what.to_owned(),
            });
        }
        self.settings
            .disabled_all
            .then(|| "Mods are off after a crash. Turn one on to use mods again.".to_owned())
    }

    fn open_folder(&mut self) {
        let dir = &self.manager.mods_dir;
        if std::fs::create_dir_all(dir).is_ok() {
            open_path(dir);
        }
    }

    fn folder_label(&self) -> String {
        self.manager.mods_dir.display().to_string()
    }
}

/// What the shell holds: the Mods screen model, its icon cache and the one-time crash prompt.
#[derive(Clone)]
pub struct ModsApp {
    model: Rc<RefCell<Controller>>,
    icons: IconCache,
    prompt: Rc<RefCell<Option<SessionRecord>>>,
}

impl std::fmt::Debug for ModsApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModsApp").finish_non_exhaustive()
    }
}

impl ModsApp {
    pub fn new(
        manager: Manager,
        settings: ModSettings,
        previous: SessionRecord,
        prompt: bool,
    ) -> Self {
        let running_heap = settings.heap_mib();
        let report = manager.scan(&settings);
        let prompt = prompt.then(|| previous.clone());
        Self {
            model: Rc::new(RefCell::new(Controller {
                manager,
                settings,
                report,
                running: Vec::new(),
                running_heap,
                live: None,
                previous,
            })),
            icons: IconCache::default(),
            prompt: Rc::new(RefCell::new(prompt)),
        }
    }

    /// Records the mods Java started with and follows its session.
    pub fn set_running(&self, ids: Vec<String>, live: Arc<Mutex<SessionRecord>>) {
        let mut model = self.model.borrow_mut();
        model.running = ids;
        model.live = Some(live);
    }

    pub fn screen(&self) -> ModsScreen {
        let model: SharedMods = self.model.clone();
        ModsScreen::with_cache(model, self.icons.clone())
    }

    /// "Start without mods": none load from now on, until the user turns one on.
    fn start_without_mods(&self) {
        let mut model = self.model.borrow_mut();
        model.settings.disabled_all = true;
        model.save();
        model.manager.session().reset();
    }

    fn reset_session(&self) {
        self.model.borrow().manager.session().reset();
    }

    /// The prompt for a previous crash or hang, once.
    pub fn take_crash_prompt(&self) -> Option<Box<dyn Screen>> {
        let record = self.prompt.borrow_mut().take()?;
        let summary = record.crash_summary().unwrap_or_default();
        let open_report = record.crash_report.clone().map(|path| {
            Box::new(move || {
                open_path(&path);
                Vec::new()
            }) as PromptAction
        });
        let report = record.crash_report.map(|p| p.display().to_string());
        let (without, retry) = (self.clone(), self.clone());
        Some(Box::new(crash_fallback_screen(
            &summary,
            report.as_deref(),
            Box::new(move || {
                without.start_without_mods();
                vec![ScreenAction::Pop]
            }),
            Box::new(move || {
                retry.reset_session();
                match relaunch() {
                    Ok(_) => vec![ScreenAction::Quit],
                    Err(e) => {
                        eprintln!("[mods] could not restart mcr: {e}");
                        vec![ScreenAction::Pop]
                    }
                }
            }),
            open_report,
        )))
    }
}

fn relaunch_command(exe: &Path, args: impl IntoIterator<Item = OsString>) -> Command {
    let mut command = Command::new(exe);
    command.args(args);
    command
}

/// A fresh mcr with this process's arguments; the caller quits so the new one boots Java.
fn relaunch() -> std::io::Result<Child> {
    relaunch_command(&std::env::current_exe()?, std::env::args_os().skip(1)).spawn()
}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::Path;

    pub fn temp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mcr-modsui-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        dir
    }

    pub fn write_jar(dir: &Path, name: &str, manifest: &str) {
        use std::io::Write;
        let file = std::fs::File::create(dir.join("mods").join(name)).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    pub fn manifest(id: &str, extra: &str) -> String {
        format!(r#"{{"schemaVersion":1,"id":"{id}","version":"1.0.0","name":"{id}"{extra}}}"#)
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{manifest, temp, write_jar};
    use super::*;

    #[test]
    fn rows_show_status_reasons_and_toggles() {
        let dir = temp("rows");
        write_jar(&dir, "a.jar", &manifest("alpha", ""));
        write_jar(&dir, "s.jar", &manifest("sodium", ""));
        write_jar(&dir, "f.jar", &manifest("freecam", ""));
        write_jar(
            &dir,
            "n.jar",
            &manifest("needy", r#","depends":{"foo":"*"}"#),
        );
        let manager = Manager::new(&dir, &dir.join("mods"));
        let settings = manager.load_settings();
        let app = ModsApp::new(manager, settings, SessionRecord::default(), false);
        let rows = app.model.borrow_mut().refresh();
        let by = |id: &str| rows.iter().find(|r| r.id == id).unwrap().clone();
        assert_eq!(by("alpha").badge, "Active");
        assert!(by("alpha").enabled && by("alpha").toggleable);
        let sodium = by("sodium");
        assert_eq!(
            (sodium.badge.as_str(), sodium.tone),
            ("Blocked", ModTone::Bad)
        );
        assert!(sodium.reason.contains("OpenGL") && !sodium.toggleable && !sodium.enabled);
        let freecam = by("freecam");
        assert_eq!(freecam.badge, "Unsupported");
        assert!(!freecam.enabled && freecam.toggleable && !freecam.reason.is_empty());
        let needy = by("needy");
        assert_eq!(needy.badge, "Not loaded");
        assert_eq!(needy.reason, "missing dependency `foo`");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn toggling_persists_and_raises_the_restart_banner() {
        let dir = temp("toggle");
        write_jar(&dir, "a.jar", &manifest("alpha", ""));
        write_jar(&dir, "b.jar", &manifest("beta", ""));
        let running = vec!["alpha".to_owned(), "beta".to_owned()];
        let manager = Manager::new(&dir, &dir.join("mods"));
        let settings = manager.load_settings();
        let app = ModsApp::new(manager, settings, SessionRecord::default(), false);
        app.set_running(running, Default::default());
        assert!(!app.model.borrow().restart_required());
        app.model.borrow_mut().set_enabled("beta", false);
        app.model.borrow_mut().refresh();
        assert!(app.model.borrow().restart_required());
        let saved = ModSettings::load(&dir.join(mcr_settings::mods::FILE_NAME));
        assert!(!saved.is_enabled("beta", true));
        app.model.borrow_mut().set_enabled("beta", true);
        app.model.borrow_mut().refresh();
        assert!(!app.model.borrow().restart_required());
        app.model.borrow_mut().set_heap_mib(3072);
        assert!(app.model.borrow().restart_required());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enabling_a_mod_clears_the_crash_fallback_flag() {
        let dir = temp("fallback");
        write_jar(&dir, "a.jar", &manifest("alpha", ""));
        let manager = Manager::new(&dir, &dir.join("mods"));
        let mut settings = manager.load_settings();
        settings.disabled_all = true;
        let app = ModsApp::new(manager, settings, SessionRecord::default(), false);
        assert!(
            app.model
                .borrow()
                .notice()
                .unwrap()
                .contains("off after a crash")
        );
        assert!(app.model.borrow_mut().refresh()[0].badge == "Disabled");
        app.model.borrow_mut().set_enabled("alpha", true);
        assert!(!app.model.borrow().settings.disabled_all);
        assert!(app.model.borrow().notice().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_notice_names_the_report() {
        let dir = temp("notice");
        let previous = SessionRecord {
            state: SessionState::Crashed,
            crash_report: Some(dir.join("crash.txt")),
        };
        std::fs::write(dir.join("crash.txt"), "Description: Mod x broke\n").unwrap();
        let manager = Manager::new(&dir, &dir.join("mods"));
        let settings = manager.load_settings();
        let app = ModsApp::new(manager, settings, previous, true);
        let notice = app.model.borrow().notice().unwrap();
        assert!(
            notice.contains("Mod x broke") && notice.contains("crash.txt"),
            "{notice}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relaunch_keeps_the_arguments() {
        let command = relaunch_command(
            Path::new("/usr/bin/mcr"),
            ["--connect", "localhost"].map(OsString::from),
        );
        assert_eq!(command.get_program(), "/usr/bin/mcr");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["--connect", "localhost"]);
    }

    #[test]
    fn the_crash_prompt_choices_update_settings_and_session() {
        let dir = temp("prompt");
        write_jar(&dir, "a.jar", &manifest("alpha", ""));
        let manager = Manager::new(&dir, &dir.join("mods"));
        let store = manager.session();
        let record = SessionRecord {
            state: SessionState::Booting,
            crash_report: None,
        };
        store.save(&record);
        let settings = manager.load_settings();
        let app = ModsApp::new(manager, settings, record, true);
        assert!(app.take_crash_prompt().is_some());
        assert!(app.take_crash_prompt().is_none(), "the prompt shows once");
        assert!(store.load().needs_fallback_prompt());
        app.start_without_mods();
        assert!(!store.load().needs_fallback_prompt());
        assert!(ModSettings::load(&dir.join(mcr_settings::mods::FILE_NAME)).disabled_all);
        store.save(&SessionRecord {
            state: SessionState::Hung,
            crash_report: None,
        });
        app.reset_session();
        assert_eq!(store.load(), SessionRecord::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
