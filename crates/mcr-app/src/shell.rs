//! Owns the app flow: menus while no session exists, a [`Play`] while one runs.

mod route;

pub use route::JavaWindow;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use anyhow::{Context, Result};
use mcr_client::{
    Client, ClientConfig, ClientError, ConductEnd, ConductReply, DialogEnd, DialogReply,
    DialogRequest, TransferTarget, WireChoice, conduct_channel, dialog_channel, sanitize_for_log,
};
use mcr_modbridge::ModFrames;
use mcr_nbt::Value;
use mcr_protocol::types::text::Component;
use mcr_render::{Camera, FrameInputs, FrameStats, Renderer};
use mcr_settings::InputKey;
use mcr_ui::{
    Clipboard, ConductScreen, ConfirmScreen, ConnectingScreen, Dialog, DialogScreen,
    DisconnectedScreen, Env, Modifiers, NoView, RenderCx, Screen, ScreenAction, ScreenStack,
    SelectWorldScreen, SharedServers, SharedWorlds, StatusHandle, TitleScreen, UiInput,
    vanilla_resources,
};

use crate::assets::Assets;
use crate::audio::Audio;
use crate::autopilot::Autopilot;
use crate::gui::{self, Gui, UiOverlay};
use crate::input::{track_modifiers, ui_inputs};
use crate::mod_sync;
use crate::play::{Parts, Play, PlayConfig, TICK_SECONDS};
use crate::respacks::{PackAction, ResourceStack, ServerPacks, downloads_dir};
use crate::servers::FileServers;
use crate::settings::{OptionsScreen, Settings, SharedSettings};
use crate::singleplayer::{EULA_URL, FileWorlds, Launch, LaunchInput, Session, Update};
use mcr_modbridge::world::{WorldEvent, WorldRequest};
use mcr_singleplayer::level::Difficulty;

/// Only a direct join of the running local server's own port counts as singleplayer for chat trust.
fn is_local_server(host: &str, port: u16, local_port: Option<u16>, transfer: bool) -> bool {
    !transfer && host == "127.0.0.1" && local_port == Some(port)
}

/// Why the local server stopped, as the player reads it.
fn stop_text(reason: Option<String>) -> String {
    match reason.filter(|r| !r.trim().is_empty()) {
        Some(r) => {
            let lines: Vec<String> = r.lines().map(sanitize_for_log).collect();
            format!("The local server stopped:\n{}", lines.join("\n"))
        }
        None => "The local server stopped".to_owned(),
    }
}

/// A join either lands, fails with a message, or is redirected by a `transfer` packet.
enum ConnectFailure {
    Message(String),
    Transfer(TransferTarget),
}

type Connect = Receiver<Result<Client, ConnectFailure>>;

const MAX_DIALOG_REQUESTS_PER_FRAME: usize = 16;
const BRIDGE_TOAST: u32 = 0x4d43_5242;

#[derive(Debug, Clone)]
pub struct ShellConfig {
    pub game_dir: PathBuf,
    pub jar: PathBuf,
    pub assets_dir: PathBuf,
    pub name: String,
    /// A Microsoft account's session for online-mode servers.
    pub online: Option<mcr_client::OnlineAuth>,
    pub render_distance: Option<i32>,
    pub connect: Option<String>,
    /// A `saves` folder to play straight away.
    pub singleplayer: Option<String>,
    /// Set in mod mode: every connection is mirrored to Java.
    pub mod_net: Option<crate::mods::ModNet>,
    pub mods: crate::mods_ui::ModsApp,
}

pub struct Shell {
    cfg: ShellConfig,
    applied_vsync: Option<bool>,
    parts: Option<Parts>,
    pub play: Option<Play>,
    stack: ScreenStack,
    gui_size: (i32, i32),
    servers: Rc<RefCell<FileServers>>,
    settings: SharedSettings,
    connecting: Option<Connect>,
    mouse: (f32, f32),
    mods: Modifiers,
    accum: f32,
    started: Instant,
    last_frame: Instant,
    pub quit: bool,
    /// The title screen asked for the launcher; the app leaves this shell.
    pub to_launcher: bool,
    pub shot: Option<PathBuf>,
    pub failure: Option<String>,
    autopilot: Option<Autopilot>,
    camera: Camera,
    packs: ServerPacks,
    resource_stack: ResourceStack,
    prompt_open: bool,
    current_address: String,
    dialogs: Option<DialogEnd>,
    conduct: Option<ConductEnd>,
    route: route::Route,
    java_settings_seen: u64,
    worlds: Rc<RefCell<FileWorlds>>,
    sp: Option<Session>,
    sp_status: Option<StatusHandle>,
    /// The session was started from Java's world screens: Java shows its progress, not native screens.
    sp_java: bool,
    /// A Java launch waiting for the EULA answer.
    pending_launch: Option<Launch>,
    /// Why a Java-flow join failed; Java hears it once the server has stopped.
    sp_java_failure: Option<String>,
    /// The local world was pausing last frame (a capture for its icon is taken when it starts).
    sp_was_paused: bool,
    builtin: mcr_client::BuiltinData,
    text_renderer: mcr_client::TextRenderer,
}

impl std::fmt::Debug for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shell").finish_non_exhaustive()
    }
}

