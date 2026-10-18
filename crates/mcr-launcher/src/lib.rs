//! The launcher's model: profiles (installations), the version list, accounts and launch plans.
//! No GPU or UI here; the app draws it with `mcr-ui`.

pub mod accounts;
pub mod launch;
pub mod profile;
pub mod store;
pub mod versions;

pub use accounts::{AccountEntry, AccountKind, SignIn, SignInEvent};
pub use launch::{LaunchAccount, LaunchError, LaunchPlan, OnlineSession};
pub use profile::{DEFAULT_PROFILE_ID, ICONS, LATEST_RELEASE, Profile, Resolution};
pub use store::{LauncherFile, Store};
pub use versions::{ManifestCache, VersionEntry, VersionKind, VersionList};

/// Seconds since the Unix epoch.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Keeps at most `max_chars` characters of `text` without control characters.
pub(crate) fn clean(text: &str, max_chars: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(max_chars)
        .collect()
}
