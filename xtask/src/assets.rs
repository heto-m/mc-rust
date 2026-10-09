use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::checksum::{file_matches, verify_or_remove};
use crate::net::download_to;
use crate::paths::Layout;

const RESOURCES_URL: &str = "https://resources.download.minecraft.net";
const WORKERS: usize = 32;

#[derive(Debug, PartialEq, Eq)]
pub struct Object {
    pub name: String,
    pub hash: String,
}

impl Object {
    fn prefix(&self) -> &str {
        &self.hash[..2]
    }

    pub fn url(&self) -> String {
        format!("{RESOURCES_URL}/{}/{}", self.prefix(), self.hash)
    }

    pub fn path(&self, objects: &Path) -> PathBuf {
        objects.join(self.prefix()).join(&self.hash)
    }
}

pub fn parse_index(json: &str) -> Result<Vec<Object>> {
    let doc: Value = serde_json::from_str(json).context("asset index is not JSON")?;
    let objects = doc
        .get("objects")
        .and_then(Value::as_object)
        .context("asset index has no objects")?;
    let sorted: BTreeMap<_, _> = objects.iter().collect();
    sorted
        .into_iter()
        .map(|(name, entry)| {
            let hash = entry
                .get("hash")
                .and_then(Value::as_str)
                .filter(|h| h.len() >= 2 && h.is_ascii())
                .with_context(|| format!("object {name} has no valid hash"))?;
            Ok(Object {
                name: name.clone(),
                hash: hash.to_string(),
            })
        })
        .collect()
}

pub fn run(layout: &Layout, ver: &str, force: bool) -> Result<()> {
    let version_json = layout.version_json(ver);
    let meta: Value =
        serde_json::from_str(&fs::read_to_string(&version_json).with_context(|| {
            format!(
                "{} missing; run `cargo xtask fetch` first",
                version_json.display()
            )
        })?)?;
    let index = meta
        .get("assetIndex")
        .context("version.json has no assetIndex")?;
    let field = |k: &str| {
        index
            .get(k)
            .and_then(Value::as_str)
            .with_context(|| format!("assetIndex.{k} missing"))
    };
    let (id, sha1, url) = (field("id")?, field("sha1")?, field("url")?);

    let root = layout.assets_dir();
    let index_path = root.join("indexes").join(format!("{id}.json"));
    if force || !file_matches(&index_path, sha1)? {
        println!("[assets] index {id}");
        download_to(url, &index_path)?;
        verify_or_remove(&index_path, sha1)?;
    }
    let objects = parse_index(&fs::read_to_string(&index_path)?)?;
    download_objects(&objects, &root.join("objects"), force)?;
    println!(
        "[assets] {} objects ready in {}",
        objects.len(),
        root.display()
    );
    Ok(())
}

fn download_objects(objects: &[Object], dir: &Path, force: bool) -> Result<()> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let fetched = AtomicUsize::new(0);
    let errors = Mutex::new(Vec::new());
    thread::scope(|s| {
        for _ in 0..WORKERS.min(objects.len().max(1)) {
            s.spawn(|| {
                while let Some(obj) = objects.get(next.fetch_add(1, Ordering::Relaxed)) {
                    match fetch_object(obj, dir, force) {
                        Ok(true) => {
                            fetched.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(false) => {}
                        Err(e) => errors.lock().unwrap().push(format!("{}: {e:#}", obj.name)),
                    }
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if n.is_multiple_of(500) {
                        println!("[assets] {n}/{}", objects.len());
                    }
                }
            });
        }
    });
    let errors = errors.into_inner().unwrap();
    if !errors.is_empty() {
        bail!("{} object(s) failed:\n{}", errors.len(), errors.join("\n"));
    }
    println!("[assets] downloaded {}", fetched.load(Ordering::Relaxed));
    Ok(())
}

fn fetch_object(obj: &Object, dir: &Path, force: bool) -> Result<bool> {
    let dest = obj.path(dir);
    if !force && file_matches(&dest, &obj.hash)? {
        return Ok(false);
    }
    download_to(&obj.url(), &dest)?;
    verify_or_remove(&dest, &obj.hash)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"{"objects":{"minecraft/sounds.json":{"hash":"abcdef0123","size":1},
        "icons/a.png":{"hash":"1234567890","size":2}}}"#;

    #[test]
    fn parses_sorted_objects() {
        let objs = parse_index(INDEX).unwrap();
        assert_eq!(objs[0].name, "icons/a.png");
        assert_eq!(objs[1].hash, "abcdef0123");
    }

    #[test]
    fn object_url_and_path_use_two_char_prefix() {
        let obj = Object {
            name: "x".into(),
            hash: "abcdef".into(),
        };
        assert_eq!(
            obj.url(),
            "https://resources.download.minecraft.net/ab/abcdef"
        );
        assert_eq!(obj.path(Path::new("/o")), Path::new("/o/ab/abcdef"));
    }

    #[test]
    fn rejects_missing_hash() {
        assert!(parse_index(r#"{"objects":{"a":{"size":1}}}"#).is_err());
    }

    #[test]
    fn existing_verified_object_is_skipped() {
        let dir = std::env::temp_dir().join(format!("xtask-assets-{}", std::process::id()));
        let obj = Object {
            name: "a".into(),
            hash: crate::checksum::sha1_hex(b"hi"),
        };
        let path = obj.path(&dir);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"hi").unwrap();
        assert!(!fetch_object(&obj, &dir, false).unwrap());
        fs::remove_dir_all(&dir).unwrap();
    }
}
