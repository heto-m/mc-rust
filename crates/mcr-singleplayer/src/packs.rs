//! Data packs and experiments for a new world: validated copies into `<world>/datapacks`.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::layout::valid_folder;
use crate::{Result, SpError};

/// Vanilla's experimental feature packs, enabled through `initial-enabled-packs`.
pub const EXPERIMENTS: &[&str] = mcr_data::FEATURE_PACKS_26_3;
pub const MAX_PACKS: usize = 64;
pub const MAX_ZIP: u64 = 64 << 20;
pub const MAX_DIR_BYTES: u64 = 256 << 20;
const MAX_DIR_ENTRIES: usize = 10_000;
const MAX_DIR_DEPTH: usize = 16;
const ZIP_MAGIC: &[u8; 4] = b"PK\x03\x04";

/// Known experiments only, in a stable order without duplicates.
pub fn experiments<'a>(ids: impl IntoIterator<Item = &'a str>) -> Vec<&'static str> {
    let wanted: Vec<&str> = ids.into_iter().take(MAX_PACKS).collect();
    EXPERIMENTS
        .iter()
        .copied()
        .filter(|e| {
            wanted
                .iter()
                .any(|w| w.strip_prefix("minecraft:").unwrap_or(w) == *e)
        })
        .collect()
}

