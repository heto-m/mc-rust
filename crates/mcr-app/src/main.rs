mod ambience;
mod animate;
mod assets;
mod audio;
mod autopilot;
mod block_entities;
mod camera_fx;
mod cli;
mod debug;
mod debug_keys;
mod editors;
mod effects;
mod engine;
mod entities;
mod entity_class;
mod entity_events;
mod entity_loops;
mod fly;
mod game;
mod game_mode_switcher;
mod gizmos;
mod grab;
mod gui;
mod hand;
#[cfg(feature = "heap-profile")]
mod heap_profile;
mod heap_trim;
mod hud;
mod input;
mod java_models;
mod key_binds;
mod language;
mod latency;
mod launcher;
mod level_events;
mod map;
mod mesh_world;
mod mod_input;
mod mod_sync;
mod mods;
mod mods_ui;
mod movement_sounds;
mod nametags;
mod needles;
mod net_world;
mod options_list;
mod options_pages;
mod outline;
mod overlays;
mod packs;
mod play;
mod post_effects;
mod preview;
mod render_system;
mod respacks;
mod screenshot;
mod servers;
mod settings;
mod shell;
mod singleplayer;
mod skins;
mod soak;
mod social;
mod source;
mod stats;
mod status;
mod subtitles;
mod target;
mod text_display;
mod view;
mod vision;
mod world_setup;

use std::sync::Arc;

#[cfg(feature = "heap-profile")]
#[global_allocator]
static HEAP: heap_profile::Sampler = heap_profile::Sampler;
use std::time::Instant;

use anyhow::{Context, Result};
use cli::Options;
use engine::Engine;
use fly::FlyController;
use mcr_render::glam::DVec3;
use mcr_render::synthetic::{Terrain, generate_world};
use mcr_render::{Camera, FrameInputs, Renderer, RendererConfig, Target};
use shell::{Shell, ShellConfig};
use stats::StatWindow;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{DeviceEvent, DeviceId, ElementState, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, CursorIcon, Fullscreen, Window, WindowId};

struct SyntheticScene {
    renderer: Renderer,
    camera: Camera,
}

enum Scene {
    Synthetic(Box<SyntheticScene>),
    World(Box<Engine>),
    Shell(Box<Shell>),
    Launcher(Box<launcher::scene::Launcher>),
}

impl Scene {
    fn camera_mut(&mut self) -> Option<&mut Camera> {
        match self {
            Self::Synthetic(s) => Some(&mut s.camera),
            Self::World(engine) => Some(&mut engine.game.camera),
            Self::Shell(shell) => Some(shell.camera_mut()),
            Self::Launcher(_) => None,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        match self {
            Self::Synthetic(s) => s.renderer.resize(width, height),
            Self::World(engine) => engine.render.renderer.resize(width, height),
            Self::Shell(shell) => shell
                .renderer_mut()
                .expect("renderer")
                .resize(width, height),
            Self::Launcher(l) => l.resize(width, height),
        }
    }

    fn renderer_mut(&mut self) -> Option<&mut Renderer> {
        match self {
            Self::Synthetic(s) => Some(&mut s.renderer),
            Self::World(engine) => Some(&mut engine.render.renderer),
            Self::Shell(shell) => shell.renderer_mut(),
            Self::Launcher(_) => None,
        }
    }
}

struct App {
    opts: Options,
    /// The options before a launcher profile was applied, restored when the title screen returns to it.
    base_opts: Options,
    /// A JVM was started in this process, so the launcher can only come back in a new process.
    jvm_started: bool,
    relaunch: bool,
    launched: Instant,
    window: Option<Arc<Window>>,
    scene: Option<Scene>,
    fly: FlyController,
    grabbed: bool,
    last_frame: Instant,
    stats: StatWindow,
    ready_reported: bool,
    exit: bool,
    mod_host: Option<mods::ModHost>,
    cursor: CursorIcon,
    ime: Option<Option<[i32; 4]>>,
    boot: Option<mods::BootPlan>,
    mods_app: Option<mods_ui::ModsApp>,
    /// The script's steps left over after the launcher's Play.
    autopilot: Option<autopilot::Autopilot>,
    /// The `fullscreen` and `exclusiveFullscreen` options last applied to the window.
    fullscreen: (bool, bool),
}

impl App {
    fn new(opts: Options, launched: Instant, started: Option<mods::Startup>) -> Self {
        let (boot, mods_app) = match started {
            Some(s) => (s.boot, Some(s.app)),
            None => (None, None),
        };
        Self {
            base_opts: opts.clone(),
            opts,
            jvm_started: false,
            relaunch: false,
            launched,
            window: None,
            scene: None,
            fly: FlyController::default(),
            grabbed: false,
            last_frame: Instant::now(),
            stats: StatWindow::default(),
            ready_reported: false,
            exit: false,
            mod_host: None,
            cursor: CursorIcon::Default,
            ime: None,
            boot,
            mods_app,
            autopilot: None,
            fullscreen: (false, false),
        }
    }

