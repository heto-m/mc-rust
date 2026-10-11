mod packets;
mod recorder;
mod server;

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use mcr_data::{Direction, ProtocolState, Version};
use mcr_net::{ConnectOptions, Connection, Event, Switch};
use recorder::Recorder;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(50);
const CHUNKS_PER_TICK: f32 = 25.0;
const MOVES_AFTER_JOIN: usize = 3;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "localhost:25565")]
    addr: String,
    #[arg(long, default_value = "Capture")]
    name: String,
    #[arg(long, default_value_t = 60)]
    seconds: u64,
    /// Reply to select_known_packs with an empty list so the server sends full registry data.
    #[arg(long)]
    no_known_packs: bool,
    /// Console commands fed to the spawned server; `sleep <secs>` pauses, `{name}` is the player.
    #[arg(long, requires = "spawn_server")]
    script: Option<PathBuf>,
    /// Run `cargo xtask server --nogui` for the duration of the capture.
    #[arg(long)]
    spawn_server: bool,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long, default_value_t = 8)]
    view_distance: i8,
}

struct Session {
    conn: Connection,
    data: &'static Version,
    rec: Recorder,
    state: ProtocolState,
    shared_state: Arc<AtomicU8>,
    no_known_packs: bool,
    view_distance: i8,
    position: Option<([f64; 3], [f32; 2])>,
    moves_left: usize,
    joined: bool,
}

impl Session {
    fn set_state(&mut self, state: ProtocolState) {
        self.state = state;
        self.shared_state.store(state as u8, Ordering::Release);
    }

    fn send(&self, state: ProtocolState, name: &str, payload: &[u8]) -> Result<()> {
        let id = lookup_id(self.data, state, Direction::Serverbound, name)
            .ok_or_else(|| anyhow!("no serverbound packet `{name}` in {state:?}"))?;
        self.conn.send(id as i32, payload)?;
        Ok(())
    }

    fn handle(&mut self, id: i32, payload: &[u8]) -> Result<bool> {
        let state = self.state;
        let name = match u32::try_from(id)
            .ok()
            .and_then(|i| self.data.packet_name(state, Direction::Clientbound, i))
        {
            Some(n) => n.strip_prefix("minecraft:").unwrap_or(n).to_owned(),
            None => format!("unknown_{id}"),
        };
        self.rec.record(
            Version::protocol_state_name(state),
            Version::direction_name(Direction::Clientbound),
            id,
            &name,
            payload,
        )?;
        match (state, name.as_str()) {
            (_, "disconnect" | "login_disconnect") => return Ok(false),
            (ProtocolState::Login, "hello") => bail!("server requires encryption (online mode)"),
            (ProtocolState::Login, "login_compression") => {
                let mut p = payload;
                self.conn
                    .enable_compression(mcr_buf::read_varint(&mut p)?)?;
            }
            (ProtocolState::Login, "login_finished") => {
                self.send(state, "login_acknowledged", &[])?;
                self.set_state(ProtocolState::Configuration);
                self.send(
                    ProtocolState::Configuration,
                    "client_information",
                    &packets::client_information(self.view_distance),
                )?;
            }
            (ProtocolState::Configuration, "select_known_packs") => {
                let reply = if self.no_known_packs {
                    packets::empty_known_packs()
                } else {
                    payload.to_vec()
                };
                self.send(state, "select_known_packs", &reply)?;
            }
            (ProtocolState::Configuration, "code_of_conduct") => {
                self.send(state, "accept_code_of_conduct", &[])?;
            }
            (ProtocolState::Configuration, "finish_configuration") => {
                self.send(state, "finish_configuration", &[])?;
                self.set_state(ProtocolState::Play);
            }
            (ProtocolState::Configuration | ProtocolState::Play, "keep_alive") => {
                let id: [u8; 8] = payload.try_into().context("keep_alive length")?;
                self.send(state, "keep_alive", &id)?;
            }
            (ProtocolState::Configuration | ProtocolState::Play, "ping") => {
                let id: [u8; 4] = payload.try_into().context("ping length")?;
                self.send(state, "pong", &id)?;
            }
            (ProtocolState::Play, "login") => {
                self.joined = true;
                self.send(state, "player_loaded", &[])?;
            }
            (ProtocolState::Play, "player_position") => {
                let tp = packets::parse_player_position(payload).context("player_position")?;
                self.send(
                    state,
                    "accept_teleportation",
                    &packets::accept_teleportation(tp.id, tp.pos, tp.rot),
                )?;
                self.position = Some((tp.pos, tp.rot));
            }
            (ProtocolState::Play, "chunk_batch_finished") => {
                self.send(
                    state,
                    "chunk_batch_received",
                    &packets::chunk_batch_received(CHUNKS_PER_TICK),
                )?;
            }
            (ProtocolState::Play, "start_configuration") => {
                self.send(state, "configuration_acknowledged", &[])?;
                self.set_state(ProtocolState::Configuration);
                self.send(
                    ProtocolState::Configuration,
                    "client_information",
                    &packets::client_information(self.view_distance),
                )?;
            }
            _ => {}
        }
        Ok(true)
    }

