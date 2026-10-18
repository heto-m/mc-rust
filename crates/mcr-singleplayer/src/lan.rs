//! Open to LAN for a loopback server: a TCP forwarder on the LAN port and vanilla's multicast announcement.

use std::io::{self, Read, Write};
use std::net::{
    IpAddr, Ipv4Addr, Shutdown, SocketAddr, SocketAddrV4, TcpListener, TcpStream, UdpSocket,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mcr_buf::{from_bytes_exact, read_varint, to_bytes, write_varint};
use mcr_protocol::handshake::{Intent, Intention};
use mcr_protocol::login::{LoginDisconnect, ServerboundHello};

/// Vanilla's LAN discovery group and port.
pub const ANNOUNCE_GROUP: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(224, 0, 2, 60), 4445);
pub const ANNOUNCE_INTERVAL: Duration = Duration::from_millis(1500);
pub const MAX_CONNECTIONS: usize = 16;
pub const MIN_PORT: u16 = 1024;
const BUFFER: usize = 16 * 1024;
const ACCEPT_POLL: Duration = Duration::from_millis(50);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_MOTD_CHARS: usize = 64;
/// Overrides the address the forwarder binds, for development and tests.
pub const BIND_ENV: &str = "MCR_LAN_BIND";
/// How long a LAN client gets to send its handshake and login start.
pub const GATE_TIMEOUT: Duration = Duration::from_secs(5);
/// Bytes read before the gate decides; a handshake and login start fit in well under this.
const MAX_GATE_BYTES: usize = 1024;
const MAX_HANDSHAKE_FRAME: usize = 1 + 5 + 3 + 255 * 3 + 2 + 5;
const MAX_HELLO_FRAME: usize = 1 + 3 + 16 * 3 + 16;
/// Vanilla reads a first byte of 0xFE as a pre-1.7 server list ping, never a handshake.
const LEGACY_PING: u8 = 0xfe;
const NAME_TAKEN: &str = r#"{"translate":"multiplayer.disconnect.name_taken"}"#;

/// Who the forwarder protects: the host's name and offline UUID, which a LAN guest must never log in as.
#[derive(Debug, Clone)]
pub struct Gate {
    pub host: String,
    pub host_uuid: [u8; 16],
}

/// What the first bytes of a LAN connection are.
#[derive(Debug, PartialEq, Eq)]
enum Opening {
    NeedMore,
    Status,
    Login { name: String, uuid: [u8; 16] },
    Bad,
}

/// One length-prefixed frame from `input` with packet id 0 (the handshake and login start ids in every version).
fn frame<'a>(input: &mut &'a [u8], max: usize) -> Result<Option<&'a [u8]>, ()> {
    let mut rest = *input;
    let len = match read_varint(&mut rest) {
        Ok(len) => usize::try_from(len).map_err(|_| ())?,
        Err(mcr_buf::DecodeError::UnexpectedEof) if input.len() < 3 => return Ok(None),
        Err(_) => return Err(()),
    };
    if len == 0 || len > max {
        return Err(());
    }
    if rest.len() < len {
        return Ok(None);
    }
    let (body, tail) = rest.split_at(len);
    let mut body = body;
    if !matches!(read_varint(&mut body), Ok(0)) {
        return Err(());
    }
    *input = tail;
    Ok(Some(body))
}

fn opening(bytes: &[u8]) -> Opening {
    if bytes.first() == Some(&LEGACY_PING) {
        return Opening::Bad;
    }
    let mut input = bytes;
    let handshake = match frame(&mut input, MAX_HANDSHAKE_FRAME) {
        Ok(Some(body)) => body,
        Ok(None) => return Opening::NeedMore,
        Err(()) => return Opening::Bad,
    };
    let Ok(intention) = from_bytes_exact::<Intention>(handshake) else {
        return Opening::Bad;
    };
    if intention.intent == Intent::Status {
        return Opening::Status;
    }
    match frame(&mut input, MAX_HELLO_FRAME) {
        Ok(Some(body)) => match from_bytes_exact::<ServerboundHello>(body) {
            Ok(hello) => Opening::Login {
                name: hello.name,
                uuid: hello.profile_id.into_bytes(),
            },
            Err(_) => Opening::Bad,
        },
        Ok(None) => Opening::NeedMore,
        Err(()) => Opening::Bad,
    }
}