    /// `Window#setFullscreen`: borderless, or the monitor's own video mode when exclusive.
    fn apply_fullscreen(&mut self, (on, exclusive): (bool, bool)) {
        self.fullscreen = (on, exclusive);
        let Some(window) = &self.window else { return };
        let monitor = window.current_monitor();
        let mode = on.then(|| {
            let video = exclusive
                .then(|| {
                    let m = monitor.as_ref()?;
                    let size = m.size();
                    m.video_modes()
                        .filter(|v| v.size() == size)
                        .max_by_key(|v| v.refresh_rate_millihertz())
                })
                .flatten();
            match video {
                Some(v) => Fullscreen::Exclusive(v),
                None => Fullscreen::Borderless(monitor.clone()),
            }
        });
        window.set_fullscreen(mode);
    }

    fn init(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("mc-rust")
                    .with_inner_size(LogicalSize::new(1280, 720)),
            )?,
        );
        if self.mods_app.is_none() {
            if let Some(path) = &self.opts.autopilot {
                self.autopilot = Some(load_autopilot(path)?);
            }
            return self.show_launcher(window);
        }
        self.start_game(window)
    }

    fn show_launcher(&mut self, window: Arc<Window>) -> Result<()> {
        let size = window.inner_size();
        let mut launcher = launcher::scene::Launcher::new(
            Target::Surface {
                target: window.clone().into(),
                width: size.width,
                height: size.height,
            },
            &self.opts,
        )?;
        launcher.use_system_clipboard();
        self.scene = Some(Scene::Launcher(Box::new(launcher)));
        self.window = Some(window);
        Ok(())
    }

    /// The title screen's Launcher: the same process while no JVM ever started in it, else a new one.
    fn back_to_launcher(&mut self) -> Result<()> {
        if self.jvm_started {
            self.relaunch = true;
            self.exit = true;
            return Ok(());
        }
        if let Some(Scene::Shell(mut shell)) = self.scene.take() {
            self.autopilot = shell.take_autopilot();
        }
        self.set_grab(false);
        self.opts = self.base_opts.clone();
        self.mods_app = None;
        self.boot = None;
        let window = self.window.take().context("no window")?;
        self.show_launcher(window)
    }

    /// Leaves the launcher for the game with the profile Play applied.
    fn leave_launcher(&mut self, plan: mcr_launcher::LaunchPlan) -> Result<()> {
        self.scene = None;
        let resolution = plan.resolution;
        launcher::apply(&mut self.opts, plan)?;
        let started = mods::startup(&self.opts)?;
        self.boot = started.boot;
        self.mods_app = Some(started.app);
        let window = self.window.take().context("no window")?;
        if let Some(r) = resolution {
            let _ = window.request_inner_size(PhysicalSize::new(r.width, r.height));
        }
        let connecting = self.opts.connect.is_some() || self.opts.singleplayer.is_some();
        if let Some(pilot) = &mut self.autopilot
            && connecting
        {
            pilot.prepend(autopilot::Step::WaitReady);
        }
        self.start_game(window)
    }

    fn start_game(&mut self, window: Arc<Window>) -> Result<()> {
        let size = window.inner_size();
        let mut config = world_setup::renderer_config(&self.opts, RendererConfig::default());
        if self.boot.is_some() {
            config.scene_texture = true;
            config.dynamic_lights = true;
            config.optional_features = mcr_modbridge::gpu::context::OPTIONAL_FEATURES;
        }
        let mut renderer = Renderer::new(
            Target::Surface {
                target: window.clone().into(),
                width: size.width,
                height: size.height,
            },
            config,
        )
        .context("creating renderer")?;
        println!(
            "adapter: {} ({:?}), draw-count {}, GPU timestamps {}",
            renderer.adapter_info().name,
            renderer.adapter_info().backend,
            renderer.uses_draw_count(),
            renderer.has_timestamps()
        );
        let mods_app = self.mods_app.clone().context("mod startup missing")?;
        if let Some(plan) = self.boot.take() {
            self.jvm_started = true;
            self.mod_host = plan.start(&self.opts, &mut renderer, &mods_app)?;
        }
        self.scene = Some(if self.opts.world.is_none() && !self.opts.synthetic {
            let mut shell = Shell::new(
                renderer,
                shell_config(
                    &self.opts,
                    self.mod_host.as_ref().map(mods::ModHost::net),
                    mods_app,
                ),
            )?;
            shell.use_system_clipboard();
            Scene::Shell(Box::new(shell))
        } else if self.opts.synthetic {
            self.synthetic_scene(renderer)?
        } else {
            let (assets, source) = world_setup::open(&self.opts)?;
            Scene::World(Box::new(Engine::new(
                renderer,
                assets,
                source,
                self.opts.camera,
                self.opts.render_distance,
            )))
        });
        if let Some(Scene::Shell(shell)) = &mut self.scene {
            if let Some(pilot) = self.autopilot.take() {
                shell.set_autopilot(pilot);
            } else if let Some(path) = &self.opts.autopilot {
                shell.set_autopilot(load_autopilot(path)?);
            }
        } else {
            println!(
                "controls: WASD move, Space/Shift up/down, Ctrl sprint, scroll speed, click grabs mouse, Esc releases, V toggles vsync"
            );
        }
        self.window = Some(window);
        Ok(())
    }

    fn synthetic_scene(&self, mut renderer: Renderer) -> Result<Scene> {
        let start = Instant::now();
        let threads = std::thread::available_parallelism().map_or(4, usize::from);
        for (key, mesh) in generate_world(&Terrain { seed: 7 }, self.opts.radius, threads) {
            renderer
                .set_section(key, &mesh)
                .context("uploading section")?;
        }
        println!(
            "synthetic terrain radius {}: {} sections in {:.2}s",
            self.opts.radius,
            renderer.section_count(),
            start.elapsed().as_secs_f32()
        );
        Ok(Scene::Synthetic(Box::new(SyntheticScene {
            renderer,
            camera: Camera {
                position: DVec3::new(8.5, 130.0, 8.5),
                yaw: 0.6,
                pitch: -0.4,
                ..Camera::default()
            },
        })))
    }

    fn set_grab(&mut self, grab: bool) {
        let Some(window) = &self.window else { return };
        if grab {
            let ok = window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok();
            window.set_cursor_visible(!ok);
            self.grabbed = ok;
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            self.grabbed = false;
        }
    }

    /// Vanilla puts the cursor back in the middle when it releases the mouse.
    fn center_cursor(&self) {
        if let Some(window) = &self.window {
            let size = window.inner_size();
            let centre =
                PhysicalPosition::new(f64::from(size.width) / 2.0, f64::from(size.height) / 2.0);
            let _ = window.set_cursor_position(centre);
        }
    }

    fn apply_java_window(&mut self, java: Option<shell::JavaWindow>) {
        let Some(window) = &self.window else { return };
        let cursor = java.map_or(CursorIcon::Default, |j| mod_input::cursor_icon(j.cursor));
        if cursor != self.cursor {
            window.set_cursor(cursor);
            self.cursor = cursor;
        }
        let ime = java.and_then(|j| j.text_input);
        if ime.is_some() != self.ime.is_some() {
            window.set_ime_allowed(ime.is_some());
        }
        if let Some(Some([x, y, w, h])) = ime
            && ime != self.ime
        {
            window.set_ime_cursor_area(PhysicalPosition::new(x, y), PhysicalSize::new(w, h));
        }
        self.ime = ime;
    }

    fn key(&mut self, code: KeyCode, pressed: bool) {
        match code {
            KeyCode::KeyW => self.fly.forward = pressed,
            KeyCode::KeyS => self.fly.back = pressed,
            KeyCode::KeyA => self.fly.left = pressed,
            KeyCode::KeyD => self.fly.right = pressed,
            KeyCode::Space => self.fly.up = pressed,
            KeyCode::ShiftLeft => self.fly.down = pressed,
            KeyCode::ControlLeft => self.fly.sprint = pressed,
            KeyCode::Escape if pressed => self.set_grab(false),
            KeyCode::KeyV if pressed => {
                self.opts.vsync = !self.opts.vsync;
                if let Some(renderer) = self.scene.as_mut().and_then(Scene::renderer_mut) {
                    renderer.set_vsync(self.opts.vsync);
                }
                println!("vsync {}", if self.opts.vsync { "on" } else { "off" });
            }
            _ => {}
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        let Some(scene) = &mut self.scene else { return };
        if let Scene::Launcher(l) = scene {
            let ticks = l.advance(dt);
            if let Some(pilot) = &mut self.autopilot {
                for _ in 0..ticks {
                    pilot.step_launcher(l);
                }
            }
            l.frame();
            l.shot = None;
            if l.quit {
                self.exit = true;
                return;
            }
            let Some(plan) = l.take_plan() else { return };
            if let Err(e) = self.leave_launcher(plan) {
                eprintln!("startup failed: {e:#}");
                self.exit = true;
            }
            return;
        }
        if let Scene::Shell(shell) = scene {
            shell.advance(dt);
            if TERMINATE.load(std::sync::atomic::Ordering::Relaxed) {
                shell.quit = true;
            }
            if shell.quit {
                self.exit = true;
                return;
            }
            if shell.to_launcher {
                if let Err(e) = self.back_to_launcher() {
                    eprintln!("launcher failed: {e:#}");
                    self.exit = true;
                }
                return;
            }
            let Some(frame) = shell.frame() else { return };
            self.stats.record(dt, &frame);
            if let Some(play) = &mut shell.play {
                let mem = play.render.renderer.memory_stats();
                if let Some(line) = self.stats.report(Some((play.pipeline_stats(), mem))) {
                    let p = play.feet();
                    let turn = play
                        .turn_latency
                        .summary()
                        .map(|t| format!(" | {t}"))
                        .unwrap_or_default();
                    println!("{line} | pos {:.1} {:.1} {:.1}{turn}", p[0], p[1], p[2]);
                    play.turn_latency.reset();
                }
            }
            let want = shell.wants_grab();
            let fullscreen = shell.fullscreen();
            if fullscreen != self.fullscreen {
                self.apply_fullscreen(fullscreen);
            }
            let Some(Scene::Shell(shell)) = &mut self.scene else {
                return;
            };
            let cap = shell.max_fps();
            let vsync = shell.sync_vsync(self.opts.vsync);
            shell.set_mod_uncapped(!vsync && !(10..260).contains(&cap));
            let java = shell.java_window();
            if want != self.grabbed {
                self.set_grab(want);
                if !want && java.is_some() {
                    self.center_cursor();
                }
            }
            self.apply_java_window(java);
            if !vsync && (10..260).contains(&cap) {
                let budget = std::time::Duration::from_secs_f64(1.0 / f64::from(cap));
                if let Some(rest) = budget.checked_sub(now.elapsed()) {
                    std::thread::sleep(rest);
                }
            }
            return;
        }
        let Some(camera) = scene.camera_mut() else {
            return;
        };
        self.fly.update(camera, dt);
        if let Some(speed) = self.opts.autofly {
            let f = camera.forward();
            camera.position += DVec3::new(f64::from(f.x), 0.0, f64::from(f.z)).normalize_or_zero()
                * speed
                * f64::from(dt);
        }

        let frame = match scene {
            Scene::Synthetic(s) => s
                .renderer
                .render(&FrameInputs {
                    camera: &s.camera,
                    sky_light: 1.0,
                })
                .ok(),
            Scene::Shell(_) | Scene::Launcher(_) => None,
            Scene::World(engine) => {
                if let Err(e) = engine.tick() {
                    eprintln!("tick failed: {e:#}");
                }
                if !self.ready_reported && engine.settled() {
                    self.ready_reported = true;
                    let p = engine.pipeline_stats();
                    println!(
                        "world ready: {} columns, {} sections meshed, {:.2}M quads, {:.2}s since launch",
                        engine.game.store.len(),
                        p.meshed,
                        f64::from(p.quads) / 1e6,
                        self.launched.elapsed().as_secs_f32()
                    );
                }
                engine.draw()
            }
        };
        let Some(frame) = frame else { return };
        self.stats.record(dt, &frame);
        let pipeline = match scene {
            Scene::Shell(_) | Scene::Launcher(_) => None,
            Scene::World(engine) => Some((
                engine.pipeline_stats(),
                engine.render.renderer.memory_stats(),
            )),
            Scene::Synthetic(s) => Some((
                render_system::PipelineStats::default(),
                s.renderer.memory_stats(),
            )),
        };
        if let Some(line) = self.stats.report(pipeline)
            && let Some(camera) = scene.camera_mut()
        {
            let p = camera.position;
            println!("{line} | pos {:.1} {:.1} {:.1}", p.x, p.y, p.z);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none()
            && let Err(e) = self.init(event_loop)
        {
            eprintln!("startup failed: {e:#}");
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(scene) = &mut self.scene {
                    scene.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(Scene::Launcher(l)) = &mut self.scene {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        l.key_input(
                            input::key_from_winit(code),
                            event.state == ElementState::Pressed,
                            event.repeat,
                            event.text.as_deref(),
                        );
                    }
                } else if let Some(Scene::Shell(shell)) = &mut self.scene {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        let pressed = event.state == ElementState::Pressed;
                        shell.key_input(
                            input::key_from_winit(code),
                            mod_input::printable(&event),
                            pressed,
                            event.repeat,
                            event.text.as_deref(),
                        );
                    }
                } else if let PhysicalKey::Code(code) = event.physical_key {
                    self.key(code, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::Focused(focused) => {
                if let Some(Scene::Shell(shell)) = &mut self.scene {
                    shell.focus(focused);
                }
            }
            WindowEvent::Ime(ime) => {
                if let (Some(Scene::Launcher(l)), Ime::Commit(text)) = (&mut self.scene, &ime) {
                    l.ui_text(text);
                } else if let Some(Scene::Shell(shell)) = &mut self.scene {
                    match ime {
                        Ime::Commit(text) => shell.ime_commit(&text),
                        Ime::Preedit(text, cursor) => shell.ime_preedit(&text, cursor),
                        Ime::Disabled => shell.ime_disabled(),
                        Ime::Enabled => {}
                    }
                }
            }
            WindowEvent::DroppedFile(path) => {
                if let Some(Scene::Shell(shell)) = &mut self.scene {
                    shell.dropped_file(&path);
                }
            }
            WindowEvent::CursorMoved { position, .. } => match &mut self.scene {
                Some(Scene::Shell(shell)) => shell.cursor_moved(position.x, position.y),
                Some(Scene::Launcher(l)) => l.cursor_moved(position.x, position.y),
                _ => {}
            },
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(Scene::Launcher(l)) = &mut self.scene {
                    l.key_input(
                        input::mouse_from_winit(button),
                        state == ElementState::Pressed,
                        false,
                        None,
                    );
                } else if let Some(Scene::Shell(shell)) = &mut self.scene {
                    if state == ElementState::Pressed && shell.wants_grab() && !self.grabbed {
                        self.set_grab(true);
                    } else if let Some(Scene::Shell(shell)) = &mut self.scene {
                        shell.key_event(
                            input::mouse_from_winit(button),
                            state == ElementState::Pressed,
                            None,
                        );
                    }
                } else if state == ElementState::Pressed && !self.grabbed {
                    self.set_grab(true);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(y),
                    MouseScrollDelta::PixelDelta(p) => p.y / 40.0,
                };
                if let Some(Scene::Launcher(l)) = &mut self.scene {
                    l.scroll(steps as f32);
                } else if let Some(Scene::Shell(shell)) = &mut self.scene {
                    let (dx, dy) = mod_input::wheel(delta);
                    shell.wheel(dx, dy, steps as f32);
                } else {
                    self.fly.scale_speed(steps);
                }
            }
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.grabbed
            && let Some(scene) = &mut self.scene
        {
            if let Scene::Shell(shell) = scene {
                shell.mouse_motion(delta.0, delta.1);
            } else {
                if let Some(camera) = scene.camera_mut() {
                    self.fly.look(camera, delta.0, delta.1);
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.exit {
            event_loop.exit();
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

fn shell_config(
    opts: &Options,
    mod_net: Option<mods::ModNet>,
    mods: mods_ui::ModsApp,
) -> ShellConfig {
    ShellConfig {
        game_dir: opts.game_dir.clone(),
        jar: opts.jar.clone(),
        assets_dir: opts.assets_dir.clone(),
        name: opts.name.clone(),
        online: opts.online.clone(),
        render_distance: opts.render_distance_set.then_some(opts.render_distance),
        connect: opts.connect.clone(),
        singleplayer: opts.singleplayer.clone(),
        mod_net,
        mods,
    }
}

pub(crate) fn load_autopilot(path: &std::path::Path) -> Result<autopilot::Autopilot> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let steps = autopilot::parse(&text).map_err(anyhow::Error::msg)?;
    Ok(autopilot::Autopilot::new(steps))
}

/// SIGTERM arrived: the window quits as if the user closed it, so Java and its caches shut down cleanly.
static TERMINATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn quit_on_sigterm() {
    extern "C" fn on_term(_: libc::c_int) {
        TERMINATE.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
    unsafe {
        libc::signal(libc::SIGTERM, on_term as *const () as libc::sighandler_t);
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn pin_mmap_threshold() {
    // glibc raises it after each large free, so evicted sound buffers would pile up in the heap.
    // SAFETY: only tunes the allocator; no other thread exists yet.
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 256 << 10);
    }
}

/// The detached `--jvm-aot-create <dir> <key>` helper that a recording session starts on exit.
fn aot_helper() -> Option<i32> {
    use mcr_modbridge::aot;
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some(aot::CREATE_ARG) {
        return None;
    }
    let (Some(dir), Some(key)) = (args.next(), args.next()) else {
        return Some(2);
    };
    aot::lower_priority();
    match aot::AotCache::open(dir, &key).and_then(|c| c.create()) {
        Ok(outcome) => {
            eprintln!("{outcome:?}");
            Some(0)
        }
        Err(e) => {
            eprintln!("{e}");
            Some(1)
        }
    }
}

fn main() -> Result<()> {
    if let Some(code) = aot_helper() {
        std::process::exit(code);
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        pin_mmap_threshold();
        quit_on_sigterm();
    }
    #[cfg(feature = "heap-profile")]
    heap_profile::start();
    launcher::remember_start_dir();
    let launched = Instant::now();
    let mut opts = match cli::parse(std::env::args().skip(1)) {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("{e}\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };
    opts.absolutize()?;
    if let Some(name) = opts.profile.clone()
        && let Err(e) = launcher::apply_profile(&mut opts, &name)
    {
        eprintln!("{e:#}");
        std::process::exit(2);
    }
    if let Some(secs) = opts.soak {
        return soak::run(&opts, std::time::Duration::from_secs(secs));
    }
    if let Some(out) = opts.screenshot.clone() {
        if opts.world.is_none() && !opts.synthetic {
            return screenshot::run_shell(&opts, &out, launched);
        }
        return screenshot::run(&opts, &out, launched);
    }
    let started = if opts.shows_launcher() {
        None
    } else {
        Some(mods::startup(&opts)?)
    };
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(opts, launched, started);
    let result = event_loop.run_app(&mut app);
    let needs_exit = app.mod_host.take().is_some_and(mods::ModHost::finish);
    result?;
    if app.relaunch {
        drop(app.window.take());
        return launcher::relaunch(&app.opts);
    }
    if needs_exit {
        std::process::exit(0);
    }
    Ok(())
}
