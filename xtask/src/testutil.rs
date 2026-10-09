use std::fs;
use std::path::PathBuf;

pub fn tempfile_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-test-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}
