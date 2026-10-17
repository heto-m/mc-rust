use std::fs;
use std::path::{Component, Path, PathBuf};

use mcr_jvm::HomeSearch;
use serde_json::Value;

use crate::http::{self, Progress};
use crate::layout::Layout;
use crate::provision::Artifact;
use crate::{Result, SpError};

pub const RUNTIME_INDEX_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
const MAX_INDEX: u64 = 4 << 20;
const MAX_MANIFEST: u64 = 16 << 20;
const MAX_FILES: usize = 10_000;
const MAX_TOTAL: u64 = 1 << 30;
const MAX_FILE: u64 = 512 << 20;
const COMPLETE: &str = ".mcr-complete";

/// A Java 25+ the way the mod layer finds one: `MCR_JAVA_HOME`, the dev JDK, `JAVA_HOME`, `java` on the path.
pub fn find_java(dev_cache: Option<&Path>) -> Option<PathBuf> {
    match HomeSearch::from_env(dev_cache).locate() {
        Ok(home) => Some(home.java()),
        Err(e) => {
            tracing::info!("no system Java for singleplayer: {e}");
            None
        }
    }
}

fn java_in(root: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    [
        root.join("bin").join(exe),
        root.join("jre.bundle/Contents/Home/bin").join(exe),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// The `java` of a previously completed runtime install.
pub fn installed_runtime(layout: &Layout, component: &str) -> Option<PathBuf> {
    let root = layout.runtime_dir(component)?;
    root.join(COMPLETE).is_file().then(|| java_in(&root))?
}

pub fn platform() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux",
        ("linux", "x86") => "linux-i386",
        ("macos", "x86_64") => "mac-os",
        ("macos", "aarch64") => "mac-os-arm64",
        ("windows", "x86_64") => "windows-x64",
        ("windows", "x86") => "windows-x86",
        ("windows", "aarch64") => "windows-arm64",
        _ => return None,
    })
}

