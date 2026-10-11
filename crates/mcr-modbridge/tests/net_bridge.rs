//! Opt-in acceptance test for the packet bridge against a running vanilla 26.3 server (offline):
//! `MCR_JVM_TESTS=1 MCR_LIVE_ADDR=localhost:25565 MCR_LIVE_CONSOLE=<file the server's stdin tails>
//!  MCR_LIVE_LOG=<server log> cargo test -p mcr-modbridge --test net_bridge -- --nocapture`
//! A recording proxy sits between Rust and the server, so the server-side view is checked on the wire.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mcr_client::{Client, ClientConfig, PlayerInput};
use mcr_modbridge::net::{NetPhase, PhaseTracker, ProbeRequest, TransitionIds, WorldProbe};
use mcr_modbridge::{Bridge, BridgeConfig, BridgeEvent, LaunchManifest, NetLink, ShutdownOutcome};
use mcr_net::{BufPool, FrameDecoder};
use mcr_protocol::{Direction, PacketIds};

const USERNAME: &str = "Bridge";
const BOOT_DEADLINE: Duration = Duration::from_secs(240);
const JOINED_FOR: Duration = Duration::from_secs(30);
const TESTMOD_CHANNEL: &str = "mcr_testmod:hello";
const VIEW_DISTANCE: i8 = 12;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[derive(Debug, Default)]
struct Wire {
    serverbound: Vec<(NetPhase, String)>,
    channels: Vec<(NetPhase, String)>,
    commands: Vec<String>,
    keep_alive_requests: Vec<i64>,
    keep_alive_answers: Vec<i64>,
    clientbound: BTreeMap<String, u64>,
    client_closed: bool,
}

impl Wire {
    fn count(&self, phase: NetPhase, name: &str) -> usize {
        self.serverbound
            .iter()
            .filter(|(p, n)| *p == phase && n == name)
            .count()
    }
}

fn string(mut body: &[u8]) -> Option<String> {
    let len = usize::try_from(mcr_buf::read_varint(&mut body).ok()?).ok()?;
    Some(String::from_utf8_lossy(body.get(..len)?).into_owned())
}

/// Reads through to the other socket only after the frames in each read are decoded, so a
/// compression switch is known before the client can react to it.
struct Recording<'a> {
    inner: &'a mut TcpStream,
    seen: Vec<u8>,
}

impl Read for Recording<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.seen.extend_from_slice(&buf[..n]);
        Ok(n)
    }
}

fn spawn_proxy(server: String) -> (u16, Arc<Mutex<Wire>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let wire = Arc::new(Mutex::new(Wire::default()));
    let log = wire.clone();
    std::thread::spawn(move || {
        for client in listener.incoming() {
            let Ok(client) = client else { return };
            let upstream = TcpStream::connect(&server).expect("proxy reaches the server");
            relay_pair(client, upstream, log.clone());
        }
    });
    (port, wire)
}

