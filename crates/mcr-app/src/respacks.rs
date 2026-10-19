//! Server resource packs: request handling, downloads and the resource stack they reload into.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, SyncSender, TrySendError, channel, sync_channel};

use anyhow::{Context, Result};
use mcr_assets::{PackFormat, ResourceManager};
use mcr_client::{PackEnd, PackLink, PackReply, PackRequest, pack_channel};
use mcr_protocol::common::{ResourcePackAction, ResourcePackResponse};
use mcr_protocol::types::text::Component;
use mcr_respack::{
    BatchResult, DownloadRequest, Downloader, Effect, MAX_CACHED_PACKS, PackManager, PromptStatus,
    parse_pack_url, parse_sha1, vacuum,
};
use mcr_ui::{PackSetting, vanilla_resources};
use uuid::Uuid;

const MAX_QUEUED: usize = 64;
/// Download batches waiting for the single download thread; more fail at once.
const MAX_PENDING_DOWNLOADS: usize = 16;

/// One thread runs every download batch in turn, like vanilla's download queue.
struct DownloadWorker<J> {
    jobs: SyncSender<J>,
}

impl<J: Send + 'static> DownloadWorker<J> {
    fn start(mut run: impl FnMut(J) + Send + 'static) -> Self {
        let (jobs, rx) = sync_channel::<J>(MAX_PENDING_DOWNLOADS);
        std::thread::Builder::new()
            .name("mcr-pack-download".into())
            .spawn(move || {
                for job in rx {
                    run(job);
                }
            })
            .ok();
        Self { jobs }
    }

    fn submit(&self, job: J) -> Result<(), J> {
        self.jobs.try_send(job).map_err(|e| match e {
            TrySendError::Full(job) | TrySendError::Disconnected(job) => job,
        })
    }
}

struct DownloadJob {
    session: u64,
    requests: Vec<DownloadRequest>,
    protect: Vec<PathBuf>,
}

/// Where the vanilla resources live; stacks server packs on top of them.
#[derive(Debug, Clone)]
pub struct ResourceStack {
    pub jar: PathBuf,
    pub assets_dir: PathBuf,
    pub index: Option<String>,
    /// Mod jars whose resources sit between vanilla and the server's packs.
    pub mods: Vec<PathBuf>,
    /// The packs Java selected from its pack folder, above the mods, lowest priority first.
    pub user: Vec<PathBuf>,
}

impl ResourceStack {
    /// `packs` is in server push order; the first pushed pack has the highest priority.
    pub fn build(&self, packs: &[PathBuf]) -> Result<ResourceManager> {
        let mut resources = vanilla_resources(
            &self.jar,
            self.index
                .as_deref()
                .map(|i| (self.assets_dir.as_path(), i)),
        )
        .context("opening vanilla resources")?;
        crate::assets::push_mod_jars(&mut resources, &self.mods);
        for path in &self.user {
            if let Err(e) = resources.push_path(path) {
                eprintln!("resource packs: skipping {}: {e}", path.display());
            }
        }
        for path in packs.iter().rev() {
            resources
                .push_untrusted_zip(path)
                .with_context(|| format!("loading pack {}", path.display()))?;
        }
        Ok(resources)
    }
}

/// The file packs Java's `options.txt` selects, lowest priority first, as Java will report them
/// (inside `game_dir` only), so the first resource stack already has them.
pub fn predicted_java_packs(game_dir: &Path) -> Vec<PathBuf> {
    let java = game_dir.join("java");
    let Ok(text) = std::fs::read_to_string(java.join("options.txt")) else {
        return Vec::new();
    };
    let Some(list) = text.lines().find_map(|l| l.strip_prefix("resourcePacks:")) else {
        return Vec::new();
    };
    let Ok(names) = serde_json::from_str::<Vec<String>>(list) else {
        return Vec::new();
    };
    let root = game_dir.canonicalize().ok();
    names
        .iter()
        .take(256)
        .filter_map(|n| n.strip_prefix("file/"))
        .filter(|n| !n.contains(['/', '\\']) && *n != ".." && *n != ".")
        .filter_map(|n| java.join("resourcepacks").join(n).canonicalize().ok())
        .filter(|p| root.as_ref().is_some_and(|r| p.starts_with(r)))
        .collect()
}