/// The manifest of `component` for `platform` in Mojang's runtime index.
pub fn manifest_ref(index: &[u8], platform: &str, component: &str) -> Result<Artifact> {
    let root: Value = serde_json::from_slice(index)
        .map_err(|e| SpError::data(format!("runtime index is not JSON: {e}")))?;
    let m = &root[platform][component][0]["manifest"];
    match (m["url"].as_str(), m["sha1"].as_str()) {
        (Some(url), Some(sha1)) => Ok(Artifact {
            url: url.to_owned(),
            sha1: sha1.to_owned(),
            size: m["size"].as_u64(),
        }),
        _ => Err(SpError::data(format!(
            "no Java runtime {component} for {platform}"
        ))),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Dir(PathBuf),
    File {
        path: PathBuf,
        url: String,
        sha1: String,
        size: u64,
        executable: bool,
    },
    Link {
        path: PathBuf,
        target: PathBuf,
    },
}

fn relative(path: &str) -> Option<PathBuf> {
    let p = Path::new(path);
    let plain = !path.is_empty()
        && path.len() <= 1024
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && p.components().all(|c| matches!(c, Component::Normal(_)));
    plain.then(|| p.to_path_buf())
}

/// Whether a link at `path` pointing at `target` stays inside the runtime root.
fn link_inside(path: &Path, target: &str) -> bool {
    if target.is_empty() || target.contains(['\\', '\0']) || Path::new(target).is_absolute() {
        return false;
    }
    let mut depth: Vec<&std::ffi::OsStr> = path
        .parent()
        .map(|p| p.iter().collect())
        .unwrap_or_default();
    for c in Path::new(target).components() {
        match c {
            Component::Normal(n) => depth.push(n),
            Component::CurDir => {}
            Component::ParentDir => {
                if depth.pop().is_none() {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

pub fn parse_manifest(json: &[u8]) -> Result<Vec<Entry>> {
    let root: Value = serde_json::from_slice(json)
        .map_err(|e| SpError::data(format!("runtime manifest is not JSON: {e}")))?;
    let files = root["files"]
        .as_object()
        .ok_or_else(|| SpError::data("runtime manifest has no files"))?;
    if files.len() > MAX_FILES {
        return Err(SpError::data("runtime manifest lists too many files"));
    }
    let mut total = 0u64;
    let mut entries = Vec::with_capacity(files.len());
    for (name, v) in files {
        let path = relative(name)
            .ok_or_else(|| SpError::data(format!("runtime path {name:?} leaves the runtime")))?;
        let entry = match v["type"].as_str() {
            Some("directory") => Entry::Dir(path),
            Some("file") => {
                let raw = &v["downloads"]["raw"];
                let (Some(url), Some(sha1), Some(size)) = (
                    raw["url"].as_str(),
                    raw["sha1"].as_str(),
                    raw["size"].as_u64(),
                ) else {
                    return Err(SpError::data(format!(
                        "runtime file {name:?} has no download"
                    )));
                };
                if size > MAX_FILE {
                    return Err(SpError::TooLarge(name.clone(), MAX_FILE));
                }
                total += size;
                Entry::File {
                    path,
                    url: url.to_owned(),
                    sha1: sha1.to_owned(),
                    size,
                    executable: v["executable"].as_bool().unwrap_or(false),
                }
            }
            Some("link") => {
                let target = v["target"].as_str().unwrap_or_default();
                if !link_inside(&path, target) {
                    return Err(SpError::data(format!(
                        "runtime link {name:?} points outside the runtime"
                    )));
                }
                Entry::Link {
                    path,
                    target: PathBuf::from(target),
                }
            }
            _ => return Err(SpError::data(format!("runtime entry {name:?} has no type"))),
        };
        entries.push(entry);
    }
    if total > MAX_TOTAL {
        return Err(SpError::TooLarge("Java runtime".into(), MAX_TOTAL));
    }
    Ok(entries)
}

/// Installs Mojang's `component` runtime into `<game dir>/runtime/<component>` and returns its `java`.
pub fn install_runtime(
    layout: &Layout,
    component: &str,
    progress: Progress<'_>,
) -> Result<PathBuf> {
    let root = layout
        .runtime_dir(component)
        .ok_or_else(|| SpError::data(format!("bad runtime name {component:?}")))?;
    let platform =
        platform().ok_or_else(|| SpError::data("no Mojang Java runtime for this platform"))?;
    let index = http::get_bytes(RUNTIME_INDEX_URL, MAX_INDEX)?;
    let manifest = manifest_ref(&index, platform, component)?;
    let json = http::get_bytes(&manifest.url, MAX_MANIFEST)?;
    if !http::sha1_hex(&json).eq_ignore_ascii_case(&manifest.sha1) {
        return Err(SpError::Hash("Java runtime manifest".into()));
    }
    let entries = parse_manifest(&json)?;
    let staging = root.with_extension("staging");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let result = fill(&staging, &entries, progress);
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    fs::write(staging.join(COMPLETE), component)?;
    let _ = fs::remove_dir_all(&root);
    fs::rename(&staging, &root)?;
    java_in(&root).ok_or_else(|| SpError::data("the Java runtime has no java executable"))
}

fn fill(root: &Path, entries: &[Entry], progress: Progress<'_>) -> Result<()> {
    let total: u64 = entries
        .iter()
        .map(|e| match e {
            Entry::File { size, .. } => *size,
            _ => 0,
        })
        .sum();
    let mut done = 0u64;
    for entry in entries {
        if let Entry::Dir(path) = entry {
            fs::create_dir_all(root.join(path))?;
        }
    }
    for entry in entries {
        let Entry::File {
            path,
            url,
            sha1,
            size,
            executable,
        } = entry
        else {
            continue;
        };
        let dest = root.join(path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let base = done;
        http::download(url, &dest, sha1, Some(*size), MAX_FILE, &mut |n, _| {
            progress(base + n, Some(total))
        })?;
        done += size;
        #[cfg(unix)]
        if *executable {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))?;
        }
        #[cfg(not(unix))]
        let _ = executable;
    }
    for entry in entries {
        if let Entry::Link { path, target } = entry {
            let dest = root.join(path);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &dest)?;
            #[cfg(not(unix))]
            if let Some(parent) = path.parent() {
                let _ = fs::copy(root.join(parent).join(target), &dest);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "a34ef485e0f2a5250d1213f8e596b4a343efbec8";

    #[test]
    fn index_lookup() {
        let index = format!(
            r#"{{"linux":{{"java-runtime-epsilon":[{{"manifest":{{"sha1":"{SHA}","size":88214,"url":"https://piston-meta.mojang.com/m.json"}}}}]}}}}"#
        );
        let m = manifest_ref(index.as_bytes(), "linux", "java-runtime-epsilon").unwrap();
        assert_eq!(m.url, "https://piston-meta.mojang.com/m.json");
        assert!(manifest_ref(index.as_bytes(), "mac-os", "java-runtime-epsilon").is_err());
    }

    fn file(name: &str) -> String {
        format!(
            r#""{name}":{{"type":"file","executable":true,"downloads":{{"raw":{{"sha1":"{SHA}","size":12328,"url":"https://x/java"}}}}}}"#
        )
    }

    #[test]
    fn manifest_entries() {
        let json = format!(
            r#"{{"files":{{"bin":{{"type":"directory"}},{},"legal/a/LICENSE":{{"type":"link","target":"../java.base/LICENSE"}}}}}}"#,
            file("bin/java")
        );
        let entries = parse_manifest(json.as_bytes()).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().any(|e| matches!(
            e,
            Entry::File { path, executable: true, .. } if path == Path::new("bin/java")
        )));
    }

    #[test]
    fn hostile_manifests_are_rejected() {
        for bad in [
            format!(r#"{{"files":{{{}}}}}"#, file("../escape")),
            format!(r#"{{"files":{{{}}}}}"#, file("/etc/passwd")),
            format!(r#"{{"files":{{{}}}}}"#, file("a\\b")),
            r#"{"files":{"lib/x":{"type":"link","target":"../../etc/passwd"}}}"#.to_owned(),
            r#"{"files":{"x":{"type":"link","target":"/etc/passwd"}}}"#.to_owned(),
            r#"{"files":{"x":{"type":"weird"}}}"#.to_owned(),
            r#"{"files":[]}"#.to_owned(),
        ] {
            assert!(parse_manifest(bad.as_bytes()).is_err(), "{bad}");
        }
        let huge = r#"{"files":{"x":{"type":"file","downloads":{"raw":{"sha1":"a","size":99999999999,"url":"https://x"}}}}}"#;
        assert!(parse_manifest(huge.as_bytes()).is_err());
    }

    #[test]
    fn links_stay_inside() {
        assert!(link_inside(
            Path::new("legal/a/LICENSE"),
            "../java.base/LICENSE"
        ));
        assert!(link_inside(Path::new("a/b"), "c"));
        assert!(!link_inside(Path::new("a/b"), "../../c"));
        assert!(!link_inside(Path::new("top"), "../x"));
    }
}