fn relay_pair(client: TcpStream, upstream: TcpStream, wire: Arc<Mutex<Wire>>) {
    let threshold = Arc::new(AtomicI32::new(-1));
    let ids = PacketIds::v26_3();
    {
        let (mut from, mut to) = (upstream.try_clone().unwrap(), client.try_clone().unwrap());
        let (wire, threshold) = (wire.clone(), threshold.clone());
        std::thread::spawn(move || {
            let mut dec = FrameDecoder::new(BufPool::new(16));
            let mut tracker = PhaseTracker::new(*TransitionIds::v26_3());
            let compression = TransitionIds::v26_3().login_compression;
            loop {
                let mut rec = Recording {
                    inner: &mut from,
                    seen: Vec::new(),
                };
                if dec.fill(&mut rec).is_err() {
                    let _ = to.shutdown(Shutdown::Both);
                    return;
                }
                let seen = rec.seen;
                while let Ok(Some(frame)) = dec.next_frame() {
                    let phase = tracker.observe(frame.id);
                    if phase == NetPhase::Login && frame.id == compression {
                        let mut p = &frame.payload[..];
                        let t = mcr_buf::read_varint(&mut p).unwrap();
                        dec.set_compression(t);
                        threshold.store(t, Ordering::Release);
                    }
                    let name = ids
                        .name_by_id(phase.state(), Direction::Clientbound, frame.id)
                        .unwrap_or("?");
                    let mut w = wire.lock().unwrap();
                    *w.clientbound.entry(name.to_owned()).or_default() += 1;
                    if name == "keep_alive" && frame.payload.len() == 8 {
                        let id = i64::from_be_bytes(frame.payload[..8].try_into().unwrap());
                        w.keep_alive_requests.push(id);
                    }
                }
                if to.write_all(&seen).is_err() {
                    return;
                }
            }
        });
    }
    let (mut from, mut to) = (client, upstream);
    std::thread::spawn(move || {
        let mut dec = FrameDecoder::new(BufPool::new(16));
        let mut phase = NetPhase::Login;
        let mut compressed = false;
        let sb = |phase: NetPhase, id| {
            ids.name_by_id(phase.state(), Direction::Serverbound, id)
                .unwrap_or("?")
                .to_owned()
        };
        loop {
            let mut rec = Recording {
                inner: &mut from,
                seen: Vec::new(),
            };
            if dec.fill(&mut rec).is_err() {
                wire.lock().unwrap().client_closed = true;
                let _ = to.shutdown(Shutdown::Both);
                return;
            }
            let seen = rec.seen;
            if !compressed && threshold.load(Ordering::Acquire) >= 0 {
                dec.set_compression(threshold.load(Ordering::Acquire));
                compressed = true;
            }
            while let Ok(Some(frame)) = dec.next_frame() {
                let name = sb(phase, frame.id);
                let mut w = wire.lock().unwrap();
                match name.as_str() {
                    "custom_payload" => {
                        if let Some(ch) = string(&frame.payload) {
                            w.channels.push((phase, ch));
                        }
                    }
                    "chat_command" => {
                        if let Some(cmd) = string(&frame.payload) {
                            w.commands.push(cmd);
                        }
                    }
                    "keep_alive" if frame.payload.len() == 8 => {
                        let id = i64::from_be_bytes(frame.payload[..8].try_into().unwrap());
                        w.keep_alive_answers.push(id);
                    }
                    _ => {}
                }
                w.serverbound.push((phase, name.clone()));
                phase = match (phase, name.as_str()) {
                    (NetPhase::Login, "login_acknowledged") => NetPhase::Configuration,
                    (NetPhase::Configuration, "finish_configuration") => NetPhase::Play,
                    (NetPhase::Play, "configuration_acknowledged") => NetPhase::Configuration,
                    (p, _) => p,
                };
            }
            if to.write_all(&seen).is_err() {
                return;
            }
        }
    });
}

fn console(line: &str) {
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(env("MCR_LIVE_CONSOLE"))
        .unwrap();
    writeln!(f, "{line}").unwrap();
}

struct Session {
    client: Client,
    link: NetLink,
    history: HashMap<u64, [f64; 3]>,
}

impl Session {
    fn tick(&mut self, input: &PlayerInput) -> Result<(), mcr_client::ClientError> {
        let start = Instant::now();
        let result = self.client.tick(input);
        let tick = self.client.ticks - 1;
        self.history
            .insert(tick, self.client.state.player.physics.pos);
        if let Some(rest) = Duration::from_millis(50).checked_sub(start.elapsed()) {
            std::thread::sleep(rest);
        }
        result
    }

    fn idle(&mut self, ticks: u32) {
        for _ in 0..ticks {
            let input = self.client.idle_input();
            self.tick(&input).expect("tick");
        }
    }

    fn probe(&mut self, request: &ProbeRequest) -> WorldProbe {
        let id = self.link.request_probe(request);
        for _ in 0..200 {
            let input = self.client.idle_input();
            self.tick(&input).expect("tick");
            if let Some(p) = self.link.take_probe()
                && p.request == id
            {
                return p;
            }
        }
        panic!("no probe answer for request {id}");
    }
}

