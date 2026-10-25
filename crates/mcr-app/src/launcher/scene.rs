use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use mcr_launcher::LaunchPlan;
use mcr_render::{Camera, FrameInputs, FrameStats, Renderer, RendererConfig, Target};
use mcr_settings::InputKey;
use mcr_ui::{
    Clipboard, Env, LauncherScreen, Modifiers, NoView, RenderCx, ScreenAction, ScreenStack,
    SharedLauncher, UiInput, vanilla_resources,
};

use super::LauncherApp;
use crate::cli::Options;
use crate::gui::{self, Gui, UiOverlay};
use crate::input::{track_modifiers, ui_inputs};
use crate::play::TICK_SECONDS;

/// The launcher has no world, so its renderer gets tiny arenas.
pub fn renderer_config(opts: &Options) -> RendererConfig {
    RendererConfig {
        vsync: opts.vsync,
        cube_arena_quads: 1 << 12,
        model_arena_quads: 1 << 12,
        max_sections: 64,
        upload_budget_bytes: 1 << 20,
        tint_tables: 64,
        max_translucent_sections: 64,
        ..RendererConfig::default()
    }
}

/// Hosts the launcher screens with their own renderer and GUI until Play produces a plan.
pub struct Launcher {
    renderer: Renderer,
    gui: Gui,
    stack: ScreenStack,
    model: Rc<RefCell<LauncherApp>>,
    gui_size: (i32, i32),
    mouse: (f32, f32),
    mods: Modifiers,
    accum: f32,
    started: Instant,
    pub quit: bool,
    pub shot: Option<PathBuf>,
}

impl std::fmt::Debug for Launcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Launcher").finish_non_exhaustive()
    }
}

impl Launcher {
    pub fn new(target: Target, opts: &Options) -> Result<Self> {
        let renderer = Renderer::new(target, renderer_config(opts))
            .context("creating the launcher renderer")?;
        let resources = Arc::new(
            vanilla_resources(&opts.jar, None)
                .with_context(|| format!("opening {}", opts.jar.display()))?,
        );
        let gui = Gui::load(resources, renderer.device(), renderer.target_format())?;
        let model = Rc::new(RefCell::new(LauncherApp::open(&opts.data_dir)));
        let mut stack = ScreenStack::new(0, 0);
        let shared: SharedLauncher = model.clone();
        stack.push(Box::new(LauncherScreen::new(shared)));
        Ok(Self {
            renderer,
            gui,
            stack,
            model,
            gui_size: (0, 0),
            mouse: (0.0, 0.0),
            mods: Modifiers::NONE,
            accum: 0.0,
            started: Instant::now(),
            quit: false,
            shot: None,
        })
    }

    pub fn use_system_clipboard(&mut self) {
        self.gui
            .clipboard
            .use_system(crate::mod_input::SystemClipboard::open());
    }

    pub fn take_plan(&mut self) -> Option<LaunchPlan> {
        self.model.borrow_mut().take_plan()
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
    }

    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Returns how many 20 Hz ticks ran, for the autopilot.
    pub fn advance(&mut self, dt: f32) -> u32 {
        self.accum += dt.clamp(0.0, 0.25);
        let mut ran = 0;
        while self.accum >= TICK_SECONDS {
            self.accum -= TICK_SECONDS;
            let mut actions = Vec::new();
            let now_ms = self.now_ms();
            let mut env = Env {
                view: &NoView,
                clipboard: &mut self.gui.clipboard,
                now_ms,
            };
            self.stack.tick(&mut env, &mut actions);
            self.actions(actions);
            ran += 1;
        }
        ran
    }

    fn actions(&mut self, actions: Vec<ScreenAction>) {
        for action in actions {
            match action {
                ScreenAction::Quit => self.quit = true,
                ScreenAction::OpenUrl(url) => gui::open_url(&url),
                ScreenAction::CopyToClipboard(text) => self.gui.clipboard.set(&text),
                _ => {}
            }
        }
        if self.stack.is_empty() {
            self.quit = true;
        }
    }

