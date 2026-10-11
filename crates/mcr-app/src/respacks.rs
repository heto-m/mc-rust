//! Server resource packs: request handling, downloads and the resource stack they reload into.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

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

/// Where the vanilla resources live; stacks server packs on top of them.
#[derive(Debug, Clone)]
pub struct ResourceStack {
    pub jar: PathBuf,
    pub assets_dir: PathBuf,
    pub index: Option<String>,
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
        for path in packs.iter().rev() {
            resources
                .push_untrusted_zip(path)
                .with_context(|| format!("loading pack {}", path.display()))?;
        }
        Ok(resources)
    }
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
            setting: PackSetting::Prompt,
            queued: Vec::new(),
            prompt_required: false,
            active: Vec::new(),
            last_reload: Vec::new(),
            reloading: Reloading::No,
        }
    }

    pub fn is_active(&self) -> bool {
        self.end.is_some()
    }

    /// Starts a connection's pack handling and returns the link to give the client.
    pub fn begin(&mut self, setting: PackSetting) -> PackLink {
        let (link, end) = pack_channel();
        self.end = Some(end);
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

    fn spawn_download(&self, requests: Vec<DownloadRequest>) {
        let downloader = self.downloader.clone();
        let protect = self.active.clone();
        let tx = self.done_tx.clone();
        std::thread::spawn(move || {
            vacuum(downloader.dir(), MAX_CACHED_PACKS, &protect);
            let mut result = BatchResult::default();
            for request in &requests {
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
            let ids = requests.iter().map(|r| r.id).collect();
            let _ = tx.send(Done { ids, result });
        });
    }
}

pub fn downloads_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("downloads")
}