fn join(link: NetLink, port: u16) -> Session {
    let mut cfg = ClientConfig::default().with_addr(&format!("127.0.0.1:{port}"));
    cfg.name = USERNAME.into();
    cfg.view_distance = VIEW_DISTANCE;
    let hooks = link.hooks("127.0.0.1", port);
    let client = Client::connect_hooked(cfg, hooks).expect("join through the bridge");
    Session {
        client,
        link,
        history: HashMap::new(),
    }
}

fn wait_event(bridge: &mut Bridge, what: &str, mut done: impl FnMut(&BridgeEvent) -> bool) {
    let start = Instant::now();
    while start.elapsed() < BOOT_DEADLINE {
        if let Some(event) = bridge.next_event(Duration::from_millis(100)) {
            if let BridgeEvent::Crash { report } = &event {
                panic!("java crashed: {report:?}");
            }
            if done(&event) {
                return;
            }
        }
    }
    panic!("no {what}");
}

fn sample_positions(client: &Client) -> Vec<[i32; 3]> {
    let [px, py, pz] = client.state.player.physics.pos;
    let (px, py, pz) = (px.floor() as i32, py.floor() as i32, pz.floor() as i32);
    let mut out = Vec::new();
    for i in 0..400 {
        if out.len() == 20 {
            break;
        }
        let x = px + (i * 7 % 41) - 20;
        let z = pz + (i * 13 % 41) - 20;
        let y = py + (i * 5 % 31) - 20;
        if client.state.chunks.block(x, y, z).is_some() {
            out.push([x, y, z]);
        }
    }
    out
}