    fn input(&mut self, input: UiInput) {
        if let UiInput::MouseMove { x, y } = input {
            self.mouse = (x, y);
        }
        let mut actions = Vec::new();
        let now_ms = self.now_ms();
        let mut env = Env {
            view: &NoView,
            clipboard: &mut self.gui.clipboard,
            now_ms,
        };
        self.stack.input(&mut env, &input, &mut actions);
        self.actions(actions);
    }

    pub fn key_input(&mut self, key: InputKey, pressed: bool, repeat: bool, text: Option<&str>) {
        track_modifiers(&mut self.mods, key, pressed);
        for input in ui_inputs(key, pressed, text, self.mods, self.mouse) {
            let input = match input {
                UiInput::Key { key, mods, .. } => UiInput::Key { key, mods, repeat },
                other => other,
            };
            self.input(input);
        }
    }

    pub fn cursor_moved(&mut self, px: f64, py: f64) {
        let (_, scale) = gui::gui_size(self.renderer.size(), 0);
        let scale = f64::from(scale);
        self.input(UiInput::MouseMove {
            x: (px / scale) as f32,
            y: (py / scale) as f32,
        });
    }

    pub fn scroll(&mut self, lines: f32) {
        let (x, y) = self.mouse;
        self.input(UiInput::Scroll {
            x,
            y,
            dx: 0.0,
            dy: lines,
            mods: mcr_ui::Modifiers::NONE,
        });
    }

    pub fn ui_text(&mut self, text: &str) {
        for c in text.chars().filter(|c| !c.is_control()) {
            self.input(UiInput::Char(c));
        }
    }

    pub fn ui_key(&mut self, key: mcr_ui::Key) {
        let mods = self.mods;
        self.input(UiInput::Key {
            key,
            mods,
            repeat: false,
        });
    }

    /// Half of a click at GUI coordinates, or a whole one with `pressed: None`.
    pub fn screen_button(
        &mut self,
        x: f32,
        y: f32,
        button: mcr_ui::MouseButton,
        pressed: Option<bool>,
    ) {
        let mods = self.mods;
        self.input(UiInput::MouseMove { x, y });
        if pressed != Some(false) {
            self.input(UiInput::MousePress {
                x,
                y,
                button,
                mods,
                double: false,
            });
        }
        if pressed != Some(true) {
            self.input(UiInput::MouseRelease { x, y, button, mods });
        }
    }

    pub fn screen_hover(&mut self, x: f32, y: f32) {
        self.input(UiInput::MouseMove { x, y });
    }

    pub fn frame(&mut self) -> Option<FrameStats> {
        let size = self.renderer.size();
        let (gui_size, _) = gui::gui_size(size, 0);
        if gui_size != self.gui_size {
            self.gui_size = gui_size;
            self.stack.resize(gui_size.0, gui_size.1);
        }
        {
            let gui = &mut self.gui;
            let mut frame = gui.ui.frame_scaled(size.0, size.1, 0);
            let mut cx = RenderCx::new(&mut frame, &mut gui.icons, &gui.assets, &NoView);
            cx.mouse = self.mouse;
            cx.now_ms = self.started.elapsed().as_millis() as u64;
            self.stack.render(&mut cx);
        }
        let camera = Camera::default();
        let mut overlay = UiOverlay {
            ui_renderer: &mut self.gui.ui_renderer,
            ui: &mut self.gui.ui,
            panorama: None,
        };
        self.renderer
            .render_with_overlay(
                &FrameInputs {
                    camera: &camera,
                    sky_light: 1.0,
                },
                Some(&mut overlay),
            )
            .ok()
    }

    pub fn read_offscreen(&self) -> Result<Vec<u8>> {
        Ok(self.renderer.read_offscreen()?)
    }
}
