use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::JvmError;

pub const REQUIRED_MAJOR: u32 = 25;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaHome {
    path: PathBuf,
    major: u32,
}

impl JavaHome {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, JvmError> {
        let mut path = path.into();
        let bundle_home = path.join("Contents/Home");
        if !path.join("release").is_file() && bundle_home.join("release").is_file() {
            path = bundle_home;
        }
        let release =
            fs::read_to_string(path.join("release")).map_err(|e| JvmError::InvalidHome {
                path: path.clone(),
                reason: format!("cannot read release file: {e}"),
            })?;
        let major = parse_release_major(&release).ok_or_else(|| JvmError::InvalidHome {
            path: path.clone(),
            reason: "release file has no parsable JAVA_VERSION".into(),
        })?;
        if major < REQUIRED_MAJOR {
            return Err(JvmError::JavaTooOld {
                path,
                found: major,
                required: REQUIRED_MAJOR,
            });
        }
        Ok(Self { path, major })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn major(&self) -> u32 {
        self.major
    }

    pub fn libjvm(&self) -> PathBuf {
        if cfg!(windows) {
            self.path.join("bin/server/jvm.dll")
        } else if cfg!(target_os = "macos") {
            self.path.join("lib/server/libjvm.dylib")
        } else {
            self.path.join("lib/server/libjvm.so")
        }
    }
}

/// Candidate Java homes, checked in field order.
#[derive(Debug, Clone, Default)]
pub struct HomeSearch {
    pub mcr_java_home: Option<PathBuf>,
    pub cache_jdk: Option<PathBuf>,
    pub java_home: Option<PathBuf>,
    pub probe_path_java: bool,
}

impl HomeSearch {
    pub fn from_env(cache_dir: Option<&Path>) -> Self {
        let var = |name| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Self {
            mcr_java_home: var("MCR_JAVA_HOME"),
            cache_jdk: cache_dir.map(|c| c.join("tools/jdk25")),
            java_home: var("JAVA_HOME"),
            probe_path_java: true,
        }
    }

    pub fn locate(&self) -> Result<JavaHome, JvmError> {
        if let Some(path) = &self.mcr_java_home {
            return JavaHome::open(path);
        }
        let mut rejected = Vec::new();
        let candidates = [self.cache_jdk.clone(), self.java_home.clone()];
        for path in candidates.into_iter().flatten() {
            if !path.exists() {
                continue;
            }
            match JavaHome::open(&path) {
                Ok(home) => return Ok(home),
                Err(e) => rejected.push(e),
            }
        }
        if self.probe_path_java
            && let Some(path) = probe_path_java_home()
        {
            match JavaHome::open(&path) {
                Ok(home) => return Ok(home),
                Err(e) => rejected.push(e),
            }
        }
        if rejected.len() == 1 {
            return Err(rejected.remove(0));
        }
        let tried = if rejected.is_empty() {
            "set MCR_JAVA_HOME or run `cargo xtask jdk --force`".to_owned()
        } else {
            rejected
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        };
        Err(JvmError::NoJavaHome {
            required: REQUIRED_MAJOR,
            tried,
        })
    }
}

pub fn dev_cache_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache")
}

pub fn parse_release_major(release: &str) -> Option<u32> {
    let line = release
        .lines()
        .find_map(|l| l.trim().strip_prefix("JAVA_VERSION="))?;
    parse_version_major(line.trim().trim_matches('"'))
}

