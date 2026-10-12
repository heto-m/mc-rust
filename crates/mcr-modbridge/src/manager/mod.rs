//! The mod manager: mods folder scan, compatibility policy, dependency pre-check and crash fallback.
//! Jars are only read as zip manifests; no class is loaded and nothing is extracted.

#[cfg(test)]
mod fixture;
pub mod policy;
pub mod resolve;
pub mod scan;
pub mod session;
pub mod version;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mcr_settings::ModSettings;

pub use policy::{Policy, PolicyEntry, PolicyStatus};
pub use resolve::HostEnv;
pub use scan::{Icon, ModInfo, ModKind, Side};
pub use session::{SessionRecord, SessionState, SessionStore, SessionTracker};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModStatus {
    /// Passed to the loader.
    Active,
    Disabled,
    Blocked(String),
    /// Left out of this start by the dependency pre-check.
    Excluded(String),
    ServerOnly,
    /// A loader mcr cannot run yet, or an unreadable jar.
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModEntry {
    pub info: ModInfo,
    pub status: ModStatus,
    /// The user's choice, or the default for a mod never toggled.
    pub enabled: bool,
    pub policy: Option<PolicyEntry>,
}

impl ModEntry {
    /// Blocked mods and those mcr cannot load have no toggle.
    pub fn toggleable(&self) -> bool {
        !matches!(
            self.status,
            ModStatus::Blocked(_) | ModStatus::ServerOnly | ModStatus::Unsupported(_)
        )
    }
}

/// Mods never toggled start enabled, except those the policy blocks or marks unsupported.
pub fn default_enabled(policy: Option<&PolicyEntry>) -> bool {
    !policy.is_some_and(|p| matches!(p.status, PolicyStatus::Blocked | PolicyStatus::Unsupported))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    pub entries: Vec<ModEntry>,
}

fn loader_name(kind: &ModKind) -> Option<String> {
    Some(match kind {
        ModKind::Fabric => return None,
        ModKind::NeoForge => "NeoForge mods are not supported yet".into(),
        ModKind::Forge => "Forge mods are not supported yet".into(),
        ModKind::Quilt => "Quilt mods are not supported yet".into(),
        ModKind::Invalid(why) => format!("unreadable: {why}"),
    })
}

/// Classifies every scanned jar against the policy, the user's choices and the dependency pre-check.
pub fn evaluate(
    mods: Vec<ModInfo>,
    policy: &Policy,
    settings: &ModSettings,
    host: &HostEnv,
) -> Report {
    let mut entries: Vec<ModEntry> = mods
        .into_iter()
        .map(|info| {
            let policy = info
                .is_fabric()
                .then(|| policy.get(&info.id).cloned())
                .flatten();
            let enabled = settings.is_enabled(&info.id, default_enabled(policy.as_ref()));
            let status = if let Some(why) = loader_name(&info.kind) {
                ModStatus::Unsupported(why)
            } else if info.side == Side::Server {
                ModStatus::ServerOnly
            } else if let Some(p) = policy
                .as_ref()
                .filter(|p| p.status == PolicyStatus::Blocked)
            {
                ModStatus::Blocked(p.reason.clone())
            } else if !enabled || settings.disabled_all {
                ModStatus::Disabled
            } else {
                ModStatus::Active
            };
            ModEntry {
                info,
                status,
                enabled,
                policy,
            }
        })
        .collect();

    let unavailable: HashMap<String, String> = entries
        .iter()
        .filter(|e| e.info.is_fabric())
        .filter_map(|e| {
            let why = match e.status {
                ModStatus::Blocked(_) => "blocked",
                ModStatus::Disabled => "disabled",
                ModStatus::ServerOnly => "server-only",
                _ => return None,
            };
            Some((e.info.id.clone(), why.to_owned()))
        })
        .collect();
    let active: Vec<usize> = (0..entries.len())
        .filter(|&i| entries[i].status == ModStatus::Active)
        .collect();
    let refs: Vec<&ModInfo> = active.iter().map(|&i| &entries[i].info).collect();
    let verdicts = resolve::resolve(&refs, &unavailable, host);
    for (&i, verdict) in active.iter().zip(verdicts) {
        if let Some(reason) = verdict {
            entries[i].status = ModStatus::Excluded(reason);
        }
    }
    Report { entries }
}

impl Report {
    pub fn active(&self) -> impl Iterator<Item = &ModEntry> {
        self.entries
            .iter()
            .filter(|e| e.status == ModStatus::Active)
    }

    /// Ids handed to the loader, sorted.
    pub fn active_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.active().map(|e| e.info.id.clone()).collect();
        ids.sort();
        ids
    }

    /// The jars for `fabric.addMods`; empty means no JVM starts.
    pub fn jars(&self) -> Vec<PathBuf> {
        self.active().map(|e| e.info.path.clone()).collect()
    }
}

/// `<data>/mods`, the policy and the host environment: everything a scan needs.
#[derive(Debug, Clone)]
pub struct Manager {
    pub data_dir: PathBuf,
    pub mods_dir: PathBuf,
    pub policy: Policy,
    pub host: HostEnv,
}

impl Manager {
    pub fn new(data_dir: &Path, mods_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_owned(),
            mods_dir: mods_dir.to_owned(),
            policy: Policy::load(data_dir),
            host: HostEnv::default(),
        }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.data_dir.join(mcr_settings::mods::FILE_NAME)
    }

    pub fn load_settings(&self) -> ModSettings {
        ModSettings::load(&self.settings_path())
    }

    pub fn scan(&self, settings: &ModSettings) -> Report {
        evaluate(
            scan::scan_dir(&self.mods_dir),
            &self.policy,
            settings,
            &self.host,
        )
    }

    pub fn session(&self) -> SessionStore {
        SessionStore::new(&self.data_dir)
    }
}

#[cfg(test)]
mod tests;
