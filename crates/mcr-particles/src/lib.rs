//! CPU-simulated particles drawn as GPU-instanced billboards.
//!
//! App integration:
//! 1. Once the resources load: `ParticleAtlas::load(&read)` then
//!    `ParticleEngine::new(atlas, Box::new(assets))`. `assets` implements [`ParticleAssets`] to hand out
//!    block, item and named textures as RGBA images.
//! 2. On `level_particles`: `engine.emit_packet(&packet)`. For block break and crack feedback call
//!    `engine.block_break(pos, state, &world)` and `engine.block_crack(pos, face, state, &world)`.
//! 3. Every client tick: `engine.tick(&world)`; every frame `engine.set_camera(eye)` so far particles are
//!    skipped. `world` implements [`ParticleWorld`] for collision, light and fluid queries.
//! 4. In the overlay callback, before the UI: `engine.record(ctx, partial, renderer.environment())`.
//! 5. While raining: `weather::RainTicker::tick(..)` once per tick.

mod atlas;
mod names;
mod render;
mod rng;
mod sim;
pub mod weather;

use std::collections::HashMap;

use mcr_protocol::play::clientbound::world::{LevelParticles, ParticleRandomization};
use mcr_protocol::types::particle::{BlockParticleKind, Particle, PositionSource};
use mcr_render::glam::DVec3;
use mcr_render::{EffectImage, Environment, OverlayCtx};

pub use atlas::{ParticleAtlas, Uv};
pub use names::PARTICLE_NAMES;
pub use sim::{POOL_CAP, Spark};

use atlas::{DynamicSlots, to_slot};
use render::Gpu;
use rng::Rng;
use sim::{Data, Kind, P, Pool, Scratch, Spawn, Target, Tick};

const MAX_EMIT: i32 = 1024;
const MAX_VELOCITY: f64 = 64.0;
const MAX_DISTANCE_SQ: f64 = 32.0 * 32.0;
const MAX_BREAK_PARTICLES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fluid {
    Water,
    Lava,
}

/// World queries the simulation needs. All coordinates are block positions; boxes are block-local
/// `[min_x, min_y, min_z, max_x, max_y, max_z]`.
pub trait ParticleWorld {
    fn collision_boxes(&self, _x: i32, _y: i32, _z: i32, _out: &mut Vec<[f32; 6]>) {}
    /// `(block, sky)` light levels, 0 to 15.
    fn light(&self, _x: i32, _y: i32, _z: i32) -> (u8, u8) {
        (0, 15)
    }
    /// The fluid in the block and the height of its surface (0 to 1).
    fn fluid(&self, _x: i32, _y: i32, _z: i32) -> Option<(Fluid, f32)> {
        None
    }
    fn is_air(&self, _x: i32, _y: i32, _z: i32) -> bool {
        true
    }
    /// A still water source, where sulfur bubbles rise.
    fn is_water_source(&self, x: i32, y: i32, z: i32) -> bool {
        matches!(self.fluid(x, y, z), Some((Fluid::Water, _)))
    }
    /// An entity's position, for vibrations that travel to it.
    fn entity_position(&self, _id: i32) -> Option<[f64; 3]> {
        None
    }
}

/// An empty world: no collision, full sky light, no fluids.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenWorld;

impl ParticleWorld for OpenWorld {}

pub trait ParticleAssets {
    /// The block's particle sprite and the terrain particle tint (white when untinted).
    fn block_texture(&self, state: i32) -> Option<(EffectImage, [f32; 3])>;
    fn item_texture(&self, item: i32) -> Option<EffectImage>;
    /// A texture by path under `textures/`, like `item/slime_ball`.
    fn named_texture(&self, path: &str) -> Option<EffectImage>;
    /// The `0xRRGGBB` colour of `falling_dust` for a block state.
    fn block_dust_color(&self, _state: i32) -> Option<u32> {
        None
    }
}

/// The particles video setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Density {
    #[default]
    All,
    Decreased,
    Minimal,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoAssets;

impl ParticleAssets for NoAssets {
    fn block_texture(&self, _: i32) -> Option<(EffectImage, [f32; 3])> {
        None
    }
    fn item_texture(&self, _: i32) -> Option<EffectImage> {
        None
    }
    fn named_texture(&self, _: &str) -> Option<EffectImage> {
        None
    }
}

