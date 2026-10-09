use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::checksum::{file_matches, verify_or_remove};
use crate::manifest::{self, Artifact, MANIFEST_URL};
use crate::net::{download_to, get_text};
use crate::paths::Layout;

pub fn resolve_version(requested: Option<&str>) -> Result<String> {
    match requested {
        Some(v) => Ok(v.to_string()),
        None => Ok(manifest::parse_manifest(&get_text(MANIFEST_URL)?)?
            .latest_release()
            .to_string()),
    }
}

pub fn fetch(layout: &Layout, requested: Option<&str>, force: bool) -> Result<String> {
    let manifest = manifest::parse_manifest(&get_text(MANIFEST_URL)?)?;
    let ver = requested.unwrap_or(manifest.latest_release()).to_string();
    let entry = manifest.find(&ver)?;

    let version_json = layout.version_json(&ver);
    if force || !file_matches(&version_json, &entry.sha1)? {
        println!("[fetch] version.json {ver}");
        download_to(&entry.url, &version_json)?;
        verify_or_remove(&version_json, &entry.sha1)?;
    }

    let meta = manifest::parse_version_meta(&fs::read_to_string(&version_json)?)?;
    fetch_jar(&layout.client_jar(&ver), &meta.client, force)?;
    fetch_jar(&layout.server_jar(&ver), &meta.server, force)?;
    println!(
        "[fetch] {ver} ready in {}",
        layout.version_dir(&ver).display()
    );
    Ok(ver)
}

fn fetch_jar(dest: &Path, artifact: &Artifact, force: bool) -> Result<()> {
    let name = dest.file_name().unwrap_or_default().to_string_lossy();
    if !force && file_matches(dest, &artifact.sha1)? {
        println!("[fetch] {name} up to date");
        return Ok(());
    }
    println!("[fetch] downloading {name}");
    download_to(&artifact.url, dest)?;
    verify_or_remove(dest, &artifact.sha1)
}
