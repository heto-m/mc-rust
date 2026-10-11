//! Sky, weather, destroy decals and particles driven by the client's world state.

use std::sync::Arc;

use mcr_buf::BlockPos;
use mcr_client::ClientState;
use mcr_data::Version;
use mcr_particles::{
    Fluid, ParticleAssets, ParticleAtlas, ParticleEngine, ParticleWorld,
    weather::{RainSound, RainTicker},
};
use mcr_protocol::play::clientbound::world::{
    ExplosionParticle, LevelParticles, ParticleRandomization,
};
use mcr_protocol::types::Particle;
use mcr_render::glam::DVec3;
use mcr_render::{
    CloudMode, CloudSettings, DestroyDecal, EffectAssets, EffectImage, Environment, Precipitation,
    WeatherSource, WorldBorderVisual,
};
use mcr_settings::Options;
use mcr_settings::value::{CloudStatus, ParticleStatus};
use mcr_world::{ChunkPos, ChunkStore, HeightmapKind};
use rustc_hash::FxHashMap;

use crate::assets::Assets;

const MAX_TRACKERS: usize = 64;
const TOTEM_TICKS: u8 = 30;
const CRIT_TICKS: u8 = 3;
const MAX_EXPLOSION_PARTICLES: i64 = 512;
const MAX_PENDING_EXPLOSIONS: usize = 64;

/// Vanilla's `TrackingEmitter`: bursts of a particle around an entity for a few ticks.
#[derive(Debug, Clone, Copy)]
struct Tracker {
    entity: i32,
    particle: &'static str,
    age: u8,
    lifetime: u8,
}

#[derive(Debug, Clone)]
struct PendingExplosion {
    center: [f64; 3],
    radius: f32,
    block_count: i32,
    particles: Vec<ExplosionParticle>,
}
const BIOME_REGISTRY: &str = "minecraft:worldgen/biome";
const SNOW_TEMPERATURE: f32 = 0.15;
const MAX_DECALS: usize = 128;
const LIGHTNING_FLASH_TICKS: u32 = 2;
const TREE_LINE: f32 = 80.0;

pub fn effect_assets(assets: &Assets) -> EffectAssets {
    EffectAssets::load(&|path| assets.read(path))
}

pub struct AppParticleAssets {
    assets: Arc<Assets>,
}

impl AppParticleAssets {
    pub fn new(assets: Arc<Assets>) -> Self {
        Self { assets }
    }

    fn texture(&self, path: &str) -> Option<EffectImage> {
        let bytes = self
            .assets
            .read(&format!("assets/minecraft/textures/{path}.png"))?;
        EffectImage::decode(&bytes)
    }
}

impl ParticleAssets for AppParticleAssets {
    fn block_texture(&self, state: i32) -> Option<(EffectImage, [f32; 3])> {
        let cache = &self.assets.cache;
        let model = cache.state(u16::try_from(state).ok()?);
        let quads = cache.quads(model);
        let (face_layer, tinted) = match model.cube.iter().find(|f| f.layer != mcr_mesh::LAYER_NONE)
        {
            Some(face) => (face.layer, face.tint != mcr_mesh::TINT_NONE),
            None => {
                let q = quads.iter().find(|q| q.normal == 0).or(quads.first())?;
                (q.layer, q.tint != 0)
            }
        };
        let layer = if model.particle == mcr_mesh::LAYER_NONE {
            face_layer
        } else {
            model.particle
        };
        let block = mcr_data::v26_3().state_block_name(u32::try_from(state).ok()?);
        let tinted = tinted && block != Some("minecraft:grass_block");
        let group = cache.textures.groups.get(usize::from(layer >> 14))?;
        let size = (group.width * group.height * 4) as usize;
        let start = usize::from(layer & 0x3FFF) * size;
        let rgba = group.mips.first()?.get(start..start + size)?.to_vec();
        let tint = if tinted { [0.47, 0.75, 0.35] } else { [1.0; 3] };
        Some((
            EffectImage {
                width: group.width,
                height: group.height,
                rgba,
            },
            tint,
        ))
    }

    fn item_texture(&self, item: i32) -> Option<EffectImage> {
        let name = mcr_data::v26_3().item_name(u32::try_from(item).ok()?)?;
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        self.texture(&format!("item/{name}"))
            .or_else(|| self.texture(&format!("block/{name}")))
    }

    fn named_texture(&self, path: &str) -> Option<EffectImage> {
        self.texture(path)
    }
}

