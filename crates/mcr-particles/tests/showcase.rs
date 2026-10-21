use std::time::Instant;

use mcr_assets::ResourceManager;
use mcr_particles::{
    Fluid, PARTICLE_NAMES, ParticleAssets, ParticleAtlas, ParticleEngine, ParticleWorld,
};
use mcr_protocol::types::particle::{BlockParticleKind, Particle};
use mcr_render::glam::DVec3;
use mcr_render::wgpu;
use mcr_render::{
    Camera, EffectImage, Environment, Face, FrameInputs, LIGHT_FULL_SKY, ModelQuad, ModelQuadDesc,
    OitStage, Overlay, OverlayCtx, RenderError, Renderer, RendererConfig, SectionMesh, Target,
    TextureGroupData,
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
    fn prepare_oit(&mut self, ctx: &mut OverlayCtx<'_>) -> bool {
        self.0.prepare_oit(ctx, 0.5, &self.1)
    }

    fn draw_oit(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        stage: OitStage,
        oit: &wgpu::BindGroup,
    ) {
        self.0.draw_oit(pass, stage, oit);
    }

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

const NEW_GRID: &[&str] = &[
    "cherry_leaves",
    "pale_oak_leaves",
    "tinted_leaves",
    "red_poplar_leaves",
    "snowflake",
    "white_ash",
    "crimson_spore",
    "warped_spore",
    "soul",
    "sculk_soul",
    "sculk_charge_pop",
    "wax_on",
    "wax_off",
    "scrape",
    "dragon_breath",
    "sonic_boom",
    "gust",
    "falling_dust",
    "dust_plume",
    "trial_spawner_detection",
    "trial_spawner_detection_ominous",
    "firefly",
    "ominous_spawning",
    "squid_ink",
    "glow_squid_ink",
    "shriek",
    "reverse_portal",
    "dolphin",
    "spit",
    "white_smoke",
    "noxious_gas",
    "infested",
    "raid_omen",
    "mycelium",
    "pause_mob_growth",
    "dust_pillar",
    "sneeze",
    "underwater",
    "spore_blossom_air",
    "sulfur_cube_goo",
];

fn new_particle(name: &str) -> Particle {
    use mcr_protocol::types::particle::ColorParticleKind;
    match name {
        "tinted_leaves" => Particle::Color {
            kind: ColorParticleKind::TintedLeaves,
            argb: 0xFF30_A030u32 as i32,
        },
        "falling_dust" | "dust_pillar" => Particle::BlockState {
            kind: if name == "dust_pillar" {
                BlockParticleKind::DustPillar
            } else {
                BlockParticleKind::FallingDust
            },
            state: 2,
        },
        "dragon_breath" => Particle::DragonBreath { power: 1.0 },
        "shriek" => Particle::Shriek { delay: 0 },
        _ => particle(name),
    }
}

#[test]
fn new_particle_showcase() {
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
    for (i, name) in NEW_GRID.iter().enumerate() {
        let (cx, cy) = ((i % cols) as f64 - 3.5, (i / cols) as f64);
        let at = [cx * 2.6, 12.0 + 3.0 * (2.0 - cy) + 0.5 * 8.0 - 8.0, 0.0];
        let p = new_particle(name);
        match *name {
            "sonic_boom" | "gust" | "shriek" => e.emit(&p, at, [0.0; 3], 0.0, 0),
            "firefly" | "dust_pillar" => e.emit(&p, at, [0.3; 3], 0.0, 8),
            _ => e.emit(&p, at, [0.25; 3], 0.02, 8),
        }
    }
    for _ in 0..6 {
        e.tick(&World);
    }
    frame(&mut r, &mut e, &env, &cam);
    save("particles_grid_new.png", &r.read_offscreen().unwrap());
    assert!(e.count() > 100);
}

#[test]
fn campfire_smoke_at_the_eye_stays_a_translucent_sprite() {
    let (Some(mut r), Some(resources)) = (renderer(), jar()) else {
        return;
    };
    let env = Environment::overworld(6000, 0.0, 0.0);
    let cam = Camera {
        position: DVec3::new(0.0, 8.0, 14.0),
        pitch: 0.0,
        ..Camera::default()
    };
    let mut e = engine(jar().unwrap());
    frame(&mut r, &mut e, &env, &cam);
    let empty = r.read_offscreen().unwrap();
    let mut e = engine(resources);
    e.emit(
        &particle("campfire_cosy_smoke"),
        [0.0, 8.0, 13.94],
        [0.0; 3],
        0.0,
        0,
    );
    e.tick(&World);
    frame(&mut r, &mut e, &env, &cam);
    let px = r.read_offscreen().unwrap();
    save("particles_smoke_at_eye.png", &px);
    let covered: Vec<(&[u8; 4], &[u8; 4])> = px
        .as_chunks::<4>()
        .0
        .iter()
        .zip(empty.as_chunks::<4>().0)
        .filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 24))
        .collect();
    assert!(
        covered.len() > 50_000,
        "the smoke fills the view: {}",
        covered.len()
    );
    let dark = covered
        .iter()
        .filter(|(a, _)| a[..3].iter().all(|&v| v < 70))
        .count();
    assert_eq!(dark, 0, "no dark texels");
    let shades: std::collections::HashSet<[u8; 3]> =
        covered.iter().map(|(a, _)| [a[0], a[1], a[2]]).collect();
    assert!(
        shades.len() > 20,
        "the sky gradient shows through each texel: {}",
        shades.len()
    );
}

