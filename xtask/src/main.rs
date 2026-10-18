mod assets;
mod checksum;
mod datagen;
mod decompile;
mod fabric;
mod fabric_server;
mod fetch;
mod golden;
mod java;
mod jdk;
mod manifest;
mod modpack;
mod neoforge;
mod net;
mod paths;
mod server;
mod setup;
mod soak;
mod soak_churn;
#[cfg(test)]
mod testutil;
mod version_tables;

use anyhow::Result;
use clap::{Parser, Subcommand};
use mcr_modmeta::pins;

use paths::Layout;

#[derive(Parser, Debug)]
#[command(name = "xtask", about = "mc-rust developer tooling")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Download version.json, client.jar and server.jar (latest release by default)
    Fetch {
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Ensure a Java 25 runtime (system java if new enough, else portable Temurin)
    Jdk {
        #[arg(long)]
        force: bool,
    },
    /// Run the vanilla data generator into .cache/versions/<ver>/generated
    Datagen {
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Decompile the client jar with Vineflower into .cache/versions/<ver>/src
    Decompile {
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Run the vanilla server from run/server-<ver>
    Server {
        #[arg(long)]
        version: Option<String>,
        #[arg(long, default_value_t = 25565)]
        port: u16,
        #[arg(long)]
        nogui: bool,
    },
    /// Download the asset index and all objects into .cache/assets
    Assets {
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Download Fabric Loader, its libraries, the vanilla libraries and natives, and the test mods
    FabricFetch {
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Download the pinned NeoForge installer and libraries, patch the client jar and write the launch manifest
    NeoforgeFetch {
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Compile the Java modules under java/ into .cache/java/<module>.jar
    JavaBuild {
        /// Write the jars here instead, leaving the shared cache alone
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// Render the test mod's GPU parity screen with vanilla OpenGL into .cache/golden/<ver>/gui.png
    GoldenGui,
    /// Run `mcr --soak` offscreen against a churned vanilla server and fail on memory or thread growth
    Soak {
        #[arg(long)]
        version: Option<String>,
        /// The mcr binary to soak
        #[arg(long, default_value = "target/debug/mcr")]
        mcr: std::path::PathBuf,
        #[arg(long, default_value_t = 20.0)]
        minutes: f64,
        #[arg(long, default_value_t = 5.0)]
        warmup_minutes: f64,
        #[arg(long, default_value_t = 25567)]
        port: u16,
        /// Largest allowed RSS (and GPU memory) slope after warm-up
        #[arg(long, default_value_t = 1.0)]
        max_slope: f64,
        /// Entities summoned per wave
        #[arg(long, default_value_t = 800)]
        wave: u32,
        /// Kicks and transfers spread over the run
        #[arg(long, default_value_t = 3)]
        disconnects: u32,
        /// Mods directory: soak mod mode and log the JVM's garbage collections
        #[arg(long)]
        mods: Option<std::path::PathBuf>,
        /// Output directory (default run/soak/soak-<time>)
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        /// Memory cap of mcr's own scope
        #[arg(long, default_value = "3G")]
        mcr_memory: String,
    },
    /// Write an older version's packet and registry id tables against the canonical version
    VersionTables {
        #[arg(long)]
        version: String,
        #[arg(long, default_value = "26.3")]
        canonical: String,
    },
    /// Download a CurseForge modpack's files into .cache/packs/<slug> and install its overrides
    ModpackFetch {
        /// The pack zip (manifest.json, overrides/)
        zip: std::path::PathBuf,
        /// mcr game dir that receives the overrides under java/ (default run/packs/<slug>)
        #[arg(long)]
        game_dir: Option<std::path::PathBuf>,
        /// Download again and overwrite installed overrides
        #[arg(long)]
        force: bool,
    },
    /// Run a Fabric dedicated server with a fetched pack's server jars (or a mods dir) on loopback (dev only: no egress fence)
    FabricServer {
        #[arg(long)]
        version: Option<String>,
        /// A pack slug under .cache/packs
        #[arg(long)]
        pack: Option<String>,
        /// A mods dir instead of a pack (the jars a server runs)
        #[arg(long)]
        mods: Option<std::path::PathBuf>,
        #[arg(long, default_value_t = 25576)]
        port: u16,
        /// Server directory (default run/fabric-server-<slug>)
        #[arg(long)]
        dir: Option<std::path::PathBuf>,
        #[arg(long, default_value_t = 3072)]
        heap_mib: u32,
        /// Memory cap of the server's scope
        #[arg(long, default_value = "4G")]
        memory: String,
    },
    /// Run fetch, jdk, datagen and decompile in order
    Setup {
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        force: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let layout = Layout::discover()?;
    match cli.command {
        Command::Fetch { version, force } => {
            fetch::fetch(&layout, version.as_deref(), force)?;
        }
        Command::Jdk { force } => {
            jdk::ensure_jdk(&layout, force)?;
        }
        Command::Datagen { version, force } => {
            let ver = fetch::resolve_version(version.as_deref())?;
            datagen::run(&layout, &ver, force)?;
        }
        Command::Decompile { version, force } => {
            let ver = fetch::resolve_version(version.as_deref())?;
            decompile::run(&layout, &ver, force)?;
        }
        Command::Server {
            version,
            port,
            nogui,
        } => {
            let ver = fetch::resolve_version(version.as_deref())?;
            server::run(&layout, &ver, port, nogui)?;
        }
        Command::Assets { version, force } => {
            let ver = fetch::resolve_version(version.as_deref())?;
            assets::run(&layout, &ver, force)?;
        }
        Command::FabricFetch { version, force } => {
            let ver = fetch::resolve_version(version.as_deref())?;
            fabric::run(&layout, &ver, force)?;
            fabric_server::ensure_profile(&layout, &ver)?;
        }
        Command::NeoforgeFetch { version, force } => {
            let ver = fetch::resolve_version(version.as_deref())?;
            neoforge::run(&layout, &ver, force)?;
        }
        Command::JavaBuild { out } => {
            java::run(&layout, out.as_deref())?;
        }
        Command::GoldenGui => {
            golden::run(&layout)?;
        }
        Command::Soak {
            version,
            mcr,
            minutes,
            warmup_minutes,
            port,
            max_slope,
            wave,
            disconnects,
            mods,
            out,
            mcr_memory,
        } => {
            let ver = version.unwrap_or_else(|| "26.3".to_owned());
            let opts = soak::Options {
                mcr,
                minutes,
                warmup_minutes,
                port,
                max_slope_mib_per_min: max_slope,
                wave,
                disconnects,
                mods,
                out,
                mcr_memory,
            };
            soak::run(&layout, &ver, &opts)?;
        }
        Command::VersionTables { version, canonical } => {
            version_tables::run(&layout, &version, &canonical)?;
        }
        Command::ModpackFetch {
            zip,
            game_dir,
            force,
        } => {
            modpack::run(
                &layout,
                &modpack::FetchOptions {
                    zip,
                    game_dir,
                    force,
                },
            )?;
        }
        Command::FabricServer {
            version,
            pack,
            mods,
            port,
            dir,
            heap_mib,
            memory,
        } => {
            let ver = version.unwrap_or_else(|| mcr_modmeta::resolve::GAME_VERSION.to_owned());
            fabric_server::run(
                &layout,
                &ver,
                &fabric_server::Options {
                    pack,
                    mods,
                    port,
                    dir,
                    heap_mib,
                    memory,
                },
            )?;
        }
        Command::Setup { version, force } => {
            let ver = fetch::resolve_version(version.as_deref())?;
            setup::run(&layout, &ver, force)?;
        }
    }
    Ok(())
}
