use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use sha1::{Digest, Sha1};
use sha2::Sha512;

pub fn sha1_hex(bytes: &[u8]) -> String {
    hex(&Sha1::digest(bytes))
}

pub fn sha512_hex(bytes: &[u8]) -> String {
    hex(&Sha512::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checksum {
    Sha1(String),
    Sha512(String),
}

impl Checksum {
    pub fn matches(&self, bytes: &[u8]) -> bool {
        match self {
            Self::Sha1(h) => sha1_hex(bytes).eq_ignore_ascii_case(h),
            Self::Sha512(h) => sha512_hex(bytes).eq_ignore_ascii_case(h),
        }
    }

    pub fn file_matches(&self, path: &Path) -> Result<bool> {
        if !path.is_file() {
            return Ok(false);
        }
        Ok(self.matches(&fs::read(path)?))
    }

    pub fn verify_or_remove(&self, path: &Path) -> Result<()> {
        if self.file_matches(path)? {
            return Ok(());
        }
        let _ = fs::remove_file(path);
        let (algo, h) = match self {
            Self::Sha1(h) => ("sha1", h),
            Self::Sha512(h) => ("sha512", h),
        };
        bail!(
            "{algo} mismatch for {} (expected {h}); file removed",
            path.display()
        )
    }
}

pub fn file_matches(path: &Path, expected: &str) -> Result<bool> {
    Checksum::Sha1(expected.to_owned()).file_matches(path)
}

pub fn verify_or_remove(path: &Path, expected: &str) -> Result<()> {
    Checksum::Sha1(expected.to_owned()).verify_or_remove(path)
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
    fn sha512_of_abc_matches_known_vector() {
        assert_eq!(
            sha512_hex(b"abc"),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
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
        let err = Checksum::Sha512("00".into())
            .verify_or_remove(&path)
            .unwrap_err();
        assert!(err.to_string().contains("sha512 mismatch"));
        assert!(!path.exists());
    }
}
