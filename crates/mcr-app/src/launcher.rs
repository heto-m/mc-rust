//! The launcher before the title screen: [`LauncherApp`] is the model behind mcr-ui's launcher
//! screens, [`scene::Launcher`] hosts them, and [`apply`] turns Play into run options.

pub mod scene;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use anyhow::{Result, bail};
use mcr_auth::UreqClient;
use mcr_auth::msa::ClientId;
use mcr_launcher::{
    AccountEntry, AccountKind, LATEST_RELEASE, LaunchAccount, LaunchPlan, ManifestCache, Profile,
    Resolution, SignIn, SignInEvent, Store, VersionKind, VersionList, now_secs,
};
use mcr_settings::mods::{MAX_HEAP_MIB, MIN_HEAP_MIB};
use mcr_ui::{
    AccountRow, LaunchView, LauncherModel, LauncherProfileRow, ProfileDraft, SignInView, VersionRow,
};

use crate::cli::Options;

enum ManifestState {
    Fetching,
    Fresh,
    Failed,
}

/// The launcher's state over `mcr-launcher`, with its network work on background threads.
pub struct LauncherApp {
    store: Store,
    versions: VersionList,
    manifest: Option<Receiver<Result<VersionList, String>>>,
    manifest_state: ManifestState,
    sign_in: Option<SignIn>,
    sign_in_view: SignInView,
    launching: Option<Receiver<Result<LaunchPlan, String>>>,
    launch_view: LaunchView,
    plan: Option<LaunchPlan>,
}

impl std::fmt::Debug for LauncherApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LauncherApp").finish_non_exhaustive()
    }
}

