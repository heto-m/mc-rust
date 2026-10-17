//! Java's only route to the web: a loopback HTTP proxy (`CONNECT` for https, absolute-form requests for
//! http) that reaches allow-listed hosts on global addresses and nothing else.

use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::net::{
    IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Hosts the stock client talks to: skins, profiles and services.
pub const MOJANG_HOSTS: &[&str] = &[
    "textures.minecraft.net",
    "sessionserver.mojang.com",
    "api.minecraftservices.com",
    "api.mojang.com",
    "discovery.minecraftservices.com",
];

const MAX_HOSTS: usize = 256;
const MAX_HOST_LEN: usize = 253;
const MAX_ADDRS: usize = 8;
const MAX_METHOD: usize = 16;
const COPY_BUF: usize = 16 * 1024;
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "proxy-connection",
    "keep-alive",
    "proxy-authorization",
    "proxy-authenticate",
    "te",
    "trailer",
    "upgrade",
    "host",
];

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Exact host names, or `*.example.com` for every subdomain.
    pub hosts: Vec<String>,
    pub ports: Vec<u16>,
    pub max_connections: usize,
    pub max_header: usize,
    /// A request head must arrive within this, and a tunnel closes after this long without traffic.
    pub idle: Duration,
    pub connect_timeout: Duration,
    /// Lets upstreams resolve to loopback or private addresses (tests only).
    pub allow_local: bool,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            hosts: MOJANG_HOSTS.iter().map(|h| (*h).to_owned()).collect(),
            ports: vec![80, 443],
            max_connections: 16,
            max_header: 8 * 1024,
            idle: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            allow_local: false,
        }
    }
}

/// The running proxy; dropping it stops accepting (open tunnels end on their own idle timeout).
#[derive(Debug)]
pub struct HttpProxy {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl HttpProxy {
    pub fn start(config: ProxyConfig) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            allow: AllowList::new(&config.hosts),
            active: AtomicUsize::new(0),
            config,
        });
        let flag = stop.clone();
        let accept = std::thread::Builder::new()
            .name("mcr-http-proxy".into())
            .spawn(move || accept_loop(&listener, &shared, &flag))?;
        tracing::debug!(%addr, "http proxy for Java listening");
        Ok(Self {
            addr,
            stop,
            accept: Some(accept),
        })
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }
}

impl Drop for HttpProxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1));
        if let Some(t) = self.accept.take() {
            let _ = t.join();
        }
    }
}

struct Shared {
    config: ProxyConfig,
    allow: AllowList,
    active: AtomicUsize,
}