#[derive(Debug)]
pub enum PackAction {
    Prompt {
        required: bool,
        message: Option<Component>,
    },
    Reload(Vec<PathBuf>),
    SaveSetting(PackSetting),
    Disconnect(Component),
}

struct Done {
    session: u64,
    ids: Vec<Uuid>,
    result: BatchResult,
}

#[derive(Debug)]
struct Queued {
    id: Uuid,
    url: String,
    hash: Option<[u8; 20]>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Reloading {
    No,
    Normal,
    Recovery,
    Final,
}

pub struct ServerPacks {
    manager: PackManager,
    end: Option<PackEnd>,
    downloader: Downloader,
    done_tx: Sender<Done>,
    done_rx: Receiver<Done>,
    worker: Option<DownloadWorker<DownloadJob>>,
    session: u64,
    setting: PackSetting,
    queued: Vec<Queued>,
    prompt_required: bool,
    active: Vec<PathBuf>,
    last_reload: Vec<PathBuf>,
    reloading: Reloading,
}

impl std::fmt::Debug for ServerPacks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerPacks")
            .field("setting", &self.setting)
            .field("active", &self.active.len())
            .finish_non_exhaustive()
    }
}

fn prompt_status(setting: PackSetting) -> PromptStatus {
    match setting {
        PackSetting::Prompt => PromptStatus::Pending,
        PackSetting::Enabled => PromptStatus::Allowed,
        PackSetting::Disabled => PromptStatus::Declined,
    }
}

impl ServerPacks {
    pub fn new(downloads: PathBuf, name: &str, uuid: Uuid) -> Self {
        let headers = vec![
            ("X-Minecraft-Username".into(), name.to_owned()),
            ("X-Minecraft-UUID".into(), uuid.simple().to_string()),
            ("X-Minecraft-Version".into(), "26.3".into()),
            ("X-Minecraft-Version-ID".into(), "26.3".into()),
            (
                "X-Minecraft-Pack-Format".into(),
                PackFormat::CURRENT.major.to_string(),
            ),
            ("User-Agent".into(), "Minecraft Java/26.3".into()),
        ];
        let (done_tx, done_rx) = channel();
        Self {
            manager: PackManager::new(PromptStatus::Pending),
            end: None,
            downloader: Downloader::new(downloads, headers),
            done_tx,
            done_rx,
            worker: None,
            session: 0,
            setting: PackSetting::Prompt,
            queued: Vec::new(),
            prompt_required: false,
            active: Vec::new(),
            last_reload: Vec::new(),
            reloading: Reloading::No,
        }
    }

    /// The server packs the last successful reload applied.
    pub fn applied(&self) -> Vec<PathBuf> {
        self.active.clone()
    }

    pub fn is_active(&self) -> bool {
        self.end.is_some()
    }

    /// Starts a connection's pack handling and returns the link to give the client.
    pub fn begin(&mut self, setting: PackSetting) -> PackLink {
        let (link, end) = pack_channel();
        self.end = Some(end);
        self.session += 1;
        self.setting = setting;
        self.queued.clear();
        self.manager.begin_session(prompt_status(setting));
        link
    }

    pub fn end_session(&mut self) -> Vec<PackAction> {
        self.end = None;
        self.queued.clear();
        let effects = self.manager.end_session();
        self.apply(effects)
    }

    fn reply(&self, id: Uuid, action: ResourcePackAction) {
        if let Some(end) = &self.end {
            end.reply(PackReply::Status(ResourcePackResponse { id, action }));
        }
    }