    fn tick(&mut self) -> Result<()> {
        if self.state != ProtocolState::Play || !self.joined {
            return Ok(());
        }
        self.send(ProtocolState::Play, "client_tick_end", &[])?;
        if let Some((pos, rot)) = self.position
            && self.moves_left > 0
        {
            let step = (MOVES_AFTER_JOIN - self.moves_left + 1) as f64;
            let moved = [pos[0] + 0.3 * step, pos[1], pos[2] + 0.2 * step];
            self.moves_left -= 1;
            self.send(
                ProtocolState::Play,
                "move_player_pos_rot",
                &packets::move_player_pos_rot(moved, [rot[0] + 15.0 * step as f32, rot[1]], true),
            )?;
        }
        Ok(())
    }
}

// Version::packet_id binary-searches a table ordered by id, so resolve names by scanning.
fn lookup_id(data: &Version, state: ProtocolState, dir: Direction, name: &str) -> Option<u32> {
    let full = format!("minecraft:{name}");
    (0..1024).find(|&i| data.packet_name(state, dir, i) == Some(full.as_str()))
}

fn split_addr(addr: &str) -> (String, u16) {
    match addr.rsplit_once(':') {
        Some((h, p)) => (h.to_owned(), p.parse().unwrap_or(25565)),
        None => (addr.to_owned(), 25565),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let data = mcr_data::v26_3();
    let (host, port) = split_addr(&args.addr);
    let out = args.out.clone().unwrap_or_else(|| {
        let suffix = if args.no_known_packs {
            "-no-known-packs"
        } else {
            ""
        };
        PathBuf::from(format!(".cache/captures/26.3{suffix}"))
    });

    let mut server = None;
    if args.spawn_server {
        std::fs::create_dir_all(".cache/captures")?;
        eprintln!("starting server");
        server = Some(server::Server::spawn(
            port,
            &PathBuf::from(".cache/captures/server.log"),
        )?);
    }

    let result = capture(&args, data, &host, port, &out, server.as_ref());
    if let Some(mut s) = server {
        s.stop();
    }
    result
}

fn capture(
    args: &Args,
    data: &'static Version,
    host: &str,
    port: u16,
    out: &Path,
    server: Option<&server::Server>,
) -> Result<()> {
    let shared_state = Arc::new(AtomicU8::new(ProtocolState::Handshake as u8));
    let compression_id = lookup_id(
        data,
        ProtocolState::Login,
        Direction::Clientbound,
        "login_compression",
    )
    .context("login_compression id")?;
    let hook_state = shared_state.clone();
    let hook = Box::new(move |id: i32, payload: &[u8]| {
        if hook_state.load(Ordering::Acquire) != ProtocolState::Login as u8
            || id != compression_id as i32
        {
            return None;
        }
        let mut p = payload;
        mcr_buf::read_varint(&mut p).ok().map(Switch::Compression)
    });
    let conn = Connection::connect_with(
        args.addr.as_str(),
        ConnectOptions {
            inbound_hook: Some(hook),
            connect_timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        },
    )?;

    let mut s = Session {
        conn,
        data,
        rec: Recorder::create(out)?,
        state: ProtocolState::Handshake,
        shared_state,
        no_known_packs: args.no_known_packs,
        view_distance: args.view_distance,
        position: None,
        moves_left: MOVES_AFTER_JOIN,
        joined: false,
    };

    s.send(
        ProtocolState::Handshake,
        "intention",
        &packets::intention(host, port),
    )?;
    s.set_state(ProtocolState::Login);
    let uuid = mcr_auth::offline::offline_uuid(&args.name)?;
    s.send(
        ProtocolState::Login,
        "hello",
        &packets::hello(&args.name, uuid.as_bytes()),
    )?;

    let deadline = Instant::now() + Duration::from_secs(args.seconds);
    let mut next_tick = Instant::now() + TICK;
    let mut script_started = false;
    let mut script = None;
    let mut error = None;
    while Instant::now() < deadline {
        if let Some(Event::Frame(f)) = s.conn.recv_timeout(Duration::from_millis(10)) {
            match s.handle(f.id, &f.payload) {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        } else if let Some(reason) = s.conn.close_reason() {
            error = Some(anyhow!("connection closed: {reason}"));
            break;
        }
        if Instant::now() >= next_tick {
            next_tick += TICK;
            if let Err(e) = s.tick() {
                error = Some(e);
                break;
            }
        }
        if s.joined
            && !script_started
            && let (Some(server), Some(path)) = (server, &args.script)
        {
            script_started = true;
            let steps = server::parse_script(&std::fs::read_to_string(path)?, &args.name);
            let console = server.console();
            script = Some(std::thread::spawn(move || {
                server::run_script(&console, steps);
            }));
        }
    }

    drop(script);
    let counts = s.rec.finish()?;
    report(&counts);
    error.map_or(Ok(()), Err)
}

fn report(counts: &std::collections::HashMap<(String, &'static str, String), usize>) {
    let mut rows: Vec<_> = counts.iter().collect();
    rows.sort();
    let mut per_state = std::collections::BTreeMap::<&str, (usize, usize)>::new();
    for ((state, _, name), n) in &rows {
        println!("{state:14} {name:48} {n}");
        let e = per_state.entry(state).or_default();
        e.0 += 1;
        e.1 += **n;
    }
    for (state, (kinds, total)) in per_state {
        println!("{state}: {kinds} packet kinds, {total} samples");
    }
}
