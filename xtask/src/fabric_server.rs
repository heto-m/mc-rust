//! `cargo xtask fabric-server`: a Fabric dedicated server with a pack's server jars, for testing mod mode.
//! Dev only: its mods reach the network unfenced; singleplayer's managed server runs behind the bridge's proxy.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use mcr_singleplayer::fabric::{FabricServer, mods_list};
use serde_json::json;

use crate::fabric::{LOADER_VERSION, fetch_one, parse_profile};
use crate::jdk::resolve_java;
use crate::modpack::{install_overrides, server_jars};
use crate::net::get_text;
use crate::paths::Layout;
use crate::server::{EulaAction, eula_action, memory_capped_to, render_properties};

const META_URL: &str = "https://meta.fabricmc.net/v2/versions/loader";
/// The pack's overrides a server reads; the rest is client content.
const SERVER_OVERRIDES: &[&str] = &["config", "defaultconfigs", "kubejs", "datapacks"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerProfile {
    pub main_class: String,
    pub class_path: Vec<PathBuf>,
}

/// Fabric's server profile for `game` and its libraries, fetched once into `.cache/fabric/`.
/// mcr's managed server reads the `server.json` written here.
pub fn ensure_profile(layout: &Layout, game: &str) -> Result<ServerProfile> {
    let fabric = layout.fabric_dir();
    let manifest = fabric.join("server.json");
    if let Ok(text) = fs::read_to_string(&manifest)
        && let Ok(p) = parse_server_manifest(&text, &layout.cache_dir())
        && p.class_path.iter().all(|p| p.is_file())
    {
        return Ok(p);
    }
    let url = format!("{META_URL}/{game}/{LOADER_VERSION}/server/json");
    println!("[fabric-server] profile {game} / loader {LOADER_VERSION}");
    let text = get_text(&url)?;
    let profile = parse_profile(&text)?;
    fs::create_dir_all(&fabric)?;
    fs::write(
        fabric.join(format!("server-profile-{game}-{LOADER_VERSION}.json")),
        &text,
    )?;
    let mut class_path = Vec::new();
    for lib in &profile.libraries {
        let dest = fabric.join("libraries").join(&lib.path);
        fetch_one(lib, &dest, false)?;
        class_path.push(dest);
    }
    let cache = layout.cache_dir();
    let rel = |p: &Path| -> Result<String> {
        Ok(p.strip_prefix(&cache)
            .context("library outside the cache")?
            .to_str()
            .context("non UTF-8 path")?
            .replace('\\', "/"))
    };
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&json!({
            "format": 1,
            "game_version": game,
            "loader_version": LOADER_VERSION,
            "main_class": profile.main_class,
            "class_path": class_path.iter().map(|p| rel(p)).collect::<Result<Vec<_>>>()?,
            "server_jar": rel(&layout.server_jar(game))?,
        }))?,
    )?;
    Ok(ServerProfile {
        main_class: profile.main_class,
        class_path,
    })
}

pub fn parse_server_manifest(text: &str, cache: &Path) -> Result<ServerProfile> {
    let p = FabricServer::parse_for(text, cache, Some(LOADER_VERSION))?;
    Ok(ServerProfile {
        main_class: p.main_class,
        class_path: p.class_path,
    })
}

#[derive(Debug)]
pub struct Options {
    pub pack: Option<String>,
    pub mods: Option<PathBuf>,
    pub port: u16,
    pub dir: Option<PathBuf>,
    pub heap_mib: u32,
    pub memory: String,
}

pub fn run(layout: &Layout, game: &str, opts: &Options) -> Result<()> {
    if opts.port == 25565 {
        bail!("port 25565 is reserved for the owner's server");
    }
    let (mods_dir, overrides, name) = match (&opts.pack, &opts.mods) {
        (Some(slug), None) => {
            let pack = layout.packs_dir().join(slug);
            (
                pack.join("mods"),
                Some(pack.join("overrides")),
                slug.clone(),
            )
        }
        (None, Some(dir)) => (dir.clone(), None, "mods".to_owned()),
        _ => bail!("pass exactly one of --pack <slug> or --mods <dir>"),
    };
    if !mods_dir.is_dir() {
        bail!(
            "{} is missing; run `cargo xtask modpack-fetch <zip>` first",
            mods_dir.display()
        );
    }
    let profile = ensure_profile(layout, game)?;
    let server_jar = layout.server_jar(game);
    if !server_jar.is_file() {
        bail!(
            "{} is missing; run `cargo xtask fetch` first",
            server_jar.display()
        );
    }
    let dir = opts
        .dir
        .clone()
        .unwrap_or_else(|| layout.fabric_server_dir(&name));
    fs::create_dir_all(&dir)?;
    // The server runs inside `dir`, so a relative path would miss its own mod list.
    let dir = std::path::absolute(&dir)?;
    let eula_path = dir.join("eula.txt");
    match eula_action(
        std::env::var("MCR_ACCEPT_EULA").ok().as_deref(),
        fs::read_to_string(&eula_path).ok().as_deref(),
    ) {
        EulaAction::Write => fs::write(&eula_path, "eula=true\n")?,
        EulaAction::Keep => {}
        EulaAction::Refuse => bail!(
            "accept the Minecraft EULA (https://aka.ms/MinecraftEULA) in {} or set MCR_ACCEPT_EULA=1",
            eula_path.display()
        ),
    }
    let props = dir.join("server.properties");
    let mut text = render_properties(fs::read_to_string(&props).ok().as_deref(), opts.port);
    if !text.lines().any(|l| l.starts_with("server-ip=")) {
        text.push_str("server-ip=127.0.0.1\n");
    }
    fs::write(&props, text)?;
    if let Some(overrides) = &overrides {
        for sub in SERVER_OVERRIDES {
            install_overrides(&overrides.join(sub), &dir.join(sub), false)?;
        }
    }
    let jars = server_jars(&mods_dir);
    let list = dir.join("mcr-mods.txt");
    fs::write(&list, mods_list(&jars)?)?;
    println!(
        "[fabric-server] {} jars from {}, port {}, dir {}",
        jars.len(),
        mods_dir.display(),
        opts.port,
        dir.display()
    );
    let class_path = std::env::join_paths(&profile.class_path)?;
    let java = resolve_java(layout)?;
    let mut cmd = memory_capped_to(&java, &opts.memory);
    cmd.arg(format!("-Xmx{}M", opts.heap_mib))
        .arg("-Xms512M")
        .arg("-Djava.awt.headless=true")
        .arg(format!("-Dfabric.gameJarPath={}", server_jar.display()))
        .arg(format!(
            "-DbundlerRepoDir={}",
            layout.fabric_dir().join("server-bundle").display()
        ))
        .arg(format!("-Dfabric.addMods=@{}", list.display()))
        .arg("-cp")
        .arg(class_path)
        .arg(&profile.main_class)
        .arg("nogui")
        .current_dir(&dir);
    let status = cmd.status()?;
    if !status.success() {
        bail!("server exited with {status}");
    }
    Ok(())
}
