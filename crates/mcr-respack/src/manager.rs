use std::collections::HashMap;
use std::path::PathBuf;

use mcr_protocol::common::ResourcePackAction as Action;
use uuid::Uuid;

use crate::download::DownloadRequest;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptStatus {
    Pending,
    Allowed,
    Declined,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Removal {
    DownloadFailed,
    ActivationFailed,
    Declined,
    Discarded,
    ServerRemoved,
    ServerReplaced,
}

impl Removal {
    fn response(self) -> Option<Action> {
        match self {
            Self::DownloadFailed => Some(Action::FailedDownload),
            Self::ActivationFailed => Some(Action::FailedReload),
            Self::Declined => Some(Action::Declined),
            Self::Discarded => Some(Action::Discarded),
            Self::ServerRemoved | Self::ServerReplaced => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Activation {
    Inactive,
    Pending,
    Active,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transfer {
    Requested,
    Pending,
    Done,
}

#[derive(Debug)]
struct Pack {
    serial: u64,
    id: Uuid,
    url: String,
    hash: Option<[u8; 20]>,
    path: Option<PathBuf>,
    removal: Option<Removal>,
    transfer: Transfer,
    activation: Activation,
    accepted: bool,
}

impl Pack {
    fn remove(&mut self, reason: Removal) {
        self.removal.get_or_insert(reason);
    }

    fn removed(&self) -> bool {
        self.removal.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Report(Uuid, Action),
    Download(Vec<DownloadRequest>),
    /// Rebuild resources from exactly these pack files, first entry pushed first by the server.
    Reload(Vec<(Uuid, PathBuf)>),
}

#[derive(Clone, Debug, Default)]
pub struct BatchResult {
    pub downloaded: HashMap<Uuid, PathBuf>,
    pub failed: Vec<Uuid>,
}

#[derive(Debug, Default)]
struct PendingReload {
    load: Vec<u64>,
    unload: Vec<u64>,
}

/// Server pack lifecycle with vanilla's `ServerPackManager` semantics; performs no I/O itself.
#[derive(Debug)]
pub struct PackManager {
    prompt: PromptStatus,
    packs: Vec<Pack>,
    reload: Option<PendingReload>,
    effects: Vec<Effect>,
    silent: bool,
    next_serial: u64,
}

impl PackManager {
    pub fn new(prompt: PromptStatus) -> Self {
        Self {
            prompt,
            packs: Vec::new(),
            reload: None,
            effects: Vec::new(),
            silent: false,
            next_serial: 0,
        }
    }

    /// Starts a new server session: reports flow to the server again.
    pub fn begin_session(&mut self, prompt: PromptStatus) {
        self.prompt = prompt;
        self.silent = false;
    }

    /// Pops every pack without telling the server, as vanilla does after a disconnect.
    pub fn end_session(&mut self) -> Vec<Effect> {
        self.silent = true;
        self.pop_all();
        self.prompt = PromptStatus::Pending;
        self.take_effects()
    }

    pub fn prompt(&self) -> PromptStatus {
        self.prompt
    }

    pub fn is_empty(&self) -> bool {
        self.packs.is_empty()
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    fn report(&mut self, id: Uuid, action: Action) {
        if !self.silent {
            self.effects.push(Effect::Report(id, action));
        }
    }

    pub fn push(&mut self, id: Uuid, url: String, hash: Option<[u8; 20]>) {
        if self.prompt == PromptStatus::Declined {
            self.report(id, Action::Declined);
            return;
        }
        for pack in self.packs.iter_mut().filter(|p| p.id == id) {
            pack.remove(Removal::ServerReplaced);
        }
        self.next_serial += 1;
        self.packs.push(Pack {
            serial: self.next_serial,
            id,
            url,
            hash,
            path: None,
            removal: None,
            transfer: Transfer::Requested,
            activation: Activation::Inactive,
            accepted: false,
        });
        if self.prompt == PromptStatus::Allowed {
            self.accept(self.packs.len() - 1);
        }
        self.tick();
    }

    fn accept(&mut self, index: usize) {
        let id = self.packs[index].id;
        self.report(id, Action::Accepted);
        self.packs[index].accepted = true;
    }

    pub fn pop(&mut self, id: Uuid) {
        if let Some(pack) = self.packs.iter_mut().find(|p| !p.removed() && p.id == id) {
            pack.remove(Removal::ServerRemoved);
            self.tick();
        }
    }

    pub fn pop_all(&mut self) {
        for pack in &mut self.packs {
            pack.remove(Removal::ServerRemoved);
        }
        self.tick();
    }

    pub fn allow(&mut self) {
        self.prompt = PromptStatus::Allowed;
        for i in 0..self.packs.len() {
            if !self.packs[i].accepted && !self.packs[i].removed() {
                self.accept(i);
            }
        }
        self.tick();
    }

    pub fn reject(&mut self) {
        self.prompt = PromptStatus::Declined;
        for pack in self.packs.iter_mut().filter(|p| !p.accepted) {
            pack.remove(Removal::Declined);
        }
        self.tick();
    }

    pub fn download_finished(&mut self, batch: &[Uuid], result: BatchResult) {
        if !result.failed.is_empty() {
            for pack in self
                .packs
                .iter_mut()
                .filter(|p| p.activation != Activation::Active)
            {
                if result.failed.contains(&pack.id) {
                    pack.remove(Removal::DownloadFailed);
                } else {
                    pack.remove(Removal::Discarded);
                }
            }
        }
        for index in 0..self.packs.len() {
            let pack = &mut self.packs[index];
            if !batch.contains(&pack.id) || pack.transfer != Transfer::Pending {
                continue;
            }
            let Some(path) = result.downloaded.get(&pack.id).cloned() else {
                continue;
            };
            pack.transfer = Transfer::Done;
            pack.path = Some(path);
            if !pack.removed() {
                let id = pack.id;
                self.report(id, Action::Downloaded);
            }
        }
        self.tick();
    }

    /// The resource rebuild requested by an [`Effect::Reload`] succeeded.
    pub fn reload_succeeded(&mut self) {
        let Some(reload) = self.reload.take() else {
            return;
        };
        for pack in &mut self.packs {
            if reload.load.contains(&pack.serial) {
                pack.activation = Activation::Active;
                if pack.removal.is_none() {
                    let id = pack.id;
                    self.effects
                        .push(Effect::Report(id, Action::SuccessfullyLoaded));
                }
            }
            if reload.unload.contains(&pack.serial) {
                pack.activation = Activation::Inactive;
            }
        }
        self.tick();
    }

    /// The rebuild failed; returns the packs that were active before, which the caller reloads to recover.
    pub fn reload_failed(&mut self) -> Vec<(Uuid, PathBuf)> {
        let mut load = Vec::new();
        for pack in &mut self.packs {
            match pack.activation {
                Activation::Inactive => pack.remove(Removal::Discarded),
                Activation::Pending => {
                    pack.activation = Activation::Inactive;
                    pack.remove(Removal::ActivationFailed);
                }
                Activation::Active => load.push(pack.serial),
            }
        }
        let paths = self.active_paths(&load);
        self.reload = Some(PendingReload {
            load,
            unload: Vec::new(),
        });
        paths
    }

    /// Recovery also failed: the caller runs with no server packs.
    pub fn recovery_failed(&mut self) {
        self.reload = None;
        for pack in &mut self.packs {
            if pack.activation != Activation::Inactive {
                pack.activation = Activation::Inactive;
            }
        }
        self.tick();
    }

    fn active_paths(&self, serials: &[u64]) -> Vec<(Uuid, PathBuf)> {
        self.packs
            .iter()
            .filter(|p| serials.contains(&p.serial))
            .filter_map(|p| Some((p.id, p.path.clone()?)))
            .collect()
    }

    fn tick(&mut self) {
        let downloads_pending = self.update_downloads();
        if !downloads_pending {
            self.trigger_reload();
        }
        self.cleanup();
    }

    fn cleanup(&mut self) {
        let mut reports = Vec::new();
        self.packs.retain(|pack| {
            if pack.activation != Activation::Inactive {
                return true;
            }
            match pack.removal {
                Some(reason) => {
                    if let Some(action) = reason.response() {
                        reports.push((pack.id, action));
                    }
                    false
                }
                None => true,
            }
        });
        for (id, action) in reports {
            self.report(id, action);
        }
    }

    fn update_downloads(&mut self) -> bool {
        let mut requests = Vec::new();
        let mut in_progress = false;
        for pack in &mut self.packs {
            if pack.removed() || !pack.accepted {
                continue;
            }
            if pack.transfer != Transfer::Done {
                in_progress = true;
            }
            if pack.transfer == Transfer::Requested {
                pack.transfer = Transfer::Pending;
                requests.push(DownloadRequest {
                    id: pack.id,
                    url: pack.url.clone(),
                    hash: pack.hash,
                });
            }
        }
        if !requests.is_empty() {
            self.effects.push(Effect::Download(requests));
        }
        in_progress
    }

    fn trigger_reload(&mut self) {
        let mut needs_reload = false;
        let mut load = Vec::new();
        let mut unload = Vec::new();
        for pack in &self.packs {
            if pack.activation == Activation::Pending {
                return;
            }
            let should_be_active =
                pack.accepted && pack.transfer == Transfer::Done && !pack.removed();
            if should_be_active && pack.activation == Activation::Inactive {
                load.push(pack.serial);
                needs_reload = true;
            }
            if pack.activation == Activation::Active {
                if should_be_active {
                    load.push(pack.serial);
                } else {
                    needs_reload = true;
                    unload.push(pack.serial);
                }
            }
        }
        if !needs_reload {
            return;
        }
        for pack in &mut self.packs {
            let touched = load.contains(&pack.serial) && pack.activation != Activation::Active
                || unload.contains(&pack.serial);
            if touched {
                pack.activation = Activation::Pending;
            }
        }
        let paths = self.active_paths(&load);
        self.reload = Some(PendingReload { load, unload });
        self.effects.push(Effect::Reload(paths));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Uuid = Uuid::from_u128(1);
    const B: Uuid = Uuid::from_u128(2);

    fn report(id: Uuid, action: Action) -> Effect {
        Effect::Report(id, action)
    }

    fn download_ok(m: &mut PackManager, ids: &[Uuid]) {
        let result = BatchResult {
            downloaded: ids
                .iter()
                .map(|id| (*id, PathBuf::from(format!("{id}.zip"))))
                .collect(),
            failed: Vec::new(),
        };
        m.download_finished(ids, result);
    }

    fn entries(m: &mut PackManager) -> Vec<Effect> {
        m.take_effects()
    }

    #[test]
    fn allowed_pack_runs_the_full_sequence() {
        let mut m = PackManager::new(PromptStatus::Allowed);
        m.push(A, "http://x".into(), None);
        let fx = entries(&mut m);
        assert_eq!(fx[0], report(A, Action::Accepted));
        assert!(matches!(&fx[1], Effect::Download(r) if r.len() == 1 && r[0].id == A));
        download_ok(&mut m, &[A]);
        let fx = entries(&mut m);
        assert_eq!(fx[0], report(A, Action::Downloaded));
        assert_eq!(
            fx[1],
            Effect::Reload(vec![(A, PathBuf::from(format!("{A}.zip")))])
        );
        m.reload_succeeded();
        assert_eq!(entries(&mut m), [report(A, Action::SuccessfullyLoaded)]);
    }

    #[test]
    fn declined_status_replies_immediately() {
        let mut m = PackManager::new(PromptStatus::Declined);
        m.push(A, "http://x".into(), None);
        assert_eq!(entries(&mut m), [report(A, Action::Declined)]);
        assert!(m.is_empty());
    }

    #[test]
    fn pending_prompt_waits_for_the_answer() {
        let mut m = PackManager::new(PromptStatus::Pending);
        m.push(A, "http://x".into(), None);
        assert!(entries(&mut m).is_empty());
        m.allow();
        let fx = entries(&mut m);
        assert_eq!(fx[0], report(A, Action::Accepted));
        assert!(matches!(fx[1], Effect::Download(_)));
        assert_eq!(m.prompt(), PromptStatus::Allowed);
    }

    #[test]
    fn rejecting_declines_unaccepted_packs() {
        let mut m = PackManager::new(PromptStatus::Pending);
        m.push(A, "http://x".into(), None);
        m.push(B, "http://y".into(), None);
        m.reject();
        assert_eq!(
            entries(&mut m),
            [report(A, Action::Declined), report(B, Action::Declined)]
        );
        m.push(A, "http://x".into(), None);
        assert_eq!(entries(&mut m), [report(A, Action::Declined)]);
    }

    #[test]
    fn failed_download_reports_and_discards_the_rest() {
        let mut m = PackManager::new(PromptStatus::Allowed);
        m.push(A, "http://x".into(), None);
        m.push(B, "http://y".into(), None);
        entries(&mut m);
        m.download_finished(
            &[A, B],
            BatchResult {
                downloaded: HashMap::new(),
                failed: vec![A],
            },
        );
        assert_eq!(
            entries(&mut m),
            [
                report(A, Action::FailedDownload),
                report(B, Action::Discarded)
            ]
        );
        assert!(m.is_empty());
    }

    #[test]
    fn failed_reload_reports_failed_reload() {
        let mut m = PackManager::new(PromptStatus::Allowed);
        m.push(A, "http://x".into(), None);
        download_ok(&mut m, &[A]);
        entries(&mut m);
        assert!(m.reload_failed().is_empty());
        m.reload_succeeded();
        assert_eq!(entries(&mut m), [report(A, Action::FailedReload)]);
        assert!(m.is_empty());
    }

    #[test]
    fn pop_unloads_without_a_report() {
        let mut m = PackManager::new(PromptStatus::Allowed);
        m.push(A, "http://x".into(), None);
        download_ok(&mut m, &[A]);
        m.reload_succeeded();
        entries(&mut m);
        m.pop(A);
        assert_eq!(entries(&mut m), [Effect::Reload(Vec::new())]);
        m.reload_succeeded();
        assert!(entries(&mut m).is_empty());
        assert!(m.is_empty());
    }

    #[test]
    fn replacing_a_pack_keeps_the_new_one() {
        let mut m = PackManager::new(PromptStatus::Allowed);
        m.push(A, "http://x".into(), None);
        download_ok(&mut m, &[A]);
        m.reload_succeeded();
        entries(&mut m);
        m.push(A, "http://z".into(), None);
        let fx = entries(&mut m);
        assert_eq!(fx[0], report(A, Action::Accepted));
        assert!(matches!(&fx[1], Effect::Download(_)));
        download_ok(&mut m, &[A]);
        let fx = entries(&mut m);
        assert_eq!(fx[0], report(A, Action::Downloaded));
        assert!(matches!(&fx[1], Effect::Reload(p) if p.len() == 1));
    }

    #[test]
    fn end_session_pops_silently() {
        let mut m = PackManager::new(PromptStatus::Allowed);
        m.push(A, "http://x".into(), None);
        download_ok(&mut m, &[A]);
        m.reload_succeeded();
        entries(&mut m);
        let fx = m.end_session();
        assert_eq!(fx, [Effect::Reload(Vec::new())]);
        m.reload_succeeded();
        assert!(entries(&mut m).is_empty());
        assert_eq!(m.prompt(), PromptStatus::Pending);
    }
}