/// `initial-enabled-packs` for the chosen experiments, or `None` for vanilla's default.
pub fn initial_enabled_packs(experiments: &[&str]) -> Option<String> {
    (!experiments.is_empty()).then(|| {
        std::iter::once("vanilla")
            .chain(experiments.iter().copied())
            .collect::<Vec<_>>()
            .join(",")
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackKind {
    Zip,
    Folder,
}

fn refused(name: &str, why: &str) -> SpError {
    SpError::Refused(format!("data pack {name}: {why}"))
}

fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// What a pack entry is, after checking it: a bounded zip starting with a local header, or a folder with `pack.mcmeta`.
pub fn check_pack(dir: &Path, name: &str) -> Result<PackKind> {
    if !valid_folder(name) {
        return Err(refused(name, "not a single file or folder name"));
    }
    let path = dir.join(name);
    let meta = fs::symlink_metadata(&path)?;
    if meta.file_type().is_symlink() {
        return Err(refused(name, "is a symbolic link"));
    }
    if meta.is_file() {
        if !is_zip(&path) {
            return Err(refused(name, "is not a .zip file"));
        }
        if meta.len() > MAX_ZIP {
            return Err(SpError::TooLarge(name.into(), MAX_ZIP));
        }
        let mut head = [0u8; 4];
        crate::safe::open_read(&path)?
            .read_exact(&mut head)
            .map_err(|_| refused(name, "is not a zip file"))?;
        if &head != ZIP_MAGIC {
            return Err(refused(name, "is not a zip file"));
        }
        return Ok(PackKind::Zip);
    }
    if !meta.is_dir() {
        return Err(refused(name, "is neither a file nor a folder"));
    }
    match fs::symlink_metadata(path.join("pack.mcmeta")) {
        Ok(m) if m.is_file() => {}
        _ => return Err(refused(name, "has no pack.mcmeta")),
    }
    walk(&path, name, |_, _| Ok(()))?;
    Ok(PackKind::Folder)
}

/// Visits every file and folder under `root` (bounded, no symlinks, only regular files); `f(relative, is_dir)`.
fn walk(root: &Path, name: &str, mut f: impl FnMut(&Path, bool) -> Result<()>) -> Result<()> {
    let mut stack = vec![(PathBuf::new(), 0usize)];
    let (mut entries, mut bytes) = (0usize, 0u64);
    while let Some((rel, depth)) = stack.pop() {
        for entry in fs::read_dir(root.join(&rel))? {
            let entry = entry?;
            entries += 1;
            if entries > MAX_DIR_ENTRIES {
                return Err(refused(name, "has too many files"));
            }
            let file_name = entry.file_name();
            let Some(text) = file_name.to_str().filter(|t| valid_folder(t)) else {
                return Err(refused(name, "has a file with an unusable name"));
            };
            let child = rel.join(text);
            let meta = fs::symlink_metadata(entry.path())?;
            if meta.file_type().is_symlink() {
                return Err(refused(name, "contains a symbolic link"));
            }
            if meta.is_dir() {
                if depth + 1 > MAX_DIR_DEPTH {
                    return Err(refused(name, "is nested too deep"));
                }
                f(&child, true)?;
                stack.push((child, depth + 1));
            } else if meta.is_file() {
                bytes += meta.len();
                if bytes > MAX_DIR_BYTES {
                    return Err(SpError::TooLarge(name.into(), MAX_DIR_BYTES));
                }
                f(&child, false)?;
            } else {
                return Err(refused(name, "contains a special file"));
            }
        }
    }
    Ok(())
}

fn read_limited(path: &Path, max: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    crate::safe::open_read(path)?
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(SpError::TooLarge(path.display().to_string(), max));
    }
    Ok(bytes)
}

/// Packs a folder offers: `.zip` files and folders with `pack.mcmeta`, sorted, at most [`MAX_PACKS`] * 4 entries read.
pub fn list_packs(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .take(MAX_PACKS * 4)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| {
            let path = dir.join(name);
            match fs::symlink_metadata(&path) {
                Ok(m) if m.is_file() => is_zip(&path),
                Ok(m) if m.is_dir() => {
                    fs::symlink_metadata(path.join("pack.mcmeta")).is_ok_and(|m| m.is_file())
                }
                _ => false,
            }
        })
        .filter(|n| valid_folder(n))
        .collect();
    out.sort();
    out.truncate(MAX_PACKS);
    out
}

/// Copies `names` from `source` into `<world>/datapacks`, checking all of them before copying any.
pub fn install(world: &Path, source: &Path, names: &[String]) -> Result<()> {
    if names.is_empty() {
        return Ok(());
    }
    if names.len() > MAX_PACKS {
        return Err(SpError::Refused(format!("at most {MAX_PACKS} data packs")));
    }
    let meta = fs::symlink_metadata(source)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(SpError::Refused(format!(
            "{}: the data pack folder is not a real folder",
            source.display()
        )));
    }
    let kinds = names
        .iter()
        .map(|n| check_pack(source, n))
        .collect::<Result<Vec<_>>>()?;
    let target = world.join("datapacks");
    crate::safe::ensure_dir(&target)?;
    for (name, kind) in names.iter().zip(kinds) {
        let from = source.join(name);
        let to = target.join(name);
        match kind {
            PackKind::Zip => crate::safe::write_file(&to, &read_limited(&from, MAX_ZIP)?)?,
            PackKind::Folder => {
                crate::safe::ensure_dir(&to)?;
                walk(&from, name, |rel, is_dir| {
                    let dest = to.join(rel);
                    if is_dir {
                        crate::safe::ensure_dir(&dest)
                    } else {
                        crate::safe::write_file(
                            &dest,
                            &read_limited(&from.join(rel), MAX_DIR_BYTES)?,
                        )
                    }
                })?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::tests::temp_dir;

    fn setup(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = temp_dir(&format!("packs-{tag}"));
        let source = root.join("packs");
        let world = root.join("world");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&world).unwrap();
        (root, source, world)
    }

    #[test]
    fn copies_zips_and_folders() {
        let (root, source, world) = setup("copy");
        fs::write(source.join("tools.zip"), b"PK\x03\x04rest").unwrap();
        fs::create_dir_all(source.join("Folder/data/x")).unwrap();
        fs::write(source.join("Folder/pack.mcmeta"), b"{}").unwrap();
        fs::write(source.join("Folder/data/x/f.json"), b"1").unwrap();
        fs::write(source.join("notes.txt"), b"x").unwrap();
        assert_eq!(list_packs(&source), ["Folder", "tools.zip"]);
        install(&world, &source, &["tools.zip".into(), "Folder".into()]).unwrap();
        assert_eq!(
            fs::read(world.join("datapacks/tools.zip")).unwrap(),
            b"PK\x03\x04rest"
        );
        assert_eq!(
            fs::read(world.join("datapacks/Folder/data/x/f.json")).unwrap(),
            b"1"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn hostile_packs_are_refused() {
        let (root, source, world) = setup("hostile");
        fs::write(source.join("fake.zip"), b"not a zip").unwrap();
        fs::create_dir(source.join("NoMeta")).unwrap();
        fs::write(source.join("tiny.zip"), b"PK").unwrap();
        for name in [
            "fake.zip",
            "NoMeta",
            "tiny.zip",
            "missing.zip",
            "../packs",
            "..",
        ] {
            assert!(check_pack(&source, name).is_err(), "{name}");
        }
        assert!(install(&world, &source, &vec!["x".to_owned(); MAX_PACKS + 1]).is_err());
        assert!(
            !world.join("datapacks").exists(),
            "nothing copied on refusal"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_never_followed() {
        use std::os::unix::fs::symlink;
        let (root, source, world) = setup("links");
        let outside = root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret"), b"s").unwrap();
        fs::create_dir(source.join("Linked")).unwrap();
        fs::write(source.join("Linked/pack.mcmeta"), b"{}").unwrap();
        symlink(outside.join("secret"), source.join("Linked/leak.json")).unwrap();
        symlink(outside.join("secret"), source.join("link.zip")).unwrap();
        assert!(install(&world, &source, &["Linked".into()]).is_err());
        assert!(install(&world, &source, &["link.zip".into()]).is_err());
        fs::write(source.join("ok.zip"), b"PK\x03\x04").unwrap();
        fs::create_dir(world.join("datapacks")).unwrap();
        symlink(outside.join("victim"), world.join("datapacks/ok.zip")).unwrap();
        assert!(install(&world, &source, &["ok.zip".into()]).is_err());
        assert!(!outside.join("victim").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn experiment_ids() {
        assert_eq!(
            experiments([
                "trade_rebalance",
                "minecraft:minecart_improvements",
                "bogus",
                "vanilla"
            ]),
            ["minecart_improvements", "trade_rebalance"]
        );
        assert_eq!(initial_enabled_packs(&[]), None);
        assert_eq!(
            initial_enabled_packs(&["trade_rebalance"]).as_deref(),
            Some("vanilla,trade_rebalance")
        );
    }
}
