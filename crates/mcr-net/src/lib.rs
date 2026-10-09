//! Frame transport: VarInt framing, zlib compression and AES-128-CFB8 over blocking sockets.

mod addr;
mod cipher;
mod codec;
mod conn;
mod dns;
mod error;
mod pool;
mod zlib;

pub use addr::{AddressError, DEFAULT_PORT, ServerAddress};
pub use codec::{
    FrameDecoder, FrameEncoder, InboundFrame, MAX_FRAME_LEN, MAX_UNCOMPRESSED_LEN, SECRET_LEN,
    Switch,
};
pub use conn::{ConnectOptions, Connection, Event, InboundHook};
pub use error::NetError;
pub use pool::{BufPool, PooledBuf};
pub use zlib::{Deflater, Inflater};