/// Collision, light and fluid queries on the client's chunks.
pub struct ClientParticleWorld<'a>(pub &'a ClientState);

pub(crate) fn light_at(chunks: &ChunkStore, x: i32, y: i32, z: i32) -> Option<(u8, u8)> {
    let column = chunks.column(ChunkPos::new(x >> 4, z >> 4))?;
    let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
    Some((
        column.block_light(lx, y, lz).unwrap_or(0),
        column.sky_light(lx, y, lz).unwrap_or(15),
    ))
}

impl ParticleWorld for ClientParticleWorld<'_> {
    fn collision_boxes(&self, x: i32, y: i32, z: i32, out: &mut Vec<[f32; 6]>) {
        let Some(state) = self.0.chunks.block(x, y, z) else {
            return;
        };
        for b in mcr_data::v26_3().collision_shape(u32::from(state.0)) {
            out.push([b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2]]);
        }
    }

    fn light(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        light_at(&self.0.chunks, x, y, z).unwrap_or((0, 15))
    }

    fn fluid(&self, x: i32, y: i32, z: i32) -> Option<(Fluid, f32)> {
        let data: &Version = mcr_data::v26_3();
        let state = self.0.chunks.block(x, y, z)?;
        let fluid = data.block_state(u32::from(state.0))?.fluid?;
        let name = data.fluid_name(fluid.fluid)?;
        let kind = if name.contains("lava") {
            Fluid::Lava
        } else if name.contains("water") {
            Fluid::Water
        } else {
            return None;
        };
        let height = if fluid.falling {
            1.0
        } else {
            f32::from(fluid.amount) / 9.0
        };
        Some((kind, height))
    }
}

/// Precipitation from the synced biome registry and the client's heightmaps.
pub struct ClientWeather<'a> {
    state: &'a ClientState,
    biomes: &'a [(bool, f32)],
}

impl WeatherSource for ClientWeather<'_> {
    fn height(&self, x: i32, z: i32) -> i32 {
        let chunks = &self.state.chunks;
        let min_y = chunks.config().min_y();
        chunks
            .column(ChunkPos::new(x >> 4, z >> 4))
            .and_then(|c| c.heightmaps().get(HeightmapKind::MotionBlocking))
            .map_or(i32::MIN / 2, |h| {
                h.first_available((x & 15) as usize, (z & 15) as usize, min_y)
            })
    }

    fn precipitation(&self, x: i32, y: i32, z: i32) -> Precipitation {
        let chunks = &self.state.chunks;
        let min_y = chunks.config().min_y();
        let Some(column) = chunks.column(ChunkPos::new(x >> 4, z >> 4)) else {
            return Precipitation::None;
        };
        let rel = y - min_y;
        if rel < 0 {
            return Precipitation::None;
        }
        let Some(section) = column.section((rel >> 4) as usize) else {
            return Precipitation::None;
        };
        let biome = section
            .biome(
                ((x & 15) >> 2) as usize,
                ((rel & 15) >> 2) as usize,
                ((z & 15) >> 2) as usize,
            )
            .0 as usize;
        let Some(&(has, temperature)) = self.biomes.get(biome) else {
            return Precipitation::None;
        };
        if !has {
            return Precipitation::None;
        }
        let adjusted = if y as f32 > TREE_LINE {
            temperature - (y as f32 - TREE_LINE) * 0.05 / 40.0
        } else {
            temperature
        };
        if adjusted < SNOW_TEMPERATURE {
            Precipitation::Snow
        } else {
            Precipitation::Rain
        }
    }

    fn light(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        light_at(&self.state.chunks, x, y, z).unwrap_or((0, 15))
    }
}

#[derive(Debug, Clone, Copy)]
struct Breaking {
    pos: BlockPos,
    stage: u8,
}

pub struct WorldEffects {
    pub particles: ParticleEngine,
    rain: RainTicker,
    biomes: Vec<(bool, f32)>,
    biome_count: usize,
    breaking: FxHashMap<i32, Breaking>,
    flash_ticks: u32,
    lightning_type: i32,
    status: ParticleStatus,
    counter: u32,
    trackers: Vec<Tracker>,
    explosions: Vec<PendingExplosion>,
}

impl std::fmt::Debug for WorldEffects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldEffects")
            .field("particles", &self.particles.count())
            .finish_non_exhaustive()
    }
}

