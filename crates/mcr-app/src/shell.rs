//! Owns the app flow: menus while no session exists, a [`Play`] while one runs.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use anyhow::{Context, Result};
use mcr_client::{Client, ClientConfig, sanitize_for_log};
use mcr_protocol::types::text::Component;
use mcr_render::{Camera, FrameInputs, FrameStats, Renderer};
use mcr_settings::InputKey;
use mcr_ui::{
    Clipboard, ConnectingScreen, DisconnectedScreen, Env, Modifiers, NoView, RenderCx, Screen,
    ScreenAction, ScreenStack, SharedServers, TitleScreen, UiInput, vanilla_resources,
};

use crate::assets::Assets;
use crate::audio::Audio;
use crate::autopilot::Autopilot;
use crate::gui::{self, Gui, UiOverlay};
use crate::input::{track_modifiers, ui_inputs};
use crate::play::{Parts, Play, PlayConfig, TICK_SECONDS};
use crate::servers::FileServers;
use crate::settings::{OptionsScreen, Settings, SharedSettings};

type Connect = Receiver<Result<Client, String>>;

#[derive(Debug, Clone)]
pub struct ShellConfig {
    pub game_dir: PathBuf,
    pub jar: PathBuf,
    pub assets_dir: PathBuf,
    pub name: String,
    pub render_distance: Option<i32>,
    pub connect: Option<String>,
}

pub struct Shell {
    cfg: ShellConfig,
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
    pub shot: Option<PathBuf>,
    pub failure: Option<String>,
    autopilot: Option<Autopilot>,
    camera: Camera,
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
    pub fn new(renderer: Renderer, cfg: ShellConfig) -> Result<Self> {
        let data = mcr_data::v26_3();
        let settings = Settings::load(&cfg.game_dir);
        let options = settings.borrow().file.options.clone();
        let assets = Arc::new(Assets::load(&cfg.jar)?);
        let index = asset_index_id(&cfg.jar);
        let resources = vanilla_resources(
            &cfg.jar,
            index.as_deref().map(|i| (cfg.assets_dir.as_path(), i)),
        )
        .context("opening resources")?;
        let gui = Gui::load(&resources, renderer.device(), renderer.target_format())?;
        let audio = Audio::start(
            &cfg.assets_dir,
            index.as_deref().unwrap_or("34"),
            &options,
            data,
        );
        if audio.is_none() {
            println!("audio: disabled");
        }
        let servers = Rc::new(RefCell::new(FileServers::load(&cfg.game_dir)));
        let now = Instant::now();
        let mut shell = Self {
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
            shot: None,
            failure: None,
            autopilot: None,
            camera: Camera::default(),
            cfg,
        };
        shell.show_title();
        if let Some(addr) = shell.cfg.connect.clone() {
            shell.connect(&addr, false);
        }
        Ok(shell)
    }

    pub fn set_autopilot(&mut self, autopilot: Autopilot) {
        self.autopilot = Some(autopilot);
    }

    pub fn script_finished(&self) -> bool {
        self.autopilot.as_ref().is_none_or(|a| a.finished)
    }

    fn show_title(&mut self) {
        self.stack.clear();
        let servers: SharedServers = self.servers.clone();
        self.push(Box::new(TitleScreen::new("26.3", servers)));
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
        let max = self.settings.borrow().file.options.gui_scale;
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
        self.play.as_ref().is_some_and(Play::wants_grab)
    }

    pub fn max_fps(&self) -> i32 {
        self.settings.borrow().file.options.max_fps
    }

    pub fn ready(&self) -> bool {
        self.play.as_ref().is_some_and(Play::ready)
    }

    fn connect(&mut self, address: &str, with_screen: bool) {
        let options = self.settings.borrow().file.options.clone();
        let mut config = ClientConfig::default().with_addr(address);
        config.name = self.cfg.name.clone();
        config.view_distance = self
            .cfg
            .render_distance
            .unwrap_or(options.render_distance)
            .clamp(2, 32) as i8;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = Client::connect(config).map_err(|e| sanitize_for_log(&e.to_string()));
            let _ = tx.send(result);
        });
        self.connecting = Some(rx);
        if with_screen {
            let (screen, _status) = ConnectingScreen::with_text("lang:connect.connecting");
            self.push(Box::new(screen));
        }
    }

    fn poll_connect(&mut self) {
        let Some(rx) = &self.connecting else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.connecting = None;
        match result {
            Ok(client) => {
                let Some(parts) = self.parts.take() else {
                    return;
                };
                println!("joined as {}", self.cfg.name);
                let cfg = PlayConfig {
                    render_distance: self.cfg.render_distance,
                };
                self.play = Some(Play::new(parts, &cfg, client));
                self.stack.pop();
            }
            Err(message) => {
                self.failure = Some(message.clone());
                self.stack.pop();
                self.push(Box::new(
                    DisconnectedScreen::new(Component::text(message))
                        .with_title(Component::translatable("connect.failed", Vec::new())),
                ));
            }
        }
    }

    fn end_session(&mut self, reason: Option<String>) {
        let Some(play) = self.play.take() else { return };
        self.parts = Some(play.finish());
        self.update_menu_size();
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
        let now = Instant::now();
        self.last_frame = now;
        self.poll_connect();
        if let Some(play) = &mut self.play {
            self.accum = 0.0;
            let ticks = play.due_ticks(dt);
            for _ in 0..ticks {
                self.tick_play();
            }
        } else {
            self.accum += dt.clamp(0.0, 0.25);
            while self.accum >= TICK_SECONDS {
                self.accum -= TICK_SECONDS;
                self.tick_menu();
            }
        }
        self.check_session();
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

    fn check_session(&mut self) {
        let Some(play) = &mut self.play else { return };
        if play.quit {
            self.quit = true;
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
                ScreenAction::CancelConnect => self.connecting = None,
                ScreenAction::OpenOptions => {
                    let screen = OptionsScreen::new(self.settings.clone());
                    self.push(Box::new(screen));
                }
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

    pub fn key_event(&mut self, key: InputKey, pressed: bool, text: Option<&str>) {
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
        if let Some(play) = &mut self.play {
            play.cursor_moved(px, py);
            return;
        }
        let max = self.settings.borrow().file.options.gui_scale;
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
        for c in text.chars().filter(|c| !c.is_control()) {
            self.ui_input(UiInput::Char(c));
        }
    }

    pub fn ui_key(&mut self, key: mcr_ui::Key) {
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
        if let Some(play) = &mut self.play {
            play.mouse_motion(dx, dy);
        }
    }

    pub fn screen_click(&mut self, x: f32, y: f32, button: mcr_ui::MouseButton) {
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

    pub fn frame(&mut self) -> Option<FrameStats> {
        if let Some(play) = &mut self.play {
            return play.frame();
        }
        self.update_menu_size();
        let parts = self.parts.as_mut()?;
        let size = parts.renderer.size();
        let max_scale = u32::try_from(self.settings.borrow().file.options.gui_scale).unwrap_or(0);
        {
            let gui = &mut parts.gui;
            let mut frame = gui.ui.frame_scaled(size.0, size.1, max_scale);
            let mut cx = RenderCx::new(&mut frame, &mut gui.icons, &gui.assets, &NoView);
            cx.mouse = self.mouse;
            cx.now_ms = self.started.elapsed().as_millis() as u64;
            self.stack.render(&mut cx);
        }
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

    pub fn read_offscreen(&mut self) -> Result<Vec<u8>> {
        let renderer = self.renderer_mut().context("no renderer")?;
        Ok(renderer.read_offscreen()?)
    }
}
