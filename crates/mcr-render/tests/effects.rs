use mcr_assets::ResourceManager;
use mcr_render::glam::DVec3;
use mcr_render::synthetic::{Terrain, generate_world};
use mcr_render::{
    Camera, DestroyDecal, EffectAssets, Environment, FrameInputs, Precipitation, RenderError,
    Renderer, RendererConfig, Target, WeatherSource, WorldBorderVisual,
};

const W: u32 = 960;
const H: u32 = 540;

struct Flat(Precipitation);

impl WeatherSource for Flat {
    fn height(&self, x: i32, z: i32) -> i32 {
        Terrain { seed: 11 }.height(x, z) + 1
    }
    fn precipitation(&self, _: i32, _: i32, _: i32) -> Precipitation {
        self.0
    }
}

fn assets() -> Option<EffectAssets> {
    let jar = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache/versions/26.3/client.jar");
    let resources = ResourceManager::vanilla(jar).ok()?;
    Some(EffectAssets::load(&|p| resources.read(p).ok().flatten()))
}

fn renderer() -> Option<Renderer> {
    let mut r = match Renderer::new(
        Target::Offscreen {
            width: W,
            height: H,
        },
        RendererConfig::default(),
    ) {
        Ok(r) => r,
        Err(RenderError::NoAdapter(why)) => {
            eprintln!("skipping GPU test: {why}");
            return None;
        }
        Err(e) => panic!("renderer creation failed: {e}"),
    };
    r.set_effect_assets(&assets()?);
    for (k, mesh) in generate_world(&Terrain { seed: 11 }, 6, 4) {
        r.set_section(k, &mesh).expect("section fits");
    }
    Some(r)
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

struct Scene<'a> {
    name: &'a str,
    env: Environment,
    yaw: f32,
    pitch: f32,
    pos: DVec3,
    time: u64,
    weather: Option<Precipitation>,
    decals: Vec<DestroyDecal>,
}

fn scene<'a>(name: &'a str, env: Environment) -> Scene<'a> {
    Scene {
        name,
        env,
        yaw: 0.0,
        pitch: 0.1,
        pos: DVec3::new(8.5, 118.0, 8.5),
        time: 1000,
        weather: None,
        decals: Vec::new(),
    }
}

fn shoot(r: &mut Renderer, s: &Scene) -> Vec<u8> {
    let cam = Camera {
        position: s.pos,
        yaw: s.yaw,
        pitch: s.pitch,
        ..Camera::default()
    };
    let mut env = s.env.clone();
    env.set_render_distance_chunks(8);
    r.set_environment(&env);
    r.set_effect_time(s.time, 0.0);
    r.set_destroy_decals(&s.decals);
    for _ in 0..3 {
        if let Some(p) = s.weather {
            r.update_weather(&cam, 10, &Flat(p));
        }
        r.render(&FrameInputs {
            camera: &cam,
            sky_light: 1.0,
        })
        .expect("render");
    }
    let px = r.read_offscreen().expect("readback");
    save(&format!("{}.png", s.name), &px);
    px
}

fn mean(px: &[u8], rows: std::ops::Range<u32>) -> [f32; 3] {
    let mut acc = [0f64; 3];
    let mut n = 0.0;
    for y in rows {
        for x in 0..W {
            let i = ((y * W + x) * 4) as usize;
            for c in 0..3 {
                acc[c] += f64::from(px[i + c]);
            }
            n += 1.0;
        }
    }
    acc.map(|v| (v / n) as f32)
}