#[test]
fn net_bridge() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1")
        || std::env::var("MCR_LIVE_ADDR").is_err()
    {
        eprintln!("skipped: set MCR_JVM_TESTS=1 and MCR_LIVE_ADDR (see the file header)");
        return;
    }
    let cache = mcr_jvm::dev_cache_dir();
    for module in ["mcr-boot", "mcr-bridge", "mcr-testmod"] {
        let jar = cache.join(format!("java/{module}.jar"));
        assert!(
            mcr_jvm::stale_java_jar(&repo().join("java").join(module), &jar).is_none(),
            "{} is stale: run `cargo xtask java-build`",
            jar.display()
        );
    }
    let manifest = LaunchManifest::load(&cache).expect("run `cargo xtask fabric-fetch`");
    let game_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("net-bridge");
    let _ = std::fs::remove_dir_all(&game_dir);
    let mut mods = manifest.test_mods.clone();
    mods.push(cache.join("java/mcr-testmod.jar"));
    let mut cfg = BridgeConfig::new(cache, game_dir, mods);
    cfg.username = USERNAME.into();
    cfg.uuid = mcr_auth::offline::offline_uuid(USERNAME)
        .unwrap()
        .hyphenated()
        .to_string();
    cfg.max_heap = "1536m".into();
    let (port, wire) = spawn_proxy(env("MCR_LIVE_ADDR"));

    let mut bridge = Bridge::start(cfg).expect("bridge start");
    wait_event(&mut bridge, "title screen", BridgeEvent::is_title_screen);
    let link = bridge.net();

    console(&format!("op {USERNAME}"));
    let title_request = link.request_probe(&ProbeRequest {
        gc: true,
        ..ProbeRequest::default()
    });
    std::thread::sleep(Duration::from_secs(2));
    let mut s = join(link, port);
    let title_probe = link
        .take_probe()
        .expect("probe answered on the title screen");
    assert_eq!(title_probe.request, title_request);
    assert!(!title_probe.level);
    let joined = Instant::now();
    s.idle(40);
    // Spectators pass through terrain, so the walk is 20 blocks wherever the world spawned us.
    console(&format!("gamemode spectator {USERNAME}"));
    s.idle(20);
    let mut walked = 0.0;
    let start_pos = s.client.state.player.physics.pos;
    while joined.elapsed() < JOINED_FOR {
        let walking = walked < 20.0;
        let mut input = s.client.idle_input();
        input.forward = walking;
        input.sprint = walking;
        s.tick(&input).expect("tick");
        let p = s.client.state.player.physics.pos;
        walked = ((p[0] - start_pos[0]).powi(2) + (p[2] - start_pos[2]).powi(2)).sqrt();
    }
    assert!(walked >= 20.0, "the player walked only {walked:.1} blocks");
    s.idle(40);

    // World equality, retried while frames in flight settle.
    let mut last = String::new();
    let mut equal = None;
    for _ in 0..20 {
        let [px, _, pz] = s.client.state.player.physics.pos;
        let center = ((px.floor() as i32) >> 4, (pz.floor() as i32) >> 4);
        let radius = i32::from(VIEW_DISTANCE) + 4;
        let positions = sample_positions(&s.client);
        let probe = s.probe(&ProbeRequest {
            center,
            radius,
            positions: positions.clone(),
            gc: true,
            disconnect: false,
        });
        let rust_chunks: BTreeSet<(i32, i32)> = s
            .client
            .state
            .chunks
            .positions()
            .map(|c| (c.x, c.z))
            .filter(|&(x, z)| (x - center.0).abs() <= radius && (z - center.1).abs() <= radius)
            .collect();
        let java_chunks: BTreeSet<(i32, i32)> = probe.chunks.iter().copied().collect();
        let rust_blocks: Vec<i32> = positions
            .iter()
            .map(|&[x, y, z]| {
                s.client
                    .state
                    .chunks
                    .block(x, y, z)
                    .map_or(-1, |b| i32::from(b.0))
            })
            .collect();
        let mut rust_entities: BTreeSet<i32> =
            s.client.state.entities.iter().map(|e| e.id).collect();
        rust_entities.insert(s.client.state.player.entity_id);
        let java_entities: BTreeSet<i32> = probe.entities.iter().copied().collect();
        let player_ok = probe.player.is_some_and(|(tick, pos)| {
            s.history
                .get(&tick)
                .is_some_and(|want| (0..3).all(|i| (pos[i] - want[i]).abs() <= 1e-6))
        });
        if rust_chunks == java_chunks
            && rust_blocks == probe.blocks
            && rust_entities == java_entities
            && player_ok
            && positions.len() == 20
        {
            equal = Some((probe, rust_chunks.len(), rust_entities.len()));
            break;
        }
        last = format!(
            "chunks rust-only {:?} java-only {:?}; blocks {:?} vs {:?}; entities rust-only {:?} java-only {:?}; player {:?} ok={player_ok}",
            rust_chunks
                .difference(&java_chunks)
                .take(8)
                .collect::<Vec<_>>(),
            java_chunks
                .difference(&rust_chunks)
                .take(8)
                .collect::<Vec<_>>(),
            rust_blocks,
            probe.blocks,
            rust_entities.difference(&java_entities).collect::<Vec<_>>(),
            java_entities.difference(&rust_entities).collect::<Vec<_>>(),
            probe.player,
        );
        s.idle(10);
    }
    let Some((probe, columns, entities)) = equal else {
        panic!("Java's world never matched Rust's: {last}");
    };

    let stats = s.link.stats();
    let w = wire.lock().unwrap();
    let hello: Vec<_> = w
        .channels
        .iter()
        .filter(|(p, c)| *p == NetPhase::Play && c == TESTMOD_CHANNEL)
        .collect();
    assert_eq!(
        hello.len(),
        1,
        "test mod payload on the wire: {:?}",
        w.channels
    );
    let brands = w
        .channels
        .iter()
        .filter(|(_, c)| c == "minecraft:brand")
        .count();
    assert_eq!(brands, 1, "only Rust's brand: {:?}", w.channels);
    assert!(
        !w.keep_alive_requests.is_empty(),
        "no keep-alive in {JOINED_FOR:?}"
    );
    let mut answers = w.keep_alive_answers.clone();
    answers.sort_unstable();
    let mut requests = w.keep_alive_requests.clone();
    requests.sort_unstable();
    assert_eq!(answers, requests, "exactly one answer per keep-alive");
    for (ack, cause) in [
        ("accept_teleportation", "player_position"),
        ("chunk_batch_received", "chunk_batch_finished"),
    ] {
        let acks = w.count(NetPhase::Play, ack);
        let causes = w.clientbound.get(cause).copied().unwrap_or(0) as usize;
        assert_eq!(acks, causes, "{ack} per {cause}");
    }
    assert_eq!(w.count(NetPhase::Play, "player_loaded"), 1);
    assert_eq!(w.count(NetPhase::Configuration, "client_information"), 1);
    assert_eq!(w.count(NetPhase::Configuration, "select_known_packs"), 1);
    assert_eq!(
        w.commands.iter().filter(|c| *c == "time query day").count(),
        1,
        "the test mod's command went through Rust once: {:?}",
        w.commands
    );
    assert_eq!(w.count(NetPhase::Play, "chat_command_signed"), 0);
    let dropped_movement: u64 = stats
        .dropped
        .iter()
        .filter(|((_, n), _)| n.starts_with("move_player") || n == "keep_alive")
        .map(|(_, c)| c)
        .sum();
    assert_eq!(stats.desyncs, 0);
    let java_payloads = w.channels.len() - brands;
    assert_eq!(
        stats.forwarded as usize, java_payloads,
        "Java forwarded only custom payloads: {stats:#?} {:?}",
        w.channels
    );
    drop(w);

    let heap_per_column =
        (probe.heap_used.saturating_sub(title_probe.heap_used)) as f64 / columns.max(1) as f64;
    eprintln!(
        "net-bridge: walked {walked:.1} blocks; forwarded {}; {columns} columns, {entities} entities equal; \
         java heap {} MiB at title, {} MiB joined, {:.1} KiB/column (upper bound); \
         {} chunk packets in {:.1} ms ({:.1} us each); java fed {} frames in {:.1} ms ({:.2} us/frame); \
         rust copied {} frames / {} MiB, spill peak {} B, hidden {}, dropped java movement+keepalive {dropped_movement}, \
         dropped {:?}",
        stats.forwarded,
        title_probe.heap_used >> 20,
        probe.heap_used >> 20,
        heap_per_column / 1024.0,
        probe.chunk_packets,
        probe.chunk_nanos as f64 / 1e6,
        probe.chunk_nanos as f64 / 1e3 / probe.chunk_packets.max(1) as f64,
        probe.frames_fed,
        probe.feed_nanos as f64 / 1e6,
        probe.feed_nanos as f64 / 1e3 / probe.frames_fed.max(1) as f64,
        stats.frames_copied,
        stats.bytes_copied >> 20,
        stats.spill_peak_bytes,
        stats.frames_hidden,
        stats.dropped,
    );

    // Java's pause-menu disconnect closes Rust's socket.
    s.link.request_probe(&ProbeRequest {
        disconnect: true,
        ..ProbeRequest::default()
    });
    let mut closed = false;
    for _ in 0..200 {
        let input = s.client.idle_input();
        if s.tick(&input).is_err() {
            closed = true;
            break;
        }
    }
    assert!(closed, "Rust kept the connection after Java disconnected");
    drop(s);
    let start = Instant::now();
    while !wire.lock().unwrap().client_closed {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "socket still open"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    // A server kick reaches Java's disconnect screen.
    std::thread::sleep(Duration::from_secs(1));
    let mut s = join(link, port);
    s.idle(60);
    console(&format!("kick {USERNAME} mcr bridge kick test"));
    let mut kicked = false;
    for _ in 0..200 {
        let input = s.client.idle_input();
        if s.tick(&input).is_err() {
            kicked = true;
            break;
        }
    }
    assert!(kicked, "server kick did not close Rust's connection");
    drop(s);
    wait_event(
        &mut bridge,
        "Java disconnect screen",
        |e| matches!(e, BridgeEvent::Screen(c) if c.ends_with("DisconnectedScreen")),
    );
    assert!(!link.is_detached());

    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );
}
