mod assets;
mod checksum;
mod datagen;
mod decompile;
mod fetch;
mod jdk;
mod manifest;
mod net;
mod paths;
mod server;
mod setup;
#[cfg(test)]
mod testutil;

use anyhow::Result;
use clap::{Parser, Subcommand};

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
        Command::Setup { version, force } => {
            let ver = fetch::resolve_version(version.as_deref())?;
            setup::run(&layout, &ver, force)?;
        }
    }
    Ok(())
}