#[test]
fn sky_screenshots() {
    let Some(mut r) = renderer() else { return };
    let noon = shoot(
        &mut r,
        &scene("noon", Environment::overworld(6000, 0.0, 0.0)),
    );
    let mut sunset = scene("sunset", Environment::overworld(12600, 0.0, 0.0));
    sunset.yaw = std::f32::consts::FRAC_PI_2;
    sunset.pitch = 0.05;
    let sunset_px = shoot(&mut r, &sunset);
    let mut night = scene(
        "midnight",
        Environment::overworld(18000 + 24000 * 3, 0.0, 0.0),
    );
    night.pitch = 0.6;
    let night_px = shoot(&mut r, &night);
    let mut sunrise = scene("sunrise", Environment::overworld(23400, 0.0, 0.0));
    sunrise.yaw = -std::f32::consts::FRAC_PI_2;
    shoot(&mut r, &sunrise);
    let mut stars = scene("full_moon_night", Environment::overworld(13500, 0.0, 0.0));
    stars.yaw = std::f32::consts::FRAC_PI_2;
    stars.pitch = 0.9;
    shoot(&mut r, &stars);

    assert!(mean(&noon, 0..100)[2] > mean(&night_px, 0..100)[2] + 40.0);
    assert!(mean(&sunset_px, 200..300)[0] > 20.0);
}

#[test]
fn cloud_and_weather_screenshots() {
    let Some(mut r) = renderer() else { return };
    let mut clouds = scene("clouds", Environment::overworld(6000, 0.0, 0.0));
    clouds.pitch = 0.45;
    clouds.pos.y = 170.0;
    let clear = shoot(&mut r, &clouds);
    clouds.name = "clouds_later";
    clouds.time = 9000;
    shoot(&mut r, &clouds);
    let mut above = scene("clouds_inside", Environment::overworld(6000, 0.0, 0.0));
    above.pos.y = 193.0;
    above.pitch = -0.1;
    shoot(&mut r, &above);

    let mut rain = scene("rain", Environment::overworld(6000, 1.0, 0.0));
    rain.weather = Some(Precipitation::Rain);
    rain.pitch = 0.0;
    rain.pos.y = 60.0;
    let rain_px = shoot(&mut r, &rain);
    let mut snow = scene("snow", Environment::overworld(6000, 1.0, 0.0));
    snow.weather = Some(Precipitation::Snow);
    snow.pos.y = 60.0;
    snow.pitch = 0.0;
    shoot(&mut r, &snow);
    let mut thunder = scene("thunder", Environment::overworld(6000, 1.0, 1.0));
    thunder.weather = Some(Precipitation::Rain);
    thunder.pos.y = 60.0;
    thunder.pitch = 0.1;
    shoot(&mut r, &thunder);
    let mut flash = scene(
        "lightning_flash",
        Environment::overworld(6000, 1.0, 1.0).with_lightning_flash(true),
    );
    flash.pos.y = 60.0;
    shoot(&mut r, &flash);

    assert!(mean(&clear, 0..200) != mean(&rain_px, 0..200));
}

#[test]
fn hidden_weather_keeps_the_rainy_sky() {
    let Some(mut r) = renderer() else { return };
    let mut rain = scene("rain_columns", Environment::overworld(6000, 1.0, 0.0));
    rain.weather = Some(Precipitation::Rain);
    rain.pitch = 0.0;
    rain.pos.y = 60.0;
    let shown = shoot(&mut r, &rain);
    r.set_weather_hidden(true);
    rain.name = "rain_hidden";
    let hidden = shoot(&mut r, &rain);
    let Some(mut bare) = renderer() else { return };
    let mut sky = scene("rain_sky_only", Environment::overworld(6000, 1.0, 0.0));
    sky.pitch = 0.0;
    sky.pos.y = 60.0;
    let sky_only = shoot(&mut bare, &sky);
    let differs = |a: &[u8], b: &[u8]| a.iter().zip(b).filter(|(x, y)| x.abs_diff(**y) > 8).count();
    assert!(differs(&shown, &hidden) > 1000, "the rain columns are gone");
    assert!(
        differs(&hidden, &sky_only) < 100,
        "the rainy sky and fog stay as without columns"
    );
}

