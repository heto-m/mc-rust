//! `servers.dat` backed server list with background status pings.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};

use mcr_settings::{ResourcePacks, ServerList};
use mcr_ui::{Favicon, OnlineStatus, PackSetting, ServerEntry, ServerStatus, ServerStore};

use crate::status::{self, PingResult};

const MAX_SERVERS: usize = 1000;
const MAX_INFLIGHT: usize = 8;

type Reply = (u64, usize, Result<PingResult, String>);

pub struct FileServers {
    path: PathBuf,
    list: ServerList,
    entries: Vec<ServerEntry>,
    statuses: Vec<Option<ServerStatus>>,
    epoch: u64,
    inflight: usize,
    tx: Sender<Reply>,
    rx: Receiver<Reply>,
    next_favicon: u64,
    last_direct: String,
}

impl FileServers {
    pub fn load(game_dir: &std::path::Path) -> Self {
        let path = game_dir.join("servers.dat");
        let list = std::fs::read(&path)
            .ok()
            .and_then(|b| ServerList::parse(&b).ok())
            .unwrap_or_default();
        let entries = list
            .servers
            .iter()
            .take(MAX_SERVERS)
            .map(|s| ServerEntry {
                name: s.name.clone(),
                address: s.ip.clone(),
            })
            .collect::<Vec<_>>();
        let (tx, rx) = channel();
        Self {
            path,
            statuses: vec![None; entries.len()],
            list,
            entries,
            epoch: 0,
            inflight: 0,
            tx,
            rx,
            next_favicon: 1,
            last_direct: String::new(),
        }
    }

    fn write(&self) {
        if let Ok(bytes) = self.list.to_bytes() {
            if let Some(dir) = self.path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&self.path, bytes);
        }
    }

    pub fn pack_setting_for(&self, address: &str) -> PackSetting {
        self.list
            .servers
            .iter()
            .find(|s| s.ip == address)
            .map_or(PackSetting::Prompt, |s| to_ui(s.resource_packs))
    }

    pub fn set_pack_setting_for(&mut self, address: &str, setting: PackSetting) {
        if let Some(s) = self.list.servers.iter_mut().find(|s| s.ip == address) {
            s.resource_packs = from_ui(setting);
            self.write();
        }
    }

    fn save(&mut self) {
        let old = std::mem::take(&mut self.list.servers);
        self.list.servers = self
            .entries
            .iter()
            .map(|e| {
                let mut s = old
                    .iter()
                    .find(|o| o.ip == e.address)
                    .cloned()
                    .unwrap_or_default();
                s.name.clone_from(&e.name);
                s.ip.clone_from(&e.address);
                s
            })
            .collect();
        self.write();
    }

    fn drain(&mut self) {
        while let Ok((epoch, index, result)) = self.rx.try_recv() {
            self.inflight = self.inflight.saturating_sub(1);
            if epoch != self.epoch || index >= self.statuses.len() {
                continue;
            }
            self.statuses[index] = Some(match result {
                Ok(r) => {
                    let favicon = r.favicon.map(|rgba| {
                        let id = self.next_favicon;
                        self.next_favicon += 1;
                        Rc::new(Favicon { id, rgba })
                    });
                    ServerStatus::Online(Box::new(OnlineStatus {
                        motd: r.motd,
                        players: format!("{}/{}", r.online, r.max),
                        version: r.version,
                        ping_ms: Some(r.latency_ms),
                        compatible: r.protocol == i64::from(mcr_data::v26_3().protocol()),
                        favicon,
                    }))
                }
                Err(message) => ServerStatus::Unreachable { message },
            });
        }
    }

    fn start(&mut self, index: usize) {
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        self.statuses[index] = Some(ServerStatus::Pinging);
        self.inflight += 1;
        let (tx, address, epoch) = (self.tx.clone(), entry.address.clone(), self.epoch);
        std::thread::spawn(move || {
            let result = status::ping(&address).map_err(|e| e.to_string());
            let _ = tx.send((epoch, index, result));
        });
    }
}

fn to_ui(setting: ResourcePacks) -> PackSetting {
    match setting {
        ResourcePacks::Prompt => PackSetting::Prompt,
        ResourcePacks::Enabled => PackSetting::Enabled,
        ResourcePacks::Disabled => PackSetting::Disabled,
    }
}

fn from_ui(setting: PackSetting) -> ResourcePacks {
    match setting {
        PackSetting::Prompt => ResourcePacks::Prompt,
        PackSetting::Enabled => ResourcePacks::Enabled,
        PackSetting::Disabled => ResourcePacks::Disabled,
    }
}

impl ServerStore for FileServers {
    fn pack_setting(&self, index: usize) -> PackSetting {
        self.list
            .servers
            .get(index)
            .map_or(PackSetting::Prompt, |s| to_ui(s.resource_packs))
    }

    fn set_pack_setting(&mut self, index: usize, setting: PackSetting) {
        if let Some(s) = self.list.servers.get_mut(index) {
            s.resource_packs = from_ui(setting);
            self.write();
        }
    }

    fn servers(&self) -> &[ServerEntry] {
        &self.entries
    }

    fn add(&mut self, entry: ServerEntry) {
        if self.entries.len() < MAX_SERVERS {
            self.entries.push(entry);
            self.statuses.push(None);
            self.save();
        }
    }

    fn update(&mut self, index: usize, entry: ServerEntry) {
        if let Some(e) = self.entries.get_mut(index) {
            *e = entry;
            self.statuses[index] = None;
            self.save();
        }
    }

    fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
            self.statuses.remove(index);
            self.epoch += 1;
            self.inflight = 0;
            self.statuses.iter_mut().for_each(|s| *s = None);
            self.save();
        }
    }

    fn swap(&mut self, a: usize, b: usize) {
        if a < self.entries.len() && b < self.entries.len() {
            self.entries.swap(a, b);
            self.statuses.swap(a, b);
            self.save();
        }
    }

    fn status(&mut self, index: usize) -> ServerStatus {
        self.drain();
        match self.statuses.get(index) {
            Some(Some(s)) => s.clone(),
            Some(None) if self.inflight < MAX_INFLIGHT => {
                self.start(index);
                ServerStatus::Pinging
            }
            _ => ServerStatus::Pinging,
        }
    }

    fn refresh(&mut self) {
        self.epoch += 1;
        self.inflight = 0;
        self.statuses.iter_mut().for_each(|s| *s = None);
    }

    fn last_direct_address(&self) -> String {
        self.last_direct.clone()
    }

    fn set_last_direct_address(&mut self, address: &str) {
        self.last_direct = address.to_owned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_survive_a_reload() {
        let dir = std::env::temp_dir().join(format!("mcr-servers-{}", std::process::id()));
        let mut list = FileServers::load(&dir);
        list.add(ServerEntry {
            name: "A".into(),
            address: "a.example:25565".into(),
        });
        list.add(ServerEntry {
            name: "B".into(),
            address: "b.example".into(),
        });
        list.swap(0, 1);
        let again = FileServers::load(&dir);
        let names: Vec<_> = again.servers().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["B", "A"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
