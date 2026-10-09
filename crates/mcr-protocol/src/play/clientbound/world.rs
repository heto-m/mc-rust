use mcr_buf::{
    BlockPos, ByteArray, Decode, DecodeError, Encode, IdOr, Identifier, RestBytes, Result,
    read_varlong, write_varlong,
};
use mcr_nbt::NetworkCompound;

use crate::packet;
use crate::types::{ChunkPos, Component, Particle, SoundEvent, Vec3d};

pub mod debug;

pub use debug::{
    DebugBlockValue, DebugChunkValue, DebugEntityValue, DebugEvent, DebugEventData,
    DebugSubscription, DebugUpdate, DebugValue,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum SoundSource {
    Master,
    Music,
    Records,
    Weather,
    Blocks,
    Hostile,
    Neutral,
    Players,
    Ambient,
    Voice,
    Ui,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Encode, Decode)]
pub struct Vec3i {
    #[mcr(varint)]
    pub x: i32,
    #[mcr(varint)]
    pub y: i32,
    #[mcr(varint)]
    pub z: i32,
}

/// Chunk-section coordinates, packed as x (22 bits) | z (22 bits) | y (20 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SectionPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Encode for SectionPos {
    fn encode(&self, out: &mut Vec<u8>) {
        let packed = (i64::from(self.x) & 0x3f_ffff) << 42
            | (i64::from(self.z) & 0x3f_ffff) << 20
            | i64::from(self.y) & 0xf_ffff;
        packed.encode(out);
    }
}

