//! Reader for vanilla Anvil world saves: region files, chunk NBT and `level.dat`.

mod chunk;
mod level;
mod region;
mod resolve;
mod world;

pub use chunk::SUPPORTED_DATA_VERSION;
pub use chunk::{ChunkStatus, Converter, ReadOptions, Stats, StatsSnapshot};
pub use level::{LevelSummary, list_worlds};
pub use region::RegionFile;
pub use resolve::{BiomeTable, BlockResolver};
pub use world::{Dimension, LoadedChunk, World};

pub type Result<T> = std::result::Result<T, AnvilError>;

#[derive(Debug, thiserror::Error)]
pub enum AnvilError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Nbt(#[from] mcr_nbt::NbtError),
    #[error("region file of {0} bytes is shorter than its header")]
    TruncatedHeader(usize),
    #[error("chunk ({lx}, {lz}): {reason}")]
    BadSector {
        lx: usize,
        lz: usize,
        reason: &'static str,
    },
    #[error("unsupported chunk compression {0}")]
    UnsupportedCompression(u8),
    #[error("corrupt lz4 stream: {0}")]
    BadLz4(&'static str),
    #[error("decompressed chunk exceeds the size limit")]
    TooLarge,
    #[error("malformed chunk: {0}")]
    Malformed(&'static str),
    #[error(transparent)]
    World(#[from] mcr_world::WorldError),
    #[error("not a world: {0}")]
    NotAWorld(String),
}
