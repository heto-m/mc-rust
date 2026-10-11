use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use sha1::{Digest, Sha1};
use uuid::Uuid;

pub const MAX_CACHED_PACKS: usize = 20;
const CHUNK: usize = 16 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("http: {0}")]
    Http(String),
    #[error("file is larger than the {0} byte limit")]
    TooLarge(u64),
    #[error("sha-1 mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("download took longer than {0:?}")]
    Timeout(Duration),
    #[error("not an http(s) url")]
    InvalidUrl,
}

#[derive(Clone, Copy, Debug)]
pub struct DownloadLimits {
    pub max_bytes: u64,
    pub connect: Duration,
    pub response: Duration,
    pub total: Duration,
}

impl Default for DownloadLimits {
    fn default() -> Self {
        Self {
            max_bytes: 262_144_000,
            connect: Duration::from_secs(10),
            response: Duration::from_secs(30),
            total: Duration::from_secs(15 * 60),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadRequest {
    pub id: Uuid,
    pub url: String,
    pub hash: Option<[u8; 20]>,
}

/// The URL when it is a plain `http` or `https` URL; vanilla rejects every other scheme.
pub fn parse_pack_url(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme_ok = scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https");
    let clean = !rest.is_empty() && !url.chars().any(|c| c.is_control() || c.is_whitespace());
    (scheme_ok && clean).then(|| url.to_owned())
}

/// Forty hex digits, as vanilla accepts; anything else means "no hash".
pub fn parse_sha1(text: &str) -> Option<[u8; 20]> {
    if text.len() != 40 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 20];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone)]
pub struct Downloader {
    dir: PathBuf,
    limits: DownloadLimits,
    headers: Vec<(String, String)>,
}

impl Downloader {
    pub fn new(dir: impl Into<PathBuf>, headers: Vec<(String, String)>) -> Self {
        Self {
            dir: dir.into(),
            limits: DownloadLimits::default(),
            headers,
        }
    }

    pub fn with_limits(mut self, limits: DownloadLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn target(&self, request: &DownloadRequest) -> PathBuf {
        match request.hash {
            Some(hash) => self.dir.join(hex(&hash)),
            None => self.dir.join(format!("pack-{}", request.id)),
        }
    }

    /// Downloads (or finds in the cache) the pack and returns its verified file.
    /// `progress` gets bytes so far and the announced total.
    pub fn fetch(
        &self,
        request: &DownloadRequest,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<PathBuf, DownloadError> {
        let url = parse_pack_url(&request.url).ok_or(DownloadError::InvalidUrl)?;
        fs::create_dir_all(&self.dir)?;
        let target = self.target(request);
        if let Some(expected) = request.hash
            && target.is_file()
        {
            if file_sha1(&target).is_ok_and(|actual| actual == expected) {
                tracing::info!("resource pack {} served from cache", request.id);
                touch(&target);
                return Ok(target);
            }
            let _ = fs::remove_file(&target);
        }
        let part = target.with_extension("part");
        let result = self.download(&url, request, &part, progress);
        match result {
            Ok(()) => {
                fs::rename(&part, &target)?;
                Ok(target)
            }
            Err(e) => {
                let _ = fs::remove_file(&part);
                Err(e)
            }
        }
    }

    fn download(
        &self,
        url: &str,
        request: &DownloadRequest,
        part: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), DownloadError> {
        let limits = self.limits;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(limits.connect))
            .timeout_recv_response(Some(limits.response))
            .timeout_global(Some(limits.total))
            .max_redirects(5)
            .build()
            .into();
        let mut get = agent.get(url);
        for (name, value) in &self.headers {
            get = get.header(name, value);
        }
        let started = Instant::now();
        let mut response = get.call().map_err(|e| DownloadError::Http(e.to_string()))?;
        let announced = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        if announced.is_some_and(|n| n > limits.max_bytes) {
            return Err(DownloadError::TooLarge(limits.max_bytes));
        }
        progress(0, announced);
        let mut body = response.body_mut().as_reader();
        let mut out = File::create(part)?;
        let mut hasher = Sha1::new();
        let mut buf = vec![0u8; CHUNK];
        let mut total = 0u64;
        loop {
            let n = body.read(&mut buf)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > limits.max_bytes {
                return Err(DownloadError::TooLarge(limits.max_bytes));
            }
            if started.elapsed() > limits.total {
                return Err(DownloadError::Timeout(limits.total));
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n])?;
            progress(total, announced);
        }
        out.flush()?;
        let actual: [u8; 20] = hasher.finalize().into();
        match request.hash {
            Some(expected) if expected != actual => Err(DownloadError::HashMismatch {
                expected: hex(&expected),
                actual: hex(&actual),
            }),
            _ => Ok(()),
        }
    }
}

fn file_sha1(path: &Path) -> std::io::Result<[u8; 20]> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hasher.finalize().into());
        }
        hasher.update(&buf[..n]);
    }
}

