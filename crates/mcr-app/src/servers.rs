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
    /// Protocol numbers from the latest pings, by address.
    protocols: Vec<(String, i64)>,
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
            protocols: Vec::new(),
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

    /// The protocol the last ping of `address` reported.
    pub fn pinged_protocol(&self, address: &str) -> Option<i32> {
        self.protocols
            .iter()
            .find(|(a, _)| a == address)
            .and_then(|&(_, p)| i32::try_from(p).ok())
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

    /// Stores the icon a server sent while joined, if it is a valid 64x64 PNG.
    pub fn set_icon_for(&mut self, address: &str, png: &[u8]) {
        use base64::Engine;
        if status::decode_png64(png).is_none() {
            return;
        }
        if let Some(s) = self.list.servers.iter_mut().find(|s| s.ip == address) {
            s.icon = Some(base64::engine::general_purpose::STANDARD.encode(png));
            self.write();
        }
    }

    pub fn name_for(&self, address: &str) -> Option<String> {
        self.list
            .servers
            .iter()
            .find(|s| s.ip == address)
            .map(|s| s.name.clone())
    }

    pub fn accepted_conduct_for(&self, address: &str) -> Option<i32> {
        self.list
            .servers
            .iter()
            .find(|s| s.ip == address)
            .map(|s| s.accepted_code_of_conduct)
            .filter(|h| *h != 0)
    }

    pub fn set_accepted_conduct_for(&mut self, address: &str, hash: i32) {
        if let Some(s) = self.list.servers.iter_mut().find(|s| s.ip == address) {
            s.accepted_code_of_conduct = hash;
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
                    let address = self.entries[index].address.clone();
                    self.protocols.retain(|(a, _)| *a != address);
                    if self.protocols.len() < MAX_SERVERS {
                        self.protocols.push((address, r.protocol));
                    }
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
                        compatible: is_compatible(r.protocol),
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

/// A server is joinable when its protocol is one mcr has a wire for.
pub fn is_compatible(protocol: i64) -> bool {
    mcr_protocol::version::supported_protocols().any(|p| i64::from(p) == protocol)
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

    /// Pings of the old epoch keep their slot until they answer, so refreshing cannot pile up threads.
    fn refresh(&mut self) {
        self.epoch += 1;
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
    fn refreshing_keeps_running_pings_counted() {
        let dir = std::env::temp_dir().join(format!("mcr-pings-{}", std::process::id()));
        let mut list = FileServers::load(&dir);
        for i in 0..32 {
            list.entries.push(ServerEntry {
                name: format!("s{i}"),
                address: "127.0.0.1:1".into(),
            });
            list.statuses.push(None);
        }
        for _ in 0..50 {
            for i in 0..32 {
                list.status(i);
            }
            assert!(list.inflight <= MAX_INFLIGHT);
            list.refresh();
        }
        let _ = std::fs::remove_dir_all(dir);
    }

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

    fn png(size: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, size, size);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&vec![200u8; (size * size * 4) as usize])
            .unwrap();
        drop(w);
        out
    }

    #[test]
    fn server_sent_icons_are_saved_only_when_valid() {
        let dir = std::env::temp_dir().join(format!("mcr-icons-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let list = ServerList {
            servers: vec![mcr_settings::ServerEntry {
                name: "x".into(),
                ip: "host:1".into(),
                ..Default::default()
            }],
        };
        std::fs::write(dir.join("servers.dat"), list.to_bytes().unwrap()).unwrap();
        let mut servers = FileServers::load(&dir);
        servers.set_icon_for("host:1", &png(32));
        servers.set_icon_for("host:1", b"not a png");
        let saved = ServerList::parse(&std::fs::read(dir.join("servers.dat")).unwrap()).unwrap();
        assert_eq!(saved.servers[0].icon, None);
        servers.set_icon_for("other:1", &png(64));
        servers.set_icon_for("host:1", &png(64));
        let saved = ServerList::parse(&std::fs::read(dir.join("servers.dat")).unwrap()).unwrap();
        assert!(
            saved.servers[0]
                .icon
                .as_deref()
                .is_some_and(|i| i.len() > 20)
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
