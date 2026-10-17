//! File access inside world folders, which may come from a download: never through a symlink.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::layout::valid_folder;
use crate::{Result, SpError};

const MAX_WALK_ENTRIES: usize = 20_000;
const MAX_WALK_DEPTH: usize = 8;

fn refused(path: &Path, why: &str) -> SpError {
    SpError::Refused(format!("{}: {why}", path.display()))
}

/// Opens an existing file for reading, failing if the last component is a symlink.
pub fn open_read(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    nofollow(&mut options);
    options.open(path)
}

#[cfg(unix)]
fn nofollow(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(libc::O_NOFOLLOW);
}

#[cfg(not(unix))]
fn nofollow(_options: &mut OpenOptions) {}

/// Fails if `path` exists and is a symlink or not a regular file.
pub fn check_file(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Err(refused(path, "is a symbolic link")),
        Ok(m) if !m.is_file() => Err(refused(path, "is not a regular file")),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Creates `dir` (its parent must exist) or accepts an existing real directory, never a symlink.
pub fn ensure_dir(dir: &Path) -> Result<()> {
    match fs::symlink_metadata(dir) {
        Ok(m) if m.file_type().is_symlink() => Err(refused(dir, "is a symbolic link")),
        Ok(m) if !m.is_dir() => Err(refused(dir, "is not a directory")),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => match fs::create_dir(dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => ensure_dir(dir),
            Err(e) => Err(e.into()),
        },
        Err(e) => Err(e.into()),
    }
}

/// Opens a brand-new file next to `target` (create_new, no-follow) for writing and renaming over it.
fn temp_beside(target: &Path) -> Result<(PathBuf, File)> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = target.parent().unwrap_or(Path::new("."));
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    for _ in 0..16 {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let tmp = dir.join(format!(".{name}.{}-{n}.mcr-tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        nofollow(&mut options);
        match options.open(&tmp) {
            Ok(file) => return Ok((tmp, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(refused(target, "no free temporary name"))
}

/// Replaces `target` with `bytes` through a fresh temp file and a rename; a symlinked target is refused.
pub fn write_file(target: &Path, bytes: &[u8]) -> Result<()> {
    check_file(target)?;
    let (tmp, mut file) = temp_beside(target)?;
    let written = file.write_all(bytes).and_then(|()| file.flush());
    drop(file);
    if let Err(e) = written.and_then(|()| fs::rename(&tmp, target)) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// Truncates or creates `path` for appending output, refusing a symlink.
pub fn create_log(path: &Path) -> Result<File> {
    check_file(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    nofollow(&mut options);
    Ok(options.open(path)?)
}

/// Fails on the first symlink under `dir` (bounded walk); a missing `dir` is fine.
pub fn reject_symlinks_under(dir: &Path) -> Result<()> {
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    while let Some((path, depth)) = stack.pop() {
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        if meta.file_type().is_symlink() {
            return Err(refused(&path, "is a symbolic link"));
        }
        if !meta.is_dir() || depth >= MAX_WALK_DEPTH {
            continue;
        }
        for entry in fs::read_dir(&path)? {
            seen += 1;
            if seen > MAX_WALK_ENTRIES {
                return Err(refused(dir, "has too many files"));
            }
            stack.push((entry?.path(), depth + 1));
        }
    }
    Ok(())
}

/// The canonical path of world `folder`: a real directory (not a symlink) directly inside `saves`.
pub fn checked_world(saves: &Path, folder: &str) -> Result<PathBuf> {
    if !valid_folder(folder) {
        return Err(SpError::Refused(format!("not a world folder: {folder}")));
    }
    let world = saves.join(folder);
    let meta = fs::symlink_metadata(&world)?;
    if meta.file_type().is_symlink() {
        return Err(refused(&world, "is a symbolic link"));
    }
    if !meta.is_dir() {
        return Err(refused(&world, "is not a directory"));
    }
    let canon_saves = fs::canonicalize(saves)?;
    let canon = fs::canonicalize(&world)?;
    if canon.parent() != Some(canon_saves.as_path()) {
        return Err(refused(&world, "is not inside the saves folder"));
    }
    Ok(canon)
}

/// Opens (or, with `create`, makes) world `folder` for a launch: the canonical world and its server
/// directory, refusing symlinked worlds, `level.dat`, `session.lock` or anything under `mcr-server`.
pub fn open_world(saves: &Path, folder: &str, create: bool) -> Result<(PathBuf, PathBuf)> {
    if !valid_folder(folder) {
        return Err(SpError::Refused(format!("not a world folder: {folder}")));
    }
    if create {
        fs::create_dir_all(saves)?;
        fs::create_dir(saves.join(folder))?;
    }
    let world = checked_world(saves, folder)?;
    for name in ["level.dat", "level.dat_old", "session.lock"] {
        check_file(&world.join(name))?;
    }
    let server_dir = world.join(crate::layout::SERVER_DIR);
    ensure_dir(&server_dir)?;
    reject_symlinks_under(&server_dir)?;
    Ok((world, server_dir))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn dirs(tag: &str) -> (PathBuf, PathBuf) {
        let root = crate::level::tests::temp_dir(&format!("safe-{tag}"));
        let saves = root.join("saves");
        let outside = root.join("outside");
        fs::create_dir_all(&saves).unwrap();
        fs::create_dir_all(&outside).unwrap();
        (saves, outside)
    }

    #[test]
    fn writes_never_follow_links() {
        let (saves, outside) = dirs("write");
        let victim = outside.join("victim");
        fs::write(&victim, "keep").unwrap();
        symlink(&victim, saves.join("ops.json")).unwrap();
        assert!(write_file(&saves.join("ops.json"), b"[]").is_err());
        assert!(create_log(&saves.join("ops.json")).is_err());
        assert!(open_read(&saves.join("ops.json")).is_err());
        assert_eq!(fs::read_to_string(&victim).unwrap(), "keep");
        write_file(&saves.join("plain"), b"one").unwrap();
        write_file(&saves.join("plain"), b"two").unwrap();
        assert_eq!(fs::read_to_string(saves.join("plain")).unwrap(), "two");
        let leftovers = fs::read_dir(&saves)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("mcr-tmp")
            })
            .count();
        assert_eq!(leftovers, 0);
        let _ = fs::remove_dir_all(saves.parent().unwrap());
    }

    #[test]
    fn directories_and_worlds() {
        let (saves, outside) = dirs("dirs");
        symlink(&outside, saves.join("linked")).unwrap();
        assert!(ensure_dir(&saves.join("linked")).is_err());
        assert!(checked_world(&saves, "linked").is_err());
        assert!(checked_world(&saves, "..").is_err());
        assert!(checked_world(&saves, "missing").is_err());
        fs::create_dir(saves.join("real")).unwrap();
        let canon = checked_world(&saves, "real").unwrap();
        assert!(canon.is_absolute() && canon.ends_with("real"));
        ensure_dir(&saves.join("real/mcr-server")).unwrap();
        ensure_dir(&saves.join("real/mcr-server")).unwrap();
        assert!(reject_symlinks_under(&saves.join("real")).is_ok());
        fs::create_dir(saves.join("real/mcr-server/logs")).unwrap();
        symlink(
            outside.join("x"),
            saves.join("real/mcr-server/logs/latest.log"),
        )
        .unwrap();
        assert!(reject_symlinks_under(&saves.join("real/mcr-server")).is_err());
        assert!(reject_symlinks_under(&saves.join("nope")).is_ok());
        let _ = fs::remove_dir_all(saves.parent().unwrap());
    }

    #[test]
    fn launches_refuse_planted_links() {
        let (saves, outside) = dirs("open");
        let (world, server) = open_world(&saves, "New", true).unwrap();
        assert!(server.is_dir() && server.starts_with(&world));
        assert!(
            open_world(&saves, "New", true).is_err(),
            "create never reuses a folder"
        );
        assert!(open_world(&saves, "New", false).is_ok());
        assert!(open_world(&saves, "../outside", false).is_err());

        fs::create_dir(saves.join("A")).unwrap();
        symlink(&outside, saves.join("A/mcr-server")).unwrap();
        assert!(
            open_world(&saves, "A", false).is_err(),
            "symlinked mcr-server"
        );

        for planted in [
            "mcr-server/eula.txt",
            "mcr-server/ops.json",
            "session.lock",
            "level.dat",
        ] {
            let w = saves.join("B");
            let _ = fs::remove_dir_all(&w);
            fs::create_dir_all(w.join("mcr-server")).unwrap();
            symlink(outside.join("target"), w.join(planted)).unwrap();
            assert!(open_world(&saves, "B", false).is_err(), "{planted}");
        }
        assert!(
            fs::read_dir(&outside).unwrap().next().is_none(),
            "nothing written outside"
        );
        let _ = fs::remove_dir_all(saves.parent().unwrap());
    }
}