/// Reads the opening within [`GATE_TIMEOUT`] and [`MAX_GATE_BYTES`]; returns it with every byte read.
fn read_opening(mut client: &TcpStream) -> io::Result<(Opening, Vec<u8>)> {
    let deadline = Instant::now() + GATE_TIMEOUT;
    let mut buf = Vec::with_capacity(256);
    let mut chunk = [0u8; 256];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        client.set_read_timeout(Some(left))?;
        let room = (MAX_GATE_BYTES - buf.len()).min(chunk.len());
        let n = client.read(&mut chunk[..room])?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        buf.extend_from_slice(&chunk[..n]);
        match opening(&buf) {
            Opening::NeedMore if buf.len() < MAX_GATE_BYTES => {}
            Opening::NeedMore => return Ok((Opening::Bad, buf)),
            other => return Ok((other, buf)),
        }
    }
}

fn login_disconnect(reason: &str) -> Vec<u8> {
    let mut body = vec![0u8];
    body.extend(to_bytes(&LoginDisconnect {
        reason: reason.to_owned(),
    }));
    let mut out = Vec::with_capacity(body.len() + 5);
    write_varint(&mut out, body.len() as i32);
    out.extend(body);
    out
}

/// The address LAN players connect to: all IPv4 interfaces, or `MCR_LAN_BIND`.
pub fn bind_address(port: u16) -> SocketAddr {
    let ip = std::env::var(BIND_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<Ipv4Addr>().ok())
        .unwrap_or(Ipv4Addr::UNSPECIFIED);
    SocketAddr::V4(SocketAddrV4::new(ip, port))
}

/// Where announcements go: the multicast group, or loopback when the forwarder only listens on loopback.
pub fn announce_target(bind: SocketAddr) -> SocketAddr {
    if bind.ip().is_loopback() {
        SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            ANNOUNCE_GROUP.port(),
        ))
    } else {
        SocketAddr::V4(ANNOUNCE_GROUP)
    }
}

/// `None` for text that is not a port vanilla accepts (empty means "any free port" and is handled by the caller).
pub fn parse_port(text: &str) -> Option<u16> {
    text.trim().parse::<u16>().ok().filter(|p| *p >= MIN_PORT)
}

/// Whether a LAN listener could bind `port` right now.
pub fn port_available(port: u16) -> bool {
    TcpListener::bind(bind_address(port)).is_ok()
}

/// The integrated server's MOTD, `<player> - <world>`, safe inside the announcement's tags.
pub fn lan_motd(player: &str, world: &str) -> String {
    format!("{player} - {world}")
        .chars()
        .filter(|c| !c.is_control() && *c != '[' && *c != ']')
        .take(MAX_MOTD_CHARS)
        .collect()
}

pub fn announcement(motd: &str, port: u16) -> String {
    format!("[MOTD]{motd}[/MOTD][AD]{port}[/AD]")
}

struct Shared {
    running: AtomicBool,
    live: AtomicUsize,
    sockets: Mutex<Vec<(u64, TcpStream)>>,
    gate: Gate,
    /// Lowercase names of guests logged in through the forwarder.
    guests: Mutex<Vec<String>>,
}

impl Shared {
    fn close_all(&self) {
        if let Ok(mut sockets) = self.sockets.lock() {
            for (_, s) in sockets.drain(..) {
                let _ = s.shutdown(Shutdown::Both);
            }
        }
    }

    fn forget(&self, id: u64) {
        if let Ok(mut sockets) = self.sockets.lock() {
            sockets.retain(|(i, _)| *i != id);
        }
    }