#[allow(missing_debug_implementations)]
pub struct ParticleEngine {
    atlas: ParticleAtlas,
    assets: Box<dyn ParticleAssets>,
    slots: DynamicSlots,
    uploads: Vec<(u32, Vec<u8>)>,
    tints: HashMap<u64, [f32; 3]>,
    dust_colors: HashMap<i32, u32>,
    pools: [Vec<P>; 3],
    kinds: Vec<(Kind, Pool)>,
    limit_overrides: Vec<bool>,
    rng: Rng,
    spawns: Vec<Spawn>,
    scratch: Scratch,
    camera: Option<DVec3>,
    density: Density,
    spores: usize,
    elder_guardian: bool,
    gpu: Option<Gpu>,
}

fn finite3(v: [f64; 3]) -> bool {
    v.iter().all(|c| c.is_finite())
}

/// Vanilla `ParticleType#getOverrideLimiter` by registry id.
pub fn overrides_limiter(type_id: i32) -> bool {
    usize::try_from(type_id)
        .ok()
        .and_then(|i| PARTICLE_NAMES.get(i))
        .is_some_and(|n| sim::overrides_limiter(n))
}

impl ParticleEngine {
    pub fn new(atlas: ParticleAtlas, assets: Box<dyn ParticleAssets>) -> Self {
        Self {
            atlas,
            assets,
            slots: DynamicSlots::default(),
            uploads: Vec::new(),
            tints: HashMap::new(),
            dust_colors: HashMap::new(),
            pools: [Vec::new(), Vec::new(), Vec::new()],
            kinds: PARTICLE_NAMES.iter().map(|n| sim::kind_of(n)).collect(),
            limit_overrides: PARTICLE_NAMES
                .iter()
                .map(|n| sim::overrides_limiter(n))
                .collect(),
            rng: Rng::new(0x9e37_79b9_7f4a_7c15),
            spawns: Vec::new(),
            scratch: Scratch::new(),
            camera: None,
            density: Density::All,
            spores: 0,
            elder_guardian: false,
            gpu: None,
        }
    }

    pub fn set_camera(&mut self, position: DVec3) {
        self.camera = position.is_finite().then_some(position);
    }

    pub fn set_density(&mut self, density: Density) {
        self.density = density;
    }

    pub fn count(&self) -> usize {
        self.pools.iter().map(Vec::len).sum()
    }

    pub fn clear(&mut self) {
        self.pools.iter_mut().for_each(Vec::clear);
        self.spores = 0;
    }

    pub fn atlas(&self) -> &ParticleAtlas {
        &self.atlas
    }

    /// Whether an `elder_guardian` particle arrived since the last call; the caller shows the ghost.
    pub fn take_elder_guardian(&mut self) -> bool {
        std::mem::take(&mut self.elder_guardian)
    }

    fn rect(
        &mut self,
        key: u64,
        load: impl FnOnce(&dyn ParticleAssets) -> Option<(EffectImage, [f32; 3])>,
    ) -> Option<(Uv, [f32; 3])> {
        if let Some(uv) = self.slots.lookup(key) {
            return Some((uv, self.tints.get(&key).copied().unwrap_or([1.0; 3])));
        }
        let (image, tint) = load(self.assets.as_ref())?;
        let (slot, uv) = self.slots.allocate(key);
        self.uploads.push((slot, to_slot(&image)));
        self.tints.insert(key, tint);
        Some((uv, tint))
    }

    fn dust_color(&mut self, state: i32) -> u32 {
        if let Some(&c) = self.dust_colors.get(&state) {
            return c;
        }
        let c = self.assets.block_dust_color(state).unwrap_or(0xFFFFFF);
        if self.dust_colors.len() < 4096 {
            self.dust_colors.insert(state, c);
        }
        c
    }

