use std::fmt;

use crate::manager::{ModInfo, ModKind};

/// The mod loader Java boots under; one per session, chosen from the enabled mods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Loader {
    #[default]
    Fabric,
    NeoForge,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "cannot mix Fabric and NeoForge mods in one session: {fabric} is a Fabric mod and {neoforge} \
     a NeoForge mod; disable or remove the mods of one loader"
)]
pub struct MixedLoaders {
    pub fabric: String,
    pub neoforge: String,
}

impl Loader {
    pub fn name(self) -> &'static str {
        match self {
            Self::Fabric => "Fabric",
            Self::NeoForge => "NeoForge",
        }
    }

    /// The launch manifest under the cache dir.
    pub fn manifest(self) -> &'static str {
        match self {
            Self::Fabric => "fabric/launch.json",
            Self::NeoForge => "neoforge/launch.json",
        }
    }

    /// The `launch.json` `loader` value; Fabric's manifest predates the field.
    pub fn id(self) -> &'static str {
        match self {
            Self::Fabric => "fabric",
            Self::NeoForge => "neoforge",
        }
    }

    pub fn fetch_command(self) -> &'static str {
        match self {
            Self::Fabric => "cargo xtask fabric-fetch",
            Self::NeoForge => "cargo xtask neoforge-fetch",
        }
    }

    pub fn of_kind(kind: &ModKind) -> Option<Self> {
        match kind {
            ModKind::Fabric => Some(Self::Fabric),
            ModKind::NeoForge => Some(Self::NeoForge),
            _ => None,
        }
    }

    /// The loader the mods need: `None` for no loadable mod, an error when they need both.
    pub fn for_mods<'a>(
        mods: impl IntoIterator<Item = &'a ModInfo>,
    ) -> Result<Option<Self>, MixedLoaders> {
        let (mut fabric, mut neoforge) = (None, None);
        for m in mods {
            match Self::of_kind(&m.kind) {
                Some(Self::Fabric) => fabric = fabric.or(Some(m)),
                Some(Self::NeoForge) => neoforge = neoforge.or(Some(m)),
                None => {}
            }
        }
        match (fabric, neoforge) {
            (Some(f), Some(n)) => Err(MixedLoaders {
                fabric: describe(f),
                neoforge: describe(n),
            }),
            (Some(_), None) => Ok(Some(Self::Fabric)),
            (None, Some(_)) => Ok(Some(Self::NeoForge)),
            (None, None) => Ok(None),
        }
    }
}

fn describe(m: &ModInfo) -> String {
    format!("`{}` ({})", m.id, m.file_name)
}

impl fmt::Display for Loader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::fixture::Jar;
    use crate::manager::scan::scan_reader;
    use std::io::Cursor;
    use std::path::Path;

    fn info(name: &str, jar: Jar) -> ModInfo {
        scan_reader(Path::new(name), name, Cursor::new(jar.build()))
    }

    fn fabric_jar(id: &str) -> Jar {
        Jar::fabric(id, "1.0.0")
    }

    fn neoforge_jar(id: &str) -> Jar {
        Jar::neoforge(id, "1.0.0")
    }

    #[test]
    fn loader_follows_the_mods() {
        let fabric = info("a.jar", fabric_jar("alpha"));
        let neo = info("b.jar", neoforge_jar("beta"));
        assert_eq!(Loader::for_mods([&fabric]), Ok(Some(Loader::Fabric)));
        assert_eq!(Loader::for_mods([&neo]), Ok(Some(Loader::NeoForge)));
        assert_eq!(Loader::for_mods(std::iter::empty()), Ok(None));
    }

    #[test]
    fn mixed_sets_are_refused_naming_both() {
        let fabric = info("a.jar", fabric_jar("alpha"));
        let neo = info("b.jar", neoforge_jar("beta"));
        let err = Loader::for_mods([&fabric, &neo]).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("`alpha` (a.jar)"), "{text}");
        assert!(text.contains("`beta` (b.jar)"), "{text}");
        assert!(text.contains("cannot mix Fabric and NeoForge"), "{text}");
    }
}