    fn track(&self, id: u64, socket: &TcpStream) -> io::Result<()> {
        let mut sockets = self
            .sockets
            .lock()
            .map_err(|_| io::Error::other("poisoned"))?;
        if !self.running.load(Ordering::Acquire) {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        sockets.push((id, socket.try_clone()?));
        Ok(())
    }

    /// Claims `name` for a guest unless it is the host's, the host's UUID, or already online.
    fn admit(&self, name: &str, uuid: &[u8; 16]) -> bool {
        let lower = name.to_ascii_lowercase();
        if lower == self.gate.host.to_ascii_lowercase() || *uuid == self.gate.host_uuid {
            return false;
        }
        let Ok(mut guests) = self.guests.lock() else {
            return false;
        };
        if guests.contains(&lower) {
            return false;
        }
        guests.push(lower);
        true
    }

    fn leave(&self, name: &str) {
        let lower = name.to_ascii_lowercase();
        if let Ok(mut guests) = self.guests.lock()
            && let Some(i) = guests.iter().position(|g| *g == lower)
        {
            guests.swap_remove(i);
        }
    }
}

/// Accepts LAN connections and pipes each to the loopback server.
pub struct Forwarder {
    port: u16,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Forwarder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Forwarder")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl Forwarder {
    /// Binds `bind` (port 0 picks a free one) and forwards to `target` the status pings and the logins `gate` admits.
    pub fn start(bind: SocketAddr, target: SocketAddr, gate: Gate) -> io::Result<Self> {
        let listener = TcpListener::bind(bind)?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared {
            running: AtomicBool::new(true),
            live: AtomicUsize::new(0),
            sockets: Mutex::new(Vec::new()),
            gate,
            guests: Mutex::new(Vec::new()),
        });
        let worker = shared.clone();
        let thread = std::thread::Builder::new()
            .name("mcr-lan-accept".into())
            .spawn(move || accept_loop(&listener, target, &worker))?;
        Ok(Self {
            port,
            shared,
            thread: Some(thread),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn connections(&self) -> usize {
        self.shared.live.load(Ordering::Acquire)
    }

    pub fn stop(&mut self) {
        self.shared.running.store(false, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.shared.close_all();
    }
}

impl Drop for Forwarder {
    fn drop(&mut self) {
        self.stop();
    }
}

fn accept_loop(listener: &TcpListener, target: SocketAddr, shared: &Arc<Shared>) {
    let mut next_id = 0u64;
    while shared.running.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((client, _)) => {
                if shared.live.load(Ordering::Acquire) >= MAX_CONNECTIONS {
                    drop(client);
                    continue;
                }
                next_id += 1;
                let id = next_id;
                shared.live.fetch_add(1, Ordering::AcqRel);
                let worker = shared.clone();
                let spawned = std::thread::Builder::new()
                    .name("mcr-lan-gate".into())
                    .spawn(move || {
                        if let Err(e) = admit_and_forward(client, target, &worker, id) {
                            tracing::debug!("lan connection: {e}");
                            worker.live.fetch_sub(1, Ordering::AcqRel);
                        }
                    });
                if spawned.is_err() {
                    shared.live.fetch_sub(1, Ordering::AcqRel);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(ACCEPT_POLL),
            Err(e) => {
                tracing::debug!("lan accept: {e}");
                std::thread::sleep(ACCEPT_POLL);
            }
        }
    }
    shared.close_all();
}

/// Lets a status ping or an admitted login through to the server; an `Err` means the connection is gone.
fn admit_and_forward(
    mut client: TcpStream,
    target: SocketAddr,
    shared: &Arc<Shared>,
    id: u64,
) -> io::Result<()> {
    client.set_nonblocking(false)?;
    shared.track(id * 2, &client)?;
    let opened = read_opening(&client);
    shared.forget(id * 2);
    let (opening, prefix) = opened?;
    let guest = match opening {
        Opening::Status => None,
        Opening::Login { name, uuid } => {
            if !shared.admit(&name, &uuid) {
                let _ = client.write_all(&login_disconnect(NAME_TAKEN));
                let _ = client.shutdown(Shutdown::Both);
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("refused login as {name}"),
                ));
            }
            Some(name)
        }
        Opening::NeedMore | Opening::Bad => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a Minecraft login",
            ));
        }
    };
    let result = forward(client, target, shared, id, &prefix, guest.clone());
    if result.is_err()
        && let Some(name) = &guest
    {
        shared.leave(name);
    }
    result
}

