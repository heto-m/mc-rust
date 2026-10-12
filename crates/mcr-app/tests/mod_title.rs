//! Opt-in: `mcr --mods` boots Fabric API and the test mod to Java's title screen offscreen, with no server.
//! `MCR_JVM_TESTS=1 cargo test -p mcr-app --test mod_title -- --nocapture --test-threads=1`

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn enabled() -> bool {
    let on = std::env::var("MCR_JVM_TESTS").as_deref() == Ok("1");
    if !on {
        eprintln!("skipped: set MCR_JVM_TESTS=1");
    }
    on
}

/// A fresh run dir with Fabric API and the test mod in `mods/`.
fn prepare(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("mod-title-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    let mods = dir.join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    let cache = repo().join(".cache");
    let fabric_api = std::fs::read_dir(cache.join("mods-test"))
        .expect("run `cargo xtask fabric-fetch`")
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("fabric-api-"))
        })
        .expect("the pinned Fabric API jar");
    std::fs::copy(&fabric_api, mods.join(fabric_api.file_name().unwrap())).unwrap();
    std::fs::copy(
        cache.join("java/mcr-testmod.jar"),
        mods.join("mcr-testmod.jar"),
    )
    .expect("run `cargo xtask java-build`");
    dir
}

fn run_title(dir: &Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_mcr"))
        .current_dir(repo())
        .args(["--size", "640x360", "--mods"])
        .arg(dir.join("mods"))
        .arg("--game-dir")
        .arg(dir.join("game"))
        .arg("--screenshot")
        .arg(dir.join("title.png"))
        .output()
        .expect("spawn mcr");
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "mcr failed:\n{text}");
    text
}

#[test]
fn mods_write_relative_paths_into_the_game_dir() {
    if !enabled() {
        return;
    }
    let dir = prepare("cwd");
    let stray = repo().join("config/mcr_testmod.txt");
    let _ = std::fs::remove_file(&stray);
    let log = run_title(&dir);
    assert!(
        log.contains("TitleScreen"),
        "never reached the title:\n{log}"
    );
    assert!(
        dir.join("game/java/config/mcr_testmod.txt").is_file(),
        "the test mod's relative config is not in Java's game dir:\n{log}"
    );
    assert!(
        !stray.exists(),
        "the test mod wrote into mcr's working directory"
    );
    assert!(dir.join("title.png").is_file());
}
