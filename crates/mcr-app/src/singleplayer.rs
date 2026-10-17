//! Local worlds: the world list's store and one managed vanilla server at a time.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use mcr_singleplayer::level::{self, Difficulty, GameMode, LevelInfo};
use mcr_singleplayer::props::{self, LaunchProps, NewWorld, WorldType};
use mcr_singleplayer::server::{Event, ManagedServer, STOP_GRACE, Spec, VanillaLaunch};
use mcr_singleplayer::{Layout, java, memory, provision, stale};
use mcr_ui::{CreateWorldSettings, Favicon, NewGameMode, WorldEntry, WorldPreset, WorldStore};

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
        let world = self.layout.world(folder).ok_or("not a world folder")?;
        level::rename(&world, name).map_err(|e| e.to_string())
    }

    fn delete(&mut self, folder: &str) -> Result<(), String> {
        if self.busy.as_deref() == Some(folder) {
            return Err("the world is open".into());
        }
        level::delete_world(&self.layout.saves(), folder).map_err(|e| e.to_string())
    }
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
    }
}

enum Prep {
    Status(String),
    Ready {
        spec: Box<Spec>,
        folder: String,
        port: u16,
    },
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

fn prepare(
    input: &LaunchInput,
    tx: &Sender<Prep>,
    cancel: &AtomicBool,
) -> Result<(Spec, String, u16), String> {
    let layout = &input.layout;
    let saves = layout.saves();
    let (folder, level, created) = match &input.launch {
        Launch::Existing(folder) => {
            let world = layout.world(folder).ok_or("not a world folder")?;
            let level =
                level::read_level(&world).map_err(|e| format!("cannot read the world: {e}"))?;
            (folder.clone(), Some(level), None)
        }
        Launch::New(s) => {
            let world = new_world(s);
            let folder =
                mcr_singleplayer::layout::folder_for(&world.name, |c| saves.join(c).exists());
            (folder, None, Some(world))
        }
    };
    let world_dir = layout.world(&folder).ok_or("not a world folder")?;
    let server_dir = layout.server_dir(&folder).ok_or("not a world folder")?;
    std::fs::create_dir_all(&server_dir).map_err(|e| format!("cannot create the world: {e}"))?;
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
        std::fs::write(server_dir.join(name), text).map_err(|e| format!("cannot write {name}: {e}"))
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
    Ok((spec, folder, port))
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

pub struct Session {
    pub folder: Option<String>,
    state: State,
    frozen: bool,
    cancel: Arc<AtomicBool>,
    /// Show the world list once the server has stopped.
    pub return_to_list: bool,
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
                Ok((spec, folder, port)) => Prep::Ready {
                    spec: Box::new(spec),
                    folder,
                    port,
                },
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
                Ok(Prep::Ready { spec, folder, port }) => {
                    self.folder = Some(folder);
                    match ManagedServer::start(*spec) {
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
                    Some(Update::Ready(*port))
                }
                Event::Exited {
                    requested, reason, ..
                } => {
                    self.state = State::Done;
                    Some(Update::Exited { requested, reason })
                }
            },
            State::Stopping(server) => match server.poll()? {
                Event::Exited {
                    requested, reason, ..
                } => {
                    self.state = State::Done;
                    Some(Update::Exited { requested, reason })
                }
                _ => None,
            },
            State::Done => None,
        }
    }

    /// Freezes the server tick while a pause screen is open.
    pub fn set_paused(&mut self, paused: bool) {
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
        let _ = std::fs::remove_dir_all(&dir);
    }
}
