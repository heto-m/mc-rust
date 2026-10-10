mod assets;
mod audio;
mod autopilot;
mod cli;
mod debug;
mod effects;
mod engine;
mod entities;
mod fly;
mod game;
mod gui;
mod hand;
mod hud;
mod input;
mod mesh_world;
mod nametags;
mod net_world;
mod outline;
mod play;
mod preview;
mod render_system;
mod screenshot;
mod servers;
mod settings;
mod shell;
mod source;
mod stats;
mod status;
mod target;
mod view;
mod world_setup;

use std::sync::Arc;
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
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

struct SyntheticScene {
    renderer: Renderer,
    camera: Camera,
}

enum Scene {
    Synthetic(Box<SyntheticScene>),
    World(Box<Engine>),
    Shell(Box<Shell>),
}

impl Scene {
    fn camera_mut(&mut self) -> &mut Camera {
        match self {
            Self::Synthetic(s) => &mut s.camera,
            Self::World(engine) => &mut engine.game.camera,
            Self::Shell(shell) => shell.camera_mut(),
        }
    }

    fn renderer_mut(&mut self) -> &mut Renderer {
        match self {
            Self::Synthetic(s) => &mut s.renderer,
            Self::World(engine) => &mut engine.render.renderer,
            Self::Shell(shell) => shell.renderer_mut().expect("renderer"),
        }
    }
}

struct App {
    opts: Options,
    launched: Instant,
    window: Option<Arc<Window>>,
    scene: Option<Scene>,
    fly: FlyController,
    grabbed: bool,
    last_frame: Instant,
    stats: StatWindow,
    ready_reported: bool,
    exit: bool,
}

impl App {
    fn new(opts: Options, launched: Instant) -> Self {
        Self {
            opts,
            launched,
            window: None,
            scene: None,
            fly: FlyController::default(),
            grabbed: false,
            last_frame: Instant::now(),
            stats: StatWindow::default(),
            ready_reported: false,
            exit: false,
        }
    }

    fn init(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("mc-rust")
                    .with_inner_size(LogicalSize::new(1280, 720)),
            )?,
        );
        let size = window.inner_size();
        let config = world_setup::renderer_config(&self.opts, RendererConfig::default());
        let renderer = Renderer::new(
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
        self.scene = Some(if self.opts.world.is_none() && !self.opts.synthetic {
            Scene::Shell(Box::new(Shell::new(renderer, shell_config(&self.opts))?))
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
            if let Some(path) = &self.opts.autopilot {
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
                if let Some(scene) = &mut self.scene {
                    scene.renderer_mut().set_vsync(self.opts.vsync);
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
        if let Scene::Shell(shell) = scene {
            shell.advance(dt);
            if shell.quit {
                self.exit = true;
                return;
            }
            let Some(frame) = shell.frame() else { return };
            self.stats.record(dt, &frame);
            if let Some(play) = &shell.play {
                let mem = play.render.renderer.memory_stats();
                if let Some(line) = self.stats.report(Some((play.pipeline_stats(), mem))) {
                    let p = play.feet();
                    println!("{line} | pos {:.1} {:.1} {:.1}", p[0], p[1], p[2]);
                }
            }
            let want = shell.wants_grab();
            let cap = shell.max_fps();
            if want != self.grabbed {
                self.set_grab(want);
            }
            if !self.opts.vsync && (10..260).contains(&cap) {
                let budget = std::time::Duration::from_secs_f64(1.0 / f64::from(cap));
                if let Some(rest) = budget.checked_sub(now.elapsed()) {
                    std::thread::sleep(rest);
                }
            }
            return;
        }
        self.fly.update(scene.camera_mut(), dt);
        if let Some(speed) = self.opts.autofly {
            let camera = scene.camera_mut();
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
            Scene::Shell(_) => None,
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
            Scene::Shell(_) => None,
            Scene::World(engine) => Some((
                engine.pipeline_stats(),
                engine.render.renderer.memory_stats(),
            )),
            Scene::Synthetic(s) => Some((
                render_system::PipelineStats::default(),
                s.renderer.memory_stats(),
            )),
        };
        if let Some(line) = self.stats.report(pipeline) {
            let p = scene.camera_mut().position;
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
                    scene.renderer_mut().resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(Scene::Shell(shell)) = &mut self.scene {
                    if let PhysicalKey::Code(code) = event.physical_key
                        && (!event.repeat || shell.accepts_repeat())
                    {
                        let pressed = event.state == ElementState::Pressed;
                        let text = event.text.as_deref();
                        shell.key_event(input::key_from_winit(code), pressed, text);
                    }
                } else if let PhysicalKey::Code(code) = event.physical_key {
                    self.key(code, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::Focused(false) => {
                if let Some(Scene::Shell(shell)) = &mut self.scene {
                    shell.lose_focus();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(Scene::Shell(shell)) = &mut self.scene {
                    shell.cursor_moved(position.x, position.y);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(Scene::Shell(shell)) = &mut self.scene {
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
                if let Some(Scene::Shell(shell)) = &mut self.scene {
                    shell.scroll(steps as f32);
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
                self.fly.look(scene.camera_mut(), delta.0, delta.1);
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

fn shell_config(opts: &Options) -> ShellConfig {
    ShellConfig {
        game_dir: opts.game_dir.clone(),
        jar: opts.jar.clone(),
        assets_dir: opts.assets_dir.clone(),
        name: opts.name.clone(),
        render_distance: opts.render_distance_set.then_some(opts.render_distance),
        connect: opts.connect.clone(),
    }
}

pub(crate) fn load_autopilot(path: &std::path::Path) -> Result<autopilot::Autopilot> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let steps = autopilot::parse(&text).map_err(anyhow::Error::msg)?;
    Ok(autopilot::Autopilot::new(steps))
}

fn main() -> Result<()> {
    let launched = Instant::now();
    let opts = match cli::parse(std::env::args().skip(1)) {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("{e}\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };
    if let Some(out) = opts.screenshot.clone() {
        if opts.world.is_none() && !opts.synthetic {
            return screenshot::run_shell(&opts, &out);
        }
        return screenshot::run(&opts, &out, launched);
    }
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut App::new(opts, launched))?;
    Ok(())
}
