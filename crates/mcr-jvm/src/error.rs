use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum JvmError {
    #[error("Java home {path} is Java {found}; mcr needs Java {required} or newer")]
    JavaTooOld {
        path: PathBuf,
        found: u32,
        required: u32,
    },
    #[error("{path} is not a usable Java home: {reason}")]
    InvalidHome { path: PathBuf, reason: String },
    #[error("no Java {required} home found ({tried})")]
    NoJavaHome { required: u32, tried: String },
    #[error("failed to load {path}: {source}")]
    Load {
        path: PathBuf,
        #[source]
        source: libloading::Error,
    },
    #[error("a JVM was already started in this process; HotSpot cannot create a second one")]
    AlreadyStarted,
    #[error("JNI_CreateJavaVM failed with code {0}")]
    Create(i32),
    #[error("invalid JVM option or argument {0:?}")]
    InvalidOption(String),
    #[error("boot class {0} not found")]
    BootClassNotFound(String),
    #[error("boot method {0}.main(long, String[]) not found")]
    BootMethodNotFound(String),
    #[error("the Java boot call threw an exception (stack trace on stderr)")]
    BootException,
    #[error("JNI call failed: {0}")]
    Jni(&'static str),
    #[error("failed to spawn the mcr-java thread: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("the mcr-java thread terminated unexpectedly")]
    ThreadLost,
}
