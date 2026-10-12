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
pub use table::{DIRS, Info, Kind, Table, TrialState, flag, fluid, tag};
pub use visual::{MAX_AMBIENT_PARTICLES, Visual, parse_particle};

/// Positions beyond the world border (30 million) are ignored, so offset arithmetic cannot overflow.
pub const WORLD_LIMIT: i32 = 30_000_000;

pub fn in_world(p: [i32; 3]) -> bool {
    p.iter().all(|c| (-WORLD_LIMIT..=WORLD_LIMIT).contains(c))
}

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

/// A player linked to a vault.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VaultPlayer {
    pub block: [i32; 3],
    /// Feet position raised by half the bounding box height.
    pub center: [f64; 3],
}

/// What a conduit is attacking: the eye position and bounding box size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConduitTarget {
    pub eye: [f64; 3],
    pub width: f32,
    pub height: f32,
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
    marker: u16,
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
            marker: u16::MAX,
        }
    }

    /// The block whose states spawn `block_marker` particles (creative mode holding a barrier or light).
    pub fn set_marker(&mut self, block: Option<u16>) {
        self.marker = block.unwrap_or(u16::MAX);
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
        if !in_world(center) {
            return Stats::default();
        }
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
        if !in_world(pos) {
            return Stats::default();
        }
        let mut scan = effects::Scan::new(self, world, sink, sounds);
        scan.sample(pos);
        scan.finish()
    }

    /// Cage smoke and flames of a spawner that has a player in range and a mob to show.
    pub fn spawner_tick(
        &mut self,
        pos: [i32; 3],
        world: &dyn AnimateWorld,
        sink: &mut dyn Sink,
    ) -> Stats {
        if !in_world(pos) {
            return Stats::default();
        }
        let mut scan = effects::Scan::new(self, world, sink, false);
        scan.spawner_cage(pos);
        scan.finish()
    }

    /// `TrialSpawner.tickClient` without the spin: the state's particles and the ambient sound.
    pub fn trial_spawner_tick(
        &mut self,
        pos: [i32; 3],
        state: TrialState,
        ominous: bool,
        world: &dyn AnimateWorld,
        sink: &mut dyn Sink,
        sounds: bool,
    ) -> Stats {
        if !in_world(pos) {
            return Stats::default();
        }
        let mut scan = effects::Scan::new(self, world, sink, sounds);
        scan.trial_spawner(pos, state, ominous);
        scan.finish()
    }

    /// `VaultBlockEntity.Client.tick` without the item spin. `facing` indexes [`DIRS`]; `players`
    /// are `(block position, centre)` of the vault's linked players.
    #[allow(clippy::too_many_arguments)]
    pub fn vault_tick(
        &mut self,
        pos: [i32; 3],
        facing: usize,
        ominous: bool,
        has_display_item: bool,
        players: &[VaultPlayer],
        connection_range: f32,
        world: &dyn AnimateWorld,
        sink: &mut dyn Sink,
        sounds: bool,
    ) -> Stats {
        if !in_world(pos) {
            return Stats::default();
        }
        let mut scan = effects::Scan::new(self, world, sink, sounds);
        scan.vault(
            pos,
            facing,
            ominous,
            has_display_item,
            players,
            connection_range,
        );
        scan.finish()
    }

    /// `ConduitBlockEntity.animationTick`: nautilus particles from the frame and at the target.
    pub fn conduit_tick(
        &mut self,
        pos: [i32; 3],
        tick_count: u32,
        effect_blocks: &[[i32; 3]],
        target: Option<ConduitTarget>,
        world: &dyn AnimateWorld,
        sink: &mut dyn Sink,
    ) -> Stats {
        if !in_world(pos) {
            return Stats::default();
        }
        let mut scan = effects::Scan::new(self, world, sink, false);
        scan.conduit(pos, tick_count, effect_blocks, target);
        scan.finish()
    }

    /// Smoke over the cooking items of a lit campfire, one client tick. `rotation` is
    /// [`Table::campfire_rotation`] and `slots` tells which of the four item slots are filled.
    pub fn campfire_food_tick(
        &mut self,
        pos: [i32; 3],
        rotation: u8,
        slots: [bool; 4],
        world: &dyn AnimateWorld,
        sink: &mut dyn Sink,
    ) -> Stats {
        if !in_world(pos) {
            return Stats::default();
        }
        let mut scan = effects::Scan::new(self, world, sink, false);
        scan.campfire_food(pos, rotation, slots);
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
        if !in_world(pos) {
            return Stats::default();
        }
        let mut scan = effects::Scan::new(self, world, sink, false);
        scan.campfire_smoke(pos, signal);
        scan.finish()
    }
}
