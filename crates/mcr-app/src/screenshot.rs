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
    if opts.bench_frames > 0 {
        engine.render.renderer.set_animation_tick(0);
    }
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
    if opts.bench_frames > 0 {
        bench(&mut engine, opts, out)?;
    }
    Ok(())
}

/// Renders each view for `--bench` frames and prints wall-clock, CPU and median GPU frame times; with
/// `--bench-modes` every view is measured under each culling configuration in turn.
fn bench(engine: &mut Engine, opts: &crate::cli::Options, out: &Path) -> Result<()> {
    let (width, height) = opts.size;
    let views = if opts.bench_views.is_empty() {
        vec![engine.game.camera]
    } else {
        opts.bench_views.clone()
    };
    let renderer = &engine.render.renderer;
    let current = [(
        "",
        renderer.visibility_graph(),
        renderer.occlusion_culling(),
    )];
    let all = [
        ("-none", false, false),
        ("-graph", true, false),
        ("-hiz", false, true),
        ("-both", true, true),
    ];
    let modes: &[(&str, bool, bool)] = if opts.bench_modes { &all } else { &current };
    let stem = out.file_stem().and_then(|s| s.to_str()).unwrap_or("bench");
    for &(name, graph, hiz) in modes {
        engine.render.renderer.set_visibility_graph(graph);
        engine.render.renderer.set_occlusion_culling(hiz);
        for (i, view) in views.iter().enumerate() {
            engine.game.camera = *view;
            engine.render.sync_camera(&engine.game);
            let mut graph_ms = engine.draw().map_or(0.0, |s| s.graph_ms);
            let pixels = engine.render.renderer.read_offscreen()?;
            let path = out.with_file_name(format!("{stem}-view{i}{name}-first.png"));
            write_png(&path, width, height, &pixels)?;
            for _ in 1..SETTLE_FRAMES {
                graph_ms = graph_ms.max(engine.draw().map_or(0.0, |s| s.graph_ms));
            }
            engine.render.renderer.wait_gpu_stats();
            let mut gpu = Vec::with_capacity(opts.bench_frames as usize);
            let (mut cpu, mut draws) = (0.0, 0);
            let start = Instant::now();
            for _ in 0..opts.bench_frames {
                let stats = engine.draw();
                cpu += stats.map_or(0.0, |s| s.cpu_ms);
                if let Some(g) = stats.and_then(|s| s.gpu) {
                    gpu.push(g.frame_ms);
                    draws = g.cube_draws + g.model_draws;
                }
            }
            engine.render.renderer.wait_gpu_stats();
            let wall = start.elapsed().as_secs_f32() * 1000.0 / opts.bench_frames as f32;
            gpu.sort_by(f32::total_cmp);
            let median = gpu.get(gpu.len() / 2).copied().unwrap_or(0.0);
            println!(
                "view {i}{name} {:.1},{:.1},{:.1} yaw {:.0} pitch {:.0}: {:.0} fps ({wall:.3} ms) | gpu median {median:.3} ms | cpu {:.3} | graph search {graph_ms:.2} ms | draws {draws}",
                view.position.x,
                view.position.y,
                view.position.z,
                view.yaw.to_degrees(),
                view.pitch.to_degrees(),
                1000.0 / wall,
                cpu / opts.bench_frames as f32,
            );
            let pixels = engine.render.renderer.read_offscreen()?;
            let path = out.with_file_name(format!("{stem}-view{i}{name}.png"));
            write_png(&path, width, height, &pixels)?;
        }
    }
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
        if connecting && steps.first() != Some(&crate::autopilot::Step::Early) {
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