impl LauncherApp {
    /// Reads the launcher file and the cached manifest, and starts fetching a fresh manifest.
    pub fn open(data_dir: &Path) -> Self {
        let cache = ManifestCache::new(data_dir);
        let versions = cache.load().unwrap_or_default();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = mcr_singleplayer::provision::fetch_manifest()
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    cache
                        .store(&bytes)
                        .ok_or_else(|| "the version manifest is malformed".to_owned())
                });
            let _ = tx.send(result);
        });
        Self {
            store: Store::open(data_dir),
            versions,
            manifest: Some(rx),
            manifest_state: ManifestState::Fetching,
            sign_in: None,
            sign_in_view: SignInView::Idle,
            launching: None,
            launch_view: LaunchView::Idle,
            plan: None,
        }
    }

    fn save(&self) {
        if let Err(e) = self.store.save() {
            eprintln!(
                "launcher: could not save {}: {e}",
                self.store.path().display()
            );
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.store.data_dir().to_owned()
    }

    /// The plan Play produced, once the account is ready.
    pub fn take_plan(&mut self) -> Option<LaunchPlan> {
        self.plan.take()
    }

    fn poll_manifest(&mut self) {
        let Some(rx) = &self.manifest else { return };
        match rx.try_recv() {
            Ok(Ok(list)) => {
                self.versions = list;
                self.manifest_state = ManifestState::Fresh;
                self.manifest = None;
            }
            Ok(Err(e)) => {
                eprintln!("launcher: version manifest: {e}");
                self.manifest_state = ManifestState::Failed;
                self.manifest = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.manifest = None,
        }
    }

    fn poll_sign_in(&mut self) {
        let Some(sign) = &mut self.sign_in else {
            return;
        };
        let Some(event) = sign.poll() else { return };
        match event {
            SignInEvent::Code {
                user_code,
                verification_uri,
                ..
            } => {
                self.sign_in_view = SignInView::Code {
                    user_code,
                    url: verification_uri,
                };
            }
            SignInEvent::Done(entry) => {
                println!("launcher: signed in as {}", entry.name);
                self.sign_in_view = SignInView::Done(entry.name.clone());
                self.store.file.add_account(entry);
                self.save();
                self.sign_in = None;
            }
            SignInEvent::Failed(reason) => {
                self.sign_in_view = SignInView::Failed(reason);
                self.sign_in = None;
            }
        }
    }

    fn poll_launch(&mut self) {
        let Some(rx) = &self.launching else { return };
        match rx.try_recv() {
            Ok(Ok(plan)) => {
                self.launching = None;
                self.finish_launch(plan);
            }
            Ok(Err(e)) => {
                self.launching = None;
                self.launch_view = LaunchView::Failed(e);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.launching = None;
                self.launch_view = LaunchView::Failed("sign-in stopped".into());
            }
        }
    }

    fn finish_launch(&mut self, plan: LaunchPlan) {
        self.store.file.touch(&plan.profile_id, now_secs());
        self.save();
        self.launch_view = LaunchView::Working("Starting...".into());
        self.plan = Some(plan);
    }

    fn row(&self, p: &Profile) -> LauncherProfileRow {
        let playable = self.versions.resolve(&p.version).is_some();
        LauncherProfileRow {
            id: p.id.clone(),
            name: p.name.clone(),
            icon: p.icon.clone(),
            version: self.version_label(&p.version),
            playable,
            note: if playable {
                String::new()
            } else {
                "not supported yet".into()
            },
            last_played: last_played(p.last_played, now_secs()),
            mods: p.mods.is_some(),
            default: p.is_default(),
        }
    }
}

fn last_played(at: u64, now: u64) -> String {
    if at == 0 {
        return "Never played".into();
    }
    let ago = now.saturating_sub(at);
    let (n, unit) = match ago {
        0..60 => return "Played just now".into(),
        60..3600 => (ago / 60, "minute"),
        3600..86_400 => (ago / 3600, "hour"),
        _ => (ago / 86_400, "day"),
    };
    format!("Played {n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

fn path_text(path: Option<&Path>) -> String {
    path.map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn path_field(text: &str) -> Option<PathBuf> {
    let text = text.trim();
    (!text.is_empty()).then(|| PathBuf::from(text))
}

fn client_id() -> Option<ClientId> {
    ClientId::from_env().ok()
}

impl LauncherModel for LauncherApp {
    fn profiles(&self) -> Vec<LauncherProfileRow> {
        self.store
            .file
            .profiles
            .iter()
            .map(|p| self.row(p))
            .collect()
    }

    fn selected(&self) -> String {
        self.store.file.selected.clone()
    }

    fn select(&mut self, id: &str) {
        if self.store.file.select(id) {
            self.save();
        }
    }

    fn can_add(&self) -> bool {
        self.store.file.can_add()
    }

    fn draft(&self, id: Option<&str>) -> ProfileDraft {
        let fresh = Profile::new("", "New profile");
        let p = id
            .and_then(|id| self.store.file.profile(id))
            .unwrap_or(&fresh);
        ProfileDraft {
            id: id.and(Some(p.id.clone())),
            name: p.name.clone(),
            icon: p.icon.clone(),
            version: p.version.clone(),
            game_dir: path_text(p.game_dir.as_deref()),
            mods: path_text(p.mods.as_deref()),
            heap_mib: p.heap_mib,
            resolution: p.resolution.map(|r| r.to_string()).unwrap_or_default(),
        }
    }

    fn save_profile(&mut self, draft: ProfileDraft) -> Result<(), String> {
        let name = draft.name.trim();
        if name.is_empty() {
            return Err("The profile needs a name".into());
        }
        let resolution = match draft.resolution.trim() {
            "" => None,
            text => Some(Resolution::parse(text).ok_or("Resolution must look like 1280x720")?),
        };
        if draft.version != LATEST_RELEASE && self.versions.resolve(&draft.version).is_none() {
            return Err(format!("{} is not supported yet", draft.version));
        }
        let mods = path_field(&draft.mods);
        if let Some(dir) = &mods
            && !dir.is_dir()
        {
            return Err("The mods folder does not exist".into());
        }
        let mut profile = draft
            .id
            .as_deref()
            .and_then(|id| self.store.file.profile(id).cloned())
            .unwrap_or_else(|| Profile::new("", ""));
        profile.name = name.to_owned();
        profile.icon = draft.icon;
        profile.version = draft.version;
        profile.game_dir = path_field(&draft.game_dir);
        profile.mods = mods;
        profile.heap_mib = draft.heap_mib;
        profile.resolution = resolution;
        let ok = if draft.id.is_some() {
            self.store.file.update_profile(profile)
        } else {
            self.store.file.add_profile(profile).is_some()
        };
        if !ok {
            return Err("The profile could not be saved".into());
        }
        self.save();
        Ok(())
    }

    fn duplicate(&mut self, id: &str) {
        if self.store.file.duplicate(id).is_some() {
            self.save();
        }
    }

    fn delete(&mut self, id: &str) {
        if self.store.file.delete(id) {
            self.save();
        }
    }

    fn icons(&self) -> Vec<String> {
        mcr_launcher::ICONS
            .iter()
            .map(|s| (*s).to_owned())
            .collect()
    }

    fn versions(&self) -> Vec<VersionRow> {
        let snapshots = self.store.file.show_snapshots;
        std::iter::once(VersionRow {
            id: LATEST_RELEASE.into(),
            label: self.version_label(LATEST_RELEASE),
            snapshot: false,
            supported: true,
        })
        .chain(self.versions.visible(snapshots).map(|e| VersionRow {
            id: e.id.clone(),
            label: e.id.clone(),
            snapshot: e.kind == VersionKind::Snapshot,
            supported: e.supported,
        }))
        .collect()
    }

    fn version_label(&self, version: &str) -> String {
        if version == LATEST_RELEASE {
            format!(
                "Latest release ({})",
                self.versions.latest_supported_release()
            )
        } else {
            version.to_owned()
        }
    }

    fn show_snapshots(&self) -> bool {
        self.store.file.show_snapshots
    }

    fn set_show_snapshots(&mut self, on: bool) {
        self.store.file.show_snapshots = on;
        self.save();
    }

    fn heap_range(&self) -> (u32, u32) {
        (MIN_HEAP_MIB, MAX_HEAP_MIB)
    }

    fn default_game_dir(&self) -> String {
        self.store
            .data_dir()
            .join("client")
            .to_string_lossy()
            .into_owned()
    }

    fn accounts(&self) -> Vec<AccountRow> {
        let active = self.store.file.active_account.clone();
        self.store
            .file
            .accounts
            .iter()
            .map(|a| AccountRow {
                key: a.key(),
                name: a.name.clone(),
                microsoft: a.kind == AccountKind::Microsoft,
                active: active.as_deref() == Some(a.key().as_str()),
            })
            .collect()
    }

    fn set_active_account(&mut self, key: &str) {
        if self.store.file.set_active(key) {
            self.save();
        }
    }

    fn offline_problem(&self, name: &str) -> Option<String> {
        AccountEntry::offline(name).err().map(|e| e.to_string())
    }

    fn add_offline(&mut self, name: &str) {
        if let Ok(entry) = AccountEntry::offline(name)
            && self.store.file.add_account(entry)
        {
            self.save();
        }
    }

    fn remove_account(&mut self, key: &str) {
        if let Some(entry) = self.store.file.remove_account(key) {
            if let Err(e) = entry.forget(&self.data_dir()) {
                eprintln!("launcher: could not remove the saved login: {e}");
            }
            self.save();
        }
    }

    fn microsoft_problem(&self) -> Option<String> {
        client_id().is_none().then(|| {
            format!(
                "Set {} to sign in with Microsoft",
                mcr_auth::msa::CLIENT_ID_ENV
            )
        })
    }

    fn start_sign_in(&mut self) {
        let Some(id) = client_id() else {
            self.sign_in_view = SignInView::Failed(self.microsoft_problem().unwrap_or_default());
            return;
        };
        self.sign_in = Some(SignIn::start(
            UreqClient::new(),
            id,
            self.data_dir(),
            now_secs(),
        ));
        self.sign_in_view = SignInView::Starting;
    }

    fn sign_in(&mut self) -> SignInView {
        self.poll_sign_in();
        self.sign_in_view.clone()
    }

    fn cancel_sign_in(&mut self) {
        if let Some(sign) = self.sign_in.take() {
            sign.cancel();
        }
        self.sign_in_view = SignInView::Idle;
    }

    fn play(&mut self) {
        if self.launching.is_some() || self.plan.is_some() {
            return;
        }
        let profile = self.store.file.selected_profile().clone();
        let Some(account) = self.store.file.active().cloned() else {
            self.launch_view = LaunchView::Failed("Add an account first".into());
            return;
        };
        let offline = LaunchAccount {
            name: account.name.clone(),
            online: None,
        };
        let plan = match LaunchPlan::new(&profile, &self.versions, self.store.data_dir(), offline) {
            Ok(plan) => plan,
            Err(e) => {
                self.launch_view = LaunchView::Failed(e.to_string());
                return;
            }
        };
        if account.kind == AccountKind::Offline {
            self.finish_launch(plan);
            return;
        }
        self.launch_view = LaunchView::Working(format!("Signing in as {}...", account.name));
        let data_dir = self.data_dir();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = account
                .launch_account(UreqClient::new(), client_id(), &data_dir, now_secs())
                .map(|account| LaunchPlan { account, ..plan })
                .map_err(|e| match e {
                    mcr_auth::AuthError::RefreshRejected | mcr_auth::AuthError::NotSignedIn => {
                        format!("{}: sign in again in Accounts", account.name)
                    }
                    other => format!("Sign-in failed: {other}"),
                });
            let _ = tx.send(result);
        });
        self.launching = Some(rx);
    }

    fn launch(&mut self) -> LaunchView {
        self.poll_launch();
        self.launch_view.clone()
    }

    fn notice(&self) -> Option<String> {
        if let Some(newer) = self.versions.newer_unsupported_release() {
            return Some(format!(
                "Minecraft {newer} is out; mcr plays {} for now",
                self.versions.latest_supported_release()
            ));
        }
        match &self.manifest_state {
            ManifestState::Fetching if !self.versions.from_manifest => {
                Some("Fetching the version list...".into())
            }
            ManifestState::Failed if self.versions.from_manifest => {
                Some("Offline: using the cached version list".into())
            }
            ManifestState::Failed => Some("Offline: the version list is unavailable".into()),
            _ => None,
        }
    }

    fn tick(&mut self) {
        self.poll_manifest();
        self.poll_sign_in();
        self.poll_launch();
    }
}

/// Applies Play's plan to the run; flags given on the command line win.
pub fn apply(opts: &mut Options, plan: LaunchPlan) -> Result<()> {
    if !opts.game_dir_set {
        opts.game_dir = plan.game_dir.clone();
    }
    if opts.mods.is_none() {
        opts.mods = plan.mods.clone();
        opts.vanilla = plan.mods.is_none();
    }
    if opts.mods.is_some() {
        opts.heap_mib = Some(plan.heap_mib);
    }
    if !opts.jar_set {
        opts.jar = plan.client_jar(crate::mods::cache_dir());
    }
    if !opts.name_set {
        opts.name = plan.account.name.clone();
        opts.online = plan.account.online.map(|s| mcr_client::OnlineAuth {
            access_token: s.access_token,
            profile_id: s.profile_id,
            keys: s.keys,
            services: s.services,
            refresh: s.refresh,
        });
    }
    if !opts.size_set
        && let Some(r) = plan.resolution
    {
        opts.size = (r.width, r.height);
    }
    opts.absolutize()?;
    println!(
        "launcher: profile {} plays {} as {}{} in {}",
        plan.profile_id,
        plan.version,
        opts.name,
        if opts.online.is_some() {
            " (online)"
        } else {
            ""
        },
        opts.game_dir.display()
    );
    Ok(())
}

/// The arguments of the process that shows the launcher again after Java ran: same data dir, and an
/// offscreen run stays offscreen.
pub fn relaunch_args(opts: &Options) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = vec![
        "--launcher".into(),
        "--data-dir".into(),
        opts.data_dir.clone().into(),
    ];
    if let Some(out) = &opts.screenshot {
        args.extend([
            "--screenshot".into(),
            out.clone().into(),
            "--size".into(),
            format!("{}x{}", opts.size.0, opts.size.1).into(),
        ]);
    }
    args
}

static START_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Java moves the process into its game dir, so a relaunch goes back to where mcr was started.
pub fn remember_start_dir() {
    if let Ok(dir) = std::env::current_dir() {
        let _ = START_DIR.set(dir);
    }
}

/// Replaces this process with a fresh `mcr --launcher`: a JVM cannot be created twice in one process.
pub fn relaunch(opts: &Options) -> Result<()> {
    let exe = std::env::current_exe()?;
    let mut command = std::process::Command::new(exe);
    command.args(relaunch_args(opts));
    if let Some(dir) = START_DIR.get() {
        command.current_dir(dir);
    }
    println!("launcher: restarting mcr for the launcher");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec().into())
    }
    #[cfg(not(unix))]
    {
        command.spawn()?;
        Ok(())
    }
}

