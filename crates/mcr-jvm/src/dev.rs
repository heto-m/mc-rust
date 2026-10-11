use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The first file under `module_dir` newer than `jar` (or `jar` itself when missing); `None` if current.
pub fn stale_java_jar(module_dir: &Path, jar: &Path) -> Option<PathBuf> {
    let Some(built) = mtime(jar) else {
        return Some(jar.to_path_buf());
    };
    let mut stack = vec![module_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if mtime(&path).is_some_and(|m| m > built) {
                return Some(path);
            }
        }
    }
    None
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn detects_missing_and_outdated_jars() {
        let dir = std::env::temp_dir().join(format!("mcr-jvm-stale-{}", std::process::id()));
        let src = dir.join("module/src");
        std::fs::create_dir_all(&src).unwrap();
        let jar = dir.join("module.jar");
        let source = src.join("A.java");
        std::fs::write(&source, "class A {}").unwrap();
        assert_eq!(stale_java_jar(&dir.join("module"), &jar), Some(jar.clone()));

        std::fs::write(&jar, "jar").unwrap();
        let old = SystemTime::now() - Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert_eq!(stale_java_jar(&dir.join("module"), &jar), None);

        std::fs::File::options()
            .write(true)
            .open(&jar)
            .unwrap()
            .set_modified(old - Duration::from_secs(60))
            .unwrap();
        assert_eq!(stale_java_jar(&dir.join("module"), &jar), Some(source));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
