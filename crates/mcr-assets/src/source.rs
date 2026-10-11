use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::{AssetError, Result};

pub trait AssetSource: Send + Sync + fmt::Debug {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>>;

    fn list(&self) -> Result<Vec<String>>;
}

trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

pub struct ZipSource {
    archive: Mutex<zip::ZipArchive<Box<dyn ReadSeek>>>,
    index: HashMap<String, usize>,
    limits: Option<ZipLimits>,
    read_budget: ReadBudget,
}

/// Actual decompressed bytes served, shared by every limited archive of one resource stack.
pub type ReadBudget = Arc<AtomicU64>;

/// Bounds for archives from untrusted sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZipLimits {
    pub max_entries: usize,
    pub max_total_bytes: u64,
    pub max_entry_bytes: u64,
    pub max_image_side: u32,
    pub max_path_len: usize,
    pub max_path_depth: usize,
}

impl ZipLimits {
    pub const SERVER_PACK: Self = Self {
        max_entries: 20_000,
        max_total_bytes: 1 << 30,
        max_entry_bytes: 128 << 20,
        max_image_side: 8192,
        max_path_len: 512,
        max_path_depth: 32,
    };

    /// Cap on bytes actually decompressed across all reads, whatever the headers claim.
    pub fn max_read_bytes(&self) -> u64 {
        self.max_total_bytes.saturating_mul(4)
    }
}

fn safe_entry_name(name: &str, limits: &ZipLimits) -> bool {
    name.len() <= limits.max_path_len
        && name.split('/').count() <= limits.max_path_depth
        && !name.is_empty()
        && !name.starts_with('/')
        && !name.contains(['\\', '\0', ':'])
        && name.split('/').all(|part| part != "..")
}

fn png_within(bytes: &[u8], max_side: u32) -> bool {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(SIGNATURE) || bytes.len() < 24 {
        return true;
    }
    let side =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    side(16) <= max_side && side(20) <= max_side
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
        Self::from_reader(Box::new(File::open(path)?), None)
    }

    /// For archives the server chose: rejects oversized archives and ignores unsafe entry names.
    pub fn open_limited(path: impl AsRef<Path>, limits: ZipLimits) -> Result<Self> {
        Self::open_limited_shared(path, limits, ReadBudget::default())
    }

    pub fn open_limited_shared(
        path: impl AsRef<Path>,
        limits: ZipLimits,
        budget: ReadBudget,
    ) -> Result<Self> {
        Self::from_reader(Box::new(File::open(path)?), Some((limits, budget)))
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        Self::from_reader(Box::new(Cursor::new(bytes)), None)
    }

    pub fn from_bytes_limited(bytes: Vec<u8>, limits: ZipLimits) -> Result<Self> {
        Self::from_reader(
            Box::new(Cursor::new(bytes)),
            Some((limits, ReadBudget::default())),
        )
    }

    fn from_reader(
        reader: Box<dyn ReadSeek>,
        limited: Option<(ZipLimits, ReadBudget)>,
    ) -> Result<Self> {
        let (limits, read_budget) = match limited {
            Some((l, b)) => (Some(l), b),
            None => (None, ReadBudget::default()),
        };
        let mut archive = zip::ZipArchive::new(reader)?;
        if let Some(l) = limits
            && archive.len() > l.max_entries
        {
            return Err(AssetError::invalid(
                "archive",
                format!(
                    "{} entries exceed the limit of {}",
                    archive.len(),
                    l.max_entries
                ),
            ));
        }
        let mut names = Vec::with_capacity(archive.len());
        for (i, name) in archive.file_names().enumerate() {
            names.push((i, name?.into_owned()));
        }
        let mut index = HashMap::with_capacity(names.len());
        let mut total = 0u64;
        for (i, name) in names {
            if name.ends_with('/') {
                continue;
            }
            if let Some(l) = limits {
                if !safe_entry_name(&name, &l) {
                    tracing::warn!("ignoring unsafe archive entry {name:?}");
                    continue;
                }
                let size = archive.by_index_raw(i)?.size();
                total = total.saturating_add(size);
                if size > l.max_entry_bytes || total > l.max_total_bytes {
                    return Err(AssetError::invalid(
                        "archive",
                        format!("{name} exceeds the uncompressed size limits"),
                    ));
                }
            }
            index.insert(name, i);
        }
        Ok(Self {
            archive: Mutex::new(archive),
            index,
            limits,
            read_budget,
        })
    }
}