impl WorldEffects {
    pub fn new(assets: Arc<Assets>) -> Self {
        let atlas = ParticleAtlas::load(&|path| assets.read(path));
        Self {
            particles: ParticleEngine::new(atlas, Box::new(AppParticleAssets::new(assets))),
            rain: RainTicker::default(),
            biomes: Vec::new(),
            biome_count: 0,
            breaking: FxHashMap::default(),
            flash_ticks: 0,
            lightning_type: mcr_data::v26_3()
                .entity_type_by_name("minecraft:lightning_bolt")
                .map_or(-1, |id| id as i32),
            status: ParticleStatus::All,
            counter: 0,
            trackers: Vec::new(),
            explosions: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.particles.clear();
        self.breaking.clear();
        self.flash_ticks = 0;
    }

    fn refresh_biomes(&mut self, state: &ClientState) {
        let Some(registry) = state.registries.get(BIOME_REGISTRY) else {
            return;
        };
        if registry.len() == self.biome_count {
            return;
        }
        self.biome_count = registry.len();
        self.biomes = registry
            .entries
            .iter()
            .map(|e| {
                let data = e.data.as_ref();
                (
                    data.and_then(|d| d.get_bool("has_precipitation"))
                        .unwrap_or(true),
                    data.and_then(|d| d.get_f32("temperature")).unwrap_or(0.5),
                )
            })
            .collect();
    }

    pub fn weather<'a>(&'a mut self, state: &'a ClientState) -> ClientWeather<'a> {
        self.refresh_biomes(state);
        ClientWeather {
            state,
            biomes: &self.biomes,
        }
    }

    pub fn on_entity_added(&mut self, state: &ClientState, id: i32) {
        if state
            .entities
            .get(id)
            .is_some_and(|e| e.kind == self.lightning_type)
        {
            self.flash_ticks = LIGHTNING_FLASH_TICKS;
        }
    }

    pub fn flash(&self) -> bool {
        self.flash_ticks > 0
    }

    pub fn destruction(&mut self, entity_id: i32, pos: BlockPos, progress: u8) {
        if progress > 9 {
            self.breaking.remove(&entity_id);
        } else if self.breaking.len() < MAX_DECALS || self.breaking.contains_key(&entity_id) {
            self.breaking.insert(
                entity_id,
                Breaking {
                    pos,
                    stage: progress,
                },
            );
        }
    }

    pub fn decals(&self, state: &ClientState, own: Option<(BlockPos, u8)>) -> Vec<DestroyDecal> {
        let data = mcr_data::v26_3();
        let make = |pos: BlockPos, stage: u8| {
            let boxes = state
                .chunks
                .block(pos.x, pos.y, pos.z)
                .map(|s| {
                    data.outline_shape(u32::from(s.0))
                        .iter()
                        .map(|b| [b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2]])
                        .collect()
                })
                .unwrap_or_default();
            DestroyDecal {
                pos: [pos.x, pos.y, pos.z],
                stage,
                boxes,
            }
        };
        self.breaking
            .values()
            .map(|b| make(b.pos, b.stage))
            .chain(own.map(|(pos, stage)| make(pos, stage)))
            .collect()
    }

    pub fn set_options(&mut self, options: &Options) {
        self.status = options.particles;
    }

    pub fn emit(
        &mut self,
        particle: &Particle,
        pos: [f64; 3],
        delta: [f32; 3],
        speed: [f32; 3],
        count: i32,
        always: bool,
    ) {
        self.counter = self.counter.wrapping_add(1);
        let (count, skip) = match self.status {
            ParticleStatus::All => (count, false),
            ParticleStatus::Decreased => (
                (count + 1) / 2,
                count <= 0 && self.counter.is_multiple_of(2),
            ),
            ParticleStatus::Minimal => (count / 4, count <= 0 || count < 4),
        };
        if skip && !always {
            return;
        }
        self.particles.emit_packet(&LevelParticles {
            particle: particle.clone(),
            override_limiter: false,
            always_show: always,
            x: pos[0],
            y: pos[1],
            z: pos[2],
            x_dist: delta[0],
            y_dist: delta[1],
            z_dist: delta[2],
            x_max_speed: speed[0],
            y_max_speed: speed[1],
            z_max_speed: speed[2],
            randomization: ParticleRandomization::Default,
            count,
        });
    }

    pub fn block_break(&mut self, state: &ClientState, pos: BlockPos, block: i32) {
        self.particles
            .block_break([pos.x, pos.y, pos.z], block, &ClientParticleWorld(state));
    }