    fn data_for(&mut self, particle: &Particle) -> (u16, Data) {
        let id = particle.type_id();
        let id16 = u16::try_from(id).unwrap_or(0);
        let data = match particle {
            Particle::BlockState { kind, state } => {
                let key = *state as u32 as u64;
                let marker_key = (3u64 << 40) | key;
                match kind {
                    BlockParticleKind::Block
                    | BlockParticleKind::BlockCrumble
                    | BlockParticleKind::DustPillar => self
                        .rect(key, |a| a.block_texture(*state))
                        .map_or(Data::None, |(rect, tint)| Data::Rect { rect, tint }),
                    BlockParticleKind::BlockMarker => self
                        .rect(marker_key, |a| {
                            a.block_texture(*state).map(|(img, _)| (img, [1.0; 3]))
                        })
                        .map_or(Data::None, |(rect, tint)| Data::Rect { rect, tint }),
                    BlockParticleKind::FallingDust => Data::Argb(self.dust_color(*state)),
                }
            }
            Particle::Item(item) => {
                let key = (1u64 << 40) | item.item as u32 as u64;
                self.rect(key, |a| a.item_texture(item.item).map(|i| (i, [1.0; 3])))
                    .map_or(Data::None, |(rect, tint)| Data::Rect { rect, tint })
            }
            Particle::Color { argb, .. } => Data::Argb(*argb as u32),
            Particle::Spell { color, power, .. } => Data::Spell {
                argb: *color as u32,
                power: *power,
            },
            Particle::DragonBreath { power } => Data::Power(*power),
            Particle::SculkCharge { roll } => Data::Roll(*roll),
            Particle::Shriek { delay } => Data::Delay(*delay),
            Particle::Geyser { water_blocks, .. } => Data::Geyser {
                water_blocks: *water_blocks,
                impulse: 0.0,
            },
            Particle::GeyserBase {
                water_blocks,
                burst_impulse_base,
                ..
            } => Data::Geyser {
                water_blocks: *water_blocks,
                impulse: *burst_impulse_base,
            },
            Particle::Vibration {
                source,
                arrival_ticks,
            } => Data::Vibration {
                target: match source {
                    PositionSource::Block(p) => Target::Block([p.x, p.y, p.z]),
                    PositionSource::Entity {
                        entity_id,
                        y_offset,
                    } => Target::Entity {
                        id: *entity_id,
                        y_offset: *y_offset,
                    },
                },
                ticks: *arrival_ticks,
            },
            Particle::Trail {
                target,
                color,
                duration,
            } => {
                let t = DVec3::new(target.x, target.y, target.z);
                if t.is_finite() {
                    Data::Trail {
                        target: t,
                        color: *color as u32,
                        ticks: *duration,
                    }
                } else {
                    Data::None
                }
            }
            Particle::Dust { color, scale } => Data::Dust {
                color: *color as u32,
                scale: *scale,
            },
            Particle::DustColorTransition { from, to, scale } => Data::Transition {
                from: *from as u32,
                to: *to as u32,
                scale: *scale,
            },
            Particle::Simple(_) => {
                let named = match names::PARTICLE_NAMES.get(usize::from(id16)) {
                    Some(&"item_slime") => Some("item/slime_ball"),
                    Some(&"item_cobweb") => Some("block/cobweb"),
                    Some(&"item_snowball") => Some("item/snowball"),
                    _ => None,
                };
                match named {
                    Some(path) => {
                        let key = (2u64 << 40) | u64::from(id16);
                        self.rect(key, |a| a.named_texture(path).map(|i| (i, [1.0; 3])))
                            .map_or(Data::None, |(rect, tint)| Data::Rect { rect, tint })
                    }
                    None => Data::None,
                }
            }
            Particle::Modded { .. } => Data::None,
        };
        (id16, data)
    }

    fn add(&mut self, spawn: Spawn) {
        let Some(&(kind, pool)) = self.kinds.get(usize::from(spawn.type_id)) else {
            return;
        };
        if kind == Kind::ElderGuardian {
            self.elder_guardian = true;
            return;
        }
        let spore = kind == Kind::Suspended(sim::Suspended::SporeAir);
        if spore && self.spores >= sim::SPORE_LIMIT {
            return;
        }
        let list = &mut self.pools[pool as usize];
        if list.len() >= POOL_CAP {
            return;
        }
        let mut spawn = spawn;
        spawn.aux = spawn
            .aux
            .clamp(DVec3::splat(-MAX_VELOCITY), DVec3::splat(MAX_VELOCITY));
        if let Some(p) = sim::construct(&spawn, kind, &self.atlas, &mut self.rng) {
            list.push(p);
            self.spores += usize::from(spore);
        }
    }