impl AssetSource for ZipSource {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let Some(&i) = self.index.get(path) else {
            return Ok(None);
        };
        let mut archive = self.archive.lock().unwrap_or_else(|e| e.into_inner());
        let file = archive.by_index(i)?;
        let Some(limits) = self.limits else {
            let mut buf = Vec::with_capacity(file.size() as usize);
            let mut file = file;
            file.read_to_end(&mut buf)?;
            return Ok(Some(buf));
        };
        let declared = file.size();
        let mut buf = Vec::new();
        file.take(limits.max_entry_bytes + 1)
            .read_to_end(&mut buf)?;
        let actual = buf.len() as u64;
        if actual > limits.max_entry_bytes || actual != declared {
            return Err(AssetError::invalid(
                path,
                "entry size exceeds the limit or disagrees with its header",
            ));
        }
        let served = self.read_budget.fetch_add(actual, Ordering::Relaxed) + actual;
        if served > limits.max_read_bytes() || !png_within(&buf, limits.max_image_side) {
            return Err(AssetError::invalid(path, "entry exceeds the size limits"));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::zip_bytes;

    const LIMITS: ZipLimits = ZipLimits {
        max_entries: 4,
        max_total_bytes: 100,
        max_entry_bytes: 50,
        max_image_side: 16,
        max_path_len: 64,
        max_path_depth: 8,
    };

    #[test]
    fn unsafe_names_are_not_indexed() {
        let bytes = zip_bytes(&[
            ("ok.txt", b"1"),
            ("../evil.txt", b"2"),
            ("/abs.txt", b"3"),
            ("a\\b.txt", b"4"),
            ("x/../y.txt", b"5"),
        ]);
        let limits = ZipLimits {
            max_entries: 9,
            ..LIMITS
        };
        let zip = ZipSource::from_bytes_limited(bytes, limits).unwrap();
        assert_eq!(zip.list().unwrap(), ["ok.txt"]);
        assert_eq!(zip.read("../evil.txt").unwrap(), None);
    }

    #[test]
    fn size_and_count_limits_reject_archives() {
        let big = [0u8; 60];
        assert!(ZipSource::from_bytes_limited(zip_bytes(&[("a", &big)]), LIMITS).is_err());
        let two = [0u8; 40];
        let three = zip_bytes(&[("a", &two), ("b", &two), ("c", &two)]);
        assert!(ZipSource::from_bytes_limited(three, LIMITS).is_err());
        let names: Vec<String> = (0..5).map(|i| format!("f{i}")).collect();
        let refs: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
        assert!(ZipSource::from_bytes_limited(zip_bytes(&refs), LIMITS).is_err());
    }

    #[test]
    fn oversized_png_is_refused() {
        let png = crate::testutil::png_bytes(32, 1, &[0; 128]);
        let limits = ZipLimits {
            max_entry_bytes: 1000,
            ..LIMITS
        };
        let zip = ZipSource::from_bytes_limited(zip_bytes(&[("a.png", &png)]), limits).unwrap();
        assert!(zip.read("a.png").is_err());
    }

    #[test]
    fn lying_header_is_rejected_at_read() {
        let mut bytes = zip_bytes(&[("a.txt", &[1u8; 40])]);
        let tag = [0x50, 0x4b, 0x01, 0x02];
        let cd = bytes.windows(4).rposition(|w| w == tag).unwrap();
        bytes[cd + 24..cd + 28].copy_from_slice(&5u32.to_le_bytes());
        let local = bytes
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x03, 0x04])
            .unwrap();
        bytes[local + 22..local + 26].copy_from_slice(&5u32.to_le_bytes());
        if let Ok(zip) = ZipSource::from_bytes_limited(bytes, LIMITS) {
            assert!(zip.read("a.txt").is_err());
        }
    }

    #[test]
    fn running_total_is_enforced_across_entries_and_archives() {
        let limits = ZipLimits {
            max_total_bytes: 100,
            max_entries: 100,
            ..LIMITS
        };
        let chunk = [0u8; 40];
        let bytes = zip_bytes(&[("a", &chunk), ("b", &chunk)]);
        let budget = ReadBudget::default();
        let mut served = 0;
        for _ in 0..6 {
            let zip = ZipSource::from_reader(
                Box::new(Cursor::new(bytes.clone())),
                Some((limits, budget.clone())),
            )
            .unwrap();
            for name in ["a", "b"] {
                if zip.read(name).is_ok() {
                    served += 40;
                }
            }
        }
        assert!(served <= limits.max_read_bytes() as usize);
        assert!(served < 480);
    }

    #[test]
    fn compressible_bomb_is_stopped() {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("bomb.bin", options).unwrap();
        std::io::Write::write_all(&mut writer, &vec![0u8; 4 << 20]).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert!(bytes.len() < 10_000);
        let limits = ZipLimits {
            max_entry_bytes: 1 << 20,
            max_total_bytes: 1 << 20,
            ..LIMITS
        };
        assert!(ZipSource::from_bytes_limited(bytes, limits).is_err());
    }

    #[test]
    fn long_and_deep_paths_are_ignored() {
        let long = "a".repeat(100);
        let deep = "d/".repeat(10) + "f";
        let bytes = zip_bytes(&[(&long, b"1"), (&deep, b"2"), ("ok", b"3")]);
        let limits = ZipLimits {
            max_entries: 9,
            ..LIMITS
        };
        let zip = ZipSource::from_bytes_limited(bytes, limits).unwrap();
        assert_eq!(zip.list().unwrap(), ["ok"]);
    }
}
