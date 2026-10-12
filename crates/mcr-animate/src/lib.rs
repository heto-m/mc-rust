//! Vanilla's client-side `animateTick`: the ambient particles and sounds of blocks, fluids and biomes
//! around the player, sampled at random positions each tick through flat per-block-state tables.

mod effects;
mod ids;
mod table;
mod visual;

#[cfg(test)]
mod tests;

use mcr_audio::{FastRandom, SoundSource};
use mcr_data::Version;
use mcr_mesh::{OffsetSpec, offset_spec};
use mcr_protocol::types::particle::Particle;

pub use ids::{Ids, overrides_limiter};
pub use table::{DIRS, Info, Kind, Table, flag, fluid, tag};
pub use visual::{MAX_AMBIENT_PARTICLES, Visual, parse_particle};

pub const SAMPLES_PER_RADIUS: u32 = 667;
pub const RADII: [u32; 2] = [16, 32];
pub const MAX_PARTICLES_PER_TICK: u32 = 2048;
pub const MAX_SOUNDS_PER_TICK: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cue {
    pub event: &'static str,
    pub pos: Option<[f64; 3]>,
    pub volume: f32,
    pub pitch: f32,
    pub source: SoundSource,
}

pub trait Sink {
    fn particle(&mut self, particle: &Particle, pos: [f64; 3], vel: [f64; 3], always: bool);
    fn sound(&mut self, cue: &Cue);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Height {
    WorldSurface,
    MotionBlockingNoLeaves,
}

/// What the scan asks of the loaded level. Unloaded positions answer `None` or the defaults.
pub trait AnimateWorld {
    fn state(&self, x: i32, y: i32, z: i32) -> Option<u16>;

    /// `(block, sky)` light levels, 0 to 15.
    fn light(&self, _x: i32, _y: i32, _z: i32) -> (u8, u8) {
        (0, 15)
    }

    fn raining_at(&self, _x: i32, _y: i32, _z: i32) -> bool {
        false
    }

    fn thundering(&self) -> bool {
        false
    }

    fn game_time(&self) -> i64 {
        0
    }

    /// The first free `y` above the column's top for the heightmap.
    fn height(&self, _kind: Height, _x: i32, _z: i32) -> i32 {
        i32::MIN
    }

    fn sky_darken(&self) -> u8 {
        0
    }

    /// Foliage colour (`0xRRGGBB`) of the biome at the block.
    fn foliage_color(&self, _x: i32, _y: i32, _z: i32) -> u32 {
        0x48B518
    }

    fn creaking_active(&self) -> bool {
        false
    }

    fn has_ambient_particles(&self) -> bool {
        true
    }

    fn ambient_particles(&self, _x: i32, _y: i32, _z: i32) -> &[(Particle, f32)] {
        &[]
    }

    fn dripstone_particle(&self, _x: i32, _y: i32, _z: i32) -> Option<&Particle> {
        None
    }

    fn water_evaporates(&self, _x: i32, _y: i32, _z: i32) -> bool {
        false
    }

    fn firefly_sounds(&self, _x: i32, _y: i32, _z: i32) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub samples: u32,
    pub particles: u32,
    pub sounds: u32,
}

#[derive(Debug)]
pub struct Animator {
    data: &'static Version,
    table: Table,
    ids: Ids,
    rng: FastRandom,
    wither_rose: OffsetSpec,
    dripstone: OffsetSpec,
}

impl Animator {
    pub fn new(data: &'static Version, seed: u64) -> Self {
        Self {
            data,
            table: Table::build(data),
            ids: Ids::new(),
            rng: FastRandom::new(seed),
            wither_rose: offset_spec("wither_rose"),
            dripstone: offset_spec("pointed_dripstone"),
        }
    }

    pub fn table(&self) -> &Table {
        &self.table
    }

    pub fn table_mut(&mut self) -> &mut Table {
        &mut self.table
    }

    pub fn data(&self) -> &'static Version {
        self.data
    }

    /// One client tick around `center`: 667 samples at radius 16 and 667 at radius 32. With `sounds` off
    /// (Java owns world audio) only particles are produced.
    pub fn tick(
        &mut self,
        center: [i32; 3],
        world: &dyn AnimateWorld,
        sink: &mut dyn Sink,
        sounds: bool,
    ) -> Stats {
        let mut scan = effects::Scan::new(self, world, sink, sounds);
        for _ in 0..SAMPLES_PER_RADIUS {
            for radius in RADII {
                let p = scan.random_offset(center, radius);
                scan.sample(p);
            }
        }
        scan.finish()
    }

    /// Runs the effects of one position, for tests and the block entity hooks.
    pub fn sample_at(
        &mut self,
        pos: [i32; 3],
        world: &dyn AnimateWorld,
        sink: &mut dyn Sink,
        sounds: bool,
    ) -> Stats {
        let mut scan = effects::Scan::new(self, world, sink, sounds);
        scan.sample(pos);
        scan.finish()
    }

    /// The rising smoke of a lit campfire's block entity, one client tick.
    pub fn campfire_tick(
        &mut self,
        pos: [i32; 3],
        signal: bool,
        world: &dyn AnimateWorld,
        sink: &mut dyn Sink,
    ) -> Stats {
        let mut scan = effects::Scan::new(self, world, sink, false);
        scan.campfire_smoke(pos, signal);
        scan.finish()
    }
}
