use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use sha1::{Digest, Sha1};

use crate::{Result, SpError};

const CHUNK: usize = 64 * 1024;

/// Bytes so far and the expected total; returning `false` cancels.
pub type Progress<'a> = &'a mut dyn FnMut(u64, Option<u64>) -> bool;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_global(Some(Duration::from_secs(30 * 60)))
        .max_redirects(5)
        .https_only(true)
        .build()
        .into()
}

fn check_url(url: &str) -> Result<()> {
    let ok = url.starts_with("https://")
        && url.len() <= 2048
        && !url.chars().any(|c| c.is_control() || c.is_whitespace());
    if ok {
        Ok(())
    } else {
        Err(SpError::Http(format!("refusing URL {:?}", truncate(url))))
    }
}

fn truncate(text: &str) -> String {
    text.chars().take(120).collect()
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    hex(&Sha1::digest(bytes))
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

pub fn file_sha1(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

fn open(url: &str) -> Result<ureq::http::Response<ureq::Body>> {
    check_url(url)?;
    agent()
        .get(url)
        .call()
        .map_err(|e| SpError::Http(format!("{}: {e}", truncate(url))))
}

pub fn get_bytes(url: &str, max: u64) -> Result<Vec<u8>> {
    let mut response = open(url)?;
    let mut out = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(max + 1)
        .read_to_end(&mut out)?;
    if out.len() as u64 > max {
        return Err(SpError::TooLarge(truncate(url), max));
    }
    Ok(out)
}

/// Downloads to `dest` through `dest.part`, checking size and SHA-1 before the rename.
pub fn download(
    url: &str,
    dest: &Path,
    sha1: &str,
    size: Option<u64>,
    max: u64,
    progress: Progress<'_>,
) -> Result<()> {
    let part = dest.with_extension("part");
    let result = download_part(url, &part, sha1, size, max, progress);
    match result {
        Ok(()) => {
            std::fs::rename(&part, dest)?;
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

fn download_part(
    url: &str,
    part: &Path,
    sha1: &str,
    size: Option<u64>,
    max: u64,
    progress: Progress<'_>,
) -> Result<()> {
    if size.is_some_and(|s| s > max) {
        return Err(SpError::TooLarge(truncate(url), max));
    }
    let mut response = open(url)?;
    let mut body = response.body_mut().as_reader();
    let mut out = File::create(part)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; CHUNK];
    let mut total = 0u64;
    if !progress(0, size) {
        return Err(SpError::Cancelled);
    }
    loop {
        let n = body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max || size.is_some_and(|s| total > s) {
            return Err(SpError::TooLarge(truncate(url), size.unwrap_or(max)));
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n])?;
        if !progress(total, size) {
            return Err(SpError::Cancelled);
        }
    }
    out.flush()?;
    if size.is_some_and(|s| s != total) || !hex(&hasher.finalize()).eq_ignore_ascii_case(sha1) {
        return Err(SpError::Hash(truncate(url)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_https() {
        assert!(check_url("https://piston-meta.mojang.com/x.json").is_ok());
        for bad in [
            "http://x",
            "file:///etc/passwd",
            "https://a b",
            "https://x\n",
        ] {
            assert!(check_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn hashes() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }
}