fn forward(
    client: TcpStream,
    target: SocketAddr,
    shared: &Arc<Shared>,
    id: u64,
    prefix: &[u8],
    guest: Option<String>,
) -> io::Result<()> {
    client.set_read_timeout(None)?;
    let mut server = TcpStream::connect_timeout(&target, CONNECT_TIMEOUT)?;
    let _ = client.set_nodelay(true);
    let _ = server.set_nodelay(true);
    server.write_all(prefix)?;
    shared.track(id * 2, &client)?;
    if let Err(e) = shared.track(id * 2 + 1, &server) {
        shared.forget(id * 2);
        return Err(e);
    }
    let pending = Arc::new(AtomicUsize::new(2));
    let pipes = [(client.try_clone()?, server.try_clone()?), (server, client)];
    for (from, to) in pipes {
        let (shared, pending, guest) = (shared.clone(), pending.clone(), guest.clone());
        std::thread::Builder::new()
            .name("mcr-lan-pipe".into())
            .spawn(move || {
                pipe(&from, &to);
                let _ = from.shutdown(Shutdown::Both);
                let _ = to.shutdown(Shutdown::Both);
                if pending.fetch_sub(1, Ordering::AcqRel) == 1 {
                    shared.forget(id * 2);
                    shared.forget(id * 2 + 1);
                    if let Some(name) = &guest {
                        shared.leave(name);
                    }
                    shared.live.fetch_sub(1, Ordering::AcqRel);
                }
            })?;
    }
    Ok(())
}

fn pipe(mut from: &TcpStream, mut to: &TcpStream) {
    let mut buf = [0u8; BUFFER];
    loop {
        match from.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                if to.write_all(&buf[..n]).is_err() {
                    return;
                }
            }
        }
    }
}

/// Sends vanilla's LAN announcement on an interval until stopped.
pub struct Announcer {
    stop: Arc<(Mutex<bool>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Announcer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Announcer").finish_non_exhaustive()
    }
}