    pub fn block_crack(&mut self, state: &ClientState, pos: BlockPos, face: u8, block: i32) {
        self.particles.block_crack(
            [pos.x, pos.y, pos.z],
            face,
            block,
            &ClientParticleWorld(state),
        );
    }

    /// The vanilla seed emitter has no sprites, so its 8 ticks of `explosion` bursts are driven here.
    /// Emits the packet's own particle (an emitter for big blasts) the way vanilla's `addParticle` does.
    pub fn totem(&mut self, entity_id: i32) {
        self.track(entity_id, "minecraft:totem_of_undying", TOTEM_TICKS);
    }

    /// Critical-hit sparks from the `animate` packet; `magic` for enchanted hits.
    pub fn crit(&mut self, entity_id: i32, magic: bool) {
        let particle = if magic {
            "minecraft:enchanted_hit"
        } else {
            "minecraft:crit"
        };
        self.track(entity_id, particle, CRIT_TICKS);
    }

    fn track(&mut self, entity: i32, particle: &'static str, lifetime: u8) {
        if self.trackers.len() < MAX_TRACKERS {
            self.trackers.push(Tracker {
                entity,
                particle,
                age: 0,
                lifetime,
            });
        }
    }

    fn random_signed(&mut self) -> f64 {
        self.counter = self
            .counter
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        f64::from(self.counter >> 8) / f64::from(1u32 << 24) * 2.0 - 1.0
    }

    fn random_unit(&mut self) -> f64 {
        (self.random_signed() + 1.0) / 2.0
    }

    fn tick_trackers(&mut self, state: &ClientState) {
        let data = mcr_data::v26_3();
        let Some(registry) = data.registry("minecraft:particle_type") else {
            self.trackers.clear();
            return;
        };
        let mut trackers = std::mem::take(&mut self.trackers);
        for t in &mut trackers {
            let body = if t.entity == state.player.entity_id {
                Some((state.player.pos(), 0.6, 1.8))
            } else {
                state.entities.get(t.entity).map(|e| {
                    let size = data
                        .entity_type_info(e.kind as u32)
                        .map_or((0.6, 1.0), |i| (f64::from(i.width), f64::from(i.height)));
                    (e.pos, size.0, size.1)
                })
            };
            let (Some((pos, width, height)), Some(id)) = (body, registry.id_of(t.particle)) else {
                t.age = t.lifetime;
                continue;
            };
            let particle = Particle::Simple(id as i32);
            for _ in 0..16 {
                let (x, y, z) = (
                    self.random_signed(),
                    self.random_signed(),
                    self.random_signed(),
                );
                if x * x + y * y + z * z > 1.0 {
                    continue;
                }
                let at = [
                    pos[0] + width * x / 4.0,
                    pos[1] + height * (0.5 + y / 4.0),
                    pos[2] + width * z / 4.0,
                ];
                self.emit(
                    &particle,
                    at,
                    [x as f32, y as f32 + 0.2, z as f32],
                    [1.0; 3],
                    0,
                    true,
                );
            }
            t.age += 1;
        }
        trackers.retain(|t| t.age < t.lifetime);
        self.trackers = trackers;
    }

    /// The packet's own particle now; block-debris particles follow on the next tick.
    pub fn explosion(
        &mut self,
        center: [f64; 3],
        radius: f32,
        particle: &Particle,
        block_count: i32,
        particles: Vec<ExplosionParticle>,
    ) {
        if !center.iter().all(|c| c.is_finite()) {
            return;
        }
        self.emit(particle, center, [1.0, 0.0, 0.0], [1.0; 3], 0, true);
        if particles.iter().any(|p| p.weight > 0)
            && radius.is_finite()
            && block_count > 0
            && self.explosions.len() < MAX_PENDING_EXPLOSIONS
        {
            self.explosions.push(PendingExplosion {
                center,
                radius: radius.clamp(0.0, 256.0),
                block_count,
                particles,
            });
        }
    }