    /// Vanilla `ClientLevel#calculateParticleLevel`, rolled once per particle.
    fn roll_density(&mut self, always_show: bool) -> Density {
        let mut level = self.density;
        if always_show && level == Density::Minimal && self.rng.below(10) == 0 {
            level = Density::Decreased;
        }
        if level == Density::Decreased && self.rng.below(3) == 0 {
            level = Density::Minimal;
        }
        level
    }

    /// Vanilla `ClientLevel#doAddParticle`: limiter-overriding particles always spawn, others only
    /// within 32 blocks of the camera and unless the particle setting drops them.
    fn add_checked(
        &mut self,
        (type_id, data): (u16, Data),
        override_limiter: bool,
        always_show: bool,
        pos: DVec3,
        vel: DVec3,
    ) {
        let level = self.roll_density(always_show);
        let forced = override_limiter
            || self
                .limit_overrides
                .get(usize::from(type_id))
                .copied()
                .unwrap_or(false);
        if !forced {
            if let Some(cam) = self.camera
                && cam.distance_squared(pos) > MAX_DISTANCE_SQ
            {
                return;
            }
            if level == Density::Minimal {
                return;
            }
        }
        self.add(Spawn {
            type_id,
            pos,
            aux: vel,
            data,
        });
    }

    /// Mirrors `level_particles` with one speed for all axes, ignoring the particle setting and the
    /// distance limit: a positive `count` scatters particles around `pos` by gaussian `delta` with
    /// gaussian velocity of magnitude `speed`; zero spawns a single particle whose velocity is
    /// `delta * speed`.
    pub fn emit(
        &mut self,
        particle: &Particle,
        pos: [f64; 3],
        delta: [f32; 3],
        speed: f32,
        count: i32,
    ) {
        let density = std::mem::replace(&mut self.density, Density::All);
        self.emit_packet(&LevelParticles {
            particle: particle.clone(),
            override_limiter: true,
            always_show: false,
            x: pos[0],
            y: pos[1],
            z: pos[2],
            x_dist: delta[0],
            y_dist: delta[1],
            z_dist: delta[2],
            x_max_speed: speed,
            y_max_speed: speed,
            z_max_speed: speed,
            count,
            randomization: ParticleRandomization::Default,
        });
        self.density = density;
    }

    /// Vanilla `level.addParticle`: one particle with exact velocity, subject to the particle setting
    /// and the 32-block limit unless its type overrides the limiter.
    pub fn add_particle(&mut self, particle: &Particle, pos: [f64; 3], vel: [f64; 3]) {
        self.add_particle_with(particle, false, false, pos, vel);
    }

    /// Vanilla `level.addParticle` with explicit `overrideLimiter` and `alwaysShow` flags.
    pub fn add_particle_with(
        &mut self,
        particle: &Particle,
        override_limiter: bool,
        always_show: bool,
        pos: [f64; 3],
        vel: [f64; 3],
    ) {
        if !finite3(pos) || !finite3(vel) {
            return;
        }
        let data = self.data_for(particle);
        self.add_checked(
            data,
            override_limiter,
            always_show,
            DVec3::from_array(pos),
            DVec3::from_array(vel),
        );
    }

    /// A firework explosion spark; colours are `0xRRGGBB`.
    pub fn firework_spark(&mut self, pos: [f64; 3], vel: [f64; 3], spark: Spark) {
        let (pos, vel) = (DVec3::from_array(pos), DVec3::from_array(vel));
        if !pos.is_finite() || !vel.is_finite() {
            return;
        }
        self.add(Spawn {
            type_id: sim::type_id("firework"),
            pos,
            aux: vel,
            data: Data::Firework(spark),
        });
    }

    /// The bright flash at a firework burst.
    pub fn firework_flash(&mut self, pos: [f64; 3], rgb: u32) {
        let pos = DVec3::from_array(pos);
        if !pos.is_finite() {
            return;
        }
        self.add(Spawn {
            type_id: sim::type_id("flash"),
            pos,
            aux: DVec3::ZERO,
            data: Data::Argb(0xFF00_0000 | rgb),
        });
    }

