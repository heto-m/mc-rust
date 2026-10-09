use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use crate::error::Result;

pub trait AssetSource: Send + Sync + fmt::Debug {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>>;

    fn list(&self) -> Result<Vec<String>>;
}

trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

pub struct ZipSource {
    archive: Mutex<zip::ZipArchive<Box<dyn ReadSeek>>>,
    index: HashMap<String, usize>,
}

impl fmt::Debug for ZipSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ZipSource")
            .field("entries", &self.index.len())
            .finish()
    }
}

impl ZipSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_reader(Box::new(File::open(path)?))
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        Self::from_reader(Box::new(Cursor::new(bytes)))
    }

    fn from_reader(reader: Box<dyn ReadSeek>) -> Result<Self> {
        let archive = zip::ZipArchive::new(reader)?;
        let mut index = HashMap::with_capacity(archive.len());
        for (i, name) in archive.file_names().enumerate() {
            let name = name?;
            if !name.ends_with('/') {
                index.insert(name.into_owned(), i);
            }
        }
        Ok(Self {
            archive: Mutex::new(archive),
            index,
        })
    }
}

impl AssetSource for ZipSource {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let Some(&i) = self.index.get(path) else {
            return Ok(None);
        };
        let mut archive = self.archive.lock().unwrap_or_else(|e| e.into_inner());
        let mut file = archive.by_index(i)?;
        let mut buf = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut buf)?;
        Ok(Some(buf))
    }

    fn list(&self) -> Result<Vec<String>> {
        Ok(self.index.keys().cloned().collect())
    }
}

#[derive(Debug)]
pub struct DirSource {
    root: PathBuf,
}

impl DirSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, path: &str) -> Option<PathBuf> {
        let rel = Path::new(path);
        rel.components()
            .all(|c| matches!(c, Component::Normal(_)))
            .then(|| self.root.join(rel))
    }

    fn walk(&self, dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.file_type()?.is_dir() {
                self.walk(&entry.path(), &rel, out)?;
            } else {
                out.push(rel);
            }
        }
        Ok(())
    }
}

impl AssetSource for DirSource {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let Some(full) = self.resolve(path) else {
            return Ok(None);
        };
        match std::fs::read(full) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn list(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        self.walk(&self.root, "", &mut out)?;
        Ok(out)
    }
}