    pub fn poll(&mut self) -> Vec<PackAction> {
        let mut actions = Vec::new();
        while let Some(request) = self.end.as_ref().and_then(|e| e.requests.try_recv().ok()) {
            match request {
                PackRequest::Push(p) => {
                    let Some(url) = parse_pack_url(&p.url) else {
                        self.reply(p.id, ResourcePackAction::InvalidUrl);
                        continue;
                    };
                    let hash = parse_sha1(&p.hash);
                    let direct = self.setting != PackSetting::Prompt
                        && (!p.required || self.setting != PackSetting::Disabled);
                    if direct {
                        let effects = {
                            self.manager.push(p.id, url, hash);
                            self.manager.take_effects()
                        };
                        actions.extend(self.apply(effects));
                    } else if self.queued.len() < MAX_QUEUED {
                        self.queued.push(Queued {
                            id: p.id,
                            url,
                            hash,
                        });
                        self.prompt_required = p.required;
                        actions.push(PackAction::Prompt {
                            required: p.required,
                            message: p.prompt,
                        });
                    }
                }
                PackRequest::Pop(id) => {
                    match id {
                        Some(id) => self.manager.pop(id),
                        None => self.manager.pop_all(),
                    }
                    let effects = self.manager.take_effects();
                    actions.extend(self.apply(effects));
                }
            }
        }
        while let Ok(done) = self.done_rx.try_recv() {
            if done.session != self.session || self.end.is_none() {
                continue;
            }
            self.manager.download_finished(&done.ids, done.result);
            let effects = self.manager.take_effects();
            actions.extend(self.apply(effects));
        }
        actions
    }

    /// The player answered the confirm screen.
    pub fn answer(&mut self, accepted: bool) -> Vec<PackAction> {
        let mut actions = Vec::new();
        let required = self.prompt_required;
        if accepted {
            self.setting = PackSetting::Enabled;
            self.manager.allow();
        } else {
            self.manager.reject();
            if !required {
                self.setting = PackSetting::Disabled;
            }
        }
        for q in std::mem::take(&mut self.queued) {
            self.manager.push(q.id, q.url, q.hash);
        }
        let effects = self.manager.take_effects();
        actions.extend(self.apply(effects));
        if accepted || !required {
            actions.push(PackAction::SaveSetting(self.setting));
        } else {
            let reason =
                Component::translatable("multiplayer.requiredTexturePrompt.disconnect", Vec::new());
            if let Some(end) = &self.end {
                end.reply(PackReply::Disconnect(reason.clone()));
            }
            actions.push(PackAction::Disconnect(reason));
        }
        actions
    }

    /// The reload asked for by the last [`PackAction::Reload`] ended.
    pub fn reload_finished(&mut self, ok: bool) -> Vec<PackAction> {
        let state = std::mem::replace(&mut self.reloading, Reloading::No);
        let mut actions = Vec::new();
        match (state, ok) {
            (Reloading::Normal | Reloading::Recovery, true) => {
                self.manager.reload_succeeded();
                self.active.clone_from(&self.last_reload);
            }
            (Reloading::Normal, false) => {
                let recovery = self.manager.reload_failed();
                let paths: Vec<PathBuf> = recovery.into_iter().map(|(_, p)| p).collect();
                self.reloading = Reloading::Recovery;
                self.last_reload.clone_from(&paths);
                actions.push(PackAction::Reload(paths));
                return actions;
            }
            (Reloading::Recovery, false) => {
                self.manager.recovery_failed();
                self.reloading = Reloading::Final;
                self.active.clear();
                actions.push(PackAction::Reload(Vec::new()));
                return actions;
            }
            _ => {}
        }
        let effects = self.manager.take_effects();
        actions.extend(self.apply(effects));
        actions
    }

    fn apply(&mut self, effects: Vec<Effect>) -> Vec<PackAction> {
        let mut actions = Vec::new();
        for effect in effects {
            match effect {
                Effect::Report(id, action) => self.reply(id, action),
                Effect::Download(requests) => self.spawn_download(requests),
                Effect::Reload(packs) => {
                    let paths: Vec<PathBuf> = packs.into_iter().map(|(_, p)| p).collect();
                    self.last_reload.clone_from(&paths);
                    self.reloading = Reloading::Normal;
                    actions.push(PackAction::Reload(paths));
                }
            }
        }
        actions
    }

