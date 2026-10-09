mod fly;

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use fly::FlyController;
use mcr_render::glam::DVec3;
use mcr_render::synthetic::{Terrain, generate_world};
use mcr_render::{Camera, FrameInputs, Renderer, RendererConfig, Target};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

struct Options {
    vsync: bool,
    radius: i32,
}

fn parse_args() -> Options {
    let mut opts = Options {
        vsync: true,
        radius: 24,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--no-vsync" => opts.vsync = false,
            "--radius" => {
                opts.radius = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(opts.radius);
            }
            other => eprintln!("ignoring unknown argument {other}"),
        }
    }
    opts
}

#[derive(Debug, Default)]
struct StatWindow {
    frames: u32,
    frame_ms: f32,
    cpu_ms: f32,
    gpu_draw_ms: f32,
    gpu_cull_ms: f32,
    gpu_samples: u32,
    draws: u32,
}

struct App {
    opts: Options,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    fly: FlyController,
    grabbed: bool,
    last_frame: Instant,
    last_report: Instant,
    stats: StatWindow,
}

impl App {
    fn new(opts: Options) -> Self {
        Self {
            opts,
            window: None,
            renderer: None,
            camera: Camera {
                position: DVec3::new(8.5, 130.0, 8.5),
                yaw: 0.6,
                pitch: -0.4,
                ..Camera::default()
            },
            fly: FlyController::default(),
            grabbed: false,
            last_frame: Instant::now(),
            last_report: Instant::now(),
            stats: StatWindow::default(),
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
        let mut renderer = Renderer::new(
            Target::Surface {
                target: window.clone().into(),
                width: size.width,
                height: size.height,
            },
            RendererConfig {
                vsync: self.opts.vsync,
                ..RendererConfig::default()
            },
        )
        .context("creating renderer")?;
        println!(
            "adapter: {} ({:?}), draw-count {}, GPU timestamps {}",
            renderer.adapter_info().name,
            renderer.adapter_info().backend,
            renderer.uses_draw_count(),
            renderer.has_timestamps()
        );

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
        println!(
            "controls: WASD move, Space/Shift up/down, Ctrl sprint, scroll speed, click grabs mouse, Esc releases, V toggles vsync"
        );
        self.window = Some(window);
        self.renderer = Some(renderer);
        Ok(())
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
                if let Some(r) = &mut self.renderer {
                    r.set_vsync(self.opts.vsync);
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
        self.fly.update(&mut self.camera, dt);

        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let Ok(s) = renderer.render(&FrameInputs {
            camera: &self.camera,
            sky_light: 1.0,
        }) else {
            return;
        };
        let st = &mut self.stats;
        st.frames += 1;
        st.frame_ms += dt * 1000.0;
        st.cpu_ms += s.cpu_ms;
        if let Some(g) = s.gpu {
            st.gpu_cull_ms += g.cull_ms;
            st.gpu_draw_ms += g.draw_ms;
            st.gpu_samples += 1;
            st.draws = g.cube_draws + g.model_draws;
        }

        if self.last_report.elapsed() >= Duration::from_secs(1) && st.frames > 0 {
            let n = st.frames as f32;
            let g = st.gpu_samples.max(1) as f32;
            let p = self.camera.position;
            println!(
                "{:.0} fps | frame {:.2} ms | cpu {:.2} ms | gpu cull {:.3} ms draw {:.2} ms | draws {} | pos {:.1} {:.1} {:.1}",
                1000.0 / (st.frame_ms / n),
                st.frame_ms / n,
                st.cpu_ms / n,
                st.gpu_cull_ms / g,
                st.gpu_draw_ms / g,
                st.draws,
                p.x,
                p.y,
                p.z
            );
            *st = StatWindow::default();
            self.last_report = Instant::now();
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
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.key(code, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } if !self.grabbed => self.set_grab(true),
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(y),
                    MouseScrollDelta::PixelDelta(p) => p.y / 40.0,
                };
                self.fly.scale_speed(steps);
            }
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.grabbed
        {
            self.fly.look(&mut self.camera, delta.0, delta.1);
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

fn main() -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut App::new(parse_args()))?;
    Ok(())
}
