use std::collections::HashMap;
use std::path::Path;

use crate::error::{AssetError, Result};
use crate::pack::{PackFormat, PackMeta};
use crate::source::{AssetSource, DirSource, ZipSource};

#[derive(Debug)]
struct Pack {
    name: String,
    source: Box<dyn AssetSource>,
    meta: PackMeta,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    pack: u32,
    real_path: u32,
}

#[derive(Debug)]
pub struct ResourceManager {
    target_format: PackFormat,
    packs: Vec<Pack>,
    paths: Vec<Box<str>>,
    index: HashMap<Box<str>, Entry>,
}

impl Default for ResourceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceManager {
    pub fn new() -> Self {
        Self::with_target_format(PackFormat::CURRENT)
    }

    pub fn with_target_format(target_format: PackFormat) -> Self {
        Self {
            target_format,
            packs: Vec::new(),
            paths: Vec::new(),
            index: HashMap::new(),
        }
    }

    pub fn vanilla(jar: impl AsRef<Path>) -> Result<Self> {
        let mut manager = Self::new();
        manager.push_pack("vanilla", Box::new(ZipSource::open(jar)?))?;
        Ok(manager)
    }

    pub fn push_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let name = path.display().to_string();
        let source: Box<dyn AssetSource> = if path.is_dir() {
            Box::new(DirSource::new(path))
        } else {
            Box::new(ZipSource::open(path)?)
        };
        self.push_pack(name, source)
    }

    pub fn push_pack(
        &mut self,
        name: impl Into<String>,
        source: Box<dyn AssetSource>,
    ) -> Result<()> {
        let name = name.into();
        let meta = match source.read("pack.mcmeta")? {
            Some(bytes) => PackMeta::parse(&bytes).map_err(|source| AssetError::Json {
                path: format!("{name}/pack.mcmeta"),
                source,
            })?,
            None => PackMeta::default(),
        };
        self.insert_pack(name, source, meta)
    }

    fn insert_pack(
        &mut self,
        name: String,
        source: Box<dyn AssetSource>,
        meta: PackMeta,
    ) -> Result<()> {
        let pack_idx = self.packs.len() as u32;
        let listing = source.list()?;
        let overlay_dirs: Vec<String> = meta
            .overlays
            .iter()
            .map(|o| format!("{}/", o.directory))
            .collect();

        for path in &listing {
            if overlay_dirs.iter().any(|d| path.starts_with(d.as_str())) {
                continue;
            }
            self.insert(path, pack_idx, path);
        }
        for overlay in meta.active_overlays(self.target_format) {
            let prefix = format!("{}/", overlay.directory);
            for path in &listing {
                if let Some(stripped) = path.strip_prefix(&prefix) {
                    self.insert(stripped, pack_idx, path);
                }
            }
        }

        self.packs.push(Pack { name, source, meta });
        Ok(())
    }

    fn insert(&mut self, virtual_path: &str, pack: u32, real_path: &str) {
        let real_idx = self.paths.len() as u32;
        self.paths.push(real_path.into());
        self.index.insert(
            virtual_path.into(),
            Entry {
                pack,
                real_path: real_idx,
            },
        );
    }

    pub fn pack_names(&self) -> impl Iterator<Item = &str> {
        self.packs.iter().map(|p| p.name.as_str())
    }

    pub fn pack_meta(&self, name: &str) -> Option<&PackMeta> {
        self.packs.iter().find(|p| p.name == name).map(|p| &p.meta)
    }

    pub fn exists(&self, path: &str) -> bool {
        self.index.contains_key(path)
    }

    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let Some(entry) = self.index.get(path) else {
            return Ok(None);
        };
        let real = &self.paths[entry.real_path as usize];
        self.packs[entry.pack as usize].source.read(real)
    }

    pub fn list_prefix<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a str> {
        self.index
            .keys()
            .map(|k| &**k)
            .filter(move |k| k.starts_with(prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, zip_bytes};

    #[test]
    fn later_pack_overrides_earlier() {
        let mut rm = ResourceManager::new();
        rm.push_pack(
            "base",
            Box::new(
                ZipSource::from_bytes(zip_bytes(&[("a.txt", b"base"), ("b.txt", b"b")])).unwrap(),
            ),
        )
        .unwrap();
        let dir = TempDir::new("override");
        dir.write("a.txt", b"over");
        rm.push_pack("over", Box::new(DirSource::new(dir.path())))
            .unwrap();
        assert_eq!(rm.read("a.txt").unwrap().unwrap(), b"over");
        assert_eq!(rm.read("b.txt").unwrap().unwrap(), b"b");
        assert_eq!(rm.read("c.txt").unwrap(), None);
    }

    #[test]
    fn zip_and_dir_agree() {
        let files: &[(&str, &[u8])] = &[("x/y.txt", b"1"), ("z.txt", b"2")];
        let zip = ZipSource::from_bytes(zip_bytes(files)).unwrap();
        let dir = TempDir::new("agree");
        for (p, b) in files {
            dir.write(p, b);
        }
        let dir = DirSource::new(dir.path());
        let mut a = zip.list().unwrap();
        let mut b = dir.list().unwrap();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        for p in &a {
            assert_eq!(zip.read(p).unwrap(), dir.read(p).unwrap());
        }
        assert_eq!(dir.read("../etc/passwd").unwrap(), None);
    }

    #[test]
    fn overlays_apply_only_when_format_matches() {
        let meta = br#"{"pack":{"pack_format":1},"overlays":{"entries":[
            {"min_format":[97,0],"max_format":[97,9],"directory":"new"},
            {"min_format":[1,0],"max_format":[2,0],"directory":"old"}]}}"#;
        let mut rm = ResourceManager::new();
        rm.push_pack(
            "p",
            Box::new(
                ZipSource::from_bytes(zip_bytes(&[
                    ("pack.mcmeta", meta),
                    ("f.txt", b"base"),
                    ("new/f.txt", b"new"),
                    ("old/f.txt", b"old"),
                    ("old/g.txt", b"old-g"),
                ]))
                .unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(rm.read("f.txt").unwrap().unwrap(), b"new");
        assert!(!rm.exists("g.txt"));
        assert!(!rm.exists("new/f.txt"));
    }
}
