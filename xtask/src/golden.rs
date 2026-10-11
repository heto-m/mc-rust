//! `golden-gui`: renders the test mod's GPU parity screen with vanilla's OpenGL backend.
//!
//! The image lands in `.cache/golden/<version>/gui.png` (gitignored: it is derived from Mojang assets).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::jdk;
use crate::paths::Layout;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const TIMEOUT: Duration = Duration::from_secs(300);
const MAX_HEAP: &str = "-Xmx2G";
const SCOPE_MEMORY: &str = "MemoryMax=3G";

pub fn golden_dir(layout: &Layout, version: &str) -> PathBuf {
    layout.cache_dir().join("golden").join(version)
}

fn strings(v: &Value, key: &str) -> Result<Vec<String>> {
    v[key]
        .as_array()
        .with_context(|| format!("launch manifest has no {key}"))?
        .iter()
        .map(|s| s.as_str().map(str::to_owned).context("non-string entry"))
        .collect()
}

fn string(v: &Value, key: &str) -> Result<String> {
    v[key]
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("launch manifest has no {key}"))
}

fn has(program: &str) -> bool {
    Command::new("which")
        .arg(program)
        .output()
        .is_ok_and(|o| o.status.success())
}

fn join(paths: &[PathBuf]) -> Result<String> {
    let parts: Option<Vec<&str>> = paths.iter().map(|p| p.to_str()).collect();
    Ok(parts.context("non UTF-8 path")?.join(":"))
}

pub fn run(layout: &Layout) -> Result<()> {
    let cache = layout.cache_dir();
    let manifest_path = layout.fabric_launch_json();
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).with_context(|| {
            format!(
                "{}: run `cargo xtask fabric-fetch`",
                manifest_path.display()
            )
        })?)?;
    let version = string(&manifest, "game_version")?;
    let testmod = layout.java_out_dir().join("mcr-testmod.jar");
    if !testmod.is_file() {
        bail!(
            "{} is missing: run `cargo xtask java-build`",
            testmod.display()
        );
    }
    let dir = golden_dir(layout, &version);
    let game_dir = dir.join("game");
    std::fs::create_dir_all(&game_dir)?;
    std::fs::write(
        game_dir.join("options.txt"),
        "onboardAccessibility:false\npreferredGraphicsBackend:\"opengl\"\nnarrator:0\nsoundCategory_master:0.0\n",
    )?;
    let out = dir.join("gui.png");
    let _ = std::fs::remove_file(&out);

    let in_cache = |p: String| cache.join(p);
    let mut class_path: Vec<PathBuf> = strings(&manifest, "class_path")?
        .into_iter()
        .map(in_cache)
        .collect();
    let game_jar = in_cache(string(&manifest, "game_jar")?);
    class_path.push(game_jar.clone());
    let mut mods: Vec<PathBuf> = strings(&manifest, "test_mods")?
        .into_iter()
        .map(in_cache)
        .collect();
    mods.push(testmod);
    let natives = in_cache(string(&manifest, "natives_dir")?);
    let natives = natives.to_str().context("non UTF-8 natives path")?;
    let java = jdk::resolve_java(layout)?;

    let headless = has("xvfb-run");
    let mut cmd = Command::new("systemd-run");
    cmd.args([
        "--user",
        "--scope",
        "-q",
        "-p",
        SCOPE_MEMORY,
        "-p",
        "MemorySwapMax=0",
    ])
    .args(["timeout", &TIMEOUT.as_secs().to_string()]);
    if headless {
        cmd.args(["xvfb-run", "-a", "-s", "-screen 0 1920x1080x24"]);
        cmd.env_remove("WAYLAND_DISPLAY")
            .env("SDL_VIDEO_DRIVER", "x11");
    } else {
        println!("[golden] no xvfb-run: opening a real window for a few seconds");
    }
    cmd.arg(&java)
        .arg(MAX_HEAP)
        .arg("-cp")
        .arg(join(&class_path)?)
        .arg(format!("-Dfabric.gameJarPath={}", game_jar.display()))
        .arg("-Dfabric.side=client")
        .arg(format!("-Dfabric.addMods={}", join(&mods)?))
        .arg(format!("-Dorg.lwjgl.librarypath={natives}"))
        .arg(format!("-Djava.library.path={natives}"))
        .arg("-Djava.awt.headless=true")
        .arg(format!("-Dmcr.testmod.gui={}", out.display()))
        .arg("-Dmcr.testmod.gui.quit=true")
        .args(strings(&manifest, "jvm_args")?)
        .arg(string(&manifest, "main_class")?)
        .args([
            "--version",
            &version,
            "--versionType",
            "release",
            "--accessToken",
            "0",
        ])
        .args(["--username", "Golden", "--offlineDeveloperMode"])
        .arg("--gameDir")
        .arg(&game_dir)
        .arg("--assetsDir")
        .arg(in_cache(string(&manifest, "assets_dir")?))
        .args(["--assetIndex", &string(&manifest, "asset_index")?])
        .args([
            "--width",
            &WIDTH.to_string(),
            "--height",
            &HEIGHT.to_string(),
        ])
        .current_dir(&game_dir);
    println!(
        "[golden] capturing the OpenGL reference into {}",
        out.display()
    );
    let status = cmd.status().context("starting the client")?;
    if !out.is_file() {
        bail!(
            "the client exited ({status}) without writing {}; see {}",
            out.display(),
            game_dir.join("logs/latest.log").display()
        );
    }
    report_backend(&game_dir.join("logs/latest.log"));
    println!("[golden] wrote {}", out.display());
    Ok(())
}

fn report_backend(log: &Path) {
    let Ok(text) = std::fs::read_to_string(log) else {
        return;
    };
    for line in text.lines().filter(|l| {
        l.contains("Backend library") || l.contains("GL_RENDERER") || l.contains("OpenGL")
    }) {
        println!("[golden] {}", line.trim());
    }
}