#[test]
fn destroy_stage_screenshot() {
    let Some(mut r) = renderer() else { return };
    let mut s = scene("destroy_stages", Environment::overworld(6000, 0.0, 0.0));
    s.pos = DVec3::new(0.5, 90.0, 12.5);
    s.pitch = 0.0;
    s.decals = (0..10)
        .map(|i| DestroyDecal {
            pos: [i * 2 - 9, 90, -4],
            stage: i as u8,
            ..DestroyDecal::default()
        })
        .collect();
    let with = shoot(&mut r, &s);
    s.decals.clear();
    s.name = "destroy_none";
    let without = shoot(&mut r, &s);
    assert_ne!(with, without);
}

fn aim(angle: f32) -> (f32, f32) {
    let d = mcr_render::glam::Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2)
        * mcr_render::glam::Mat4::from_rotation_x(angle)
        * mcr_render::glam::Vec4::Y;
    (f32::atan2(-d.x, -d.z), d.y.clamp(-1.0, 1.0).asin())
}

#[test]
fn celestial_bodies_are_where_the_angles_say() {
    let Some(mut r) = renderer() else { return };
    let env = Environment::overworld(15000, 0.0, 0.0);
    let (yaw, pitch) = aim(env.moon_angle);
    let mut moon = scene("moon_phase0", env);
    (moon.yaw, moon.pitch) = (yaw, pitch);
    shoot(&mut r, &moon);
    let env = Environment::overworld(15000 + 24000 * 2, 0.0, 0.0);
    let (yaw, pitch) = aim(env.moon_angle);
    let mut moon = scene("moon_phase2", env);
    (moon.yaw, moon.pitch) = (yaw, pitch);
    shoot(&mut r, &moon);
    let env = Environment::overworld(3000, 0.0, 0.0);
    let (yaw, pitch) = aim(env.sun_angle);
    let mut sun = scene("sun_morning", env);
    (sun.yaw, sun.pitch) = (yaw, pitch);
    let px = shoot(&mut r, &sun);
    let centre = ((H / 2 * W + W / 2) * 4) as usize;
    assert!(px[centre] > 240 && px[centre + 1] > 240);
}

#[test]
fn hostile_environment_renders_without_panic() {
    let Some(mut r) = renderer() else { return };
    let mut env = Environment::overworld(u64::MAX, f32::NAN, f32::INFINITY);
    env.rain_level = f32::NAN;
    env.sun_angle = f32::NAN;
    env.cloud_height = Some(f32::INFINITY);
    r.set_environment(&env);
    r.set_effect_time(u64::MAX, f32::NAN);
    r.set_destroy_decals(&vec![
        DestroyDecal {
            pos: [i32::MAX, 0, i32::MIN],
            stage: 255,
            boxes: vec![[f32::NAN; 6]; 5000],
            quads: vec![[[f32::NAN; 3]; 4]; 5000],
        };
        5000
    ]);
    let cam = Camera {
        position: DVec3::new(f64::NAN, f64::INFINITY, 1e300),
        ..Camera::default()
    };
    r.update_weather(&cam, u32::MAX, &Flat(Precipitation::Rain));
    r.render(&FrameInputs {
        camera: &cam,
        sky_light: 1.0,
    })
    .expect("render");
}

#[test]
fn world_border_wall_screenshot() {
    let Some(mut r) = renderer() else { return };
    let mut s = scene("world_border_off", Environment::overworld(6000, 0.0, 0.0));
    s.pos = DVec3::new(0.5, 100.0, 0.5);
    s.pitch = 0.2;
    let without = shoot(&mut r, &s);
    r.set_world_border(
        Some(WorldBorderVisual {
            min_x: -6.0,
            max_x: 6.0,
            min_z: -6.0,
            max_z: 6.0,
            color: [0.125, 0.627, 1.0],
            alpha: 1.0,
            scroll: 0.25,
        }),
        128.0,
    );
    s.name = "world_border_on";
    let with = shoot(&mut r, &s);
    let (a, b) = (mean(&without, 0..H), mean(&with, 0..H));
    assert!(b[2] > a[2] + 1.0, "{a:?} -> {b:?}");
}