/// `--profile <name>`: applies that profile and the active account without showing the launcher.
pub fn apply_profile(opts: &mut Options, name: &str) -> Result<()> {
    let store = Store::open(&opts.data_dir);
    let Some(profile) = store.file.find_by_name(name).cloned() else {
        let names: Vec<&str> = store
            .file
            .profiles
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        bail!("no profile named {name:?}; profiles: {}", names.join(", "));
    };
    let versions = ManifestCache::new(&opts.data_dir)
        .load()
        .unwrap_or_default();
    let account = match store.file.active() {
        Some(entry) => entry
            .launch_account(UreqClient::new(), client_id(), &opts.data_dir, now_secs())
            .map_err(|e| anyhow::anyhow!("account {}: {e}", entry.name))?,
        None => LaunchAccount {
            name: opts.name.clone(),
            online: None,
        },
    };
    let plan = LaunchPlan::new(&profile, &versions, &opts.data_dir, account)?;
    let mut store = store;
    store.file.touch(&profile.id, now_secs());
    if let Err(e) = store.save() {
        eprintln!("launcher: could not save {}: {e}", store.path().display());
    }
    apply(opts, plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relaunch_keeps_the_data_dir_and_offscreen_runs() {
        let mut opts = crate::cli::parse(
            ["--data-dir", "/d", "--connect", "h", "--mods", "/m"]
                .iter()
                .map(|s| s.to_string()),
        )
        .unwrap();
        let args = |o: &Options| -> Vec<String> {
            relaunch_args(o)
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(args(&opts), ["--launcher", "--data-dir", "/d"]);
        opts.screenshot = Some("/o.png".into());
        opts.size = (640, 480);
        assert_eq!(
            args(&opts),
            [
                "--launcher",
                "--data-dir",
                "/d",
                "--screenshot",
                "/o.png",
                "--size",
                "640x480"
            ]
        );
        let reparsed = crate::cli::parse(args(&opts)).unwrap();
        assert!(reparsed.shows_launcher());
        assert_eq!(reparsed.data_dir, PathBuf::from("/d"));
    }

    #[test]
    fn last_played_text() {
        assert_eq!(last_played(0, 100), "Never played");
        assert_eq!(last_played(100, 130), "Played just now");
        assert_eq!(last_played(100, 100 + 120), "Played 2 minutes ago");
        assert_eq!(last_played(100, 100 + 3600), "Played 1 hour ago");
        assert_eq!(last_played(100, 100 + 3 * 86_400), "Played 3 days ago");
    }

    fn plan(mods: Option<&str>) -> LaunchPlan {
        let mut p = Profile::new("p1", "P");
        p.game_dir = Some("/games/p".into());
        p.mods = mods.map(PathBuf::from);
        p.heap_mib = 3072;
        p.resolution = Resolution::new(1280, 720);
        LaunchPlan::new(
            &p,
            &VersionList::builtin(),
            Path::new("/data"),
            LaunchAccount {
                name: "Steve".into(),
                online: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn plan_fills_unset_options() {
        let mut opts = crate::cli::parse(std::iter::empty()).unwrap();
        apply(&mut opts, plan(None)).unwrap();
        assert_eq!(opts.game_dir, Path::new("/games/p"));
        assert!(opts.vanilla && opts.mods.is_none() && opts.heap_mib.is_none());
        assert_eq!(opts.name, "Steve");
        assert_eq!(opts.size, (1280, 720));
        assert!(opts.jar.ends_with("versions/26.3/client.jar"));

        let mut opts = crate::cli::parse(std::iter::empty()).unwrap();
        apply(&mut opts, plan(Some("/games/mods"))).unwrap();
        assert!(!opts.vanilla);
        assert_eq!(opts.mods.as_deref(), Some(Path::new("/games/mods")));
        assert_eq!(opts.heap_mib, Some(3072));
    }

    #[test]
    fn explicit_flags_win() {
        let args = "--game-dir g --name Bot --size 640x480 --jar x.jar --mods m";
        let mut opts = crate::cli::parse(args.split(' ').map(str::to_owned)).unwrap();
        apply(&mut opts, plan(None)).unwrap();
        assert!(opts.game_dir.ends_with("g"));
        assert_eq!(opts.name, "Bot");
        assert_eq!(opts.size, (640, 480));
        assert!(opts.jar.ends_with("x.jar"));
        assert!(opts.mods.as_deref().is_some_and(|m| m.ends_with("m")));
        assert!(!opts.vanilla);
    }
}
