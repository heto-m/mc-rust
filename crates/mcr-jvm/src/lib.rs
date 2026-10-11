//! Embedded HotSpot JVM host. Keep it free of Minecraft knowledge.

mod dev;
mod doorbell;
mod error;
mod home;
mod host;
mod jni;
mod options;
mod ring;
mod table;
pub mod upcall;
mod vm;

pub use dev::stale_java_jar;
pub use doorbell::Doorbell;
pub use error::JvmError;
pub use home::{HomeSearch, JavaHome, REQUIRED_MAJOR, dev_cache_dir, parse_release_major};
pub use host::{HOST_CALL_PANICKED, HOST_CALL_UNAVAILABLE, HostCallFn};
pub use jni::StaticVoidMethod;
pub use options::JvmOptions;
pub use ring::{
    MAX_CAPACITY, MIN_CAPACITY, Message, PADDING_KIND, PushError, RECORD_HEADER_SIZE, Ring,
    RingError, RingReader, RingWriter,
};
pub use table::{
    ENTRIES, EXTENSION_MAGIC, EntryDesc, ExtensionTable, Flavour, TABLE_HEADER_SIZE, TABLE_MAGIC,
    TABLE_VERSION, UPCALLS, UpcallDesc,
};
pub use vm::{
    DEFAULT_BOOT_CLASS, DESTROY_TIMEOUT, DestroyOutcome, JAVA_THREAD_NAME, JAVA_THREAD_STACK,
    JavaVm, JvmConfig, VmOutcome, exit_code, release_exit,
};