    /// Vanilla `handleParticleEvent`, honouring both packet flags and the randomization type.
    pub fn emit_packet(&mut self, packet: &LevelParticles) {
        let pos = [packet.x, packet.y, packet.z];
        let delta = [packet.x_dist, packet.y_dist, packet.z_dist].map(f64::from);
        let speed = [packet.x_max_speed, packet.y_max_speed, packet.z_max_speed].map(f64::from);
        let (override_limiter, always_show) = (packet.override_limiter, packet.always_show);
        let (count, randomization) = (packet.count, packet.randomization);
        if !finite3(pos) || !finite3(delta) || !finite3(speed) {
            return;
        }
        let at = DVec3::from_array(pos);
        let data = self.data_for(&packet.particle);
        let delta = DVec3::from_array(delta);
        let speed = DVec3::from_array(speed);
        if count <= 0 {
            self.add_checked(data, override_limiter, always_show, at, delta * speed);
            return;
        }
        for _ in 0..count.min(MAX_EMIT) {
            let (offset, vel) = match randomization {
                ParticleRandomization::Default => {
                    let mut g = || self.rng.gaussian();
                    let offset = DVec3::new(g(), g(), g()) * delta;
                    (offset, DVec3::new(g(), g(), g()) * speed)
                }
                ParticleRandomization::Alternative
                | ParticleRandomization::AlternativeWithSpeed => {
                    let mut u = || self.rng.f64();
                    let offset = DVec3::new(u(), u(), u()) * delta;
                    let vel = if randomization == ParticleRandomization::AlternativeWithSpeed {
                        DVec3::new(u(), u(), u()) * speed
                    } else {
                        speed
                    };
                    (offset, vel)
                }
            };
            self.add_checked(data, override_limiter, always_show, at + offset, vel);
        }
    }

    /// A single particle of a registry type by name, ignoring the limiter.
    pub fn add_simple(&mut self, name: &str, pos: DVec3, vel: DVec3) {
        if !pos.is_finite() || !vel.is_finite() {
            return;
        }
        self.add(Spawn {
            type_id: sim::type_id(name),
            pos,
            aux: vel,
            data: Data::None,
        });
    }

