use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use crate::http::{self, Progress};
use crate::layout::{Layout, VERSION};
use crate::{Result, SpError};

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const MAX_MANIFEST: u64 = 8 << 20;
const MAX_VERSION_JSON: u64 = 2 << 20;
const MAX_SERVER_JAR: u64 = 512 << 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub url: String,
    pub sha1: String,
    pub size: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionInfo {
    pub server: Artifact,
    pub java_component: Option<String>,
    pub java_major: Option<u32>,
}

fn sha1_ok(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn artifact(v: &Value) -> Option<Artifact> {
    let url = v["url"].as_str()?.to_owned();
    let sha1 = v["sha1"].as_str().filter(|s| sha1_ok(s))?.to_owned();
    Some(Artifact {
        url,
        sha1,
        size: v["size"].as_u64(),
    })
}

/// The version JSON entry for `id` in Mojang's version manifest.
pub fn find_version(manifest: &[u8], id: &str) -> Result<Artifact> {
    let root: Value = serde_json::from_slice(manifest)
        .map_err(|e| SpError::data(format!("version manifest is not JSON: {e}")))?;
    root["versions"]
        .as_array()
        .and_then(|list| list.iter().find(|v| v["id"].as_str() == Some(id)))
        .and_then(artifact)
        .ok_or_else(|| SpError::data(format!("version {id} is not in the version manifest")))
}

pub fn parse_version(json: &[u8]) -> Result<VersionInfo> {
    let root: Value = serde_json::from_slice(json)
        .map_err(|e| SpError::data(format!("version JSON is not JSON: {e}")))?;
    let server = artifact(&root["downloads"]["server"])
        .ok_or_else(|| SpError::data("version JSON has no server download"))?;
    let java = &root["javaVersion"];
    Ok(VersionInfo {
        server,
        java_component: java["component"].as_str().map(str::to_owned),
        java_major: java["majorVersion"]
            .as_u64()
            .and_then(|m| u32::try_from(m).ok()),
    })
}

/// The cached version JSON, or a fresh one from the manifest (checked against its SHA-1 there).
pub fn version_info(layout: &Layout) -> Result<VersionInfo> {
    let path = layout.version_json();
    if let Ok(bytes) = fs::read(&path)
        && let Ok(info) = parse_version(&bytes)
    {
        return Ok(info);
    }
    let manifest = http::get_bytes(MANIFEST_URL, MAX_MANIFEST)?;
    let entry = find_version(&manifest, VERSION)?;
    let json = http::get_bytes(&entry.url, MAX_VERSION_JSON)?;
    if !http::sha1_hex(&json).eq_ignore_ascii_case(&entry.sha1) {
        return Err(SpError::Hash(format!("version {VERSION} JSON")));
    }
    let info = parse_version(&json)?;
    fs::create_dir_all(layout.version_dir())?;
    fs::write(&path, &json)?;
    Ok(info)
}

/// A verified server jar, downloading it when it is missing or does not match.
pub fn ensure_server_jar(layout: &Layout, progress: Progress<'_>) -> Result<PathBuf> {
    let jar = layout.server_jar();
    let info = version_info(layout)?;
    if jar.is_file()
        && http::file_sha1(&jar).is_ok_and(|h| h.eq_ignore_ascii_case(&info.server.sha1))
    {
        return Ok(jar);
    }
    fs::create_dir_all(layout.version_dir())?;
    http::download(
        &info.server.url,
        &jar,
        &info.server.sha1,
        info.server.size,
        MAX_SERVER_JAR,
        progress,
    )?;
    Ok(jar)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c";

    #[test]
    fn manifest_lookup() {
        let manifest = format!(
            r#"{{"latest":{{"release":"26.3"}},"versions":[
            {{"id":"26.2","url":"https://x/26.2.json","sha1":"{SHA}"}},
            {{"id":"26.3","url":"https://x/26.3.json","sha1":"{SHA}"}}]}}"#
        );
        let entry = find_version(manifest.as_bytes(), "26.3").unwrap();
        assert_eq!(entry.url, "https://x/26.3.json");
        assert!(find_version(manifest.as_bytes(), "1.0").is_err());
        assert!(find_version(b"{", "26.3").is_err());
        let bad_sha = manifest.replace(SHA, "zz");
        assert!(find_version(bad_sha.as_bytes(), "26.3").is_err());
    }

    #[test]
    fn version_json_server_and_java() {
        let json = format!(
            r#"{{"downloads":{{"server":{{"sha1":"{SHA}","size":62294556,
            "url":"https://piston-data.mojang.com/v1/objects/{SHA}/server.jar"}}}},
            "javaVersion":{{"component":"java-runtime-epsilon","majorVersion":25}}}}"#
        );
        let info = parse_version(json.as_bytes()).unwrap();
        assert_eq!(info.server.size, Some(62_294_556));
        assert_eq!(info.java_component.as_deref(), Some("java-runtime-epsilon"));
        assert_eq!(info.java_major, Some(25));
        assert!(parse_version(br#"{"downloads":{}}"#).is_err());
    }
}
