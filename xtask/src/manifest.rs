use anyhow::{Context, Result, bail};
use serde_json::Value;

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

pub struct Manifest {
    latest_release: String,
    versions: Vec<VersionEntry>,
}

pub struct VersionEntry {
    pub id: String,
    pub url: String,
    pub sha1: String,
}

pub struct Artifact {
    pub url: String,
    pub sha1: String,
}

pub struct VersionMeta {
    pub client: Artifact,
    pub server: Artifact,
}

impl Manifest {
    pub fn latest_release(&self) -> &str {
        &self.latest_release
    }

    pub fn find(&self, id: &str) -> Result<&VersionEntry> {
        self.versions
            .iter()
            .find(|v| v.id == id)
            .with_context(|| format!("version {id} not found in the version manifest"))
    }
}

pub fn parse_manifest(text: &str) -> Result<Manifest> {
    let root: Value = serde_json::from_str(text).context("version manifest is not JSON")?;
    let latest_release = str_at(&root["latest"], "release")?.to_string();
    let versions = root["versions"]
        .as_array()
        .context("version manifest has no versions array")?
        .iter()
        .map(|v| {
            Ok(VersionEntry {
                id: str_at(v, "id")?.to_string(),
                url: str_at(v, "url")?.to_string(),
                sha1: str_at(v, "sha1")?.to_string(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Manifest {
        latest_release,
        versions,
    })
}

pub fn parse_version_meta(text: &str) -> Result<VersionMeta> {
    let root: Value = serde_json::from_str(text).context("version.json is not JSON")?;
    let downloads = &root["downloads"];
    Ok(VersionMeta {
        client: artifact(&downloads["client"])?,
        server: artifact(&downloads["server"])?,
    })
}

fn artifact(v: &Value) -> Result<Artifact> {
    Ok(Artifact {
        url: str_at(v, "url")?.to_string(),
        sha1: str_at(v, "sha1")?.to_string(),
    })
}

fn str_at<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    match v.get(key).and_then(Value::as_str) {
        Some(s) => Ok(s),
        None => bail!("missing string field `{key}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
        "latest": { "release": "26.3", "snapshot": "26.4-snapshot-1" },
        "versions": [
            { "id": "26.4-snapshot-1", "type": "snapshot",
              "url": "https://example.test/snap.json", "sha1": "1111" },
            { "id": "26.3", "type": "release",
              "url": "https://example.test/26.3.json", "sha1": "2222" }
        ]
    }"#;

    const VERSION_JSON: &str = r#"{
        "id": "26.3",
        "downloads": {
            "client": { "sha1": "aaaa", "size": 1, "url": "https://example.test/client.jar" },
            "server": { "sha1": "bbbb", "size": 2, "url": "https://example.test/server.jar" }
        }
    }"#;

    #[test]
    fn latest_release_is_read_from_latest_field() {
        let m = parse_manifest(MANIFEST).unwrap();
        assert_eq!(m.latest_release(), "26.3");
    }

    #[test]
    fn find_returns_entry_with_url_and_sha1() {
        let m = parse_manifest(MANIFEST).unwrap();
        let e = m.find("26.3").unwrap();
        assert_eq!(e.url, "https://example.test/26.3.json");
        assert_eq!(e.sha1, "2222");
    }

    #[test]
    fn find_unknown_id_names_the_id() {
        let m = parse_manifest(MANIFEST).unwrap();
        let err = m.find("9.9.9").err().unwrap();
        assert!(err.to_string().contains("9.9.9"));
    }

    #[test]
    fn version_meta_yields_client_and_server_downloads() {
        let meta = parse_version_meta(VERSION_JSON).unwrap();
        assert_eq!(meta.client.url, "https://example.test/client.jar");
        assert_eq!(meta.client.sha1, "aaaa");
        assert_eq!(meta.server.url, "https://example.test/server.jar");
        assert_eq!(meta.server.sha1, "bbbb");
    }

    #[test]
    fn malformed_manifest_is_an_error() {
        assert!(parse_manifest(r#"{"latest":{}}"#).is_err());
    }
}