impl Decode<'_> for SectionPos {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let v = i64::decode(input)?;
        Ok(Self {
            x: (v >> 42) as i32,
            y: (v << 44 >> 44) as i32,
            z: (v << 22 >> 42) as i32,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct BundleDelimiter;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct BlockChangedAck {
    #[mcr(varint)]
    pub sequence: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct BlockDestruction {
    #[mcr(varint)]
    pub entity_id: i32,
    pub pos: BlockPos,
    pub progress: u8,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct BlockEntityData {
    pub pos: BlockPos,
    #[mcr(varint)]
    pub block_entity_type: i32,
    pub tag: NetworkCompound,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct BlockEvent {
    pub pos: BlockPos,
    pub b0: u8,
    pub b1: u8,
    #[mcr(varint)]
    pub block: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct BlockUpdate {
    pub pos: BlockPos,
    #[mcr(varint)]
    pub block_state: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct AddTransientBlock {
    pub pos: BlockPos,
    #[mcr(varint)]
    pub block_state: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ChunkBatchFinished {
    #[mcr(varint)]
    pub batch_size: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ChunkBatchStart;

/// Serialised biome containers, one per section; decoded by `mcr-world` with registry sizes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ChunkBiomeData<'a> {
    pub pos: ChunkPos,
    pub buffer: ByteArray<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ChunksBiomes<'a> {
    pub chunks: Vec<ChunkBiomeData<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum DebugSampleType {
    TickTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct DebugSample {
    pub sample: Vec<i64>,
    pub sample_type: DebugSampleType,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ExplosionParticle {
    pub particle: Particle,
    pub scaling: f32,
    pub speed: f32,
    #[mcr(varint)]
    pub weight: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Explode {
    pub center: Vec3d,
    pub radius: f32,
    pub block_count: i32,
    pub player_knockback: Option<Vec3d>,
    pub particle: Particle,
    pub sound: IdOr<SoundEvent>,
    pub block_particles: Vec<ExplosionParticle>,
    pub play_sound: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ForgetLevelChunk {
    pub pos: ChunkPos,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct GameEvent {
    pub event: u8,
    pub param: f32,
}

impl GameEvent {
    pub const NO_RESPAWN_BLOCK_AVAILABLE: u8 = 0;
    pub const START_RAINING: u8 = 1;
    pub const STOP_RAINING: u8 = 2;
    pub const CHANGE_GAME_MODE: u8 = 3;
    pub const WIN_GAME: u8 = 4;
    pub const DEMO_EVENT: u8 = 5;
    pub const PLAY_ARROW_HIT_SOUND: u8 = 6;
    pub const RAIN_LEVEL_CHANGE: u8 = 7;
    pub const THUNDER_LEVEL_CHANGE: u8 = 8;
    pub const PUFFER_FISH_STING: u8 = 9;
    pub const GUARDIAN_ELDER_EFFECT: u8 = 10;
    pub const IMMEDIATE_RESPAWN: u8 = 11;
    pub const LIMITED_CRAFTING: u8 = 12;
    pub const LEVEL_CHUNKS_LOAD_START: u8 = 13;
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct GameTestHighlightPos {
    pub absolute_pos: BlockPos,
    pub relative_pos: BlockPos,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct InitializeBorder {
    pub new_center_x: f64,
    pub new_center_z: f64,
    pub old_size: f64,
    pub new_size: f64,
    #[mcr(varlong)]
    pub lerp_time: i64,
    #[mcr(varint)]
    pub new_absolute_max_size: i32,
    #[mcr(varint)]
    pub warning_blocks: i32,
    #[mcr(varint)]
    pub warning_time: i32,
}

/// `payload` is chunk data (heightmaps, section buffer, block entities) immediately followed by
/// light data; `mcr_world::ChunkData::decode` and `LightData::decode` consume it in that order.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LevelChunkWithLight<'a> {
    pub x: i32,
    pub z: i32,
    pub payload: RestBytes<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LevelEvent {
    pub event: i32,
    pub pos: BlockPos,
    pub data: i32,
    pub global: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ParticleRandomization {
    Default,
    Alternative,
    AlternativeWithSpeed,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct LevelParticles {
    pub particle: Particle,
    pub override_limiter: bool,
    pub always_show: bool,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub x_dist: f32,
    pub y_dist: f32,
    pub z_dist: f32,
    pub x_max_speed: f32,
    pub y_max_speed: f32,
    pub z_max_speed: f32,
    #[mcr(varint)]
    pub count: i32,
    pub randomization: ParticleRandomization,
}

/// Light data for one chunk, decoded by `mcr_world::LightData::decode`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LightUpdate<'a> {
    #[mcr(varint)]
    pub x: i32,
    #[mcr(varint)]
    pub z: i32,
    pub payload: RestBytes<'a>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetBorderCenter {
    pub new_center_x: f64,
    pub new_center_z: f64,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetBorderLerpSize {
    pub old_size: f64,
    pub new_size: f64,
    #[mcr(varlong)]
    pub lerp_time: i64,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetBorderSize {
    pub size: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SetBorderWarningDelay {
    #[mcr(varint)]
    pub warning_delay: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SetBorderWarningDistance {
    #[mcr(varint)]
    pub warning_blocks: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SetCamera {
    #[mcr(varint)]
    pub camera_id: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SetChunkCacheCenter {
    #[mcr(varint)]
    pub x: i32,
    #[mcr(varint)]
    pub z: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SetChunkCacheRadius {
    #[mcr(varint)]
    pub radius: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetDefaultSpawnPosition {
    pub dimension: Identifier,
    pub pos: BlockPos,
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SetSimulationDistance {
    #[mcr(varint)]
    pub simulation_distance: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ClockUpdate {
    #[mcr(varint)]
    pub clock: i32,
    #[mcr(varlong)]
    pub total_ticks: i64,
    pub partial_tick: f32,
    pub rate: f32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetTime {
    pub game_time: i64,
    pub clock_updates: Vec<ClockUpdate>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SoundEntity {
    pub sound: IdOr<SoundEvent>,
    pub source: SoundSource,
    #[mcr(varint)]
    pub entity_id: i32,
    pub volume: f32,
    pub pitch: f32,
    pub seed: i64,
}

/// Coordinates are fixed-point with 3 fractional bits.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Sound {
    pub sound: IdOr<SoundEvent>,
    pub source: SoundSource,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub volume: f32,
    pub pitch: f32,
    pub seed: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopSound {
    pub source: Option<SoundSource>,
    pub name: Option<Identifier>,
}

impl Encode for StopSound {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(u8::from(self.source.is_some()) | u8::from(self.name.is_some()) << 1);
        if let Some(source) = &self.source {
            source.encode(out);
        }
        if let Some(name) = &self.name {
            name.encode(out);
        }
    }
}

impl Decode<'_> for StopSound {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let flags = u8::decode(input)?;
        let source = if flags & 1 != 0 {
            Some(SoundSource::decode(input)?)
        } else {
            None
        };
        let name = if flags & 2 != 0 {
            Some(Identifier::decode(input)?)
        } else {
            None
        };
        Ok(Self { source, name })
    }
}

/// One changed block inside a section: coordinates relative to the section (0..16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionBlockChange {
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub block_state: u32,
}

impl Encode for SectionBlockChange {
    fn encode(&self, out: &mut Vec<u8>) {
        let packed = u64::from(self.block_state) << 12
            | u64::from(self.x & 15) << 8
            | u64::from(self.z & 15) << 4
            | u64::from(self.y & 15);
        write_varlong(out, packed as i64);
    }
}

impl Decode<'_> for SectionBlockChange {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let packed = read_varlong(input)? as u64;
        let state = packed >> 12;
        Ok(Self {
            x: (packed >> 8 & 15) as u8,
            y: (packed & 15) as u8,
            z: (packed >> 4 & 15) as u8,
            block_state: u32::try_from(state).map_err(|_| DecodeError::InvalidValue {
                what: "block state id",
                value: state as i64,
            })?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SectionBlocksUpdate {
    pub section: SectionPos,
    pub changes: Vec<SectionBlockChange>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TestInstanceBlockStatus {
    pub status: Component,
    pub size: Option<Vec3i>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TickingState {
    pub tick_rate: f32,
    pub is_frozen: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TickingStep {
    #[mcr(varint)]
    pub tick_steps: i32,
}

#[cfg(test)]
mod tests;

packet!(BundleDelimiter, "bundle_delimiter", Clientbound);
packet!(BlockChangedAck, "block_changed_ack", Clientbound);
packet!(BlockDestruction, "block_destruction", Clientbound);
packet!(BlockEntityData, "block_entity_data", Clientbound);
packet!(BlockEvent, "block_event", Clientbound);
packet!(BlockUpdate, "block_update", Clientbound);
packet!(AddTransientBlock, "add_transient_block", Clientbound);
packet!(ChunkBatchFinished, "chunk_batch_finished", Clientbound);
packet!(ChunkBatchStart, "chunk_batch_start", Clientbound);
packet!(ChunksBiomes<'a>, "chunks_biomes", Clientbound);
packet!(DebugSample, "debug_sample", Clientbound);
packet!(Explode, "explode", Clientbound);
packet!(ForgetLevelChunk, "forget_level_chunk", Clientbound);
packet!(GameEvent, "game_event", Clientbound);
packet!(GameTestHighlightPos, "game_test_highlight_pos", Clientbound);
packet!(InitializeBorder, "initialize_border", Clientbound);
packet!(
    LevelChunkWithLight<'a>,
    "level_chunk_with_light",
    Clientbound
);
packet!(LevelEvent, "level_event", Clientbound);
packet!(LevelParticles, "level_particles", Clientbound);
packet!(LightUpdate<'a>, "light_update", Clientbound);
packet!(SetBorderCenter, "set_border_center", Clientbound);
packet!(SetBorderLerpSize, "set_border_lerp_size", Clientbound);
packet!(SetBorderSize, "set_border_size", Clientbound);
packet!(
    SetBorderWarningDelay,
    "set_border_warning_delay",
    Clientbound
);
packet!(
    SetBorderWarningDistance,
    "set_border_warning_distance",
    Clientbound
);
packet!(SetCamera, "set_camera", Clientbound);
packet!(SetChunkCacheCenter, "set_chunk_cache_center", Clientbound);
packet!(SetChunkCacheRadius, "set_chunk_cache_radius", Clientbound);
packet!(
    SetDefaultSpawnPosition,
    "set_default_spawn_position",
    Clientbound
);
packet!(
    SetSimulationDistance,
    "set_simulation_distance",
    Clientbound
);
packet!(SetTime, "set_time", Clientbound);
packet!(SoundEntity, "sound_entity", Clientbound);
packet!(Sound, "sound", Clientbound);
packet!(StopSound, "stop_sound", Clientbound);
packet!(SectionBlocksUpdate, "section_blocks_update", Clientbound);
packet!(
    TestInstanceBlockStatus,
    "test_instance_block_status",
    Clientbound
);
packet!(TickingState, "ticking_state", Clientbound);
packet!(TickingStep, "ticking_step", Clientbound);