    fn spawn_download(&mut self, requests: Vec<DownloadRequest>) {
        let downloader = self.downloader.clone();
        let tx = self.done_tx.clone();
        let worker = self.worker.get_or_insert_with(|| {
            DownloadWorker::start(move |job: DownloadJob| {
                let result = download_batch(&downloader, &job.requests, &job.protect);
                let ids = job.requests.iter().map(|r| r.id).collect();
                let _ = tx.send(Done {
                    session: job.session,
                    ids,
                    result,
                });
            })
        });
        let job = DownloadJob {
            session: self.session,
            requests,
            protect: self.active.clone(),
        };
        if let Err(job) = worker.submit(job) {
            let ids: Vec<Uuid> = job.requests.iter().map(|r| r.id).collect();
            let result = BatchResult {
                failed: ids.clone(),
                ..BatchResult::default()
            };
            let _ = self.done_tx.send(Done {
                session: job.session,
                ids,
                result,
            });
        }
    }
}

fn download_batch(
    downloader: &Downloader,
    requests: &[DownloadRequest],
    protect: &[PathBuf],
) -> BatchResult {
    vacuum(downloader.dir(), MAX_CACHED_PACKS, protect);
    let mut result = BatchResult::default();
    for request in requests {
        let mut next_log = 0u64;
        let mut progress = |bytes: u64, total: Option<u64>| {
            if bytes >= next_log {
                tracing::info!("resource pack {}: {bytes} of {total:?} bytes", request.id);
                next_log = bytes + (8 << 20);
            }
        };
        match downloader.fetch(request, &mut progress) {
            Ok(path) => {
                result.downloaded.insert(request.id, path);
            }
            Err(e) => {
                tracing::warn!("resource pack {} failed: {e}", request.id);
                result.failed.push(request.id);
            }
        }
    }
    result
}

pub fn downloads_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("downloads")
}

#[cfg(test)]
mod download_tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn one_download_thread_with_a_bounded_queue() {
        let gate = Arc::new(Barrier::new(2));
        let held = gate.clone();
        let worker = DownloadWorker::start(move |first: bool| {
            if first {
                held.wait();
            }
        });
        let mut accepted = 0;
        for i in 0..1000 {
            if worker.submit(i == 0).is_ok() {
                accepted += 1;
            }
        }
        assert!(accepted <= MAX_PENDING_DOWNLOADS + 1, "{accepted}");
        gate.wait();
    }

    #[test]
    fn results_of_an_earlier_session_are_ignored() {
        let dir = std::env::temp_dir().join(format!("mcr-packs-{}", std::process::id()));
        let mut packs = ServerPacks::new(dir, "Soak", Uuid::nil());
        let _first = packs.begin(PackSetting::Enabled);
        let stale = packs.session;
        let _second = packs.begin(PackSetting::Enabled);
        let id = Uuid::from_u128(7);
        packs
            .done_tx
            .send(Done {
                session: stale,
                ids: vec![id],
                result: BatchResult {
                    failed: vec![id],
                    ..BatchResult::default()
                },
            })
            .unwrap();
        assert!(packs.poll().is_empty());
        assert!(packs.done_rx.try_recv().is_err());
    }
}

#[cfg(test)]
mod predict_tests {
    use super::*;

    #[test]
    fn predicted_packs_stay_inside_the_game_dir() {
        let base = std::env::temp_dir().join(format!("mcr-predict-{}", std::process::id()));
        let game = base.join("game");
        let packs = game.join("java/resourcepacks");
        std::fs::create_dir_all(packs.join("a")).unwrap();
        std::fs::write(packs.join("b.zip"), b"").unwrap();
        std::fs::create_dir_all(base.join("outside")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(base.join("outside"), packs.join("escape")).unwrap();
        std::fs::write(
            game.join("java/options.txt"),
            "version:4000\nresourcePacks:[\"vanilla\",\"file/a\",\"fabric\",\"file/../../outside\",\"file/..\",\"file/escape\",\"file/missing\",\"file/b.zip\"]\nlang:en_us\n",
        )
        .unwrap();
        let got = predicted_java_packs(&game);
        let packs = packs.canonicalize().unwrap();
        assert_eq!(got, [packs.join("a"), packs.join("b.zip")]);
        std::fs::write(game.join("java/options.txt"), "resourcePacks:[\"file/a\"").unwrap();
        assert!(predicted_java_packs(&game).is_empty());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