struct Slot<'a>(&'a AtomicUsize);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn accept_loop(listener: &TcpListener, shared: &Arc<Shared>, stop: &AtomicBool) {
    for stream in listener.incoming() {
        if stop.load(Ordering::Acquire) {
            return;
        }
        let Ok(mut stream) = stream else {
            continue;
        };
        if shared.active.fetch_add(1, Ordering::AcqRel) >= shared.config.max_connections {
            shared.active.fetch_sub(1, Ordering::AcqRel);
            let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
            let _ = reply(&mut stream, Status::Busy);
            continue;
        }
        let conn = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("mcr-http-proxy-conn".into())
            .spawn(move || {
                let _slot = Slot(&conn.active);
                if let Err(e) = serve(stream, &conn) {
                    tracing::trace!("proxy connection ended: {e}");
                }
            });
        if spawned.is_err() {
            shared.active.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    BadRequest,
    Forbidden,
    Timeout,
    HeaderTooLarge,
    BadGateway,
    Busy,
}

impl Status {
    fn line(self) -> &'static str {
        match self {
            Self::BadRequest => "400 Bad Request",
            Self::Forbidden => "403 Forbidden",
            Self::Timeout => "408 Request Timeout",
            Self::HeaderTooLarge => "431 Request Header Fields Too Large",
            Self::BadGateway => "502 Bad Gateway",
            Self::Busy => "503 Service Unavailable",
        }
    }
}

fn reply(stream: &mut TcpStream, status: Status) -> io::Result<()> {
    let text = format!(
        "HTTP/1.1 {}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        status.line()
    );
    stream.write_all(text.as_bytes())
}

fn serve(mut client: TcpStream, shared: &Shared) -> io::Result<()> {
    let cfg = &shared.config;
    client.set_read_timeout(Some(cfg.idle))?;
    client.set_write_timeout(Some(cfg.idle))?;
    let (head, rest) = match read_head(&mut client, cfg.max_header, cfg.idle) {
        Ok(parts) => parts,
        Err(HeadError::Io(e)) => return Err(e),
        Err(HeadError::Status(s)) => return reply(&mut client, s),
    };
    let request = match parse(&head) {
        Ok(r) => r,
        Err(s) => return reply(&mut client, s),
    };
    if !shared.allow.permits(&request.host) || !cfg.ports.contains(&request.port) {
        tracing::debug!(
            host = request.host,
            port = request.port,
            "proxy denied a host"
        );
        return reply(&mut client, Status::Forbidden);
    }
    let mut upstream = match connect(&request.host, request.port, cfg) {
        Ok(s) => s,
        Err(s) => return reply(&mut client, s),
    };
    upstream.set_write_timeout(Some(cfg.idle))?;
    match &request.forward {
        None => client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?,
        Some(head) => upstream.write_all(head.as_bytes())?,
    }
    upstream.write_all(&rest)?;
    tunnel(client, upstream, cfg.idle)
}

#[derive(Debug)]
enum HeadError {
    Io(io::Error),
    Status(Status),
}

/// The request head (through the blank line) and whatever bytes followed it in the same reads.
fn read_head(
    stream: &mut TcpStream,
    max: usize,
    within: Duration,
) -> Result<(Vec<u8>, Vec<u8>), HeadError> {
    let start = Instant::now();
    let mut buf = Vec::with_capacity(1024.min(max));
    let mut chunk = [0u8; 1024];
    loop {
        let n = match stream.read(&mut chunk) {
            Ok(0) => return Err(HeadError::Io(io::ErrorKind::UnexpectedEof.into())),
            Ok(n) => n,
            Err(e) if timed_out(&e) => return Err(HeadError::Status(Status::Timeout)),
            Err(e) => return Err(HeadError::Io(e)),
        };
        let from = buf.len().saturating_sub(3);
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = find(&buf[from..], b"\r\n\r\n") {
            let end = from + i + 4;
            if end > max {
                return Err(HeadError::Status(Status::HeaderTooLarge));
            }
            let rest = buf.split_off(end);
            return Ok((buf, rest));
        }
        if buf.len() > max {
            return Err(HeadError::Status(Status::HeaderTooLarge));
        }
        if start.elapsed() >= within {
            return Err(HeadError::Status(Status::Timeout));
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn timed_out(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

#[derive(Debug, PartialEq, Eq)]
struct Request {
    host: String,
    port: u16,
    /// The rewritten origin-form head for an absolute-form request; `None` for `CONNECT`.
    forward: Option<String>,
}

fn parse(head: &[u8]) -> Result<Request, Status> {
    let bad = Status::BadRequest;
    let text = std::str::from_utf8(head).map_err(|_| bad)?;
    let text = text.strip_suffix("\r\n\r\n").ok_or(bad)?;
    let mut lines = text.split("\r\n");
    let line = lines.next().ok_or(bad)?;
    if !clean(line) {
        return Err(bad);
    }
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(bad);
    };
    if method.is_empty()
        || method.len() > MAX_METHOD
        || !method.bytes().all(|b| b.is_ascii_uppercase())
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1")
        || target.is_empty()
        || !target.bytes().all(|b| b > 0x20 && b != 0x7f)
    {
        return Err(bad);
    }
    let mut headers = Vec::new();
    for line in lines {
        if !clean(line) || line.starts_with([' ', '\t']) {
            return Err(bad);
        }
        let (name, value) = line.split_once(':').ok_or(bad)?;
        if name.is_empty() || !name.bytes().all(is_token) {
            return Err(bad);
        }
        headers.push((name, value.trim()));
    }
    if method == "CONNECT" {
        let (host, port) = authority(target, None)?;
        return Ok(Request {
            host,
            port,
            forward: None,
        });
    }
    let rest = target
        .get(..7)
        .filter(|s| s.eq_ignore_ascii_case("http://"))
        .map(|_| &target[7..])
        .ok_or(bad)?;
    let split = rest.find(['/', '?']).unwrap_or(rest.len());
    let (auth, path) = rest.split_at(split);
    let (host, port) = authority(auth, Some(80))?;
    let mut out = String::with_capacity(head.len() + 64);
    out.push_str(method);
    out.push(' ');
    if !path.starts_with('/') {
        out.push('/');
    }
    out.push_str(path);
    out.push(' ');
    out.push_str(version);
    out.push_str("\r\nHost: ");
    out.push_str(&host);
    if port != 80 {
        out.push(':');
        out.push_str(&port.to_string());
    }
    out.push_str("\r\n");
    for (name, value) in headers {
        if !HOP_BY_HOP.iter().any(|h| name.eq_ignore_ascii_case(h)) {
            out.push_str(name);
            out.push_str(": ");
            out.push_str(value);
            out.push_str("\r\n");
        }
    }
    out.push_str("Connection: close\r\n\r\n");
    Ok(Request {
        host,
        port,
        forward: Some(out),
    })
}

fn clean(line: &str) -> bool {
    line.bytes().all(|b| b == b'\t' || (b >= 0x20 && b != 0x7f))
}

fn is_token(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// `host[:port]` with a host name or IPv4 literal; no userinfo, no IPv6 literals.
fn authority(text: &str, default_port: Option<u16>) -> Result<(String, u16), Status> {
    let bad = Status::BadRequest;
    let (host, port) = match text.rsplit_once(':') {
        Some((h, p)) => {
            let port = p.parse::<u16>().ok().filter(|&p| p != 0).ok_or(bad)?;
            (h, port)
        }
        None => (text, default_port.ok_or(bad)?),
    };
    let host = normalize(host).ok_or(bad)?;
    Ok((host, port))
}

fn normalize(host: &str) -> Option<String> {
    let host = host.strip_suffix('.').unwrap_or(host);
    let ok = !host.is_empty()
        && host.len() <= MAX_HOST_LEN
        && host
            .split('.')
            .all(|l| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-'))
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    ok.then(|| host.to_ascii_lowercase())
}

#[derive(Debug, Default)]
struct AllowList {
    exact: HashSet<String>,
    suffixes: Vec<String>,
}

impl AllowList {
    fn new(hosts: &[String]) -> Self {
        let mut list = Self::default();
        for entry in hosts.iter().take(MAX_HOSTS) {
            match entry.strip_prefix("*.") {
                Some(base) => match normalize(base) {
                    Some(base) if base.contains('.') => list.suffixes.push(format!(".{base}")),
                    _ => tracing::warn!("ignoring proxy allow-list entry {entry:?}"),
                },
                None => match normalize(entry) {
                    Some(host) => {
                        list.exact.insert(host);
                    }
                    None => tracing::warn!("ignoring proxy allow-list entry {entry:?}"),
                },
            }
        }
        list
    }

    fn permits(&self, host: &str) -> bool {
        self.exact.contains(host) || self.suffixes.iter().any(|s| host.ends_with(s.as_str()))
    }
}

fn connect(host: &str, port: u16, cfg: &ProxyConfig) -> Result<TcpStream, Status> {
    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|_| Status::BadGateway)?
        .take(MAX_ADDRS)
        .collect();
    if addrs.is_empty() {
        return Err(Status::BadGateway);
    }
    if !cfg.allow_local && !addrs.iter().all(|a| is_global(a.ip())) {
        tracing::debug!(host, "proxy refused a host resolving to a local address");
        return Err(Status::Forbidden);
    }
    addrs
        .iter()
        .find_map(|a| TcpStream::connect_timeout(a, cfg.connect_timeout).ok())
        .ok_or(Status::BadGateway)
}

fn is_global(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_global_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_global_v4(v4),
            None => is_global_v6(v6),
        },
    }
}

fn is_global_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
        || (a == 198 && (18..20).contains(&b))
        || a >= 240)
}

fn is_global_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || first == 0x2001 && ip.segments()[1] == 0x0db8
        || first == 0x0064 && ip.segments()[1] == 0xff9b)
}