fn parse_version_major(version: &str) -> Option<u32> {
    let mut parts = version.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

fn probe_path_java_home() -> Option<PathBuf> {
    let out = Command::new("java")
        .args(["-XshowSettings:properties", "-version"])
        .output()
        .ok()?;
    parse_java_home_property(&String::from_utf8_lossy(&out.stderr))
}

pub fn parse_java_home_property(settings: &str) -> Option<PathBuf> {
    settings.lines().find_map(|l| {
        let value = l.trim().strip_prefix("java.home")?.trim_start();
        let value = value.strip_prefix('=')?.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str, version: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mcr-jvm-home-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("release"),
            format!("IMPLEMENTOR=\"Test\"\nJAVA_VERSION=\"{version}\"\nOS_NAME=\"Linux\"\n"),
        )
        .unwrap();
        dir
    }

    fn search(mcr: Option<&Path>, cache: Option<&Path>, java: Option<&Path>) -> HomeSearch {
        HomeSearch {
            mcr_java_home: mcr.map(Path::to_path_buf),
            cache_jdk: cache.map(Path::to_path_buf),
            java_home: java.map(Path::to_path_buf),
            probe_path_java: false,
        }
    }

    #[test]
    fn parses_release_versions() {
        assert_eq!(parse_release_major("JAVA_VERSION=\"25.0.4.1\""), Some(25));
        assert_eq!(parse_release_major("X=1\nJAVA_VERSION=\"21\"\n"), Some(21));
        assert_eq!(parse_release_major("JAVA_VERSION=\"1.8.0_402\""), Some(8));
        assert_eq!(parse_release_major("JAVA_VERSION=\"26-ea\""), Some(26));
        assert_eq!(parse_release_major("IMPLEMENTOR=\"x\""), None);
    }

    #[test]
    fn parses_java_home_property() {
        let text = "Property settings:\n    java.class.path = \n    java.home = /usr/lib/jvm/java-25\n    java.vendor = x\n";
        assert_eq!(
            parse_java_home_property(text),
            Some(PathBuf::from("/usr/lib/jvm/java-25"))
        );
        assert_eq!(parse_java_home_property("java.vendor = x"), None);
    }

    #[test]
    fn old_jdk_rejected_naming_home_and_required_version() {
        let old = fixture("old", "21.0.4");
        let err = search(None, None, Some(&old)).locate().unwrap_err();
        let msg = err.to_string();
        assert!(matches!(
            err,
            JvmError::JavaTooOld {
                found: 21,
                required: 25,
                ..
            }
        ));
        assert!(msg.contains(&old.display().to_string()), "{msg}");
        assert!(msg.contains("25"), "{msg}");
    }

    #[test]
    fn override_wins_over_java_home() {
        let a = fixture("override-a", "25.0.1");
        let b = fixture("override-b", "26.0.0");
        let home = search(Some(&a), None, Some(&b)).locate().unwrap();
        assert_eq!(home.path(), a);
        assert_eq!(home.major(), 25);
    }

    #[test]
    fn invalid_override_is_an_error_not_a_fallback() {
        let old = fixture("override-old", "17.0.2");
        let good = fixture("override-good", "25");
        let err = search(Some(&old), Some(&good), None).locate().unwrap_err();
        assert!(matches!(err, JvmError::JavaTooOld { found: 17, .. }));
    }

    #[test]
    fn cache_jdk_precedes_java_home_and_skips_old_ones() {
        let cache = fixture("cache", "25.0.4");
        let java = fixture("javahome", "26");
        assert_eq!(
            search(None, Some(&cache), Some(&java))
                .locate()
                .unwrap()
                .path(),
            cache
        );
        let old_cache = fixture("cache-old", "21");
        assert_eq!(
            search(None, Some(&old_cache), Some(&java))
                .locate()
                .unwrap()
                .path(),
            java
        );
    }

    #[test]
    fn missing_candidates_report_no_home() {
        let missing = std::env::temp_dir().join("mcr-jvm-home-does-not-exist");
        let err = search(None, Some(&missing), None).locate().unwrap_err();
        assert!(matches!(err, JvmError::NoJavaHome { required: 25, .. }));
    }

    #[test]
    fn libjvm_path_is_under_lib_server_on_linux() {
        if cfg!(target_os = "linux") {
            let home = JavaHome::open(fixture("libjvm", "25")).unwrap();
            assert!(home.libjvm().ends_with("lib/server/libjvm.so"));
        }
    }
}