fn asset_index_id(jar: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(jar.parent()?.join("version.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value["assetIndex"]["id"].as_str().map(str::to_owned)
}

impl Shell {
    /// Hands the menus the desktop clipboard; offscreen runs keep a private one.
    pub fn use_system_clipboard(&mut self) {
        if let Some(p) = &mut self.parts {
            p.gui
                .clipboard
                .use_system(crate::mod_input::SystemClipboard::open());
        }
    }

    pub fn new(renderer: Renderer, cfg: ShellConfig) -> Result<Self> {
        let data = mcr_data::v26_3();
        let settings = Settings::load(&cfg.game_dir);
        let options = settings.borrow().file.options.clone();
        let mod_jars = cfg
            .mod_net
            .as_ref()
            .map(|n| n.jars.clone())
            .unwrap_or_default();
        let assets = Arc::new(Assets::load_with_mods(&cfg.jar, &mod_jars)?);
        let index = asset_index_id(&cfg.jar);
        let resources = Arc::new(
            vanilla_resources(
                &cfg.jar,
                index.as_deref().map(|i| (cfg.assets_dir.as_path(), i)),
            )
            .context("opening resources")?,
        );
        let mut gui = Gui::load(resources, renderer.device(), renderer.target_format())?;
        gui.sync_language(&options.language);
        let mut audio = Audio::start(
            &cfg.assets_dir,
            index.as_deref().unwrap_or("34"),
            &options,
            data,
        );
        if audio.is_none() {
            println!("audio: disabled");
        }
        let java_sound = cfg.mod_net.as_ref().and_then(|n| n.sound.clone());
        if let (Some(audio), Some(link)) = (&mut audio, java_sound)
            && !audio.attach_java(link)
        {
            println!("audio: Java's sounds could not attach");
        }
        let servers = Rc::new(RefCell::new(FileServers::load(&cfg.game_dir)));
        let worlds = Rc::new(RefCell::new(FileWorlds::new(
            mcr_singleplayer::Layout::new(&cfg.game_dir),
        )));
        let packs = ServerPacks::new(
            downloads_dir(&cfg.game_dir),
            &cfg.name,
            mcr_auth::offline::offline_uuid(&cfg.name).unwrap_or_default(),
        );
        let resource_stack = ResourceStack {
            jar: cfg.jar.clone(),
            assets_dir: cfg.assets_dir.clone(),
            index: index.clone(),
            mods: mod_jars,
        };
        let now = Instant::now();
        let builtin = assets.builtin.clone();
        let text_renderer = gui.text_renderer();
        let mut shell = Self {
            builtin,
            text_renderer,
            applied_vsync: None,
            parts: Some(Parts {
                renderer,
                gui,
                assets,
                audio,
                settings: settings.clone(),
            }),
            play: None,
            stack: ScreenStack::new(0, 0),
            gui_size: (0, 0),
            servers,
            settings,
            connecting: None,
            mouse: (0.0, 0.0),
            mods: Modifiers::NONE,
            accum: 0.0,
            started: now,
            last_frame: now,
            quit: false,
            to_launcher: false,
            shot: None,
            failure: None,
            autopilot: None,
            camera: Camera::default(),
            packs,
            resource_stack,
            prompt_open: false,
            current_address: String::new(),
            dialogs: None,
            conduct: None,
            route: route::Route::new(cfg.mod_net.as_ref().and_then(|n| n.input.clone())),
            java_settings_seen: 0,
            worlds,
            sp: None,
            sp_status: None,
            sp_java: false,
            pending_launch: None,
            sp_java_failure: None,
            sp_was_paused: false,
            cfg,
        };
        shell.show_title();
        if let Some(prompt) = shell.cfg.mods.take_crash_prompt() {
            shell.push(prompt);
        }
        if shell.mod_frames().is_none()
            && let Some(addr) = shell.cfg.connect.take()
        {
            shell.connect(&addr, false);
        }
        if shell.mod_frames().is_none()
            && let Some(folder) = shell.cfg.singleplayer.take()
        {
            shell.play_world(Launch::Existing(folder), false);
        }
        Ok(shell)
    }

    fn close_dialog_screens(&mut self) {
        while self.stack.top_is_dialog() {
            self.stack.pop();
        }
    }

    fn close_connecting_screen(&mut self) {
        self.close_dialog_screens();
        while self.prompt_open {
            self.prompt_open = false;
            self.stack.pop();
        }
        self.stack.pop();
    }

    fn end_pack_session(&mut self) {
        if self.prompt_open {
            self.prompt_open = false;
            match &mut self.play {
                Some(play) => play.close_top_screen(),
                None => {
                    self.stack.pop();
                }
            }
        }
        let actions = self.packs.end_session();
        self.pack_actions(actions);
    }

    fn poll_conduct(&mut self) {
        let answers = self
            .play
            .as_mut()
            .map(Play::take_conduct_answers)
            .unwrap_or_default();
        for (accept, remember, hash) in answers {
            self.conduct_answer(accept, remember, hash);
        }
        let Some(end) = &self.conduct else { return };
        let Ok(request) = end.requests.try_recv() else {
            return;
        };
        let screen = Box::new(ConductScreen::new(&request.text, request.hash));
        match &mut self.play {
            Some(play) => play.show_screen(screen),
            None => self.push(screen),
        }
    }

    fn conduct_answer(&mut self, accept: bool, remember: bool, hash: i32) {
        if let Some(end) = &self.conduct {
            end.reply(if accept {
                ConductReply::Accept
            } else {
                ConductReply::Decline
            });
        }
        if remember {
            self.servers
                .borrow_mut()
                .set_accepted_conduct_for(&self.current_address, hash);
        }
    }

    fn poll_dialogs(&mut self) {
        let Some(end) = &self.dialogs else { return };
        let mut requests = Vec::new();
        while let Ok(request) = end.requests.try_recv() {
            requests.push(request);
            if requests.len() >= MAX_DIALOG_REQUESTS_PER_FRAME {
                break;
            }
        }
        if let Some(play) = &mut self.play {
            for reply in play.take_dialog_replies() {
                end.reply(reply);
            }
        }
        for request in requests {
            match request {
                DialogRequest::Show(_) | DialogRequest::Clear if self.route.gui => {}
                DialogRequest::Show(compound) => {
                    let Some(dialog) = Dialog::from_nbt(&Value::Compound(compound)) else {
                        eprintln!("ignoring a malformed server dialog");
                        continue;
                    };
                    match &mut self.play {
                        Some(play) => play.show_dialog(dialog),
                        None => {
                            if self.stack.top_is_dialog() {
                                self.stack.pop();
                            }
                            self.push(Box::new(DialogScreen::new(dialog, Vec::new())));
                        }
                    }
                }
                DialogRequest::Clear => match &mut self.play {
                    Some(play) => play.clear_dialog(),
                    None => {
                        if self.stack.top_is_dialog() {
                            self.stack.pop();
                        }
                    }
                },
            }
        }
    }

    fn poll_packs(&mut self) {
        if !self.packs.is_active() {
            return;
        }
        let answers = self.play.as_mut().map(Play::take_pack_answers);
        for accepted in answers.into_iter().flatten() {
            self.pack_answer(accepted);
        }
        let actions = self.packs.poll();
        self.pack_actions(actions);
    }

    fn pack_answer(&mut self, accepted: bool) {
        self.prompt_open = false;
        let actions = self.packs.answer(accepted);
        self.pack_actions(actions);
    }

    fn pack_actions(&mut self, mut actions: Vec<PackAction>) {
        while !actions.is_empty() {
            let mut next = Vec::new();
            for action in actions {
                match action {
                    PackAction::Prompt { required, message } => {
                        self.show_pack_prompt(required, message.as_ref());
                    }
                    PackAction::Reload(paths) => {
                        let ok = self.reload_resources(&paths);
                        if ok && let Some(frames) = self.mod_frames() {
                            frames.lock().set_server_packs(&paths);
                        }
                        next.extend(self.packs.reload_finished(ok));
                    }
                    PackAction::SaveSetting(setting) => {
                        self.servers
                            .borrow_mut()
                            .set_pack_setting_for(&self.current_address, setting);
                    }
                    PackAction::Disconnect(reason) => self.pack_disconnect(&reason),
                }
            }
            actions = next;
        }
    }

    fn show_pack_prompt(&mut self, required: bool, message: Option<&Component>) {
        let (title, line, yes, no) = if required {
            (
                "lang:multiplayer.requiredTexturePrompt.line1",
                "lang:multiplayer.requiredTexturePrompt.line2",
                "lang:gui.proceed",
                "lang:menu.disconnect",
            )
        } else {
            (
                "lang:multiplayer.texturePrompt.line1",
                "lang:multiplayer.texturePrompt.line2",
                "lang:gui.yes",
                "lang:gui.no",
            )
        };
        let mut text = line.to_owned();
        if let Some(prompt) = message {
            let plain: String = prompt
                .to_plain_string()
                .chars()
                .filter(|c| !c.is_control() || *c == '\n')
                .take(512)
                .collect();
            text.push_str("\n\n");
            text.push_str(&plain.replace('§', ""));
        }
        let screen = ConfirmScreen::new(
            title,
            &text,
            vec![
                (
                    yes.into(),
                    Box::new(|| vec![ScreenAction::ResourcePackAnswer(true), ScreenAction::Pop]),
                ),
                (
                    no.into(),
                    Box::new(|| vec![ScreenAction::ResourcePackAnswer(false), ScreenAction::Pop]),
                ),
            ],
        );
        if self.prompt_open {
            match &mut self.play {
                Some(play) => play.close_top_screen(),
                None => {
                    self.stack.pop();
                }
            }
        }
        self.prompt_open = true;
        match &mut self.play {
            Some(play) => play.show_screen(Box::new(screen)),
            None => self.push(Box::new(screen)),
        }
    }

    fn pack_disconnect(&mut self, reason: &Component) {
        let text = sanitize_for_log(&reason.to_plain_string());
        match &mut self.play {
            Some(play) => play.force_close(text),
            None => {
                self.connecting = None;
                self.failure = Some(text);
                self.close_connecting_screen();
                self.push(Box::new(
                    DisconnectedScreen::new(reason.clone())
                        .with_title(Component::translatable("connect.failed", Vec::new())),
                ));
            }
        }
    }

    fn reload_resources(&mut self, paths: &[PathBuf]) -> bool {
        let started = Instant::now();
        let built = self
            .resource_stack
            .build(paths)
            .and_then(|r| Assets::from_resources(&self.cfg.jar, r));
        let assets = match built {
            Ok(assets) => Arc::new(assets),
            Err(e) => {
                eprintln!("resource reload failed: {e:#}");
                return false;
            }
        };
        let applied = match (&mut self.play, &mut self.parts) {
            (Some(play), _) => play.reload_resources(assets),
            (None, Some(parts)) => parts.gui.reload(&assets.resources).map(|()| {
                if let Some(audio) = &mut parts.audio {
                    audio.reload(&assets.resources);
                }
                parts.assets = assets;
            }),
            (None, None) => return false,
        };
        match applied {
            Ok(()) => {
                println!(
                    "resource packs: reloaded {} pack(s) in {:?}",
                    paths.len(),
                    started.elapsed()
                );
                true
            }
            Err(e) => {
                eprintln!("resource reload failed: {e:#}");
                false
            }
        }
    }

    pub fn set_autopilot(&mut self, autopilot: Autopilot) {
        self.autopilot = Some(autopilot);
    }

    pub fn take_autopilot(&mut self) -> Option<Autopilot> {
        self.autopilot.take()
    }

    pub fn script_finished(&self) -> bool {
        self.autopilot.as_ref().is_none_or(|a| a.finished)
    }

    fn languages(&self) -> Vec<mcr_ui::LanguageInfo> {
        self.parts
            .as_ref()
            .map(|p| p.gui.ui.lang().languages().to_vec())
            .unwrap_or_default()
    }

    fn show_title(&mut self) {
        self.stack.clear();
        let servers: SharedServers = self.servers.clone();
        let worlds: SharedWorlds = self.worlds.clone();
        self.push(Box::new(
            TitleScreen::new("26.3", servers).with_worlds(worlds),
        ));
    }

    fn show_world_list(&mut self) {
        self.show_title();
        let worlds: SharedWorlds = self.worlds.clone();
        self.push(Box::new(SelectWorldScreen::new(worlds)));
    }

    fn eula_prompt(&mut self, launch: Launch) {
        let layout = self.worlds.borrow().layout().clone();
        let screen = ConfirmScreen::new(
            "Minecraft EULA",
            &format!(
                "Singleplayer runs Mojang's Minecraft server on this computer.\nBy continuing you agree to the Minecraft EULA:\n§9{EULA_URL}"
            ),
            vec![
                (
                    "I agree".into(),
                    Box::new(move || match layout.accept_eula() {
                        Ok(()) => vec![ScreenAction::Pop, launch.action()],
                        Err(e) => {
                            eprintln!("could not save the EULA agreement: {e}");
                            vec![ScreenAction::Pop]
                        }
                    }),
                ),
                (
                    "Read the EULA".into(),
                    Box::new(|| vec![ScreenAction::OpenUrl(EULA_URL.into())]),
                ),
                (
                    "lang:gui.cancel".into(),
                    Box::new(|| vec![ScreenAction::Pop]),
                ),
            ],
        );
        self.push(Box::new(screen));
    }

    /// Starts a world's local server behind a progress screen (Java's for `from_java`); one at a time.
    fn play_world(&mut self, launch: Launch, from_java: bool) {
        if self.sp.is_some() || self.play.is_some() {
            eprintln!("singleplayer: a world is already open");
            if from_java {
                self.java_world(WorldEvent::Failed("A world is already open".into()));
            }
            return;
        }
        let layout = self.worlds.borrow().layout().clone();
        if !layout.eula_accepted() {
            if from_java {
                self.pending_launch = Some(launch);
                self.java_world(WorldEvent::Eula);
            } else {
                self.eula_prompt(launch);
            }
            return;
        }
        let options = self.settings.borrow().file.options.clone();
        let input = LaunchInput {
            layout,
            launch,
            player: self.cfg.name.clone(),
            uuid: mcr_auth::offline::offline_uuid(&self.cfg.name)
                .ok()
                .map(|u| u.hyphenated().to_string()),
            view_distance: self.cfg.render_distance.unwrap_or(options.render_distance),
            simulation_distance: options.simulation_distance,
            dev_cache: crate::mods::cache_dir().to_path_buf(),
            server_mods: self
                .cfg
                .mod_net
                .as_ref()
                .map(|n| n.server_jars.clone())
                .unwrap_or_default(),
            server_proxy: self.cfg.mod_net.as_ref().map(|n| n.proxy_port),
        };
        if !from_java {
            let (screen, status) = ConnectingScreen::with_text("lang:createWorld.preparing");
            self.push(Box::new(screen));
            self.sp_status = Some(status);
        }
        self.sp_java = from_java;
        self.sp_java_failure = None;
        self.failure = None;
        self.sp = Some(Session::start(input));
    }

    /// Progress for Java's world screens; dropped without mods.
    fn java_world(&self, event: WorldEvent) {
        if let Some(frames) = self.mod_frames() {
            frames.lock().world(event);
        }
    }

    /// Play, Create, the EULA answer and Cancel from Java's world screens.
    fn poll_mod_worlds(&mut self) {
        let Some(net) = &self.cfg.mod_net else { return };
        for request in net.take_world_requests() {
            match request {
                WorldRequest::Play(folder) => self.play_world(Launch::Existing(folder), true),
                WorldRequest::Create(w) => self.play_world(
                    Launch::New(crate::singleplayer::java_create_settings(&w)),
                    true,
                ),
                WorldRequest::Eula(agreed) => {
                    let Some(launch) = self.pending_launch.take() else {
                        continue;
                    };
                    if !agreed {
                        continue;
                    }
                    let layout = self.worlds.borrow().layout().clone();
                    match layout.accept_eula() {
                        Ok(()) => self.play_world(launch, true),
                        Err(e) => self.java_world(WorldEvent::Failed(format!(
                            "could not save the EULA agreement: {e}"
                        ))),
                    }
                }
                WorldRequest::Lan(lan) => self.start_lan(
                    &mcr_ui::LanSettings {
                        mode: lan.mode,
                        cheats: lan.cheats,
                        port: lan.port,
                    },
                    true,
                ),
                WorldRequest::Difficulty(id) => self.local_difficulty(id),
                WorldRequest::LockDifficulty => self.lock_local_difficulty(),
                WorldRequest::Cancel => {
                    self.pending_launch = None;
                    match &mut self.sp {
                        Some(sp) if self.sp_java => {
                            sp.stop();
                            if self.play.is_none() {
                                self.connecting = None;
                                self.end_pack_session();
                            }
                        }
                        _ => self.java_world(WorldEvent::Stopped(None)),
                    }
                }
            }
        }
    }

    /// A Java-flow session update goes to Java; the shell only keeps its own state.
    fn java_sp_update(&mut self, update: Update) {
        match update {
            Update::Status(text) => self.java_world(WorldEvent::Status(text)),
            Update::Ready(port) => {
                let folder = self.sp.as_ref().and_then(|s| s.folder.clone());
                self.worlds.borrow_mut().set_busy(folder);
                self.java_world(WorldEvent::Ready);
                self.connect(&format!("127.0.0.1:{port}"), false);
            }
            Update::Failed(message) => {
                eprintln!("singleplayer: {message}");
                self.sp = None;
                self.sp_java = false;
                self.connecting = None;
                self.worlds.borrow_mut().set_busy(None);
                self.failure = Some(message.clone());
                self.java_world(WorldEvent::Failed(message));
            }
            Update::Exited { requested, reason } => {
                self.sp = None;
                self.sp_java = false;
                self.worlds.borrow_mut().set_busy(None);
                let reason = (!requested).then(|| stop_text(reason));
                if let (Some(play), Some(text)) = (&mut self.play, &reason) {
                    play.force_close(text.clone());
                }
                let failure = self.sp_java_failure.take();
                self.java_world(WorldEvent::Stopped(reason.or(failure)));
            }
        }
    }

    /// The running local world has no `icon.png` yet.
    fn local_icon_missing(&self) -> bool {
        let Some(folder) = self.sp.as_ref().and_then(|s| s.folder.as_deref()) else {
            return false;
        };
        let saves = self.worlds.borrow().layout().saves();
        mcr_singleplayer::layout::valid_folder(folder)
            && saves
                .join(folder)
                .join("icon.png")
                .symlink_metadata()
                .is_err()
    }

    /// Writes the world icon captured during the session, if the world still has none.
    fn save_world_icon(&self, icon: Option<Vec<u8>>) {
        let (Some(icon), Some(folder)) = (icon, self.sp.as_ref().and_then(|s| s.folder.clone()))
        else {
            return;
        };
        let saves = self.worlds.borrow().layout().saves();
        match mcr_singleplayer::icon::write_if_missing(&saves, &folder, &icon) {
            Ok(true) => println!("singleplayer: wrote the icon of {folder}"),
            Ok(false) => {}
            Err(e) => eprintln!("singleplayer: world icon: {e}"),
        }
    }

    /// Opens the local world to LAN; the outcome goes to chat, or to Java in mod mode.
    /// The local world's session and connection, unless the world is hardcore or locked.
    fn difficulty_target(&mut self) -> Option<(&mut Session, &mut Play)> {
        let (Some(sp), Some(play)) = (&mut self.sp, &mut self.play) else {
            return None;
        };
        let world = &play.world.client.state.world;
        (!world.hardcore && !world.difficulty_locked && !sp.difficulty_locked())
            .then_some((sp, play))
    }

    /// Java's World Options in a local world: gamemasters use vanilla's packet, which the console's
    /// admin broadcast would otherwise echo into their chat.
    fn local_difficulty(&mut self, id: u8) {
        let Some((sp, play)) = self.difficulty_target() else {
            eprintln!("singleplayer: difficulty change refused");
            return;
        };
        if play.world.client.state.player.permission_level >= 2 {
            if let Err(e) = play.world.client.change_difficulty(id) {
                eprintln!("singleplayer: difficulty change failed: {e}");
            }
        } else if !sp.set_difficulty(Difficulty::from_id(i64::from(id))) {
            eprintln!("singleplayer: difficulty change refused");
        }
    }

    fn lock_local_difficulty(&mut self) {
        let locked = self
            .difficulty_target()
            .is_some_and(|(sp, _)| sp.lock_difficulty());
        if locked {
            self.java_world(WorldEvent::DifficultyLocked);
        } else {
            eprintln!("singleplayer: difficulty lock refused");
        }
    }

    fn start_lan(&mut self, settings: &mcr_ui::LanSettings, from_java: bool) {
        let result = match &mut self.sp {
            Some(sp) => sp.open_lan(settings, &self.cfg.name),
            None => Err("no local world is running".into()),
        };
        if let Err(e) = &result {
            eprintln!("singleplayer: open to LAN failed: {e}");
        }
        if let (Ok(_), Some(play)) = (&result, &mut self.play) {
            play.lan_open = true;
        }
        if from_java {
            self.java_world(WorldEvent::Lan(result));
            return;
        }
        let Some(play) = &mut self.play else { return };
        let message = match result {
            Ok(port) => Component::translatable(
                "menu.multiplayerOptions.publish.started.lan",
                vec![mcr_protocol::types::text::Arg::Str(port.to_string())],
            ),
            Err(_) => Component::translatable("commands.publish.failed", Vec::new()),
        };
        play.local_chat(&message);
    }

    fn sp_failed(&mut self, message: String) {
        eprintln!("singleplayer: {message}");
        self.connecting = None;
        if self.sp_status.take().is_some() {
            self.close_connecting_screen();
        }
        self.failure = Some(message.clone());
        self.push(Box::new(
            DisconnectedScreen::new(Component::text(message))
                .with_title(Component::text("The world could not be opened")),
        ));
    }

    fn poll_singleplayer(&mut self) {
        let paused = crate::singleplayer::should_freeze(
            self.play.is_some(),
            self.route.gui,
            self.route.java.pausing,
            self.play.as_ref().is_some_and(Play::pause_open),
        );
        let menu = self.play.as_mut().is_some_and(Play::take_pause_opened);
        if (menu || (paused && !self.sp_was_paused))
            && self.local_icon_missing()
            && let Some(play) = &mut self.play
        {
            play.request_icon_capture();
        }
        self.sp_was_paused = paused;
        let updates = match &mut self.sp {
            Some(sp) => {
                sp.set_paused(paused);
                sp.poll()
            }
            None => return,
        };
        for update in updates {
            if self.sp_java {
                self.java_sp_update(update);
                continue;
            }
            match update {
                Update::Status(text) => {
                    if let Some(status) = &self.sp_status {
                        *status.borrow_mut() = text;
                    }
                }
                Update::Ready(port) => {
                    if let Some(status) = &self.sp_status {
                        *status.borrow_mut() = "lang:connect.joining".into();
                    }
                    let folder = self.sp.as_ref().and_then(|s| s.folder.clone());
                    self.worlds.borrow_mut().set_busy(folder);
                    self.connect(&format!("127.0.0.1:{port}"), false);
                }
                Update::Failed(message) => {
                    self.sp = None;
                    self.worlds.borrow_mut().set_busy(None);
                    self.sp_failed(message);
                }
                Update::Exited { requested, reason } => {
                    let to_list = self.sp.as_ref().is_some_and(|s| s.return_to_list);
                    self.sp = None;
                    self.worlds.borrow_mut().set_busy(None);
                    let text = stop_text(reason);
                    if let Some(play) = &mut self.play {
                        if !requested {
                            play.force_close(text);
                        }
                    } else if requested {
                        self.sp_status = None;
                        if to_list {
                            self.show_world_list();
                        }
                    } else {
                        self.sp_failed(text);
                    }
                }
            }
        }
        if self.sp.as_ref().is_some_and(Session::is_done) {
            self.sp = None;
            self.worlds.borrow_mut().set_busy(None);
            if std::mem::take(&mut self.sp_java) {
                let failure = self.sp_java_failure.take();
                self.java_world(WorldEvent::Stopped(failure));
            }
        }
    }

    /// The native Mods screen, from the main menu and from Java's "mcr" button.
    pub fn open_mods(&mut self) {
        let screen = Box::new(self.cfg.mods.screen());
        match &mut self.play {
            Some(play) => play.show_screen(screen),
            None => self.push(screen),
        }
    }

    fn push(&mut self, screen: Box<dyn Screen>) {
        self.update_menu_size();
        self.stack.push(screen);
    }

    fn renderer_size(&self) -> (u32, u32) {
        match (&self.parts, &self.play) {
            (Some(p), _) => p.renderer.size(),
            (None, Some(play)) => play.render.renderer.size(),
            _ => (1, 1),
        }
    }

    fn update_menu_size(&mut self) {
        let max = self.gui_scale_setting();
        let (size, _) = gui::gui_size(self.renderer_size(), max);
        if size != self.gui_size {
            self.gui_size = size;
            self.stack.resize(size.0, size.1);
        }
    }

    pub fn renderer_mut(&mut self) -> Option<&mut Renderer> {
        match (&mut self.parts, &mut self.play) {
            (Some(p), _) => Some(&mut p.renderer),
            (None, Some(play)) => Some(&mut play.render.renderer),
            _ => None,
        }
    }

    pub fn wants_grab(&self) -> bool {
        if let Some(grab) = self.route_grab() {
            return grab;
        }
        self.play.as_ref().is_some_and(Play::wants_grab)
    }

    /// mcr's frames are not paced by vsync or a cap, so Java's frames may run one ahead.
    pub fn set_mod_uncapped(&self, uncapped: bool) {
        if let Some(frames) = self.mod_frames() {
            frames.lock().set_uncapped(uncapped);
        }
    }

    pub fn max_fps(&self) -> i32 {
        self.settings.borrow().file.options.max_fps
    }

    /// Applies the saved VSync option (off when `allowed` is false) to the live renderer.
    pub fn sync_vsync(&mut self, allowed: bool) -> bool {
        let want = allowed && self.settings.borrow().file.options.enable_vsync;
        if self.applied_vsync != Some(want)
            && let Some(renderer) = self.renderer_mut()
        {
            renderer.set_vsync(want);
            self.applied_vsync = Some(want);
        }
        want
    }

    pub fn ready(&self) -> bool {
        self.play.as_ref().is_some_and(Play::ready)
    }

    fn connect(&mut self, address: &str, with_screen: bool) {
        self.connect_with(address, with_screen, None);
    }

    fn connect_with(&mut self, address: &str, with_screen: bool, transfer: Option<TransferTarget>) {
        let options = self.settings.borrow().file.options.clone();
        let mut config = ClientConfig::default().with_addr(address);
        if let Some(target) = &transfer {
            config = config.for_transfer(target);
        }
        config.name = self.cfg.name.clone();
        config.online = self.cfg.online.clone();
        config.only_show_secure_chat = options.only_show_secure_chat;
        config.builtin_data = Some(self.builtin.clone());
        config.text_renderer = Some(self.text_renderer.clone());
        config.local_server = is_local_server(
            &config.host,
            config.port,
            self.sp.as_ref().and_then(Session::local_port),
            transfer.is_some(),
        );
        config.wire = if self.sp.is_some() {
            WireChoice::Canonical
        } else {
            match self.servers.borrow().pinged_protocol(address) {
                Some(protocol) if transfer.is_none() => WireChoice::Protocol(protocol),
                _ => WireChoice::Auto,
            }
        };
        config.view_distance = self
            .cfg
            .render_distance
            .unwrap_or(options.render_distance)
            .clamp(2, 32) as i8;
        config.pack_link = Some(
            self.packs
                .begin(self.servers.borrow().pack_setting_for(address)),
        );
        let (dialog_link, dialog_end) = dialog_channel();
        config.dialog_link = Some(dialog_link);
        self.dialogs = Some(dialog_end);
        let (conduct_link, conduct_end) = conduct_channel();
        config.conduct_link = Some(conduct_link);
        config.accepted_conduct = self.servers.borrow().accepted_conduct_for(address);
        self.conduct = Some(conduct_end);
        self.current_address = address.to_owned();
        let hooks = self
            .cfg
            .mod_net
            .as_ref()
            .map(|m| m.link.hooks(&config.host, config.port))
            .unwrap_or_default();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = Client::connect_hooked(config, hooks).map_err(|e| match e {
                ClientError::Transferred(target) => ConnectFailure::Transfer(*target),
                other => ConnectFailure::Message(sanitize_for_log(&other.to_string())),
            });
            let _ = tx.send(result);
        });
        self.connecting = Some(rx);
        if with_screen {
            let (screen, _status) = ConnectingScreen::with_text("lang:connect.connecting");
            self.push(Box::new(screen));
        }
    }

    /// Java's multiplayer screen asked to join; Rust connects and Java follows the stream.
    fn poll_mod_connects(&mut self) {
        let Some(net) = &self.cfg.mod_net else { return };
        let requests = net.take_connect_requests();
        if let Some(address) = requests.last()
            && self.play.is_none()
            && self.connecting.is_none()
        {
            let with_screen = !self.route.gui;
            self.connect(&address.clone(), with_screen);
        }
    }

    fn poll_connect(&mut self) {
        let Some(rx) = &self.connecting else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.connecting = None;
        match result {
            Ok(client) => {
                let Some(mut parts) = self.parts.take() else {
                    return;
                };
                let data = mcr_data::v26_3();
                if !std::ptr::eq(parts.assets.data, data) {
                    let started = Instant::now();
                    match parts.assets.for_data(data) {
                        Ok(assets) => {
                            println!(
                                "assets: models baked for {} block states in {:?}",
                                data.state_count(),
                                started.elapsed()
                            );
                            parts.assets = Arc::new(assets);
                        }
                        Err(e) => {
                            eprintln!("assets: rebaking for the server's registries failed: {e:#}")
                        }
                    }
                }
                println!("joined as {}", self.cfg.name);
                let cfg = PlayConfig {
                    render_distance: self.cfg.render_distance,
                    game_dir: self.cfg.game_dir.clone(),
                    local_server: client.config.local_server,
                };
                self.sp_status = None;
                self.play = Some(Play::new(parts, &cfg, client));
                self.close_dialog_screens();
                self.stack.pop();
            }
            Err(ConnectFailure::Transfer(target)) => {
                self.end_pack_session();
                self.close_connecting_screen();
                self.follow_transfer(target);
            }
            Err(ConnectFailure::Message(message)) => {
                if let Some(sp) = &mut self.sp {
                    sp.stop();
                    self.sp_status = None;
                    if self.sp_java {
                        self.sp_java_failure = Some(message.clone());
                    }
                }
                self.end_pack_session();
                self.failure = Some(message.clone());
                if self.route.gui {
                    // Java's own connect screen shows the failure once the bridge session closes.
                    self.show_title();
                    return;
                }
                self.close_connecting_screen();
                self.push(Box::new(
                    DisconnectedScreen::new(Component::text(message))
                        .with_title(Component::translatable("connect.failed", Vec::new())),
                ));
            }
        }
    }

    fn end_session(&mut self, reason: Option<String>) {
        let Some(mut play) = self.play.take() else {
            return;
        };
        self.save_world_icon(play.take_world_icon());
        self.parts = Some(play.finish());
        self.end_pack_session();
        self.update_menu_size();
        if self.sp_java || self.route.gui {
            if let Some(sp) = &mut self.sp {
                sp.stop();
            }
            self.failure = reason;
            self.show_title();
            return;
        }
        if let Some(sp) = &mut self.sp {
            sp.stop();
            sp.return_to_list = reason.is_none();
            if reason.is_none() {
                self.show_world_list();
                let (screen, status) = ConnectingScreen::with_text("lang:menu.savingLevel");
                self.push(Box::new(screen.without_cancel()));
                self.sp_status = Some(status);
                return;
            }
        }
        match reason {
            Some(reason) => {
                self.failure = Some(reason.clone());
                self.push(Box::new(
                    DisconnectedScreen::new(Component::text(reason))
                        .with_title(Component::translatable("disconnect.lost", Vec::new())),
                ));
            }
            None => self.show_title(),
        }
    }

    pub fn advance(&mut self, dt: f32) {
        self.update_route();
        let now = Instant::now();
        self.last_frame = now;
        self.poll_connect();
        self.poll_singleplayer();
        self.poll_mod_worlds();
        self.poll_mod_connects();
        self.sync_java_settings();
        self.poll_packs();
        self.poll_dialogs();
        self.poll_conduct();
        if let Some(settings) = self.play.as_mut().and_then(Play::take_lan_request) {
            self.start_lan(&settings, false);
        }
        if let Some(icon) = self.play.as_mut().and_then(Play::take_server_icon) {
            self.servers
                .borrow_mut()
                .set_icon_for(&self.current_address, &icon);
        }
        // Java must finish loading before it mirrors a connection.
        if self.mods_settled()
            && let Some(addr) = self.cfg.connect.take()
        {
            self.connect(&addr, false);
        }
        if self.mods_settled()
            && let Some(folder) = self.cfg.singleplayer.take()
        {
            self.play_world(Launch::Existing(folder), self.mod_frames().is_some());
        }
        let mut ran = 0;
        if let Some(play) = &mut self.play {
            self.accum = 0.0;
            let ticks = play.due_ticks(dt);
            for _ in 0..ticks {
                self.tick_play();
            }
            ran = ticks;
        } else {
            self.accum += dt.clamp(0.0, 0.25);
            while self.accum >= TICK_SECONDS {
                self.accum -= TICK_SECONDS;
                self.tick_menu();
                ran += 1;
            }
        }
        let partial = match &self.play {
            Some(play) => play.partial(),
            None => self.accum / TICK_SECONDS,
        };
        if let Some(frames) = self.mod_frames() {
            frames.lock().add_ticks(ran, partial);
        }
        self.check_session();
    }

    /// Java's vanilla key rebinds become mcr's (and are saved); mcr's bindings go to Java when they change.
    fn sync_java_settings(&mut self) {
        let Some(net) = &self.cfg.mod_net else { return };
        let changes = net.take_key_changes();
        if !changes.is_empty() {
            let edits = mod_sync::java_key_edits(&self.settings.borrow().file.bindings, &changes);
            if !edits.is_empty() {
                self.settings.borrow_mut().edit(|f| {
                    for (index, key) in edits {
                        f.bindings.set(index, key);
                    }
                });
            }
        }
        let Some(frames) = &net.frames else { return };
        let settings = self.settings.borrow();
        if settings.version != self.java_settings_seen {
            self.java_settings_seen = settings.version;
            frames
                .lock()
                .set_key_bindings(mod_sync::key_table(&settings.file.bindings));
        }
    }

    /// mcr's render distance as its fog uses it, and its entity distance scaling.
    fn java_view(&self) -> (u32, f32) {
        let options = &self.settings.borrow().file.options;
        let distance = match &self.play {
            Some(play) => play.render_distance(),
            None => self.cfg.render_distance.unwrap_or(options.render_distance),
        };
        (
            u32::try_from(distance.clamp(2, 64)).unwrap_or(2),
            options.entity_distance_scaling as f32,
        )
    }

    /// Without mods, or once Java finished loading and its GUI is composed.
    pub fn mods_settled(&self) -> bool {
        self.mod_frames().is_none_or(|f| {
            let link = f.lock();
            link.gui_active() && link.java().loaded
        })
    }

    /// Java stopped for good, before or after its GUI showed.
    pub fn mods_gone(&self) -> bool {
        self.mod_frames().is_some_and(|f| f.lock().java_gone())
    }

    /// Starts Java's compose timings afresh (e.g. once in game).
    pub fn reset_mod_stats(&self) {
        if let Some(f) = self.mod_frames() {
            f.lock().reset_stats();
        }
    }

    /// Hands the light sources a Java mod computed to the terrain renderer, unless the user turned them off.
    fn update_dynamic_lights(&mut self) {
        let frames = self.cfg.mod_net.as_ref().and_then(|n| n.frames.as_ref());
        let Some(frames) = frames else {
            return;
        };
        let link = frames.lock();
        let provider = link.dynamic_light_provider();
        if let Some(play) = &mut self.play {
            let on = self.settings.borrow().file.options.mcr_dynamic_lights;
            let lights = if on { link.dynamic_lights() } else { &[] };
            play.render.renderer.set_dynamic_lights(lights);
        }
        drop(link);
        let mut settings = self.settings.borrow_mut();
        if settings.dynamic_lights_provider != provider {
            settings.dynamic_lights_provider = provider;
        }
    }

    pub(crate) fn mod_frames(&self) -> Option<&ModFrames> {
        self.cfg.mod_net.as_ref()?.frames.as_ref()
    }

    /// Java's GUI is composed this frame, so the native HUD and title screen stay hidden.
    fn java_gui(&mut self) -> bool {
        let Some(frames) = self.mod_frames() else {
            return false;
        };
        let (active, notice) = {
            let mut link = frames.lock();
            if let Some(line) = link.take_profile_line() {
                eprintln!("[mods] profile: {line}");
            }
            (link.gui_active(), link.take_notice())
        };
        if let Some(message) = notice {
            eprintln!("[mods] {message}");
            let toasts = match (&mut self.play, &mut self.parts) {
                (Some(play), _) => Some(play.gui_mut()),
                (None, Some(parts)) => Some(&mut parts.gui),
                (None, None) => None,
            };
            if let Some(gui) = toasts {
                gui.toasts.add_system(
                    &mut gui.ui,
                    mcr_ui::SystemToastId::new(BRIDGE_TOAST),
                    "Mods",
                    Some(&message),
                );
            }
        }
        active
    }

    fn tick_play(&mut self) {
        let Some(mut play) = self.play.take() else {
            return;
        };
        let mut pilot = self.autopilot.take();
        if let Some(p) = &mut pilot {
            self.play = Some(play);
            p.step(self);
            play = self.play.take().unwrap_or_else(|| unreachable!());
        }
        self.autopilot = pilot;
        play.tick();
        self.play = Some(play);
    }

    fn tick_menu(&mut self) {
        let ticks = self.started.elapsed().as_millis() as u64 / 50;
        if let Some(audio) = self.parts.as_mut().and_then(|p| p.audio.as_mut()) {
            audio.poll(ticks);
            audio.tick_music(Some(&mcr_audio::Music::menu()), 1.0, false);
        }
        if let Some(mut p) = self.autopilot.take() {
            p.step(self);
            self.autopilot = Some(p);
        }
        let mut actions = Vec::new();
        {
            let mut env = Env {
                view: &NoView,
                clipboard: match &mut self.parts {
                    Some(p) => &mut p.gui.clipboard,
                    None => return,
                },
                now_ms: self.started.elapsed().as_millis() as u64,
            };
            self.stack.tick(&mut env, &mut actions);
        }
        self.menu_actions(actions);
    }

    /// Vanilla reconnects to the new address straight away, carrying the cookies over.
    fn follow_transfer(&mut self, target: TransferTarget) {
        let address = format!("{}:{}", target.host, target.port);
        println!("transfer to {}", sanitize_for_log(&address));
        self.connect_with(&address, true, Some(target));
    }

    fn check_session(&mut self) {
        let Some(play) = &mut self.play else { return };
        if play.quit {
            self.quit = true;
        }
        if let Some(target) = play.transfer.take() {
            let Some(play) = self.play.take() else { return };
            self.parts = Some(play.finish());
            self.end_pack_session();
            self.follow_transfer(target);
            return;
        }
        if play.leave {
            self.end_session(None);
        } else if let Some(reason) = play.closed().map(str::to_owned) {
            self.end_session(Some(reason));
        }
    }

    fn menu_actions(&mut self, actions: Vec<ScreenAction>) {
        for action in actions {
            match action {
                ScreenAction::Connect(address) => {
                    self.connect(&address, true);
                }
                ScreenAction::CancelConnect => {
                    self.connecting = None;
                    self.end_pack_session();
                    if let Some(sp) = &mut self.sp {
                        sp.stop();
                        self.sp_status = None;
                    }
                }
                ScreenAction::PlayWorld(folder) => self.play_world(Launch::Existing(folder), false),
                ScreenAction::CreateWorld(settings) => {
                    self.play_world(Launch::New(*settings), false)
                }
                ScreenAction::ResourcePackAnswer(accepted) => self.pack_answer(accepted),
                ScreenAction::ConductAnswer {
                    accept,
                    remember,
                    hash,
                } => self.conduct_answer(accept, remember, hash),
                ScreenAction::ShowDialog(reference) => {
                    if let Some(end) = &self.dialogs {
                        end.reply(DialogReply::Open(reference));
                    }
                }
                ScreenAction::CustomClick { id, payload } => {
                    if let Some(end) = &self.dialogs {
                        end.reply(DialogReply::Custom { id, payload });
                    }
                }
                ScreenAction::DialogDisconnect => {
                    if let Some(end) = &self.dialogs {
                        end.reply(DialogReply::Disconnect(Component::translatable(
                            "menu.custom_screen_info.disconnect",
                            Vec::new(),
                        )));
                    }
                }
                ScreenAction::PlaySound(name) => {
                    if let Some(a) = self.parts.as_mut().and_then(|p| p.audio.as_mut()) {
                        a.play(name, None, 1.0, 1.0, mcr_audio::SoundSource::Ui);
                    }
                }
                ScreenAction::OpenOptions => {
                    let screen = OptionsScreen::new(self.settings.clone(), self.languages());
                    self.push(Box::new(screen));
                }
                ScreenAction::OpenMods => self.open_mods(),
                ScreenAction::OpenUrl(url) => gui::open_url(&url),
                ScreenAction::CopyToClipboard(text) => {
                    if let Some(p) = &mut self.parts {
                        p.gui.clipboard.set(&text);
                    }
                }
                ScreenAction::ClickSound => {
                    if let Some(a) = self.parts.as_mut().and_then(|p| p.audio.as_mut()) {
                        a.click();
                    }
                }
                ScreenAction::Quit => self.quit = true,
                ScreenAction::Launcher => {
                    self.to_launcher = self.play.is_none() && self.sp.is_none()
                }
                _ => {}
            }
        }
    }

    fn menu_input(&mut self, input: UiInput) {
        if let UiInput::MouseMove { x, y } = input {
            self.mouse = (x, y);
        }
        let Some(parts) = &mut self.parts else { return };
        let mut actions = Vec::new();
        let mut env = Env {
            view: &NoView,
            clipboard: &mut parts.gui.clipboard,
            now_ms: self.started.elapsed().as_millis() as u64,
        };
        self.stack.input(&mut env, &input, &mut actions);
        self.menu_actions(actions);
    }

    pub fn ui_input(&mut self, input: UiInput) {
        match &mut self.play {
            Some(play) if play.screen_open() => play.ui_input(input),
            Some(_) => {}
            None => self.menu_input(input),
        }
    }

    /// Scripted keys: printable keys type what a US layout would.
    pub fn key_event(&mut self, key: InputKey, pressed: bool, text: Option<&str>) {
        let printable = match key {
            InputKey::Keyboard(hid) => u32::try_from(hid)
                .ok()
                .and_then(mcr_modbridge::input::us_printable),
            _ => None,
        };
        self.key_input(key, printable, pressed, false, text);
    }

    /// A key or button from the window; `printable` is the layout's character without modifiers.
    pub fn key_input(
        &mut self,
        key: InputKey,
        printable: Option<char>,
        pressed: bool,
        repeat: bool,
        text: Option<&str>,
    ) {
        if self.route_key(key, printable, pressed, repeat, text) || repeat && !self.accepts_repeat()
        {
            return;
        }
        if let Some(play) = &mut self.play {
            play.key_event(key, pressed, text);
            return;
        }
        track_modifiers(&mut self.mods, key, pressed);
        for input in ui_inputs(key, pressed, text, self.mods, self.mouse) {
            self.menu_input(input);
        }
    }

    pub fn cursor_moved(&mut self, px: f64, py: f64) {
        if self.route_cursor(px, py) {
            return;
        }
        if let Some(play) = &mut self.play {
            play.cursor_moved(px, py);
            return;
        }
        let max = self.gui_scale_setting();
        let (_, scale) = gui::gui_size(self.renderer_size(), max);
        let scale = f64::from(scale);
        self.menu_input(UiInput::MouseMove {
            x: (px / scale) as f32,
            y: (py / scale) as f32,
        });
    }

    pub fn scroll(&mut self, lines: f32) {
        if let Some(play) = &mut self.play {
            play.scroll(lines);
            return;
        }
        let (x, y) = self.mouse;
        self.menu_input(UiInput::Scroll {
            x,
            y,
            dx: 0.0,
            dy: lines,
        });
    }

    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    pub fn accepts_repeat(&self) -> bool {
        self.play.as_ref().is_none_or(Play::screen_open)
    }

    pub fn ui_text(&mut self, text: &str) {
        if self.route_text(text) {
            return;
        }
        for c in text.chars().filter(|c| !c.is_control()) {
            self.ui_input(UiInput::Char(c));
        }
    }

    pub fn ui_key(&mut self, key: mcr_ui::Key) {
        if self.route_ui_key(key) {
            return;
        }
        let mods = self.mods;
        self.ui_input(UiInput::Key {
            key,
            mods,
            repeat: false,
        });
    }

    pub fn lose_focus(&mut self) {
        if let Some(play) = &mut self.play {
            play.lose_focus();
        }
    }

    pub fn mouse_motion(&mut self, dx: f64, dy: f64) {
        if self.route_motion(dx, dy) {
            return;
        }
        if let Some(play) = &mut self.play {
            play.mouse_motion(dx, dy);
        }
    }

    pub fn screen_click(&mut self, x: f32, y: f32, button: mcr_ui::MouseButton) {
        if self.route_click(x, y, button) {
            return;
        }
        let mods = self.mods;
        self.ui_input(UiInput::MouseMove { x, y });
        self.ui_input(UiInput::MousePress {
            x,
            y,
            button,
            mods,
            double: false,
        });
        self.ui_input(UiInput::MouseRelease { x, y, button, mods });
    }

    /// Half of a click, so scripts can drag between a press and its release.
    pub fn screen_button(&mut self, x: f32, y: f32, button: mcr_ui::MouseButton, pressed: bool) {
        if self.route_button(x, y, button, pressed) {
            return;
        }
        let mods = self.mods;
        self.ui_input(UiInput::MouseMove { x, y });
        self.ui_input(if pressed {
            UiInput::MousePress {
                x,
                y,
                button,
                mods,
                double: false,
            }
        } else {
            UiInput::MouseRelease { x, y, button, mods }
        });
    }

    pub fn screen_hover(&mut self, x: f32, y: f32) {
        if !self.route_hover(x, y) {
            self.ui_input(UiInput::MouseMove { x, y });
        }
    }

    pub fn frame(&mut self) -> Option<FrameStats> {
        let java_gui = self.java_gui();
        let java_perspective = self.mod_frames().and_then(|f| f.lock().java_perspective());
        if let Some(play) = &mut self.play {
            play.follow_perspective(java_perspective);
        }
        let third_person = self
            .play
            .as_ref()
            .and_then(|play| play.third_person_distance(play.partial()));
        let mut fov_scale = 1.0;
        let (distance, entity_scale) = self.java_view();
        let mut java_camera = None;
        let mut java_weather = false;
        let mut java_steps = false;
        let mut java_turn = false;
        let mut view_roll = 0.0;
        let java_world = self.mod_frames().is_some_and(|f| {
            let mut link = f.lock();
            link.set_third_person(third_person);
            link.set_view(distance, entity_scale);
            fov_scale = link.java().fov_scale;
            java_camera = link.java_camera();
            java_weather = link.weather_replaced();
            java_steps = link.steps_replaced();
            java_turn = link.turn_captured();
            view_roll = link.view_roll();
            link.set_camera_handoff(
                java_camera
                    .and(self.play.as_ref())
                    .map(|play| play.player_camera(play.partial())),
            );
            link.world_active()
        });
        self.update_dynamic_lights();
        self.update_route();
        if let Some(play) = &mut self.play {
            play.java_gui = java_gui;
            play.java_world = java_world;
            play.java_fov_scale = if java_world { fov_scale } else { 1.0 };
            play.java_camera = java_camera;
            play.java_weather = java_weather;
            play.java_steps = java_steps;
            play.java_turn = java_turn;
            play.java_view_roll = view_roll;
            return play.frame();
        }
        self.update_menu_size();
        let max_scale = u32::try_from(self.gui_scale_setting()).unwrap_or(0);
        let parts = self.parts.as_mut()?;
        parts
            .gui
            .sync_language(&self.settings.borrow().file.options.language);
        let size = parts.renderer.size();
        {
            let gui = &mut parts.gui;
            let mut frame = gui.ui.frame_scaled(size.0, size.1, max_scale);
            let mut cx = RenderCx::new(&mut frame, &mut gui.icons, &gui.assets, &NoView);
            cx.mouse = self.mouse;
            cx.now_ms = self.started.elapsed().as_millis() as u64;
            if !java_gui || self.stack.len() > 1 {
                self.stack.render(&mut cx);
            }
            if !gui.toasts.is_empty() {
                gui.toasts.draw(&mut cx);
            }
        }
        let now_ms = self.started.elapsed().as_millis() as u64;
        let display_time = self
            .settings
            .borrow()
            .file
            .options
            .notification_display_time;
        parts.gui.tick_toasts(now_ms, display_time);
        let camera = Camera::default();
        let mut overlay = UiOverlay {
            ui_renderer: &mut parts.gui.ui_renderer,
            ui: &mut parts.gui.ui,
        };
        parts
            .renderer
            .render_with_overlay(
                &FrameInputs {
                    camera: &camera,
                    sky_light: 1.0,
                },
                Some(&mut overlay),
            )
            .ok()
    }

    pub fn respawn_if_dead(&mut self) {
        if let Some(play) = &mut self.play {
            play.respawn_if_dead();
        }
    }

    pub fn idle(&self) -> bool {
        self.play.is_none() && self.connecting.is_none() && self.sp.is_none()
    }

    pub fn rejoin(&mut self) -> bool {
        if !self.idle() || self.current_address.is_empty() {
            return false;
        }
        self.failure = None;
        self.show_title();
        let address = self.current_address.clone();
        self.connect(&address, true);
        true
    }

    pub fn soak_stats(&self) -> crate::soak::Stats {
        let renderer = match (&self.parts, &self.play) {
            (Some(p), _) => Some(&p.renderer),
            (None, Some(play)) => Some(&play.render.renderer),
            _ => None,
        };
        let (entity_layers, entities, chunks) =
            self.play.as_ref().map_or((0, 0, 0), Play::soak_counts);
        crate::soak::Stats {
            gpu: renderer.map(Renderer::gpu_counters).unwrap_or_default(),
            entity_layers,
            entities,
            chunks,
        }
    }

    pub fn read_offscreen(&mut self) -> Result<Vec<u8>> {
        let renderer = self.renderer_mut().context("no renderer")?;
        Ok(renderer.read_offscreen()?)
    }
}

#[cfg(test)]
mod tests {
    use super::{is_local_server, stop_text};

    #[test]
    fn only_the_running_local_port_is_the_local_server() {
        assert!(is_local_server("127.0.0.1", 40000, Some(40000), false));
        assert!(!is_local_server("127.0.0.1", 25565, Some(40000), false));
        assert!(!is_local_server("localhost", 40000, Some(40000), false));
        assert!(!is_local_server("127.0.0.1", 40000, Some(40000), true));
        assert!(!is_local_server("127.0.0.1", 40000, None, false));
    }

    #[test]
    fn stop_reason_keeps_lines_and_escapes_controls() {
        assert_eq!(stop_text(None), "The local server stopped");
        assert_eq!(
            stop_text(Some("a\u{1b}[31m\nb".into())),
            "The local server stopped:\na\\u{1b}[31m\nb"
        );
    }
}