impl Announcer {
    pub fn start(
        motd: &str,
        port: u16,
        target: SocketAddr,
        interval: Duration,
    ) -> io::Result<Self> {
        let local = match target.ip() {
            IpAddr::V4(ip) if ip.is_loopback() => Ipv4Addr::LOCALHOST,
            _ => Ipv4Addr::UNSPECIFIED,
        };
        let socket = UdpSocket::bind(SocketAddrV4::new(local, 0))?;
        let message = announcement(motd, port).into_bytes();
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("mcr-lan-announce".into())
            .spawn(move || {
                let (lock, wake) = &*flag;
                let Ok(mut stopped) = lock.lock() else { return };
                while !*stopped {
                    if let Err(e) = socket.send_to(&message, target) {
                        tracing::debug!("lan announcement: {e}");
                    }
                    match wake.wait_timeout(stopped, interval) {
                        Ok((guard, _)) => stopped = guard,
                        Err(_) => return,
                    }
                }
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    pub fn stop(&mut self) {
        let (lock, wake) = &*self.stop;
        if let Ok(mut stopped) = lock.lock() {
            *stopped = true;
        }
        wake.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Announcer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn loopback(port: u16) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
    }

    /// A loopback "server" that echoes every byte back.
    fn echo_server() -> (SocketAddr, JoinHandle<()>, Arc<AtomicBool>) {
        let listener = TcpListener::bind(loopback(0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let run = Arc::new(AtomicBool::new(true));
        let flag = run.clone();
        let thread = std::thread::spawn(move || {
            while flag.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((s, _)) => {
                        s.set_nonblocking(false).unwrap();
                        std::thread::spawn(move || {
                            let mut buf = [0u8; 1024];
                            let mut s = s;
                            while let Ok(n) = s.read(&mut buf) {
                                if n == 0 || s.write_all(&buf[..n]).is_err() {
                                    break;
                                }
                            }
                        });
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        (addr, thread, run)
    }

    fn wait_until(mut f: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    const HOST_UUID: [u8; 16] = [7; 16];

    fn gate() -> Gate {
        Gate {
            host: "Host".into(),
            host_uuid: HOST_UUID,
        }
    }

    fn framed(body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        write_varint(&mut out, body.len() as i32 + 1);
        out.push(0);
        out.extend_from_slice(body);
        out
    }

    fn handshake(intent: Intent) -> Vec<u8> {
        framed(&to_bytes(&Intention {
            protocol_version: 777,
            host: "localhost".into(),
            port: 25565,
            intent,
        }))
    }

    fn hello(name: &str, uuid: [u8; 16]) -> Vec<u8> {
        let mut body = Vec::new();
        mcr_buf::encode_str(name, &mut body);
        body.extend_from_slice(&uuid);
        framed(&body)
    }

    fn opening_bytes(name: &str, uuid: [u8; 16]) -> Vec<u8> {
        let mut out = handshake(Intent::Login);
        out.extend(hello(name, uuid));
        out
    }

    fn connect(port: u16) -> TcpStream {
        let s = TcpStream::connect(loopback(port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        s
    }

    /// Logs in as `name` through the forwarder and reads the echoed opening back.
    fn login(port: u16, name: &str) -> TcpStream {
        let mut s = connect(port);
        let opening = opening_bytes(name, [1; 16]);
        s.write_all(&opening).unwrap();
        let mut back = vec![0u8; opening.len()];
        s.read_exact(&mut back).unwrap();
        assert_eq!(back, opening, "the opening reaches the server unchanged");
        s
    }

    fn closed(s: &mut TcpStream) -> bool {
        let mut buf = [0u8; 64];
        loop {
            match s.read(&mut buf) {
                Ok(0) | Err(_) => return true,
                Ok(_) => {}
            }
        }
    }

    /// The login disconnect a refused client gets, then the end of the stream.
    fn refused(s: &mut TcpStream) {
        let mut got = Vec::new();
        let _ = s.read_to_end(&mut got);
        assert_eq!(got, login_disconnect(NAME_TAKEN));
    }

    #[test]
    fn forwards_both_ways_and_stops() {
        let (server, thread, run) = echo_server();
        let mut fwd = Forwarder::start(loopback(0), server, gate()).unwrap();
        assert_ne!(fwd.port(), 0);
        let mut client = login(fwd.port(), "Guest");
        let payload: Vec<u8> = (0..100_000u32).map(|i| i as u8).collect();
        let writer = {
            let mut c = client.try_clone().unwrap();
            let p = payload.clone();
            std::thread::spawn(move || c.write_all(&p).unwrap())
        };
        let mut back = vec![0u8; payload.len()];
        client.read_exact(&mut back).unwrap();
        writer.join().unwrap();
        assert_eq!(back, payload);
        assert!(wait_until(|| fwd.connections() == 1));
        fwd.stop();
        assert!(closed(&mut client), "stop closes forwarded connections");
        assert!(
            TcpStream::connect(loopback(fwd.port())).is_err(),
            "the listener is closed"
        );
        run.store(false, Ordering::Release);
        thread.join().unwrap();
    }

    #[test]
    fn status_pings_pass() {
        let (server, thread, run) = echo_server();
        let fwd = Forwarder::start(loopback(0), server, gate()).unwrap();
        let mut s = connect(fwd.port());
        let mut ping = handshake(Intent::Status);
        ping.extend([1, 0]);
        s.write_all(&ping).unwrap();
        let mut back = vec![0u8; ping.len()];
        s.read_exact(&mut back).unwrap();
        assert_eq!(back, ping);
        drop((s, fwd));
        run.store(false, Ordering::Release);
        thread.join().unwrap();
    }

    #[test]
    fn connection_count_is_bounded() {
        let (server, thread, run) = echo_server();
        let fwd = Forwarder::start(loopback(0), server, gate()).unwrap();
        let mut held: Vec<TcpStream> = (0..MAX_CONNECTIONS)
            .map(|i| login(fwd.port(), &format!("G{i}")))
            .collect();
        assert!(wait_until(|| fwd.connections() == MAX_CONNECTIONS));
        let mut extra = connect(fwd.port());
        let _ = extra.write_all(&opening_bytes("Extra", [2; 16]));
        assert!(closed(&mut extra), "over the cap");
        let mut buf = [0u8; 1];
        held[0].write_all(b"y").unwrap();
        held[0].read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"y", "live connections keep working");
        drop(held);
        assert!(
            wait_until(|| fwd.connections() == 0),
            "closed pipes are forgotten"
        );
        drop(fwd);
        run.store(false, Ordering::Release);
        thread.join().unwrap();
    }

    #[test]
    fn the_host_cannot_be_impersonated() {
        let (server, thread, run) = echo_server();
        let fwd = Forwarder::start(loopback(0), server, gate()).unwrap();
        for (name, uuid) in [
            ("Host", [1; 16]),
            ("HOST", [1; 16]),
            ("hoSt", [1; 16]),
            ("Other", HOST_UUID),
        ] {
            let mut s = connect(fwd.port());
            s.write_all(&opening_bytes(name, uuid)).unwrap();
            refused(&mut s);
        }
        assert!(wait_until(|| fwd.connections() == 0));
        drop(fwd);
        run.store(false, Ordering::Release);
        thread.join().unwrap();
    }

    #[test]
    fn names_already_online_are_refused() {
        let (server, thread, run) = echo_server();
        let fwd = Forwarder::start(loopback(0), server, gate()).unwrap();
        let first = login(fwd.port(), "Guest");
        let mut twin = connect(fwd.port());
        twin.write_all(&opening_bytes("gUEST", [3; 16])).unwrap();
        refused(&mut twin);
        drop(first);
        assert!(wait_until(|| fwd.connections() == 0));
        let again = login(fwd.port(), "guest");
        drop((again, fwd));
        run.store(false, Ordering::Release);
        thread.join().unwrap();
    }

    #[test]
    fn garbage_and_oversized_openings_are_dropped() {
        let mut long_name = handshake(Intent::Login);
        long_name.extend(hello(&"n".repeat(17), [1; 16]));
        let mut wrong_id = handshake(Intent::Login);
        wrong_id[1] = 1;
        let mut bad_intent = handshake(Intent::Login);
        let last = bad_intent.len() - 1;
        bad_intent[last] = 9;
        let cases: Vec<Vec<u8>> = vec![
            vec![0xfe, 0x01, 0xfa],
            vec![0xff, 0xff, 0xff, 0xff, 0x0f],
            vec![0xff, 0x7f],
            vec![0x00, 0x00],
            wrong_id,
            bad_intent,
            long_name,
            vec![0x80; MAX_GATE_BYTES],
        ];
        for case in &cases {
            assert_eq!(opening(case), Opening::Bad, "{case:02x?}");
        }
        assert_eq!(opening(&handshake(Intent::Login)), Opening::NeedMore);
        assert_eq!(opening(&[0x05]), Opening::NeedMore);
        assert_eq!(
            opening(&opening_bytes("Guest", [4; 16])),
            Opening::Login {
                name: "Guest".into(),
                uuid: [4; 16]
            }
        );

        let (server, thread, run) = echo_server();
        let fwd = Forwarder::start(loopback(0), server, gate()).unwrap();
        for case in &cases {
            let mut s = connect(fwd.port());
            let _ = s.write_all(case);
            let mut got = Vec::new();
            let _ = s.read_to_end(&mut got);
            assert!(got.is_empty(), "nothing reaches the server: {case:02x?}");
        }
        let mut endless = connect(fwd.port());
        let mut flood = handshake(Intent::Login);
        flood.extend(vec![0x80; 4096]);
        let _ = endless.write_all(&flood);
        assert!(closed(&mut endless));
        assert!(wait_until(|| fwd.connections() == 0));
        drop(fwd);
        run.store(false, Ordering::Release);
        thread.join().unwrap();
    }

    #[test]
    fn slow_openings_time_out_without_blocking_others() {
        let (server, thread, run) = echo_server();
        let fwd = Forwarder::start(loopback(0), server, gate()).unwrap();
        let started = Instant::now();
        let mut slow = connect(fwd.port());
        slow.write_all(&handshake(Intent::Login)[..2]).unwrap();
        let quick = login(fwd.port(), "Quick");
        assert!(
            started.elapsed() < GATE_TIMEOUT,
            "a slow client does not hold up the next"
        );
        assert!(closed(&mut slow));
        let waited = started.elapsed();
        assert!(
            waited >= GATE_TIMEOUT - Duration::from_millis(100) && waited < GATE_TIMEOUT * 2,
            "{waited:?}"
        );
        drop((quick, fwd));
        run.store(false, Ordering::Release);
        thread.join().unwrap();
    }

    #[test]
    fn unreachable_server_drops_the_client() {
        let dead = TcpListener::bind(loopback(0))
            .unwrap()
            .local_addr()
            .unwrap();
        let fwd = Forwarder::start(loopback(0), dead, gate()).unwrap();
        let mut client = connect(fwd.port());
        client.write_all(&opening_bytes("Guest", [1; 16])).unwrap();
        assert!(closed(&mut client));
        assert!(wait_until(|| fwd.connections() == 0));
        assert!(
            fwd.shared.guests.lock().unwrap().is_empty(),
            "the name is free again"
        );
    }

    #[test]
    fn announces_on_loopback_until_stopped() {
        let socket = UdpSocket::bind(loopback(0)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let target = socket.local_addr().unwrap();
        let motd = lan_motd("Steve", "x[/MOTD][AD]25565[/AD]\u{7}");
        let mut announcer =
            Announcer::start(&motd, 25601, target, Duration::from_millis(20)).unwrap();
        let mut buf = [0u8; 512];
        for _ in 0..2 {
            let (n, _) = socket.recv_from(&mut buf).unwrap();
            let text = std::str::from_utf8(&buf[..n]).unwrap();
            assert_eq!(text, "[MOTD]Steve - x/MOTDAD25565/AD[/MOTD][AD]25601[/AD]");
        }
        announcer.stop();
        socket
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut late = 0;
        while socket.recv_from(&mut buf).is_ok() {
            late += 1;
            assert!(late < 3, "silent after stop");
        }
    }

    #[test]
    fn motd_and_ports() {
        assert_eq!(lan_motd("A", &"w".repeat(200)).chars().count(), 64);
        assert_eq!(parse_port(" 25601 "), Some(25601));
        assert_eq!(parse_port("80"), None);
        assert_eq!(parse_port("70000"), None);
        assert_eq!(parse_port("x"), None);
        assert_eq!(
            announce_target(loopback(1)),
            loopback(ANNOUNCE_GROUP.port())
        );
        assert_eq!(
            announce_target("0.0.0.0:1".parse().unwrap()),
            SocketAddr::V4(ANNOUNCE_GROUP)
        );
    }
}
