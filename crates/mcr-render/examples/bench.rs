//! `cargo run --release -p mcr-render --example bench -- [radius] [frames] [width] [height]`

use std::time::Instant;

use mcr_render::glam::DVec3;
use mcr_render::synthetic::{Terrain, generate_world};
use mcr_render::{Camera, FrameInputs, Renderer, RendererConfig, Target};

fn main() {
    let args: Vec<u32> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let radius = args.first().copied().unwrap_or(48) as i32;
    let frames = args.get(1).copied().unwrap_or(600);
    let (w, h) = (
        args.get(2).copied().unwrap_or(1920),
        args.get(3).copied().unwrap_or(1080),
    );

    let mut r = Renderer::new(
        Target::Offscreen {
            width: w,
            height: h,
        },
        RendererConfig {
            cube_arena_quads: 24 << 20,
            ..RendererConfig::default()
        },
    )
    .expect("renderer");
    println!(
        "adapter {} ({:?}), draw-count {}, timestamps {}",
        r.adapter_info().name,
        r.adapter_info().backend,
        r.uses_draw_count(),
        r.has_timestamps()
    );

    let threads = std::thread::available_parallelism().map_or(4, usize::from);
    let t = Instant::now();
    let world = generate_world(&Terrain { seed: 7 }, radius, threads);
    println!(
        "meshed {} sections in {:.2}s on {threads} threads",
        world.len(),
        t.elapsed().as_secs_f32()
    );

    let t = Instant::now();
    for (key, mesh) in &world {
        r.set_section(*key, mesh).expect("fits");
    }
    let queue_ms = t.elapsed().as_secs_f32() * 1000.0;
    let cam = Camera {
        position: DVec3::new(8.5, 118.0, 8.5),
        pitch: -0.2,
        ..Camera::default()
    };
    let t = Instant::now();
    let mut upload_frames = 0;
    while r.pending_upload_bytes() > 0 || upload_frames == 0 {
        r.render(&FrameInputs {
            camera: &cam,
            sky_light: 1.0,
        })
        .unwrap();
        upload_frames += 1;
    }
    r.wait_gpu_stats();
    println!(
        "uploaded {} cube quads ({:.1} MB) + {} model quads: queue {queue_ms:.1} ms, flush {:.1} ms over {upload_frames} frames",
        r.cube_quads_used(),
        f64::from(r.cube_quads_used()) * 8.0 / 1e6,
        r.model_quads_used(),
        t.elapsed().as_secs_f32() * 1000.0
    );

    let (mut cpu, mut cull, mut draw, mut n_gpu) = (0.0f32, 0.0f32, 0.0f32, 0u32);
    let (mut cube_draws, mut model_draws) = (0, 0);
    let t = Instant::now();
    for i in 0..frames {
        let cam = Camera {
            yaw: i as f32 / frames as f32 * std::f32::consts::TAU,
            ..cam
        };
        let s = r
            .render(&FrameInputs {
                camera: &cam,
                sky_light: 1.0,
            })
            .unwrap();
        cpu += s.cpu_ms;
        if let Some(g) = s.gpu {
            cull += g.cull_ms;
            draw += g.draw_ms;
            n_gpu += 1;
            cube_draws = g.cube_draws;
            model_draws = g.model_draws;
        }
    }
    r.wait_gpu_stats();
    let wall = t.elapsed().as_secs_f32() * 1000.0 / frames as f32;
    let n = n_gpu.max(1) as f32;
    println!("{w}x{h}, radius {radius}, {frames} frames");
    println!(
        "frame time (wall, GPU-bound, no vsync): {wall:.3} ms ({:.0} fps)",
        1000.0 / wall
    );
    println!("CPU render() submission: {:.3} ms avg", cpu / frames as f32);
    println!(
        "GPU: cull {:.3} ms, draw {:.3} ms avg; last frame visible draws cube {cube_draws} model {model_draws}",
        cull / n,
        draw / n
    );
    let m = r.memory_stats();
    let mb = |b: u64| b as f64 / (1 << 20) as f64;
    println!(
        "GPU memory (ours): arenas {:.1}+{:.1} MB (used {:.1}+{:.1}), culling {:.1} MB, textures {:.2} MB, target {:.1} MB, total {:.1} MB",
        mb(m.cube_arena_bytes),
        mb(m.model_arena_bytes),
        mb(m.cube_used_bytes),
        mb(m.model_used_bytes),
        mb(m.culling_bytes),
        mb(m.texture_bytes),
        mb(m.target_bytes),
        mb(m.total_bytes())
    );
    if let Some(report) = r.allocator_report() {
        println!(
            "wgpu allocator: {:.1} MB allocated, {:.1} MB reserved",
            mb(report.total_allocated_bytes),
            mb(report.total_reserved_bytes)
        );
    }
}
