use std::time::Instant;

use mcr_assets::ResourceManager;
use mcr_particles::{
    Fluid, PARTICLE_NAMES, ParticleAssets, ParticleAtlas, ParticleEngine, ParticleWorld,
};
use mcr_protocol::types::particle::{BlockParticleKind, Particle};
use mcr_render::glam::DVec3;
use mcr_render::wgpu;
use mcr_render::{
    Camera, EffectImage, Environment, FrameInputs, Overlay, OverlayCtx, RenderError, Renderer,
    RendererConfig, Target,
};

const W: u32 = 1280;
const H: u32 = 720;

struct Jar(ResourceManager);

impl ParticleAssets for Jar {
    fn block_texture(&self, state: i32) -> Option<(EffectImage, [f32; 3])> {
        let name = if state % 2 == 0 { "stone" } else { "dirt" };
        self.named_texture(&format!("block/{name}"))
            .map(|i| (i, [1.0; 3]))
    }
    fn item_texture(&self, _: i32) -> Option<EffectImage> {
        self.named_texture("item/diamond")
    }
    fn named_texture(&self, path: &str) -> Option<EffectImage> {
        let bytes = self
            .0
            .read(&format!("assets/minecraft/textures/{path}.png"))
            .ok()??;
        EffectImage::decode(&bytes)
    }
}

struct World;

impl ParticleWorld for World {
    fn fluid(&self, _: i32, y: i32, _: i32) -> Option<(Fluid, f32)> {
        (y >= 12).then_some((Fluid::Water, 1.0))
    }
}

struct Draw<'a>(&'a mut ParticleEngine, Environment);

impl Overlay for Draw<'_> {
    fn record(&mut self, ctx: &mut OverlayCtx<'_>) {
        self.0.record(ctx, 0.5, &self.1);
    }
}

fn jar() -> Option<ResourceManager> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache/versions/26.3/client.jar");
    ResourceManager::vanilla(path).ok()
}

fn renderer() -> Option<Renderer> {
    match Renderer::new(
        Target::Offscreen {
            width: W,
            height: H,
        },
        RendererConfig::default(),
    ) {
        Ok(r) => Some(r),
        Err(RenderError::NoAdapter(why)) => {
            eprintln!("skipping GPU test: {why}");
            None
        }
        Err(e) => panic!("renderer creation failed: {e}"),
    }
}

fn engine(resources: ResourceManager) -> ParticleEngine {
    let atlas = ParticleAtlas::load(&|p| resources.read(p).ok().flatten());
    ParticleEngine::new(atlas, Box::new(Jar(resources)))
}

fn save(name: &str, rgba: &[u8]) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/effects");
    std::fs::create_dir_all(&dir).unwrap();
    let file = std::fs::File::create(dir.join(name)).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn frame(r: &mut Renderer, e: &mut ParticleEngine, env: &Environment, cam: &Camera) {
    r.set_environment(env);
    r.render_with_overlay(
        &FrameInputs {
            camera: cam,
            sky_light: 1.0,
        },
        Some(&mut Draw(e, env.clone())),
    )
    .expect("render");
}

const GRID: &[&str] = &[
    "block",
    "smoke",
    "flame",
    "explosion",
    "explosion_emitter",
    "crit",
    "enchanted_hit",
    "heart",
    "angry_villager",
    "happy_villager",
    "note",
    "portal",
    "dust",
    "dust_color_transition",
    "item",
    "splash",
    "rain",
    "bubble",
    "bubble_pop",
    "dripping_lava",
    "dripping_water",
    "poof",
    "cloud",
    "campfire_cosy_smoke",
    "soul_fire_flame",
    "sweep_attack",
    "damage_indicator",
    "totem_of_undying",
    "end_rod",
    "witch",
    "effect",
    "entity_effect",
    "firework",
    "large_smoke",
    "lava",
    "snowflake",
    "ash",
    "glow",
    "electric_spark",
    "falling_water",
];

