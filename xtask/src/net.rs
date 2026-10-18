use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Sent with every request; nothing that identifies the user.
pub const USER_AGENT: &str = "mc-rust-dev/0.1";

pub fn get_text(url: &str) -> Result<String> {
    let mut resp = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("GET {url}"))?;
    Ok(resp.body_mut().read_to_string()?)
}

pub fn download_to(url: &str, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let part = part_path(dest);
    let mut resp = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("GET {url}"))?;
    {
        let mut reader = resp.body_mut().as_reader();
        let mut file = File::create(&part)?;
        io::copy(&mut reader, &mut file).with_context(|| format!("writing {}", part.display()))?;
        file.sync_all()?;
    }
    fs::rename(&part, dest)?;
    Ok(())
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_path_appends_suffix_in_same_directory() {
        assert_eq!(
            part_path(Path::new("/c/client.jar")),
            Path::new("/c/client.jar.part")
        );
    }
}
