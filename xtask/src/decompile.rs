use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

use crate::jdk::resolve_java;
use crate::net::download_to;
use crate::paths::Layout;

const VINEFLOWER_URL: &str =
    "https://repo1.maven.org/maven2/org/vineflower/vineflower/1.12.0/vineflower-1.12.0.jar";

pub fn decompile_args(vineflower: &Path, client_jar: &Path, src: &Path) -> Vec<OsString> {
    [
        "-Xmx6g".into(),
        "-jar".into(),
        vineflower.into(),
        "-dgs=1".into(),
        "-log=WARN".into(),
        "--only=net/minecraft".into(),
        "--only=com/mojang".into(),
        client_jar.into(),
        src.into(),
    ]
    .into()
}

pub fn run(layout: &Layout, ver: &str, force: bool) -> Result<()> {
    let client_jar = layout.client_jar(ver);
    if !client_jar.is_file() {
        bail!(
            "{} is missing; run `cargo xtask fetch --version {ver}` first",
            client_jar.display()
        );
    }
    let src = layout.src_dir(ver);
    if !force && src.join("net/minecraft").is_dir() {
        println!("[decompile] {ver} already decompiled");
        return Ok(());
    }

    let vineflower = layout.vineflower_jar();
    if !vineflower.is_file() {
        println!("[decompile] downloading Vineflower 1.12.0");
        download_to(VINEFLOWER_URL, &vineflower)?;
    }

    let java = resolve_java(layout)?;
    fs::create_dir_all(&src)?;
    println!("[decompile] decompiling {ver} into {}", src.display());
    let status = Command::new(java)
        .args(decompile_args(&vineflower, &client_jar, &src))
        .status()?;
    if !status.success() {
        bail!("Vineflower failed ({status})");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_set_heap_and_limit_to_minecraft_packages() {
        let args: Vec<String> = decompile_args(
            Path::new("/t/vineflower.jar"),
            Path::new("/v/client.jar"),
            Path::new("/v/src"),
        )
        .into_iter()
        .map(|a| a.into_string().unwrap())
        .collect();
        assert_eq!(
            args,
            [
                "-Xmx6g",
                "-jar",
                "/t/vineflower.jar",
                "-dgs=1",
                "-log=WARN",
                "--only=net/minecraft",
                "--only=com/mojang",
                "/v/client.jar",
                "/v/src",
            ]
        );
    }
}
