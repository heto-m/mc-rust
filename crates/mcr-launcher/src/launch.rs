use std::path::{Path, PathBuf};

use std::sync::Arc;

use mcr_auth::Secret;
use mcr_auth::chat::ServicesKeys;
use mcr_auth::keys::{KeyRefresh, PlayerKeys};

use crate::profile::{Profile, Resolution};
use crate::versions::VersionList;

/// A Microsoft account's session for online-mode servers.
#[derive(Debug, Clone)]
pub struct OnlineSession {
    pub access_token: Secret,
    /// 32 hex digits.
    pub profile_id: String,
    /// Chat signing keys; `None` when the certificate service failed.
    pub keys: Option<PlayerKeys>,
    /// Mojang's keys for checking other players' chat keys; empty when unavailable.
    pub services: Arc<ServicesKeys>,
    pub refresh: Option<KeyRefresh>,
}

#[derive(Debug, Clone)]
pub struct LaunchAccount {
    pub name: String,
    pub online: Option<OnlineSession>,
}

/// Everything Play applies to the run.
#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub profile_id: String,
    pub version: String,
    pub game_dir: PathBuf,
    /// `None` keeps mod mode off.
    pub mods: Option<PathBuf>,
    pub heap_mib: u32,
    pub resolution: Option<Resolution>,
    pub account: LaunchAccount,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LaunchError {
    #[error("{0} is not supported yet")]
    Unsupported(String),
}

impl LaunchPlan {
    pub fn new(
        profile: &Profile,
        versions: &VersionList,
        data_dir: &Path,
        account: LaunchAccount,
    ) -> Result<Self, LaunchError> {
        let version = versions
            .resolve(&profile.version)
            .ok_or_else(|| LaunchError::Unsupported(profile.version.clone()))?;
        Ok(Self {
            profile_id: profile.id.clone(),
            version: version.to_owned(),
            game_dir: profile.game_dir_in(data_dir),
            mods: profile.mods.clone(),
            heap_mib: profile.heap_mib,
            resolution: profile.resolution,
            account,
        })
    }

    /// The client jar assets load from: always the canonical version's, whichever version the
    /// profile plays, because older versions are translated onto it.
    pub fn client_jar(&self, cache: &Path) -> PathBuf {
        cache
            .join("versions")
            .join(mcr_protocol::version::GAME_VERSION)
            .join("client.jar")
    }
}

#[cfg(test)]
mod tests {
    use mcr_protocol::version::GAME_VERSION;

    use super::*;
    use crate::profile::LATEST_RELEASE;

    fn offline() -> LaunchAccount {
        LaunchAccount {
            name: "Steve".into(),
            online: None,
        }
    }

    #[test]
    fn vanilla_profile_uses_default_game_dir() {
        let plan = LaunchPlan::new(
            &Profile::latest_release(),
            &VersionList::builtin(),
            Path::new("/data"),
            offline(),
        )
        .unwrap();
        assert_eq!(plan.version, GAME_VERSION);
        assert_eq!(plan.game_dir, Path::new("/data/client"));
        assert!(plan.mods.is_none());
        assert_eq!(
            plan.client_jar(Path::new(".cache")),
            Path::new(".cache/versions")
                .join(GAME_VERSION)
                .join("client.jar")
        );
    }

    #[test]
    fn mod_profile_and_unsupported_version() {
        let mut p = Profile::new("m", "Mods");
        p.version = GAME_VERSION.into();
        p.game_dir = Some("/games/m".into());
        p.mods = Some("/games/m/mods".into());
        p.heap_mib = 3072;
        let plan =
            LaunchPlan::new(&p, &VersionList::builtin(), Path::new("/d"), offline()).unwrap();
        assert_eq!(plan.game_dir, Path::new("/games/m"));
        assert_eq!(plan.mods.as_deref(), Some(Path::new("/games/m/mods")));
        assert_eq!(plan.heap_mib, 3072);
        p.version = "1.8.9".into();
        assert_eq!(
            LaunchPlan::new(&p, &VersionList::builtin(), Path::new("/d"), offline()).unwrap_err(),
            LaunchError::Unsupported("1.8.9".into())
        );
        p.version = LATEST_RELEASE.into();
        assert!(LaunchPlan::new(&p, &VersionList::builtin(), Path::new("/d"), offline()).is_ok());
    }

    #[test]
    fn every_supported_version_loads_canonical_assets() {
        for &version in mcr_protocol::version::SUPPORTED_VERSIONS {
            let mut p = Profile::new("v", "V");
            p.version = version.into();
            let plan =
                LaunchPlan::new(&p, &VersionList::builtin(), Path::new("/d"), offline()).unwrap();
            assert_eq!(plan.version, version);
            assert_eq!(
                plan.client_jar(Path::new("c")),
                Path::new("c/versions")
                    .join(GAME_VERSION)
                    .join("client.jar")
            );
        }
    }
}
