//! Local worlds: the world list's store and one managed vanilla server at a time.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use mcr_singleplayer::lan::{self, Announcer, Forwarder};
use mcr_singleplayer::level::{self, Difficulty, GameMode, LevelInfo};
use mcr_singleplayer::props::{self, LaunchProps, NewWorld, WorldType};
use mcr_singleplayer::server::{Event, ManagedServer, STOP_GRACE, Spec, VanillaLaunch};
use mcr_singleplayer::{Layout, flat, java, memory, packs, provision, recreate, safe, stale};
use mcr_ui::{
    CreateWorldSettings, Favicon, LanSettings, NewGameMode, WorldEntry, WorldPreset, WorldStore,
};

pub const EULA_URL: &str = "https://aka.ms/MinecraftEULA";
const MAX_UPDATES_PER_POLL: usize = 64;

pub struct FileWorlds {
    layout: Layout,
    entries: Vec<WorldEntry>,
    busy: Option<String>,
}

impl std::fmt::Debug for FileWorlds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileWorlds")
            .field("worlds", &self.entries.len())
            .finish()
    }
}

fn mode_key(mode: GameMode) -> &'static str {
    match mode {
        GameMode::Survival => "gameMode.survival",
        GameMode::Creative => "gameMode.creative",
        GameMode::Adventure => "gameMode.adventure",
        GameMode::Spectator => "gameMode.spectator",
    }
}

