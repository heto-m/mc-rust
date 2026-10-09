use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use sha1::{Digest, Sha1};

pub fn sha1_hex(bytes: &[u8]) -> String {
    Sha1::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn file_matches(path: &Path, expected: &str) -> Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    let bytes = fs::read(path)?;
    Ok(sha1_hex(&bytes).eq_ignore_ascii_case(expected))
}

pub fn verify_or_remove(path: &Path, expected: &str) -> Result<()> {
    if file_matches(path, expected)? {
        return Ok(());
    }
    fs::remove_file(path)?;
    bail!(
        "sha1 mismatch for {} (expected {expected}); file removed",
        path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::tempfile_dir;

    #[test]
    fn sha1_of_abc_matches_known_vector() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn file_matches_detects_mismatch() {
        let dir = tempfile_dir("checksum-match");
        let path = dir.join("f.bin");
        fs::write(&path, b"abc").unwrap();
        assert!(file_matches(&path, "A9993E364706816ABA3E25717850C26C9CD0D89D").unwrap());
        assert!(!file_matches(&path, "0000000000000000000000000000000000000000").unwrap());
    }

    #[test]
    fn file_matches_is_false_when_missing() {
        let dir = tempfile_dir("checksum-missing");
        assert!(!file_matches(&dir.join("nope"), "00").unwrap());
    }

    #[test]
    fn verify_or_remove_deletes_bad_file() {
        let dir = tempfile_dir("checksum-remove");
        let path = dir.join("bad.bin");
        fs::write(&path, b"abc").unwrap();
        assert!(verify_or_remove(&path, "00").is_err());
        assert!(!path.exists());
    }
}