#[test]
fn particles_fade_into_the_fog_like_terrain() {
    let (Some(mut r), Some(_)) = (renderer(), jar()) else {
        return;
    };
    let mut env = Environment::overworld(6000, 0.0, 0.0);
    let cam = Camera {
        position: DVec3::new(0.0, 8.0, 14.0),
        pitch: 0.0,
        ..Camera::default()
    };
    let shot = |r: &mut Renderer, env: &Environment, with: bool| {
        let mut e = engine(jar().unwrap());
        if with {
            for i in 0..40 {
                e.add_simple(
                    "flame",
                    DVec3::new(
                        f64::from(i % 8) * 0.3 - 1.2,
                        8.0,
                        10.0 - f64::from(i / 8) * 0.01,
                    ),
                    DVec3::ZERO,
                );
            }
        }
        frame(r, &mut e, env, &cam);
        r.read_offscreen().unwrap()
    };
    let clear = shot(&mut r, &env, false);
    let lit = shot(&mut r, &env, true);
    let changed = |a: &[u8], b: &[u8]| -> Vec<[u8; 4]> {
        a.as_chunks::<4>()
            .0
            .iter()
            .zip(b.as_chunks::<4>().0)
            .filter(|(x, y)| (0..3).any(|c| x[c].abs_diff(y[c]) > 8))
            .map(|(x, _)| *x)
            .collect()
    };
    assert!(changed(&lit, &clear).len() > 200, "flames show without fog");
    env.fog_start = 0.0;
    env.fog_end = 1.0;
    let clear = shot(&mut r, &env, false);
    let fogged = shot(&mut r, &env, true);
    let fog = env.fog_color.map(|c| (c * 255.0).round() as u8);
    let fogged = changed(&fogged, &clear);
    assert!(fogged.len() > 200, "flames differ from the sky behind them");
    for px in fogged {
        assert!(
            (0..3).all(|c| px[c].abs_diff(fog[c]) <= 2),
            "{px:?} against fog {fog:?}"
        );
    }
}

fn water_pane() -> SectionMesh {
    let mut m = SectionMesh::default();
    for (x, y) in [
        (5.0, 5.0),
        (7.5, 5.0),
        (5.0, 7.5),
        (7.5, 7.5),
        (10.0, 5.0),
        (10.0, 7.5),
        (5.0, 10.0),
        (7.5, 10.0),
        (10.0, 10.0),
    ] {
        m.translucent.push(
            ModelQuad::encode(&ModelQuadDesc {
                corners: [
                    [x, y, 10.0],
                    [x + 2.5, y, 10.0],
                    [x, y + 2.5, 10.0],
                    [x + 2.5, y + 2.5, 10.0],
                ],
                uvs: [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]],
                layer: 0,
                light: [LIGHT_FULL_SKY; 4],
                shade_face: Face::PosZ,
                flags: 0,
                ao: [63; 4],
                tint: 0,
                cell: [(x + 1.25) as u8, (y + 1.25) as u8, 10],
            })
            .unwrap(),
        );
    }
    m
}

fn blue_group() -> TextureGroupData {
    TextureGroupData {
        width: 16,
        height: 16,
        layers: 1,
        mips: (0..5)
            .map(|m| [0u8, 0, 255, 160].repeat((16usize >> m).pow(2)))
            .collect(),
        animations: Vec::new(),
    }
}

/// Mean colour where flames behind a translucent blue pane cover the view.
fn flames_behind_water(improved: bool) -> Option<[f32; 3]> {
    let mut r = renderer()?;
    r.set_improved_transparency(improved);
    r.set_texture_groups(&[blue_group()]);
    r.set_section([0, 0, 0], &water_pane()).unwrap();
    let env = Environment::overworld(6000, 0.0, 0.0);
    let cam = Camera {
        position: DVec3::new(8.75, 8.75, 20.0),
        ..Camera::default()
    };
    let shot = |r: &mut Renderer, with: bool| {
        let mut e = engine(jar().unwrap());
        if with {
            for i in 0..60 {
                let (x, y) = (
                    f64::from(i % 10) * 0.25 + 7.5,
                    f64::from(i / 10) * 0.25 + 7.9,
                );
                e.add_simple("flame", DVec3::new(x, y, 4.0), DVec3::ZERO);
            }
        }
        frame(r, &mut e, &env, &cam);
        r.read_offscreen().unwrap()
    };
    let clear = shot(&mut r, false);
    let lit = shot(&mut r, true);
    let changed: Vec<&[u8; 4]> = lit
        .as_chunks::<4>()
        .0
        .iter()
        .zip(clear.as_chunks::<4>().0)
        .filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 8))
        .map(|(a, _)| a)
        .collect();
    assert!(changed.len() > 100, "flames show: {}", changed.len());
    let n = changed.len() as f32;
    Some(std::array::from_fn(|c| {
        changed.iter().map(|p| f32::from(p[c])).sum::<f32>() / n
    }))
}

#[test]
fn particles_behind_water_are_tinted_with_improved_transparency() {
    let (Some(classic), Some(_)) = (flames_behind_water(false), jar()) else {
        return;
    };
    let improved = flames_behind_water(true).unwrap();
    assert!(
        improved[2] > classic[2] + 20.0 && improved[0] < classic[0],
        "the water now covers the flames: classic {classic:?}, improved {improved:?}"
    );
}