    /// Vanilla's `ClientExplosionTracker`: up to 512 debris particles a tick, shared by block count.
    fn tick_explosions(&mut self, state: &ClientState) {
        let explosions = std::mem::take(&mut self.explosions);
        if self.status != ParticleStatus::All {
            return;
        }
        let total: i64 = explosions.iter().map(|e| i64::from(e.block_count)).sum();
        for _ in 0..total.min(MAX_EXPLOSION_PARTICLES) {
            let mut pick = (self.random_unit() * total as f64) as i64;
            let Some(e) = explosions.iter().find(|e| {
                pick -= i64::from(e.block_count);
                pick < 0
            }) else {
                continue;
            };
            let (x, y, z) = (
                self.random_signed(),
                self.random_signed(),
                self.random_signed(),
            );
            let len = (x * x + y * y + z * z).sqrt();
            if len < 1.0e-4 {
                continue;
            }
            let dir = [x / len, y / len, z / len];
            let r = self.random_unit().cbrt() * f64::from(e.radius);
            let local = dir.map(|d| d * r);
            let at = [0, 1, 2].map(|i| e.center[i] + local[i]);
            if !is_air(state, at) {
                continue;
            }
            let total_weight: i64 = e.particles.iter().map(|p| i64::from(p.weight.max(0))).sum();
            let mut w = (self.random_unit() * total_weight as f64) as i64;
            let Some(info) = e.particles.iter().find(|p| {
                w -= i64::from(p.weight.max(0));
                w < 0
            }) else {
                continue;
            };
            let (a, b) = (self.random_unit(), self.random_unit());
            let base = 0.5 / (r / f64::from(e.radius.max(1.0e-3)) + 0.1) * a * b + 0.3;
            let speed = base * f64::from(info.speed);
            let pos = [0, 1, 2].map(|i| e.center[i] + local[i] * f64::from(info.scaling));
            let vel = dir.map(|d| (d * speed) as f32);
            let particle = info.particle.clone();
            self.emit(&particle, pos, vel, [1.0; 3], 0, false);
        }
    }

    /// One 20 Hz step: simulation, rain splashes and the lightning flash timer.
    pub fn tick(
        &mut self,
        state: &ClientState,
        camera: DVec3,
        options: &Options,
    ) -> Option<RainSound> {
        self.flash_ticks = self.flash_ticks.saturating_sub(1);
        self.particles.set_camera(camera);
        let world = ClientParticleWorld(state);
        self.particles.tick(&world);
        self.tick_trackers(state);
        self.tick_explosions(state);
        let rain = state.world.rain_level;
        if rain > 0.0 && self.status != ParticleStatus::Minimal {
            self.refresh_biomes(state);
            let source = ClientWeather {
                state,
                biomes: &self.biomes,
            };
            let radius = options.weather_radius.clamp(3, 10) as u32;
            return self
                .rain
                .tick(&mut self.particles, camera, rain, radius, &source, &world);
        }
        None
    }
}

pub fn cloud_settings(options: &Options) -> CloudSettings {
    CloudSettings {
        mode: match options.render_clouds {
            CloudStatus::Off => CloudMode::Off,
            CloudStatus::Fast => CloudMode::Flat,
            CloudStatus::Fancy => CloudMode::Fancy,
        },
        range_chunks: options.cloud_range.clamp(2, 128) as u32,
    }
}

/// Vanilla's border wall look: status colour and `(1 - distance / render distance)^4` opacity.
pub fn border_visual(
    state: &ClientState,
    camera: [f64; 3],
    distance: f64,
    partial: f32,
    millis: u128,
) -> Option<WorldBorderVisual> {
    let b = &state.world.border;
    let half = b.size_at(partial) / 2.0;
    let to_border = b.distance_to(camera[0], camera[2]);
    let alpha = (1.0 - to_border / distance).powi(4).clamp(0.0, 1.0) as f32;
    if !half.is_finite() || !alpha.is_finite() || alpha <= 0.0 {
        return None;
    }
    let rgb = if b.lerp_ticks_left <= 0 {
        0x20A0FF
    } else if b.target_size < b.from_size {
        0xFF3030
    } else {
        0x40FF80
    };
    let channel = |shift: u32| ((rgb >> shift) & 0xFF) as f32 / 255.0;
    Some(WorldBorderVisual {
        min_x: b.center_x - half,
        max_x: b.center_x + half,
        min_z: b.center_z - half,
        max_z: b.center_z + half,
        color: [channel(16), channel(8), channel(0)],
        alpha,
        scroll: (millis % 3000) as f32 / 3000.0,
    })
}

pub fn camera_in_water(chunks: &mcr_world::ChunkStore, pos: [f64; 3]) -> bool {
    mcr_physics::camera_in_water(chunks, mcr_physics::PhysicsData::v26_3(), pos)
}

