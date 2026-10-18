use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::accounts::AccountEntry;
use crate::profile::{DEFAULT_PROFILE_ID, Profile, valid_id};

pub const FILE_NAME: &str = "launcher_profiles.json";
pub const MAX_FILE_BYTES: u64 = 1 << 20;
pub const MAX_PROFILES: usize = 64;
pub const MAX_ACCOUNTS: usize = 32;
pub const DEFAULT_OFFLINE_NAME: &str = "Player";
const FORMAT: u64 = 1;

/// Everything in `launcher_profiles.json`: no secrets, user-editable, so read as hostile input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherFile {
    pub profiles: Vec<Profile>,
    pub selected: String,
    pub accounts: Vec<AccountEntry>,
    /// [`AccountEntry::key`] of the account Play uses.
    pub active_account: Option<String>,
    pub show_snapshots: bool,
}

impl Default for LauncherFile {
    fn default() -> Self {
        let mut file = Self {
            profiles: Vec::new(),
            selected: DEFAULT_PROFILE_ID.to_owned(),
            accounts: Vec::new(),
            active_account: None,
            show_snapshots: false,
        };
        file.normalize();
        file
    }
}

impl LauncherFile {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return None;
        }
        let root: Value = serde_json::from_slice(bytes).ok()?;
        root.as_object()?;
        let profiles = root["profiles"]
            .as_array()
            .into_iter()
            .flatten()
            .take(MAX_PROFILES)
            .filter_map(Profile::from_json)
            .collect();
        let accounts = root["accounts"]
            .as_array()
            .into_iter()
            .flatten()
            .take(MAX_ACCOUNTS)
            .filter_map(AccountEntry::from_json)
            .collect();
        let mut file = Self {
            profiles,
            selected: root["selected_profile"]
                .as_str()
                .unwrap_or(DEFAULT_PROFILE_ID)
                .to_owned(),
            accounts,
            active_account: root["active_account"]
                .as_str()
                .filter(|k| k.len() <= 64)
                .map(str::to_owned),
            show_snapshots: root["show_snapshots"].as_bool().unwrap_or(false),
        };
        file.normalize();
        Some(file)
    }

    pub fn to_json(&self) -> Vec<u8> {
        let v = json!({
            "format": FORMAT,
            "selected_profile": self.selected,
            "active_account": self.active_account,
            "show_snapshots": self.show_snapshots,
            "profiles": self.profiles.iter().map(Profile::to_json).collect::<Vec<_>>(),
            "accounts": self.accounts.iter().map(AccountEntry::to_json).collect::<Vec<_>>(),
        });
        serde_json::to_vec_pretty(&v).unwrap_or_default()
    }

    /// The default profile first, unique ids, a valid selection and active account.
    fn normalize(&mut self) {
        self.profiles.truncate(MAX_PROFILES);
        let mut seen = std::collections::HashSet::new();
        for i in 0..self.profiles.len() {
            if !seen.insert(self.profiles[i].id.clone()) {
                let id = self.fresh_id();
                seen.insert(id.clone());
                self.profiles[i].id = id;
            }
        }
        match self.profiles.iter().position(Profile::is_default) {
            Some(0) => {}
            Some(i) => {
                let default = self.profiles.remove(i);
                self.profiles.insert(0, default);
            }
            None => {
                if self.profiles.len() >= MAX_PROFILES {
                    self.profiles.pop();
                }
                self.profiles.insert(0, Profile::latest_release());
            }
        }
        if self.profile(&self.selected).is_none() {
            self.selected = DEFAULT_PROFILE_ID.to_owned();
        }
        let mut keys = std::collections::HashSet::new();
        self.accounts.retain(|a| keys.insert(a.key()));
        self.accounts.truncate(MAX_ACCOUNTS);
        if self.accounts.is_empty() {
            self.accounts
                .push(AccountEntry::offline(DEFAULT_OFFLINE_NAME).expect("valid name"));
        }
        if self.active().is_none() {
            self.active_account = self.accounts.first().map(AccountEntry::key);
        }
    }

    fn fresh_id(&self) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        let mut n = nanos;
        loop {
            let id = format!("p{:x}", n & 0xffff_ffff_ffff);
            if valid_id(&id) && self.profile(&id).is_none() {
                return id;
            }
            n = n.wrapping_add(1);
        }
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn selected_profile(&self) -> &Profile {
        self.profile(&self.selected).unwrap_or(&self.profiles[0])
    }

    /// Exact name first, then ignoring case.
    pub fn find_by_name(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name).or_else(|| {
            self.profiles
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
        })
    }

    pub fn can_add(&self) -> bool {
        self.profiles.len() < MAX_PROFILES
    }

    /// Adds `profile` under a new id and selects it.
    pub fn add_profile(&mut self, mut profile: Profile) -> Option<String> {
        if !self.can_add() {
            return None;
        }
        profile.id = self.fresh_id();
        profile.sanitize();
        let id = profile.id.clone();
        self.profiles.push(profile);
        self.selected = id.clone();
        Some(id)
    }

    pub fn update_profile(&mut self, mut profile: Profile) -> bool {
        profile.sanitize();
        match self.profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(slot) => {
                *slot = profile;
                true
            }
            None => false,
        }
    }

    pub fn duplicate(&mut self, id: &str) -> Option<String> {
        let mut copy = self.profile(id)?.clone();
        copy.name = format!("{} (copy)", copy.name);
        copy.last_played = 0;
        self.add_profile(copy)
    }

    /// The default profile cannot be deleted.
    pub fn delete(&mut self, id: &str) -> bool {
        if id == DEFAULT_PROFILE_ID {
            return false;
        }
        let before = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        if self.selected == id {
            self.selected = DEFAULT_PROFILE_ID.to_owned();
        }
        self.profiles.len() != before
    }

    pub fn select(&mut self, id: &str) -> bool {
        let found = self.profile(id).is_some();
        if found {
            self.selected = id.to_owned();
        }
        found
    }

    pub fn touch(&mut self, id: &str, now: u64) {
        if let Some(p) = self.profiles.iter_mut().find(|p| p.id == id) {
            p.last_played = now;
        }
    }

    pub fn active(&self) -> Option<&AccountEntry> {
        let key = self.active_account.as_deref()?;
        self.accounts.iter().find(|a| a.key() == key)
    }

    /// Adds or replaces the account with the same key and makes it active.
    pub fn add_account(&mut self, entry: AccountEntry) -> bool {
        let key = entry.key();
        if let Some(slot) = self.accounts.iter_mut().find(|a| a.key() == key) {
            *slot = entry;
        } else if self.accounts.len() < MAX_ACCOUNTS {
            self.accounts.push(entry);
        } else {
            return false;
        }
        self.active_account = Some(key);
        true
    }

    pub fn remove_account(&mut self, key: &str) -> Option<AccountEntry> {
        let index = self.accounts.iter().position(|a| a.key() == key)?;
        let removed = self.accounts.remove(index);
        self.normalize();
        Some(removed)
    }

    pub fn set_active(&mut self, key: &str) -> bool {
        let found = self.accounts.iter().any(|a| a.key() == key);
        if found {
            self.active_account = Some(key.to_owned());
        }
        found
    }
}