/// Copies both ways until either side closes or no byte moves for `idle`.
fn tunnel(client: TcpStream, upstream: TcpStream, idle: Duration) -> io::Result<()> {
    let base = Instant::now();
    let last = Arc::new(AtomicU64::new(0));
    client.set_read_timeout(Some(idle))?;
    upstream.set_read_timeout(Some(idle))?;
    let (c_read, u_write) = (client.try_clone()?, upstream.try_clone()?);
    let up_last = last.clone();
    let up = std::thread::Builder::new()
        .name("mcr-http-proxy-up".into())
        .spawn(move || pump(c_read, u_write, &up_last, base, idle))?;
    pump(
        upstream.try_clone()?,
        client.try_clone()?,
        &last,
        base,
        idle,
    );
    let _ = client.shutdown(Shutdown::Both);
    let _ = upstream.shutdown(Shutdown::Both);
    let _ = up.join();
    Ok(())
}

fn pump(mut from: TcpStream, mut to: TcpStream, last: &AtomicU64, base: Instant, idle: Duration) {
    let mut buf = vec![0u8; COPY_BUF];
    loop {
        match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if to.write_all(&buf[..n]).is_err() {
                    break;
                }
                last.store(base.elapsed().as_millis() as u64, Ordering::Relaxed);
            }
            Err(e) if timed_out(&e) => {
                let now = base.elapsed().as_millis() as u64;
                let quiet = now.saturating_sub(last.load(Ordering::Relaxed));
                if quiet >= idle.as_millis() as u64 {
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    let _ = to.shutdown(Shutdown::Write);
}

#[cfg(test)]
mod tests;