    fn block_shape(&self, world: &dyn ParticleWorld, pos: [i32; 3]) -> Vec<[f32; 6]> {
        let mut boxes = Vec::new();
        world.collision_boxes(pos[0], pos[1], pos[2], &mut boxes);
        boxes.truncate(16);
        if boxes.is_empty() {
            boxes.push([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        }
        boxes
    }

    /// Terrain particles for a destroyed block: a grid of at least 2 and about 4 per block side.
    pub fn block_break(&mut self, pos: [i32; 3], state: i32, world: &dyn ParticleWorld) {
        let block = Particle::BlockState {
            kind: BlockParticleKind::Block,
            state,
        };
        let (type_id, data) = self.data_for(&block);
        if matches!(data, Data::None) {
            return;
        }
        let mut spawned = 0;
        for b in self.block_shape(world, pos) {
            let size = [b[3] - b[0], b[4] - b[1], b[5] - b[2]];
            if !size.iter().all(|s| s.is_finite()) {
                continue;
            }
            let n = size.map(|s| ((s / 0.25).ceil().max(2.0) as i32).min(8));
            for x in 0..n[0] {
                for y in 0..n[1] {
                    for z in 0..n[2] {
                        if spawned >= MAX_BREAK_PARTICLES {
                            return;
                        }
                        spawned += 1;
                        let f = [
                            (f64::from(x) + 0.5) / f64::from(n[0]),
                            (f64::from(y) + 0.5) / f64::from(n[1]),
                            (f64::from(z) + 0.5) / f64::from(n[2]),
                        ];
                        let p = DVec3::new(
                            f64::from(pos[0]) + f[0] * f64::from(size[0]) + f64::from(b[0]),
                            f64::from(pos[1]) + f[1] * f64::from(size[1]) + f64::from(b[1]),
                            f64::from(pos[2]) + f[2] * f64::from(size[2]) + f64::from(b[2]),
                        );
                        self.add(Spawn {
                            type_id,
                            pos: p,
                            aux: DVec3::new(f[0] - 0.5, f[1] - 0.5, f[2] - 0.5),
                            data,
                        });
                    }
                }
            }
        }
    }

    /// A single hit particle on `face` (0 down, 1 up, 2 north, 3 south, 4 west, 5 east).
    pub fn block_crack(&mut self, pos: [i32; 3], face: u8, state: i32, world: &dyn ParticleWorld) {
        let block = Particle::BlockState {
            kind: BlockParticleKind::Block,
            state,
        };
        let (type_id, data) = self.data_for(&block);
        if matches!(data, Data::None) {
            return;
        }
        let shape = self.block_shape(world, pos);
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for b in &shape {
            for a in 0..3 {
                lo[a] = lo[a].min(b[a]);
                hi[a] = hi[a].max(b[3 + a]);
            }
        }
        let origin = pos.map(f64::from);
        let mut at = [0.0f64; 3];
        for a in 0..3 {
            let span = f64::from(hi[a] - lo[a]) - 0.4;
            at[a] = origin[a] + f64::from(self.rng.f32()) * span + 0.2 + f64::from(lo[a]);
        }
        match face {
            0 => at[1] = origin[1] + f64::from(lo[1]) - 0.1,
            1 => at[1] = origin[1] + f64::from(hi[1]) + 0.1,
            2 => at[2] = origin[2] + f64::from(lo[2]) - 0.1,
            3 => at[2] = origin[2] + f64::from(hi[2]) + 0.1,
            4 => at[0] = origin[0] + f64::from(lo[0]) - 0.1,
            _ => at[0] = origin[0] + f64::from(hi[0]) + 0.1,
        }
        self.add(Spawn {
            type_id,
            pos: DVec3::from_array(at),
            aux: DVec3::ZERO,
            data,
        });
        if let Some(p) = self.pools[Pool::Terrain as usize].last_mut() {
            p.vel.x *= 0.2;
            p.vel.z *= 0.2;
            p.vel.y = (p.vel.y - 0.1) * 0.2 + 0.1;
            p.size *= 0.6;
        }
    }

    /// Advances the simulation one 20 Hz tick.
    pub fn tick(&mut self, world: &dyn ParticleWorld) {
        let mut spawns = std::mem::take(&mut self.spawns);
        spawns.clear();
        for list in &mut self.pools {
            let mut i = 0;
            while i < list.len() {
                let alive = {
                    let mut t = Tick {
                        world,
                        atlas: &self.atlas,
                        rng: &mut self.rng,
                        spawns: &mut spawns,
                        scratch: &mut self.scratch,
                    };
                    sim::tick(&mut list[i], &mut t)
                };
                if alive {
                    i += 1;
                } else {
                    let gone = list.swap_remove(i);
                    if gone.kind == Kind::Suspended(sim::Suspended::SporeAir) {
                        self.spores = self.spores.saturating_sub(1);
                    }
                }
            }
        }
        for s in spawns.drain(..) {
            self.add(s);
        }
        self.spawns = spawns;
    }

    fn flush_uploads(&mut self, ctx: &OverlayCtx<'_>) {
        if let Some(gpu) = &self.gpu {
            for (slot, rgba) in self.uploads.drain(..) {
                gpu.upload_slot(ctx.queue, slot, &rgba);
            }
        }
    }

    /// Draws every live particle into the frame after the world pass, depth-tested against it.
    pub fn record(&mut self, ctx: &mut OverlayCtx<'_>, partial: f32, env: &Environment) {
        if self.count() == 0 {
            return;
        }
        if self.gpu.as_ref().is_none_or(|g| g.format() != ctx.format) {
            if self.gpu.is_some() {
                self.slots = DynamicSlots::default();
                self.uploads.clear();
                self.tints.clear();
                self.pools[Pool::Terrain as usize].clear();
            }
            self.gpu = Some(Gpu::new(ctx, &self.atlas));
        }
        self.flush_uploads(ctx);
        let partial = if partial.is_finite() {
            partial.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if let Some(gpu) = &mut self.gpu {
            gpu.record(ctx, &self.pools, &self.atlas, partial, env);
        }
    }

    /// Builds the instance data without a GPU; used by benchmarks.
    #[doc(hidden)]
    pub fn instance_count(&self, camera: DVec3, partial: f32, env: &Environment) -> usize {
        let lightmap = env.lightmap(1.0);
        let mut out = [Vec::new(), Vec::new(), Vec::new()];
        render::build_instances(
            &self.pools,
            &self.atlas,
            camera,
            partial,
            &lightmap,
            &mut out,
        );
        out.iter().map(Vec::len).sum()
    }
}

#[cfg(test)]
mod tests;