fn touch(path: &Path) {
    if let Ok(file) = File::options().write(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

/// Deletes the oldest files beyond `keep`, never touching `protect`.
pub fn vacuum(dir: &Path, keep: usize, protect: &[PathBuf]) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, path) in files.into_iter().skip(keep) {
        if protect.contains(&path) {
            continue;
        }
        if let Err(e) = fs::remove_file(&path) {
            tracing::warn!("cannot evict {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use super::*;

    fn serve(body: Vec<u8>, announce: Option<u64>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/pack.zip", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let length = announce.unwrap_or(body.len() as u64);
            let head =
                format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n");
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
            drop::<TcpStream>(stream);
        });
        (url, handle)
    }

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mcr-respack-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn sha(bytes: &[u8]) -> [u8; 20] {
        Sha1::digest(bytes).into()
    }

    #[test]
    fn url_and_hash_parsing() {
        assert!(parse_pack_url("https://a.b/c.zip").is_some());
        assert!(parse_pack_url("HTTP://a.b/c.zip").is_some());
        for bad in [
            "file:///etc/passwd",
            "ftp://x/y",
            "a.b/c",
            "http://",
            "http://a b",
            "javascript:alert(1)",
        ] {
            assert!(parse_pack_url(bad).is_none(), "{bad}");
        }
        assert_eq!(parse_sha1(&"ab".repeat(20)), Some([0xab; 20]));
        assert!(parse_sha1("xyz").is_none());
        assert!(parse_sha1(&"g".repeat(40)).is_none());
    }

    #[test]
    fn downloads_verifies_and_caches() {
        let dir = temp("ok");
        let body = b"zip bytes".to_vec();
        let (url, server) = serve(body.clone(), None);
        let dl = Downloader::new(&dir, Vec::new());
        let request = DownloadRequest {
            id: Uuid::from_u128(1),
            url,
            hash: Some(sha(&body)),
        };
        let mut last = 0;
        let path = dl.fetch(&request, &mut |n, _| last = n).unwrap();
        server.join().unwrap();
        assert_eq!(last, body.len() as u64);
        assert_eq!(fs::read(&path).unwrap(), body);
        assert_eq!(
            path.file_name().unwrap().to_str().unwrap(),
            hex(&sha(&body))
        );
        let again = dl.fetch(&request, &mut |_, _| {}).unwrap();
        assert_eq!(again, path);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn hash_mismatch_leaves_nothing() {
        let dir = temp("mismatch");
        let (url, server) = serve(b"real".to_vec(), None);
        let dl = Downloader::new(&dir, Vec::new());
        let request = DownloadRequest {
            id: Uuid::from_u128(2),
            url,
            hash: Some(sha(b"other")),
        };
        assert!(matches!(
            dl.fetch(&request, &mut |_, _| {}),
            Err(DownloadError::HashMismatch { .. })
        ));
        server.join().unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn oversize_is_refused_by_header_and_by_bytes() {
        let limits = DownloadLimits {
            max_bytes: 10,
            ..DownloadLimits::default()
        };
        for announce in [Some(1_000_000), None] {
            let dir = temp("big");
            let (url, server) = serve(vec![7; 100], announce);
            let dl = Downloader::new(&dir, Vec::new()).with_limits(limits);
            let request = DownloadRequest {
                id: Uuid::from_u128(3),
                url,
                hash: None,
            };
            assert!(matches!(
                dl.fetch(&request, &mut |_, _| {}),
                Err(DownloadError::TooLarge(10))
            ));
            let _ = server.join();
            assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
            let _ = fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn non_http_url_is_refused() {
        let dl = Downloader::new(temp("scheme"), Vec::new());
        let request = DownloadRequest {
            id: Uuid::nil(),
            url: "file:///etc/passwd".into(),
            hash: None,
        };
        assert!(matches!(
            dl.fetch(&request, &mut |_, _| {}),
            Err(DownloadError::InvalidUrl)
        ));
    }

    #[test]
    fn vacuum_keeps_newest_and_protected() {
        let dir = temp("vacuum");
        fs::create_dir_all(&dir).unwrap();
        let now = SystemTime::now();
        for i in 0..5u64 {
            let path = dir.join(format!("f{i}"));
            let file = File::create(&path).unwrap();
            file.set_modified(now - Duration::from_secs(100 * (5 - i)))
                .unwrap();
        }
        vacuum(&dir, 2, &[dir.join("f0")]);
        let mut left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["f0", "f3", "f4"]);
        let _ = fs::remove_dir_all(dir);
    }
}
