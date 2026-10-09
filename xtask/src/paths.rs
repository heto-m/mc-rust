use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub struct Layout {
    root: PathBuf,
}

impl Layout {
    pub fn discover() -> Result<Self> {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest
            .parent()
            .context("xtask manifest directory has no parent")?;
        Ok(Self::new(root.to_path_buf()))
    }

    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join(".cache")
    }

    pub fn version_dir(&self, ver: &str) -> PathBuf {
        self.cache_dir().join("versions").join(ver)
    }

    pub fn version_json(&self, ver: &str) -> PathBuf {
        self.version_dir(ver).join("version.json")
    }

    pub fn client_jar(&self, ver: &str) -> PathBuf {
        self.version_dir(ver).join("client.jar")
    }

    pub fn server_jar(&self, ver: &str) -> PathBuf {
        self.version_dir(ver).join("server.jar")
    }

    pub fn datagen_dir(&self, ver: &str) -> PathBuf {
        self.version_dir(ver).join("datagen")
    }

    pub fn generated_dir(&self, ver: &str) -> PathBuf {
        self.version_dir(ver).join("generated")
    }

    pub fn src_dir(&self, ver: &str) -> PathBuf {
        self.version_dir(ver).join("src")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.cache_dir().join("assets")
    }

    pub fn tools_dir(&self) -> PathBuf {
        self.cache_dir().join("tools")
    }

    pub fn jdk_dir(&self) -> PathBuf {
        self.tools_dir().join("jdk25")
    }

    pub fn vineflower_jar(&self) -> PathBuf {
        self.tools_dir().join("vineflower-1.12.0.jar")
    }

    pub fn server_run_dir(&self, ver: &str) -> PathBuf {
        self.root.join("run").join(format!("server-{ver}"))
    }
}

pub fn java_bin(jdk_dir: &Path) -> PathBuf {
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    if cfg!(target_os = "macos") {
        jdk_dir.join("Contents/Home/bin").join(exe)
    } else {
        jdk_dir.join("bin").join(exe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout::new(PathBuf::from("/repo"))
    }

    #[test]
    fn version_artifacts_match_cache_layout() {
        let l = layout();
        assert_eq!(
            l.version_json("26.3"),
            Path::new("/repo/.cache/versions/26.3/version.json")
        );
        assert_eq!(
            l.client_jar("26.3"),
            Path::new("/repo/.cache/versions/26.3/client.jar")
        );
        assert_eq!(
            l.server_jar("26.3"),
            Path::new("/repo/.cache/versions/26.3/server.jar")
        );
        assert_eq!(
            l.datagen_dir("26.3"),
            Path::new("/repo/.cache/versions/26.3/datagen")
        );
        assert_eq!(
            l.generated_dir("26.3"),
            Path::new("/repo/.cache/versions/26.3/generated")
        );
        assert_eq!(
            l.src_dir("26.3"),
            Path::new("/repo/.cache/versions/26.3/src")
        );
    }

    #[test]
    fn tool_paths_match_cache_layout() {
        let l = layout();
        assert_eq!(l.jdk_dir(), Path::new("/repo/.cache/tools/jdk25"));
        assert_eq!(
            l.vineflower_jar(),
            Path::new("/repo/.cache/tools/vineflower-1.12.0.jar")
        );
    }

    #[test]
    fn server_run_dir_is_per_version_under_run() {
        assert_eq!(
            layout().server_run_dir("26.3"),
            Path::new("/repo/run/server-26.3")
        );
    }

    #[test]
    fn java_bin_is_under_bin_on_linux() {
        if cfg!(all(not(windows), not(target_os = "macos"))) {
            assert_eq!(
                java_bin(Path::new("/repo/.cache/tools/jdk25")),
                Path::new("/repo/.cache/tools/jdk25/bin/java")
            );
        }
    }
}
