use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use serde_json::Value;

pub const POLICY_FILE: &str = "mod-policy.json";
const BUILTIN: &str = include_str!("../../data/mod-policy.json");
const MAX_BYTES: u64 = 1 << 20;
const MAX_ENTRIES: usize = 4096;
const MAX_REASON: usize = 200;
const MAX_HTTP_HOSTS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyStatus {
    /// Cannot be enabled.
    Blocked,
    /// Enabled only explicitly, with a warning.
    Unsupported,
    /// Loads, but some features are inert.
    Partial,
}

impl PolicyStatus {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "blocked" => Some(Self::Blocked),
            "unsupported" => Some(Self::Unsupported),
            "partial" => Some(Self::Partial),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyEntry {
    pub id: String,
    pub status: PolicyStatus,
    pub reason: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("policy is not valid JSON: {0}")]
    Json(String),
    #[error("policy has no `mods` list")]
    NoMods,
    #[error("policy has too many entries")]
    TooMany,
    #[error("entry {0} is malformed")]
    Entry(usize),
    #[error("mod id `{0}` appears twice")]
    Duplicate(String),
    #[error("policy file is too large")]
    TooLarge,
    #[error("`http_hosts` must be a list of at most 256 host names")]
    HttpHosts,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Policy {
    entries: Vec<PolicyEntry>,
    index: HashMap<String, usize>,
    http_hosts: Vec<String>,
}

impl Policy {
    pub fn builtin() -> Self {
        Self::parse(BUILTIN).expect("the built-in mod policy is valid")
    }

