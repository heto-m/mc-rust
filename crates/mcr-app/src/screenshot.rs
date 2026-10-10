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

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
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
