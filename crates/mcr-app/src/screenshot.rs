use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use mcr_render::{Renderer, RendererConfig, Target};

use crate::engine::Engine;
use crate::world_setup;

const SETTLE_FRAMES: u32 = 4;
const TIMEOUT: Duration = Duration::from_secs(900);

pub(crate) fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}

pub fn run(opts: &crate::cli::Options, out: &Path, launched: Instant) -> Result<()> {
    let (width, height) = opts.size;
    let renderer = Renderer::new(
        Target::Offscreen { width, height },
        world_setup::renderer_config(opts, RendererConfig::default()),
    )
    .context("creating renderer")?;
    let (assets, source) = world_setup::open(opts)?;
    let streaming = Instant::now();
    let mut engine = Engine::new(renderer, assets, source, opts.camera, opts.render_distance);
    loop {
        engine.tick()?;
        engine.draw();
        if engine.settled() {
            break;
        }
        if streaming.elapsed() > TIMEOUT {
            bail!("world did not settle within {TIMEOUT:?}");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let p = engine.pipeline_stats();
    println!(
        "settled: {} columns, {} sections meshed, {:.2}M quads, {:.2}s streaming ({:.2}s since launch), peak rss {} MB, gpu {} MB",
        engine.game.store.len(),
        p.meshed,
        f64::from(p.quads) / 1e6,
        streaming.elapsed().as_secs_f32(),
        launched.elapsed().as_secs_f32(),
        world_setup::peak_rss_mb(),
        engine.render.renderer.memory_stats().total_bytes() >> 20,
    );
    for _ in 0..SETTLE_FRAMES {
        engine.draw();
    }
    let pixels = engine.render.renderer.read_offscreen()?;
    write_png(out, width, height, &pixels)?;
    println!("wrote {}", out.display());
    Ok(())
}

pub fn run_shell(opts: &crate::cli::Options, out: &Path) -> Result<()> {
    use crate::shell::{Shell, ShellConfig};
    let (width, height) = opts.size;
    let renderer = Renderer::new(
        Target::Offscreen { width, height },
        world_setup::renderer_config(opts, RendererConfig::default()),
    )
    .context("creating renderer")?;
    let config = ShellConfig {
        game_dir: opts.game_dir.clone(),
        jar: opts.jar.clone(),
        assets_dir: opts.assets_dir.clone(),
        name: opts.name.clone(),
        render_distance: opts.render_distance_set.then_some(opts.render_distance),
        connect: opts.connect.clone(),
    };
    let mut shell = Shell::new(renderer, config)?;
    let connecting = opts.connect.is_some();
    let mut scripted = false;
    if let Some(path) = &opts.autopilot {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut steps = crate::autopilot::parse(&text).map_err(anyhow::Error::msg)?;
        if connecting {
            steps.insert(0, crate::autopilot::Step::WaitReady);
        }
        shell.set_autopilot(crate::autopilot::Autopilot::new(steps));
        scripted = true;
    }
    let started = Instant::now();
    let mut last = Instant::now();
    let mut ready_ticks: Option<u64> = None;
    let mut frames = 0u32;
    let mut frame_times: Vec<f32> = Vec::new();
    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32();
        last = now;
        shell.advance(dt);
        if ready_ticks.is_none()
            && shell.ready()
            && let Some(play) = &shell.play
        {
            ready_ticks = Some(play.ticks);
            println!(
                "ready: {} ticks, {:.2}s since start",
                play.ticks,
                started.elapsed().as_secs_f32()
            );
        }
        let t0 = Instant::now();
        shell.frame();
        frames += 1;
        if ready_ticks.is_some() {
            frame_times.push(t0.elapsed().as_secs_f32() * 1000.0);
        }
        if let Some(path) = shell.shot.take() {
            let pixels = shell.read_offscreen()?;
            write_png(&path, width, height, &pixels)?;
            println!("wrote {}", path.display());
        }
        if connecting && shell.play.is_none() && shell.failure.is_some() {
            bail!("disconnected: {}", shell.failure.take().unwrap_or_default());
        }
        let done = if connecting {
            match (ready_ticks, shell.play.as_ref()) {
                (Some(start), Some(play)) => match opts.after_ticks {
                    Some(n) => play.ticks >= start + u64::from(n),
                    None => !scripted || shell.script_finished(),
                },
                _ => false,
            }
        } else {
            frames > 30 && (!scripted || shell.script_finished())
        };
        if done || shell.quit {
            break;
        }
        if started.elapsed() > TIMEOUT {
            bail!("timed out");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..3 {
        shell.frame();
    }
    let pixels = shell.read_offscreen()?;
    write_png(out, width, height, &pixels)?;
    println!("wrote {}", out.display());
    if !frame_times.is_empty() {
        frame_times.sort_by(f32::total_cmp);
        let n = frame_times.len();
        let mean = frame_times.iter().sum::<f32>() / n as f32;
        println!(
            "frame time ms over {n} frames: mean {mean:.2} p50 {:.2} p99 {:.2} max {:.2}",
            frame_times[n / 2],
            frame_times[(n * 99 / 100).min(n - 1)],
            frame_times[n - 1]
        );
    }
    if let Some(play) = &shell.play {
        println!(
            "final: pos {:?} teleports {} chunks {} peak rss {} MB",
            play.feet(),
            play.world.client.state.player.teleports_received,
            play.world.client.state.chunks.len(),
            world_setup::peak_rss_mb()
        );
    }
    Ok(())
}