fn particle(name: &str) -> Particle {
    let id = PARTICLE_NAMES.iter().position(|n| *n == name).unwrap() as i32;
    match name {
        "block" => Particle::BlockState {
            kind: BlockParticleKind::Block,
            state: 2,
        },
        "dust" => Particle::Dust {
            color: 0xff3030,
            scale: 2.0,
        },
        "dust_color_transition" => Particle::DustColorTransition {
            from: 0xff0000,
            to: 0x0000ff,
            scale: 2.0,
        },
        "effect" | "entity_effect" => Particle::Spell {
            kind: mcr_protocol::types::particle::SpellParticleKind::Effect,
            color: 0xff40a0f0u32 as i32,
            power: 1.0,
        },
        "item" => Particle::Item(mcr_protocol::types::item::ItemStackTemplate {
            item: 1,
            count: 1,
            components: Default::default(),
        }),
        _ => Particle::Simple(id),
    }
}

#[test]
fn particle_showcase() {
    let (Some(mut r), Some(resources)) = (renderer(), jar()) else {
        return;
    };
    let mut e = engine(resources);
    let env = Environment::overworld(6000, 0.0, 0.0);
    let cam = Camera {
        position: DVec3::new(0.0, 8.0, 14.0),
        pitch: 0.0,
        ..Camera::default()
    };
    let cols = 8;
    for (i, name) in GRID.iter().enumerate() {
        let (cx, cy) = ((i % cols) as f64 - 3.5, (i / cols) as f64);
        let at = [cx * 2.6, 12.0 + 3.0 * (2.0 - cy) + 0.5 * 8.0 - 8.0, 0.0];
        let p = particle(name);
        match *name {
            "explosion" => e.emit(&p, at, [1.0, 0.0, 0.0], 1.0, 0),
            "explosion_emitter" => e.emit(&p, at, [0.0; 3], 0.0, 0),
            "sweep_attack" => e.emit(&p, at, [0.5, 0.0, 0.0], 1.0, 0),
            "note" => e.emit(&p, at, [0.4, 0.0, 0.0], 1.0, 0),
            _ => e.emit(&p, at, [0.25; 3], 0.02, 8),
        }
    }
    for _ in 0..4 {
        e.tick(&World);
    }
    frame(&mut r, &mut e, &env, &cam);
    let px = r.read_offscreen().unwrap();
    save("particles_grid.png", &px);
    assert!(e.count() > 40);

    let bright = Environment::overworld(6000, 0.0, 0.0);
    let mut e2 = engine(jar().unwrap());
    for ix in 0..2 {
        e2.block_break([ix * 3, 10, 0], ix + 1, &World);
    }
    e2.block_crack([-3, 10, 0], 1, 2, &World);
    for _ in 0..2 {
        e2.tick(&World);
    }
    let cam2 = Camera {
        position: DVec3::new(2.0, 11.0, 6.0),
        ..Camera::default()
    };
    frame(&mut r, &mut e2, &bright, &cam2);
    save("particles_block_break.png", &r.read_offscreen().unwrap());
}

#[test]
fn ten_thousand_particles_update_and_render() {
    let (Some(mut r), Some(resources)) = (renderer(), jar()) else {
        return;
    };
    let mut e = engine(resources);
    let env = Environment::overworld(6000, 0.0, 0.0);
    let cam = Camera {
        position: DVec3::new(0.0, 10.0, 12.0),
        ..Camera::default()
    };
    let names = [
        "smoke", "flame", "crit", "poof", "cloud", "dust", "end_rod", "heart",
    ];
    for round in 0..40 {
        for name in names {
            e.emit(&particle(name), [0.0, 10.0, 0.0], [3.0; 3], 0.05, 40);
        }
        if round % 10 == 0 {
            e.tick(&World);
        }
    }
    let live = e.count();
    assert!(live > 10_000 - 3_000, "only {live} particles");
    let start = Instant::now();
    let ticks = 5;
    for _ in 0..ticks {
        e.tick(&World);
    }
    let tick_ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(ticks);
    let start = Instant::now();
    let frames = 20;
    for _ in 0..frames {
        frame(&mut r, &mut e, &env, &cam);
    }
    let _ = r.device().poll(wgpu::PollType::wait_indefinitely());
    let frame_ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
    println!("{live} particles: update {tick_ms:.3} ms/tick, record+submit {frame_ms:.3} ms/frame");
    assert!(tick_ms < 25.0, "update too slow: {tick_ms}");
    save("particles_10k.png", &r.read_offscreen().unwrap());
}