/// The launcher file at `<data dir>/launcher_profiles.json`.
#[derive(Debug)]
pub struct Store {
    data_dir: PathBuf,
    pub file: LauncherFile,
}

impl Store {
    /// Defaults when the file is missing; a file that cannot be read as one is renamed to `.bad`.
    pub fn open(data_dir: &Path) -> Self {
        let path = data_dir.join(FILE_NAME);
        let file = match read_capped(&path) {
            Ok(Some(bytes)) => LauncherFile::parse(&bytes).unwrap_or_else(|| {
                let bad = path.with_extension("json.bad");
                eprintln!(
                    "launcher: {} is not a launcher file; keeping it as {}",
                    path.display(),
                    bad.display()
                );
                let _ = std::fs::rename(&path, &bad);
                LauncherFile::default()
            }),
            Ok(None) => LauncherFile::default(),
            Err(e) => {
                eprintln!("launcher: cannot read {}: {e}", path.display());
                LauncherFile::default()
            }
        };
        Self {
            data_dir: data_dir.to_owned(),
            file,
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn path(&self) -> PathBuf {
        self.data_dir.join(FILE_NAME)
    }

    pub fn save(&self) -> std::io::Result<()> {
        write_atomic(&self.path(), &self.file.to_json())
    }
}

/// `None` when missing; files over the cap read as garbage.
fn read_capped(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

/// Writes through a synced sibling temp file and a rename.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::tests::{UUID, temp_dir};

    #[test]
    fn defaults() {
        let f = LauncherFile::default();
        assert_eq!(f.profiles.len(), 1);
        assert!(f.selected_profile().is_default());
        assert_eq!(f.selected_profile().name, "Latest release");
        assert_eq!(f.active().unwrap().name, DEFAULT_OFFLINE_NAME);
    }

    #[test]
    fn create_edit_duplicate_delete_round_trip() {
        let dir = temp_dir("store");
        let mut store = Store::open(&dir);
        let id = store
            .file
            .add_profile(Profile::new("", "Survival"))
            .unwrap();
        assert_eq!(store.file.selected, id);
        let mut edited = store.file.profile(&id).unwrap().clone();
        edited.icon = "tnt".into();
        assert!(store.file.update_profile(edited));
        let copy = store.file.duplicate(&id).unwrap();
        assert_eq!(store.file.profile(&copy).unwrap().name, "Survival (copy)");
        assert!(store.file.delete(&copy));
        assert!(!store.file.delete(DEFAULT_PROFILE_ID));
        store.file.select(&id);
        store
            .file
            .add_account(AccountEntry::microsoft("Notch", UUID).unwrap());
        store.save().unwrap();

        let again = Store::open(&dir);
        assert_eq!(again.file, store.file);
        assert_eq!(again.file.selected_profile().name, "Survival");
        assert_eq!(again.file.selected_profile().icon, "tnt");
        assert_eq!(again.file.active().unwrap().name, "Notch");
        let text = std::fs::read_to_string(again.path()).unwrap();
        assert!(!text.contains("token"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hostile_file_is_bounded() {
        let name = "n".repeat(1 << 20);
        let profiles: Vec<Value> = (0..10_000)
            .map(
                |i| json!({"id": format!("p{i}"), "name": if i < 3 { name.as_str() } else { "x" }}),
            )
            .collect();
        let accounts: Vec<Value> = (0..1000)
            .map(|i| json!({"kind": "offline", "name": format!("u{i}")}))
            .chain([json!({"kind": "offline", "name": "bad name!"})])
            .collect();
        let v = json!({"profiles": profiles, "accounts": accounts});
        let bytes = serde_json::to_vec(&v).unwrap();
        assert!(LauncherFile::parse(&bytes).is_none(), "over 1 MiB");

        let small: Vec<Value> = (0..200)
            .map(|i| json!({"id": if i % 2 == 0 { "dup".to_owned() } else { format!("p{i}") }, "name": "z".repeat(500)}))
            .collect();
        let accounts: Vec<Value> = (0..100)
            .map(|i| json!({"kind": "offline", "name": format!("u{i}")}))
            .collect();
        let v = json!({"profiles": small, "accounts": accounts, "selected_profile": "nope", "active_account": "x".repeat(1000)});
        let f = LauncherFile::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert!(f.profiles.len() <= MAX_PROFILES);
        assert!(f.profiles[0].is_default());
        assert!(f.profiles.iter().all(|p| p.name.chars().count() <= 64));
        let ids: std::collections::HashSet<_> = f.profiles.iter().map(|p| &p.id).collect();
        assert_eq!(ids.len(), f.profiles.len());
        assert_eq!(f.accounts.len(), MAX_ACCOUNTS);
        assert_eq!(f.selected, DEFAULT_PROFILE_ID);
        assert_eq!(f.active().unwrap().name, "u0");
    }

    #[test]
    fn garbage_file_is_kept_aside() {
        let dir = temp_dir("garbage");
        std::fs::write(dir.join(FILE_NAME), b"\x00not json").unwrap();
        let store = Store::open(&dir);
        assert_eq!(store.file, LauncherFile::default());
        assert!(dir.join("launcher_profiles.json.bad").exists());
        assert!(!dir.join(FILE_NAME).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_and_accounts() {
        let mut f = LauncherFile::default();
        f.add_profile(Profile::new("", "Survival")).unwrap();
        assert_eq!(f.find_by_name("survival").unwrap().name, "Survival");
        assert!(f.find_by_name("nope").is_none());
        let steve = AccountEntry::offline("Steve").unwrap();
        f.add_account(steve.clone());
        assert_eq!(f.active(), Some(&steve));
        assert!(f.remove_account(&steve.key()).is_some());
        assert_eq!(f.active().unwrap().name, DEFAULT_OFFLINE_NAME);
        f.remove_account("offline:player");
        assert_eq!(f.accounts.len(), 1, "an account always exists");
    }
}