impl FileWorlds {
    pub fn new(layout: Layout) -> Self {
        Self {
            layout,
            entries: Vec::new(),
            busy: None,
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// The world whose server runs; it cannot be renamed or deleted meanwhile.
    pub fn set_busy(&mut self, folder: Option<String>) {
        self.busy = folder;
    }
}

impl WorldStore for FileWorlds {
    fn worlds(&self) -> &[WorldEntry] {
        &self.entries
    }

    fn refresh(&mut self) {
        self.entries = level::scan(&self.layout.saves())
            .into_iter()
            .map(|w| WorldEntry {
                played: level::format_played(w.info.last_played),
                mode_key: mode_key(w.info.game_mode).into(),
                hardcore: w.info.hardcore,
                cheats: self
                    .layout
                    .server_dir(&w.folder)
                    .and_then(|d| props::read_properties_file(&d.join(props::WORLD_META)))
                    .as_deref()
                    .and_then(props::read_cheats)
                    .unwrap_or(w.info.cheats),
                version: w.info.version.unwrap_or_default(),
                icon: w.icon.map(|rgba| {
                    let mut h = DefaultHasher::new();
                    rgba.hash(&mut h);
                    Rc::new(Favicon {
                        id: h.finish(),
                        rgba,
                    })
                }),
                name: w.info.name,
                folder: w.folder,
            })
            .collect();
    }

    fn rename(&mut self, folder: &str, name: &str) -> Result<(), String> {
        if self.busy.as_deref() == Some(folder) {
            return Err("the world is open".into());
        }
        let world = safe::checked_world(&self.layout.saves(), folder).map_err(|e| e.to_string())?;
        level::rename(&world, name).map_err(|e| e.to_string())
    }

    fn delete(&mut self, folder: &str) -> Result<(), String> {
        if self.busy.as_deref() == Some(folder) {
            return Err("the world is open".into());
        }
        level::delete_world(&self.layout.saves(), folder).map_err(|e| e.to_string())
    }

    fn recreate(&self, folder: &str) -> Result<CreateWorldSettings, String> {
        let world = safe::checked_world(&self.layout.saves(), folder).map_err(|e| e.to_string())?;
        let r = recreate::read(&world).map_err(|e| e.to_string())?;
        let cheats = self
            .layout
            .server_dir(folder)
            .and_then(|d| props::read_properties_file(&d.join(props::WORLD_META)))
            .as_deref()
            .and_then(props::read_cheats)
            .unwrap_or(r.level.cheats);
        Ok(recreate_settings(&r, cheats, &world))
    }

    fn data_packs(&self) -> (Option<PathBuf>, Vec<String>) {
        let dir = self.pack_dir();
        let list = packs::list_packs(&dir);
        (Some(dir), list)
    }

    fn open_pack_folder(&mut self) {
        let dir = self.pack_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("cannot create {}: {e}", dir.display());
            return;
        }
        crate::mods_ui::open_path(&dir);
    }

    fn check_flat(&self, layers: &str) -> Result<(), String> {
        flat::generator_settings(layers, &known_block).map(|_| ())
    }
}

impl FileWorlds {
    /// Where the native Data Packs screen takes packs from.
    fn pack_dir(&self) -> PathBuf {
        self.layout.game_dir().join("datapacks")
    }
}

fn known_block(id: &str) -> bool {
    mcr_data::v26_3().block_by_name(id).is_some()
}

/// Create New World's settings from a world read for Re-Create.
fn recreate_settings(
    r: &recreate::Recreate,
    cheats: bool,
    world: &std::path::Path,
) -> CreateWorldSettings {
    let mode = if r.level.hardcore {
        NewGameMode::Hardcore
    } else if r.level.game_mode == GameMode::Creative {
        NewGameMode::Creative
    } else {
        NewGameMode::Survival
    };
    CreateWorldSettings {
        name: r.level.name.clone(),
        mode,
        difficulty: match r.level.difficulty {
            Difficulty::Peaceful => 0,
            Difficulty::Easy => 1,
            Difficulty::Normal => 2,
            Difficulty::Hard => 3,
        },
        cheats: cheats && !r.level.hardcore,
        preset: match r.world_type {
            WorldType::Normal => WorldPreset::Normal,
            WorldType::Flat => WorldPreset::Flat,
            WorldType::LargeBiomes => WorldPreset::LargeBiomes,
            WorldType::Amplified => WorldPreset::Amplified,
        },
        seed: r.seed.clone(),
        structures: r.structures,
        bonus_chest: r.bonus_chest,
        game_rules: r.game_rules.clone(),
        experiments: r.experiments.clone(),
        data_packs: r.data_packs.clone(),
        pack_dir: (!r.data_packs.is_empty()).then(|| world.join("datapacks")),
        flat: r.generator_settings.as_deref().map(|g| {
            flat::preset_string(g)
                .filter(|_| plain_layers(g))
                .unwrap_or_else(|| g.to_owned())
        }),
    }
}

/// Generator JSON with nothing a preset string cannot express.
fn plain_layers(settings: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(settings)
        .ok()
        .and_then(|v| {
            v.as_object()
                .map(|o| o.keys().all(|k| k == "layers" || k == "biome"))
        })
        .unwrap_or(false)
}

#[derive(Clone, Debug)]
pub enum Launch {
    Existing(String),
    New(CreateWorldSettings),
}

impl Launch {
    pub fn action(&self) -> mcr_ui::ScreenAction {
        match self {
            Self::Existing(folder) => mcr_ui::ScreenAction::PlayWorld(folder.clone()),
            Self::New(s) => mcr_ui::ScreenAction::CreateWorld(Box::new(s.clone())),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LaunchInput {
    pub layout: Layout,
    pub launch: Launch,
    pub player: String,
    pub uuid: Option<String>,
    pub view_distance: i32,
    pub simulation_distance: i32,
    /// The repository `.cache`, searched for the dev JDK like the mod layer does.
    pub dev_cache: PathBuf,
}

fn new_world(s: &CreateWorldSettings) -> NewWorld {
    NewWorld {
        name: s.name.clone(),
        game_mode: if s.mode == NewGameMode::Creative {
            GameMode::Creative
        } else {
            GameMode::Survival
        },
        hardcore: s.mode == NewGameMode::Hardcore,
        difficulty: Difficulty::from_id(i64::from(s.difficulty)),
        cheats: s.cheats && s.mode != NewGameMode::Hardcore,
        world_type: match s.preset {
            WorldPreset::Normal => WorldType::Normal,
            WorldPreset::Flat => WorldType::Flat,
            WorldPreset::LargeBiomes => WorldType::LargeBiomes,
            WorldPreset::Amplified => WorldType::Amplified,
        },
        seed: s.seed.clone(),
        structures: s.structures,
        bonus_chest: s.bonus_chest,
        game_rules: mcr_data::game_rules::changed_rules(
            s.game_rules.iter().map(|(k, v)| (k.as_str(), v.as_str())),
        )
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect(),
        experiments: packs::experiments(s.experiments.iter().map(String::as_str))
            .into_iter()
            .map(str::to_owned)
            .collect(),
        generator_settings: None,
    }
}

/// Java's Create New World options as the native screen's settings.
pub fn java_create_settings(w: &mcr_modbridge::world::NewWorldRequest) -> CreateWorldSettings {
    use mcr_modbridge::world::{NewMode, NewPreset};
    CreateWorldSettings {
        name: w.name.clone(),
        mode: match w.mode {
            NewMode::Survival => NewGameMode::Survival,
            NewMode::Hardcore => NewGameMode::Hardcore,
            NewMode::Creative => NewGameMode::Creative,
        },
        difficulty: w.difficulty.min(3),
        cheats: w.cheats,
        preset: match w.preset {
            NewPreset::Normal => WorldPreset::Normal,
            NewPreset::Flat => WorldPreset::Flat,
            NewPreset::LargeBiomes => WorldPreset::LargeBiomes,
            NewPreset::Amplified => WorldPreset::Amplified,
        },
        seed: w.seed.clone(),
        structures: w.structures,
        bonus_chest: w.bonus_chest,
        game_rules: w.game_rules.clone(),
        experiments: w.experiments.clone(),
        data_packs: w.packs.clone(),
        pack_dir: w.pack_dir.clone(),
        flat: w.flat.clone(),
    }
}

/// Whether the local server should be frozen: Java's pause rule while its GUI is composed, else the native one.
pub fn should_freeze(
    in_world: bool,
    java_gui: bool,
    java_pausing: bool,
    native_pause: bool,
) -> bool {
    in_world && if java_gui { java_pausing } else { native_pause }
}

/// What a prepared launch hands to the session besides the server spec.
struct Prepared {
    spec: Spec,
    folder: String,
    world_dir: PathBuf,
    port: u16,
    world_name: String,
    cheats: bool,
    /// Game rules to set once a new world's server is ready.
    rules: Vec<(String, String)>,
}

enum Prep {
    Status(String),
    Ready(Box<Prepared>),
    Failed(String),
}

fn free_port() -> std::io::Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

/// Bytes-to-percent status updates, sent only when the percentage changes.
fn progress<'a>(
    tx: &'a Sender<Prep>,
    cancel: &'a AtomicBool,
    what: &'a str,
) -> impl FnMut(u64, Option<u64>) -> bool + 'a {
    let mut last = None;
    move |done, total| {
        let pct = total.filter(|t| *t > 0).map(|t| (done * 100 / t).min(100));
        if pct != last {
            last = pct;
            let text = match pct {
                Some(p) => format!("{what}: {p}%"),
                None => format!("{what}…"),
            };
            let _ = tx.send(Prep::Status(text));
        }
        !cancel.load(Ordering::Relaxed)
    }
}

/// Checks a new world's superflat layers and data packs before its folder exists.
fn check_new(s: &CreateWorldSettings) -> Result<Option<String>, String> {
    let generator = match (&s.flat, s.preset) {
        (Some(text), WorldPreset::Flat) => Some(
            flat::generator_settings(text, &known_block)
                .map_err(|e| format!("The superflat layers cannot be used: {e}"))?,
        ),
        _ => None,
    };
    if !s.data_packs.is_empty() {
        let dir = s
            .pack_dir
            .as_deref()
            .ok_or("the data packs have no folder")?;
        if s.data_packs.len() > packs::MAX_PACKS {
            return Err(format!("at most {} data packs", packs::MAX_PACKS));
        }
        for name in &s.data_packs {
            packs::check_pack(dir, name)
                .map_err(|e| format!("The world cannot be created: {e}"))?;
        }
    }
    Ok(generator)
}

fn prepare(
    input: &LaunchInput,
    tx: &Sender<Prep>,
    cancel: &AtomicBool,
) -> Result<Prepared, String> {
    let layout = &input.layout;
    let saves = layout.saves();
    let refused = |e: mcr_singleplayer::SpError| format!("The world cannot be used: {e}");
    let (folder, world_dir, server_dir, level, created) = match &input.launch {
        Launch::Existing(folder) => {
            let (world, server) = safe::open_world(&saves, folder, false).map_err(refused)?;
            let level =
                level::read_level(&world).map_err(|e| format!("cannot read the world: {e}"))?;
            (folder.clone(), world, server, Some(level), None)
        }
        Launch::New(s) => {
            let mut new = new_world(s);
            new.generator_settings = check_new(s)?;
            let folder = mcr_singleplayer::layout::folder_for(&new.name, |c| {
                saves.join(c).symlink_metadata().is_ok()
            });
            let (world, server) = safe::open_world(&saves, &folder, true).map_err(refused)?;
            if let Some(dir) = s.pack_dir.as_deref().filter(|_| !s.data_packs.is_empty())
                && let Err(e) = packs::install(&world, dir, &s.data_packs)
            {
                let _ = level::delete_world(&saves, &folder);
                return Err(format!("The data packs could not be copied: {e}"));
            }
            (folder, world, server, None, Some(new))
        }
    };
    let _ = tx.send(Prep::Status("Checking the world…".into()));
    if stale::clear_stale(&world_dir, &server_dir).map_err(|e| e.to_string())? {
        println!("singleplayer: stopped a leftover server for {folder}");
    }

    let _ = tx.send(Prep::Status("Checking the server…".into()));
    let jar =
        provision::ensure_server_jar(layout, &mut progress(tx, cancel, "Downloading the server"))
            .map_err(|e| format!("The server could not be downloaded: {e}"))?;
    let java = match java::find_java(Some(&input.dev_cache)) {
        Some(java) => java,
        None => {
            let info = provision::version_info(layout).map_err(|e| e.to_string())?;
            let component = info
                .java_component
                .unwrap_or_else(|| "java-runtime-epsilon".into());
            match java::installed_runtime(layout, &component) {
                Some(java) => java,
                None => java::install_runtime(
                    layout,
                    &component,
                    &mut progress(tx, cancel, "Downloading Java"),
                )
                .map_err(|e| format!("Java could not be installed: {e}"))?,
            }
        }
    };
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    // The server's cwd is its world directory, so relative paths would resolve there.
    let java = std::path::absolute(&java).unwrap_or(java);
    let jar = std::path::absolute(&jar).unwrap_or(jar);

    let write = |name: &str, text: &str| {
        safe::write_file(&server_dir.join(name), text.as_bytes())
            .map_err(|e| format!("cannot write {name}: {e}"))
    };
    write(
        "eula.txt",
        "# The player agreed to the Minecraft EULA in mcr (https://aka.ms/MinecraftEULA).\neula=true\n",
    )?;
    let port = free_port().map_err(|e| format!("no free local port: {e}"))?;
    let existing = props::read_properties_file(&server_dir.join("server.properties"));
    let (launch_props, cheats, bonus) = match (&created, &level) {
        (Some(world), _) => {
            write(props::WORLD_META, &props::render_meta(world.cheats))?;
            (
                LaunchProps::for_new(&folder, world, port),
                world.cheats,
                world.bonus_chest,
            )
        }
        (None, Some(level)) => {
            let meta = props::read_properties_file(&server_dir.join(props::WORLD_META));
            let cheats = meta
                .as_deref()
                .and_then(props::read_cheats)
                .unwrap_or(level.cheats);
            (
                LaunchProps::for_existing(&folder, level, port),
                cheats,
                false,
            )
        }
        (None, None) => (
            LaunchProps::for_existing(&folder, &LevelInfo::default(), port),
            false,
            false,
        ),
    };
    let launch_props = launch_props.with_distances(input.view_distance, input.simulation_distance);
    write(
        "server.properties",
        &props::render_properties(existing.as_deref(), &launch_props),
    )?;
    let op = input
        .uuid
        .as_deref()
        .filter(|_| cheats)
        .map(|uuid| (uuid, input.player.as_str()));
    write("ops.json", &props::render_ops(op))?;

    let heap_mib = memory::server_heap_mib();
    let scope = memory::scope_available();
    let spec = Spec::vanilla(
        layout,
        &VanillaLaunch {
            java: &java,
            jar: &jar,
            folder: &folder,
            port,
            heap_mib,
            scope,
            bonus_chest: bonus,
        },
    )
    .ok_or("not a world folder")?;
    println!(
        "singleplayer: {folder} on 127.0.0.1:{port}, heap {heap_mib} MiB{}",
        if scope {
            " in a memory-capped scope"
        } else {
            ""
        }
    );
    let world_name = match (&created, &level) {
        (Some(w), _) => w.name.clone(),
        (None, Some(l)) if !l.name.trim().is_empty() => l.name.clone(),
        _ => folder.clone(),
    };
    Ok(Prepared {
        spec,
        world_dir,
        world_name,
        cheats,
        rules: created.map(|w| w.game_rules).unwrap_or_default(),
        folder,
        port,
    })
}

/// What the shell hears from a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// Status line for the progress screen (`lang:key|arg` or plain text).
    Status(String),
    Ready(u16),
    Failed(String),
    /// The server stopped; `requested` when mcr asked it to.
    Exited {
        requested: bool,
        reason: Option<String>,
    },
}

enum State {
    Preparing(Receiver<Prep>),
    Running {
        server: ManagedServer,
        ready: bool,
        port: u16,
    },
    Stopping(ManagedServer),
    Done,
}

/// The LAN forwarder and its announcement while the world is open to LAN.
struct Lan {
    forwarder: Forwarder,
    announcer: Option<Announcer>,
}

pub struct Session {
    pub folder: Option<String>,
    state: State,
    frozen: bool,
    cancel: Arc<AtomicBool>,
    /// Show the world list once the server has stopped.
    pub return_to_list: bool,
    world_name: String,
    cheats: bool,
    rules: Vec<(String, String)>,
    lan: Option<Lan>,
    world_dir: Option<PathBuf>,
    /// Locked from World Options; written into `level.dat` once the server stopped.
    difficulty_locked: bool,
}

const GAME_MODES: [&str; 4] = ["survival", "creative", "adventure", "spectator"];

/// A name the console can take as one player argument (no selectors, no spaces).
fn plain_player(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The console lines that apply Open to LAN's choices for `host`.
pub fn lan_commands(settings: &LanSettings, host: &str, world_cheats: bool) -> Vec<String> {
    let mode = GAME_MODES[usize::from(settings.mode.min(3))];
    let mut lines = vec![format!("defaultgamemode {mode}")];
    if plain_player(host) {
        lines.push(format!("gamemode {mode} {host}"));
        if settings.cheats != world_cheats {
            let verb = if settings.cheats { "op" } else { "deop" };
            lines.push(format!("{verb} {host}"));
        }
    }
    lines
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("folder", &self.folder)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Provisions on a background thread; the server itself is spawned from [`Session::poll`].
    pub fn start(input: LaunchInput) -> Self {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let folder = match &input.launch {
            Launch::Existing(f) => Some(f.clone()),
            Launch::New(_) => None,
        };
        std::thread::spawn(move || {
            let msg = match prepare(&input, &tx, &flag) {
                Ok(prepared) => Prep::Ready(Box::new(prepared)),
                Err(e) => Prep::Failed(e),
            };
            let _ = tx.send(msg);
        });
        Self {
            folder,
            state: State::Preparing(rx),
            frozen: false,
            cancel,
            return_to_list: false,
            world_name: String::new(),
            cheats: false,
            rules: Vec::new(),
            lan: None,
            world_dir: None,
            difficulty_locked: false,
        }
    }

    /// The port LAN players join on, while the world is open to LAN.
    pub fn lan_port(&self) -> Option<u16> {
        self.lan.as_ref().map(|l| l.forwarder.port())
    }

    /// Opens the running world to LAN and applies the game mode and cheats; returns the port.
    pub fn open_lan(&mut self, settings: &LanSettings, host: &str) -> Result<u16, String> {
        let State::Running {
            server,
            ready: true,
            port,
        } = &self.state
        else {
            return Err("the world is not running".into());
        };
        if let Some(port) = self.lan_port() {
            return Ok(port);
        }
        let bind = lan::bind_address(settings.port.unwrap_or(0));
        let target = std::net::SocketAddr::from(([127, 0, 0, 1], *port));
        let host_uuid = mcr_auth::offline::offline_uuid(host).map_err(|e| e.to_string())?;
        let gate = lan::Gate {
            host: host.to_owned(),
            host_uuid: host_uuid.into_bytes(),
        };
        let forwarder = Forwarder::start(bind, target, gate).map_err(|e| e.to_string())?;
        let lan_port = forwarder.port();
        let motd = lan::lan_motd(host, &self.world_name);
        let announcer = Announcer::start(
            &motd,
            lan_port,
            lan::announce_target(bind),
            lan::ANNOUNCE_INTERVAL,
        )
        .map_err(|e| eprintln!("singleplayer: LAN announcement: {e}"))
        .ok();
        for line in lan_commands(settings, host, self.cheats) {
            server.console(&line);
        }
        if self.frozen && server.console("tick unfreeze") {
            self.frozen = false;
        }
        println!("singleplayer: open to LAN on {bind} (port {lan_port})");
        self.lan = Some(Lan {
            forwarder,
            announcer,
        });
        Ok(lan_port)
    }

    fn close_lan(&mut self) {
        if let Some(mut lan) = self.lan.take() {
            if let Some(a) = &mut lan.announcer {
                a.stop();
            }
            lan.forwarder.stop();
        }
    }

    pub fn is_done(&self) -> bool {
        matches!(self.state, State::Done)
    }

    pub fn poll(&mut self) -> Vec<Update> {
        let mut out = Vec::new();
        for _ in 0..MAX_UPDATES_PER_POLL {
            let Some(update) = self.poll_one() else { break };
            out.push(update);
        }
        out
    }

    fn poll_one(&mut self) -> Option<Update> {
        match &mut self.state {
            State::Preparing(rx) => match rx.try_recv() {
                Ok(Prep::Status(text)) => Some(Update::Status(text)),
                Ok(Prep::Ready(prepared)) => {
                    let Prepared {
                        spec,
                        folder,
                        world_dir,
                        port,
                        world_name,
                        cheats,
                        rules,
                    } = *prepared;
                    self.folder = Some(folder);
                    self.world_dir = Some(world_dir);
                    self.world_name = world_name;
                    self.cheats = cheats;
                    self.rules = rules;
                    match ManagedServer::start(spec) {
                        Ok(server) => {
                            self.state = State::Running {
                                server,
                                ready: false,
                                port,
                            };
                            Some(Update::Status("Starting the server…".into()))
                        }
                        Err(e) => {
                            self.state = State::Done;
                            Some(Update::Failed(format!("The server could not start: {e}")))
                        }
                    }
                }
                Ok(Prep::Failed(e)) => {
                    self.state = State::Done;
                    Some(Update::Failed(e))
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.state = State::Done;
                    Some(Update::Failed("world preparation stopped".into()))
                }
            },
            State::Running {
                server,
                ready,
                port,
            } => match server.poll()? {
                Event::Preparing(Some(p)) => {
                    Some(Update::Status(format!("lang:menu.preparingSpawn|{p}")))
                }
                Event::Preparing(None) => Some(Update::Status("Loading the world…".into())),
                Event::Ready => {
                    *ready = true;
                    for (rule, value) in std::mem::take(&mut self.rules) {
                        server.console(&format!("gamerule {rule} {value}"));
                    }
                    Some(Update::Ready(*port))
                }
                Event::Exited {
                    requested, reason, ..
                } => {
                    self.state = State::Done;
                    self.store_difficulty_lock();
                    Some(Update::Exited { requested, reason })
                }
            },
            State::Stopping(server) => match server.poll()? {
                Event::Exited {
                    requested, reason, ..
                } => {
                    self.state = State::Done;
                    self.store_difficulty_lock();
                    Some(Update::Exited { requested, reason })
                }
                _ => None,
            },
            State::Done => None,
        }
    }

    fn running_server(&self) -> Option<&ManagedServer> {
        match &self.state {
            State::Running {
                server,
                ready: true,
                ..
            } => Some(server),
            _ => None,
        }
    }

    pub fn difficulty_locked(&self) -> bool {
        self.difficulty_locked
    }

    /// World Options' difficulty through the console; refused once locked.
    pub fn set_difficulty(&self, difficulty: Difficulty) -> bool {
        !self.difficulty_locked
            && self
                .running_server()
                .is_some_and(|s| s.console(&format!("difficulty {}", difficulty.name())))
    }

    /// The dedicated server has no lock command, so the lock waits for the server to stop.
    pub fn lock_difficulty(&mut self) -> bool {
        if self.running_server().is_none() {
            return false;
        }
        self.difficulty_locked = true;
        true
    }

    fn store_difficulty_lock(&mut self) {
        if let (true, Some(dir)) = (std::mem::take(&mut self.difficulty_locked), &self.world_dir)
            && let Err(e) = level::lock_difficulty(dir)
        {
            eprintln!("singleplayer: could not lock the difficulty: {e}");
        }
    }

    /// Freezes the server tick while a pause screen is open (never while open to LAN).
    pub fn set_paused(&mut self, paused: bool) {
        let paused = paused && self.lan.is_none();
        let State::Running {
            server,
            ready: true,
            ..
        } = &self.state
        else {
            return;
        };
        if paused != self.frozen
            && server.console(if paused {
                "tick freeze"
            } else {
                "tick unfreeze"
            })
        {
            self.frozen = paused;
        }
    }

    /// Saves and stops the server (killed after a grace period); a preparing session is cancelled.
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.close_lan();
        match std::mem::replace(&mut self.state, State::Done) {
            State::Running { server, .. } => {
                if self.frozen {
                    server.console("tick unfreeze");
                    self.frozen = false;
                }
                server.stop(STOP_GRACE);
                self.state = State::Stopping(server);
            }
            other @ State::Stopping(_) => self.state = other,
            State::Preparing(_) | State::Done => {}
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if self.difficulty_locked {
            // Dropping the server stops it and waits, and its last save would overwrite the lock.
            drop(std::mem::replace(&mut self.state, State::Done));
            self.store_difficulty_lock();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_settings_map_to_new_world() {
        let s = CreateWorldSettings {
            name: "W".into(),
            mode: NewGameMode::Hardcore,
            difficulty: 1,
            cheats: true,
            preset: WorldPreset::LargeBiomes,
            seed: "x".into(),
            structures: false,
            bonus_chest: true,
            ..CreateWorldSettings::default()
        };
        let w = new_world(&s);
        assert!(w.hardcore);
        assert!(!w.cheats, "hardcore has no cheats");
        assert_eq!(w.game_mode, GameMode::Survival);
        assert_eq!(w.world_type, WorldType::LargeBiomes);
        assert!(w.bonus_chest && !w.structures);
        let creative = new_world(&CreateWorldSettings {
            mode: NewGameMode::Creative,
            ..s
        });
        assert_eq!(creative.game_mode, GameMode::Creative);
        assert!(creative.cheats);
    }

    #[test]
    fn java_create_options_map_to_native_settings() {
        use mcr_modbridge::world::{NewMode, NewPreset, NewWorldRequest};
        let s = java_create_settings(&NewWorldRequest {
            name: "Flat".into(),
            mode: NewMode::Creative,
            difficulty: 1,
            cheats: true,
            preset: NewPreset::Flat,
            seed: "7".into(),
            structures: false,
            bonus_chest: true,
            game_rules: vec![("keep_inventory".into(), "true".into())],
            experiments: vec!["trade_rebalance".into()],
            packs: vec!["a.zip".into()],
            pack_dir: Some("/tmp/packs".into()),
            flat: Some("{\"layers\":[]}".into()),
        });
        assert_eq!(s.mode, NewGameMode::Creative);
        assert_eq!(s.preset, WorldPreset::Flat);
        assert_eq!(
            (s.difficulty, s.cheats, s.structures, s.bonus_chest),
            (1, true, false, true)
        );
        assert_eq!(s.data_packs, ["a.zip"]);
        assert_eq!(s.flat.as_deref(), Some("{\"layers\":[]}"));
        let w = new_world(&s);
        assert_eq!(w.world_type, WorldType::Flat);
        assert_eq!(w.game_mode, GameMode::Creative);
        assert_eq!(
            w.game_rules,
            [("keep_inventory".to_owned(), "true".to_owned())]
        );
        assert_eq!(w.experiments, ["trade_rebalance"]);
    }

    #[test]
    fn lan_commands_apply_mode_and_cheats_for_the_host_only() {
        let creative = LanSettings {
            mode: 1,
            cheats: true,
            port: None,
        };
        assert_eq!(
            lan_commands(&creative, "Steve", false),
            [
                "defaultgamemode creative",
                "gamemode creative Steve",
                "op Steve"
            ]
        );
        assert_eq!(
            lan_commands(&creative, "Steve", true),
            ["defaultgamemode creative", "gamemode creative Steve"]
        );
        let survival = LanSettings {
            mode: 0,
            cheats: false,
            port: Some(25601),
        };
        assert_eq!(
            lan_commands(&survival, "Steve", true),
            [
                "defaultgamemode survival",
                "gamemode survival Steve",
                "deop Steve"
            ]
        );
        for hostile in ["@a", "x y", "", "seventeen_chars__"] {
            assert_eq!(
                lan_commands(&creative, hostile, false),
                ["defaultgamemode creative"],
                "{hostile:?}"
            );
        }
    }

    #[test]
    fn creation_extras_are_validated() {
        let s = CreateWorldSettings {
            game_rules: vec![
                ("keep_inventory".into(), "true".into()),
                ("random_tick_speed".into(), "3".into()),
                ("bogus".into(), "1".into()),
            ],
            experiments: vec!["trade_rebalance".into(), "evil".into()],
            ..CreateWorldSettings::default()
        };
        let w = new_world(&s);
        assert_eq!(
            w.game_rules,
            [("keep_inventory".to_owned(), "true".to_owned())]
        );
        assert_eq!(w.experiments, ["trade_rebalance"]);
        let flat = CreateWorldSettings {
            preset: WorldPreset::Flat,
            flat: Some("minecraft:bedrock,minecraft:not_a_block".into()),
            ..CreateWorldSettings::default()
        };
        assert!(check_new(&flat).is_err(), "unknown blocks are refused");
        let flat = CreateWorldSettings {
            flat: Some("minecraft:bedrock,3*minecraft:stone;minecraft:desert".into()),
            ..flat
        };
        let json = check_new(&flat).unwrap().unwrap();
        assert!(json.contains("minecraft:desert"));
        let normal = CreateWorldSettings {
            preset: WorldPreset::Normal,
            ..flat
        };
        assert_eq!(
            check_new(&normal).unwrap(),
            None,
            "layers only for superflat"
        );
        let packs = CreateWorldSettings {
            data_packs: vec!["missing.zip".into()],
            pack_dir: Some(std::env::temp_dir()),
            ..CreateWorldSettings::default()
        };
        assert!(check_new(&packs).is_err());
    }

    #[test]
    fn recreate_maps_to_create_settings() {
        let r = recreate::Recreate {
            level: LevelInfo {
                name: "Old".into(),
                game_mode: GameMode::Creative,
                difficulty: Difficulty::Easy,
                ..LevelInfo::default()
            },
            seed: "42".into(),
            structures: false,
            bonus_chest: true,
            world_type: WorldType::Flat,
            generator_settings: Some(
                r#"{"biome":"minecraft:plains","layers":[{"block":"minecraft:bedrock","height":1}]}"#
                    .into(),
            ),
            game_rules: vec![("keep_inventory".into(), "true".into())],
            experiments: vec!["trade_rebalance".into()],
            data_packs: vec!["a.zip".into()],
        };
        let s = recreate_settings(&r, true, std::path::Path::new("/saves/Old"));
        assert_eq!(s.name, "Old");
        assert_eq!(
            (s.mode, s.difficulty, s.cheats),
            (NewGameMode::Creative, 1, true)
        );
        assert_eq!(
            (s.seed.as_str(), s.structures, s.bonus_chest),
            ("42", false, true)
        );
        assert_eq!(s.preset, WorldPreset::Flat);
        assert_eq!(
            s.flat.as_deref(),
            Some("minecraft:bedrock;minecraft:plains")
        );
        assert_eq!(
            s.pack_dir.as_deref(),
            Some(std::path::Path::new("/saves/Old/datapacks"))
        );
        assert_eq!(s.game_rules.len(), 1);
    }

    #[test]
    fn freeze_follows_java_while_its_gui_is_up() {
        assert!(should_freeze(true, true, true, false));
        assert!(!should_freeze(true, true, false, true), "Java's rule wins");
        assert!(should_freeze(true, false, false, true));
        assert!(
            !should_freeze(false, true, true, true),
            "no world, no freeze"
        );
    }

    #[test]
    fn world_store_lists_renames_and_guards_the_open_world() {
        let dir = std::env::temp_dir().join(format!("mcr-app-worlds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let world = dir.join("saves/One");
        std::fs::create_dir_all(&world).unwrap();
        let mut data = mcr_nbt::Compound::new();
        data.insert("LevelName", "One");
        data.insert("LastPlayed", mcr_nbt::Value::Long(1));
        let mut root = mcr_nbt::Compound::new();
        root.insert("Data", mcr_nbt::Value::Compound(data));
        let bytes = mcr_nbt::write_disk("", &root, mcr_nbt::Compression::Gzip).unwrap();
        let bytes_for_link = bytes.clone();
        std::fs::write(world.join("level.dat"), bytes).unwrap();
        let mut store = FileWorlds::new(Layout::new(&dir));
        store.refresh();
        assert_eq!(store.worlds().len(), 1);
        assert_eq!(store.worlds()[0].mode_key, "gameMode.survival");
        store.set_busy(Some("One".into()));
        assert!(store.delete("One").is_err());
        assert!(store.rename("One", "Two").is_err());
        store.set_busy(None);
        store.rename("One", "Two").unwrap();
        store.refresh();
        assert_eq!(store.worlds()[0].name, "Two");
        store.delete("One").unwrap();
        store.refresh();
        assert!(store.worlds().is_empty());
        #[cfg(unix)]
        {
            let elsewhere = dir.join("elsewhere");
            std::fs::create_dir_all(&elsewhere).unwrap();
            std::fs::write(elsewhere.join("level.dat"), &bytes_for_link).unwrap();
            std::os::unix::fs::symlink(&elsewhere, dir.join("saves/Linked")).unwrap();
            store.refresh();
            assert!(store.worlds().is_empty(), "a symlinked world is not listed");
            assert!(store.rename("Linked", "X").is_err());
            assert!(store.delete("Linked").is_err());
            assert!(elsewhere.join("level.dat").exists());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