    /// `<data>/mod-policy.json` replaces the built-in policy when it parses; a bad file is logged and ignored.
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(POLICY_FILE);
        let mut text = String::new();
        let read = std::fs::File::open(&path)
            .and_then(|f| f.take(MAX_BYTES + 1).read_to_string(&mut text));
        match read {
            Ok(n) if n as u64 > MAX_BYTES => {
                tracing::warn!("{}: {}", path.display(), PolicyError::TooLarge);
            }
            Ok(_) => match Self::parse(&text) {
                Ok(policy) => return policy,
                Err(e) => tracing::warn!("{}: {e}; using the built-in policy", path.display()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!("{}: {e}", path.display()),
        }
        Self::builtin()
    }

    pub fn parse(text: &str) -> Result<Self, PolicyError> {
        if text.len() as u64 > MAX_BYTES {
            return Err(PolicyError::TooLarge);
        }
        let root: Value =
            serde_json::from_str(text).map_err(|e| PolicyError::Json(e.to_string()))?;
        let list = root["mods"].as_array().ok_or(PolicyError::NoMods)?;
        if list.len() > MAX_ENTRIES {
            return Err(PolicyError::TooMany);
        }
        let mut policy = Self::default();
        if let Some(hosts) = root.get("http_hosts") {
            policy.http_hosts = hosts
                .as_array()
                .filter(|a| a.len() <= MAX_HTTP_HOSTS)
                .and_then(|a| {
                    a.iter()
                        .map(|h| h.as_str().filter(|h| h.len() <= 255).map(str::to_owned))
                        .collect()
                })
                .ok_or(PolicyError::HttpHosts)?;
        }
        for (i, item) in list.iter().enumerate() {
            let bad = || PolicyError::Entry(i);
            let id = item["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(bad)?;
            let status = item["status"]
                .as_str()
                .and_then(PolicyStatus::parse)
                .ok_or_else(bad)?;
            let reason = item["reason"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(bad)?;
            if policy.index.contains_key(id) {
                return Err(PolicyError::Duplicate(id.to_owned()));
            }
            policy.index.insert(id.to_owned(), policy.entries.len());
            policy.entries.push(PolicyEntry {
                id: id.to_owned(),
                status,
                reason: reason.chars().take(MAX_REASON).collect(),
            });
        }
        Ok(policy)
    }

    pub fn get(&self, id: &str) -> Option<&PolicyEntry> {
        self.index.get(id).map(|&i| &self.entries[i])
    }

    pub fn entries(&self) -> &[PolicyEntry] {
        &self.entries
    }

    /// Hosts mods may reach over HTTP besides Mojang's, through the bridge's proxy.
    pub fn http_hosts(&self) -> &[String] {
        &self.http_hosts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_ids_are_unique_and_reasons_are_one_line() {
        let list: Value = serde_json::from_str(BUILTIN).unwrap();
        let ids: Vec<&str> = list["mods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap())
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "duplicate policy ids");
        let policy = Policy::builtin();
        assert_eq!(policy.entries().len(), ids.len());
        for e in policy.entries() {
            assert!(!e.reason.trim().is_empty(), "{} has no reason", e.id);
            assert!(!e.reason.contains('\n'), "{} reason spans lines", e.id);
        }
    }

    #[test]
    fn builtin_covers_the_design_table() {
        let p = Policy::builtin();
        for id in [
            "sodium",
            "iris",
            "immediatelyfast",
            "xaerominimap",
            "fancymenu",
        ] {
            assert_eq!(p.get(id).unwrap().status, PolicyStatus::Blocked, "{id}");
        }
        for id in ["voicechat", "freecam", "litematica", "dynamic_fps"] {
            assert_eq!(p.get(id).unwrap().status, PolicyStatus::Unsupported, "{id}");
        }
        for id in [
            "continuity",
            "moreculling",
            "lambdynlights",
            "c2me",
            "bobby",
        ] {
            assert_eq!(p.get(id).unwrap().status, PolicyStatus::Partial, "{id}");
        }
        assert!(p.get("fabric-api").is_none());
    }

    #[test]
    fn http_hosts_are_optional_and_bounded() {
        assert!(
            Policy::parse(r#"{"mods":[]}"#)
                .unwrap()
                .http_hosts()
                .is_empty()
        );
        let p = Policy::parse(r#"{"mods":[],"http_hosts":["api.modrinth.com","*.example.org"]}"#);
        assert_eq!(
            p.unwrap().http_hosts(),
            ["api.modrinth.com", "*.example.org"]
        );
        let many = format!("[{}]", vec![r#""a.b""#; 257].join(","));
        for bad in [r#""x""#, "[1]", many.as_str()] {
            let text = format!(r#"{{"mods":[],"http_hosts":{bad}}}"#);
            assert_eq!(Policy::parse(&text), Err(PolicyError::HttpHosts), "{bad}");
        }
    }

    #[test]
    fn rejects_bad_policies() {
        let entry = |extra: &str| format!(r#"{{"mods":[{extra}]}}"#);
        assert!(matches!(Policy::parse("nope"), Err(PolicyError::Json(_))));
        assert_eq!(Policy::parse("{}"), Err(PolicyError::NoMods));
        assert_eq!(
            Policy::parse(&entry(r#"{"id":"a","status":"blocked","reason":""}"#)),
            Err(PolicyError::Entry(0))
        );
        assert_eq!(
            Policy::parse(&entry(r#"{"id":"a","status":"fine","reason":"x"}"#)),
            Err(PolicyError::Entry(0))
        );
        let dup = r#"{"id":"a","status":"blocked","reason":"x"},{"id":"a","status":"partial","reason":"y"}"#;
        assert_eq!(
            Policy::parse(&entry(dup)),
            Err(PolicyError::Duplicate("a".into()))
        );
    }

    #[test]
    fn data_dir_overrides_the_builtin() {
        let dir = std::env::temp_dir().join(format!("mcr-policy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(POLICY_FILE),
            r#"{"mods":[{"id":"only","status":"partial","reason":"test"}]}"#,
        )
        .unwrap();
        let p = Policy::load(&dir);
        assert!(p.get("only").is_some());
        assert!(p.get("sodium").is_none());
        std::fs::write(dir.join(POLICY_FILE), "garbage").unwrap();
        assert!(Policy::load(&dir).get("sodium").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
