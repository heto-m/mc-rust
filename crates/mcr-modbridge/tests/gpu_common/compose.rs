//! An offscreen mcr renderer composing Java's GUI, driven like mcr's frame loop.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use mcr_modbridge::frame::LinkState;
use mcr_modbridge::{Bridge, BridgeConfig, BridgeEvent, ComposeStats, ModFrames, TickClock};
use mcr_render::glam::DVec3;
use mcr_render::{Camera, FrameInputs, RenderError, Renderer, RendererConfig, Target};

use super::DEADLINE;

const FRAME: Duration = Duration::from_micros(16_667);

pub struct Rig {
    pub renderer: Renderer,
    pub frames: ModFrames,
    pub bridge: Bridge,
    pub camera: Camera,
    pub infos: HashMap<String, String>,
    pub screen: String,
    pub frame_count: u64,
    /// Why the bridge stopped composing, if it did.
    pub detached: Option<String>,
    pub resumed: bool,
    clock: TickClock,
    last: Instant,
    feed_ticks: bool,
}

impl Rig {
    /// `None` without a GPU adapter. The window size is `cfg`'s.
    pub fn start(mut cfg: BridgeConfig) -> Option<Self> {
        let (width, height) = (cfg.width, cfg.height);
        let renderer = match Renderer::new(
            Target::Offscreen { width, height },
            RendererConfig {
                scene_texture: true,
                optional_features: mcr_modbridge::gpu::context::OPTIONAL_FEATURES,
                ..RendererConfig::default()
            },
        ) {
            Ok(r) => r,
            Err(RenderError::NoAdapter(why)) => {
                eprintln!("skipped: no GPU adapter ({why})");
                return None;
            }
            Err(e) => panic!("renderer: {e}"),
        };
        mcr_modbridge::gpu::install(
            renderer.device().clone(),
            renderer.queue().clone(),
            renderer.adapter_info().clone(),
        )
        .expect("install the shared device");
        cfg.compose = true;
        let bridge = Bridge::start(cfg).expect("bridge start");
        let frames = bridge.frames().expect("compose mode has a frame link");
        let mut renderer = renderer;
        renderer.set_external(Some(Box::new(frames.clone())));
        Some(Self {
            renderer,
            frames,
            bridge,
            camera: Camera {
                position: DVec3::new(0.5, 80.0, 0.5),
                ..Camera::default()
            },
            infos: HashMap::new(),
            screen: String::new(),
            frame_count: 0,
            detached: None,
            resumed: false,
            clock: TickClock::default(),
            last: Instant::now(),
            feed_ticks: true,
        })
    }

    /// Stops feeding ticks, so Java's count can settle on Rust's.
    pub fn stop_ticks(&mut self) {
        self.feed_ticks = false;
    }

    /// One mcr frame at about 60 Hz: ticks, render with Java's GUI composed, events. Returns the
    /// ticks Rust ran (and Java will run) for this frame.
    pub fn frame(&mut self) -> u32 {
        let now = Instant::now();
        let (ticks, partial) = self.clock.advance(now - self.last);
        self.last = now;
        if self.feed_ticks {
            self.frames.lock().add_ticks(ticks, partial);
        }
        self.renderer
            .render(&FrameInputs {
                camera: &self.camera,
                sky_light: 1.0,
            })
            .expect("render");
        self.frame_count += 1;
        while let Some(event) = self.bridge.try_event() {
            match event {
                BridgeEvent::Crash { report } => panic!("java crashed: {report:?}"),
                BridgeEvent::VmExited { code } => panic!("java exited with {code}"),
                BridgeEvent::RenderDetached { reason } => self.detached = Some(reason),
                BridgeEvent::RenderResumed => self.resumed = true,
                BridgeEvent::Screen(s) => self.screen = s,
                BridgeEvent::Info { key, value } => {
                    self.infos.insert(key, value);
                }
                _ => {}
            }
        }
        let spent = now.elapsed();
        if spent < FRAME {
            std::thread::sleep(FRAME - spent);
        }
        if self.feed_ticks { ticks } else { 0 }
    }

    /// Renders until `done` holds, failing after the shared deadline.
    pub fn until(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let start = Instant::now();
        while !done(self) {
            assert!(start.elapsed() < DEADLINE, "timed out waiting for {what}");
            self.frame();
        }
    }

    pub fn run_for(&mut self, d: Duration) {
        let start = Instant::now();
        while start.elapsed() < d {
            self.frame();
        }
    }

    /// Vanilla's automatic GUI scale for the current size.
    pub fn gui_scale(&self) -> u32 {
        let (w, h) = self.renderer.size();
        let mut s = 1;
        while s < w && s < h && w / (s + 1) >= 320 && h / (s + 1) >= 240 {
            s += 1;
        }
        s
    }

    pub fn loaded(&self) -> bool {
        let link = self.frames.lock();
        link.state() == LinkState::Running && link.java().loaded && link.gui_active()
    }

    pub fn stats(&self) -> ComposeStats {
        self.frames.lock().stats()
    }

    pub fn pixels(&self) -> Rgba {
        let (w, h) = self.renderer.size();
        Rgba {
            width: w,
            height: h,
            pixels: self.renderer.read_offscreen().expect("readback"),
        }
    }
}

pub use super::image::Rgba;

/// A widget from Java's `screen_layout` report, in pixels (top-left origin).
#[derive(Debug, Clone, Copy)]
pub struct Widget {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// `layout` is `scale|x,y,w,h,label|...` in GUI units.
pub fn widget(layout: &str, label: &str) -> Option<Widget> {
    let mut parts = layout.split('|');
    let scale: u32 = parts.next()?.parse().ok()?;
    parts.find_map(|p| {
        let f: Vec<&str> = p.splitn(5, ',').collect();
        (f.len() == 5 && f[4] == label).then(|| Widget {
            x: f[0].parse::<u32>().unwrap_or(0) * scale,
            y: f[1].parse::<u32>().unwrap_or(0) * scale,
            w: f[2].parse::<u32>().unwrap_or(0) * scale,
            h: f[3].parse::<u32>().unwrap_or(0) * scale,
        })
    })
}

/// Near-white pixels (button text) in `r` and the mean x of them.
pub fn text_pixels(img: &Rgba, r: Widget) -> (u32, f64) {
    let (mut n, mut sum) = (0u32, 0u64);
    for y in r.y..(r.y + r.h).min(img.height) {
        for x in r.x..(r.x + r.w).min(img.width) {
            let i = ((y * img.width + x) * 4) as usize;
            if img.pixels[i..i + 3].iter().all(|&c| c >= 0xE0) {
                n += 1;
                sum += u64::from(x);
            }
        }
    }
    (
        n,
        if n == 0 {
            0.0
        } else {
            sum as f64 / f64::from(n)
        },
    )
}

/// Pixels that differ from `base` by more than a little.
pub fn changed(a: &Rgba, b: &Rgba, r: Widget) -> u32 {
    let mut n = 0;
    for y in r.y..(r.y + r.h).min(a.height) {
        for x in r.x..(r.x + r.w).min(a.width) {
            let i = ((y * a.width + x) * 4) as usize;
            if (0..3).any(|c| a.pixels[i + c].abs_diff(b.pixels[i + c]) > 24) {
                n += 1;
            }
        }
    }
    n
}