/// Vanilla's underwater fog: dark blue, starting behind the camera, opening up over 30 seconds.
pub fn apply_underwater(env: &mut Environment, ticks_under: u32, sky_light: f32) {
    let vision = (ticks_under.min(600) as f32 / 600.0).max(0.25);
    let light = 0.1 + 0.9 * (sky_light / 15.0).clamp(0.0, 1.0) * env.sky_factor;
    env.fog_color = [0.02 * light, 0.02 * light, 0.2 * light];
    env.fog_start = -8.0;
    env.fog_end = 96.0 * vision;
}

pub fn environment(
    state: &ClientState,
    render_distance: i32,
    flash: bool,
    brightness: f32,
) -> Environment {
    let mut env = match state.dimension.name.as_str() {
        "minecraft:the_nether" => Environment::nether(),
        "minecraft:the_end" => Environment::end(),
        "minecraft:overworld" => {
            let w = &state.world;
            Environment::overworld(
                w.day_time().unwrap_or(6000).max(0) as u64,
                w.rain_level,
                w.thunder_level,
            )
        }
        _ => Environment::default(),
    };
    env = env.with_lightning_flash(flash);
    env.set_render_distance_chunks(render_distance.max(2) as u32);
    env.brightness = if brightness.is_finite() {
        brightness.clamp(0.0, 1.0)
    } else {
        0.5
    };
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_particles_spawn_with_real_assets() {
        let jar = std::path::Path::new("../../.cache/versions/26.3/client.jar");
        if !jar.exists() {
            return;
        }
        let assets = Arc::new(Assets::load(jar).unwrap());
        let data = mcr_data::v26_3();
        let stone = data.block_by_name("minecraft:stone").unwrap();
        let state = data.block_info(stone).unwrap().default_state as i32;
        let provider = AppParticleAssets::new(assets.clone());
        let (image, _) = provider.block_texture(state).expect("stone texture");
        assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);
        let mut fx = WorldEffects::new(assets);
        fx.particles
            .block_break([0, 0, 0], state, &mcr_particles::OpenWorld);
        assert!(fx.particles.count() > 0);
    }

    #[test]
    fn grass_block_breaks_into_untinted_dirt_particles() {
        let jar = std::path::Path::new("../../.cache/versions/26.3/client.jar");
        if !jar.exists() {
            return;
        }
        let provider = AppParticleAssets::new(Arc::new(Assets::load(jar).unwrap()));
        let data = mcr_data::v26_3();
        let state = |name: &str| {
            let block = data.block_by_name(name).unwrap();
            data.block_info(block).unwrap().default_state as i32
        };
        let (grass, tint) = provider
            .block_texture(state("minecraft:grass_block"))
            .unwrap();
        let (dirt, _) = provider.block_texture(state("minecraft:dirt")).unwrap();
        assert!(
            grass.rgba == dirt.rgba,
            "grass block particles must use block/dirt"
        );
        assert_eq!(tint, [1.0; 3]);
        let (_, leaves) = provider
            .block_texture(state("minecraft:oak_leaves"))
            .unwrap();
        assert_ne!(leaves, [1.0; 3]);
    }

    #[test]
    fn cloud_status_maps_to_modes() {
        let mut o = Options {
            render_clouds: CloudStatus::Off,
            ..Options::default()
        };
        assert_eq!(cloud_settings(&o).mode, CloudMode::Off);
        o.render_clouds = CloudStatus::Fast;
        assert_eq!(cloud_settings(&o).mode, CloudMode::Flat);
        o.cloud_range = 9999;
        assert_eq!(cloud_settings(&o).range_chunks, 128);
    }
}

#[cfg(test)]
mod underwater_tests {
    use super::*;

    #[test]
    fn underwater_fog_opens_over_thirty_seconds() {
        let mut env = Environment::overworld(6000, 0.0, 0.0);
        apply_underwater(&mut env, 0, 15.0);
        assert!((env.fog_end - 24.0).abs() < 1e-3 && env.fog_start < 0.0);
        apply_underwater(&mut env, u32::MAX, 15.0);
        assert!((env.fog_end - 96.0).abs() < 1e-3);
        assert!(env.fog_color[2] > env.fog_color[0]);
    }
}

fn is_air(state: &ClientState, at: [f64; 3]) -> bool {
    let [x, y, z] = at.map(|v| v.floor() as i32);
    state.chunks.block(x, y, z).is_none_or(|b| {
        mcr_data::v26_3()
            .block_state(u32::from(b.0))
            .is_none_or(|s| s.is_air())
    })
}
