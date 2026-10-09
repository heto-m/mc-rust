use std::ffi::OsString;
use std::fs::{self, File};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

use crate::jdk::resolve_java;
use crate::paths::Layout;

pub fn datagen_args(server_jar: &Path) -> Vec<OsString> {
    [
        "-DbundlerMainClass=net.minecraft.data.Main".into(),
        "-jar".into(),
        server_jar.into(),
        "--reports".into(),
        "--server".into(),
        "--output".into(),
        "../generated".into(),
    ]
    .into()
}

pub fn run(layout: &Layout, ver: &str, force: bool) -> Result<()> {
    let server_jar = layout.server_jar(ver);
    if !server_jar.is_file() {
        bail!(
            "{} is missing; run `cargo xtask fetch --version {ver}` first",
            server_jar.display()
        );
    }
    let generated = layout.generated_dir(ver);
    if !force && generated.join("reports").is_dir() && generated.join("data").is_dir() {
        println!("[datagen] {ver} already generated");
        return Ok(());
    }

    let java = resolve_java(layout)?;
    let work = layout.datagen_dir(ver);
    fs::create_dir_all(&work)?;
    fs::create_dir_all(&generated)?;
    let log_path = work.join("datagen.log");
    let log = File::create(&log_path)?;

    println!("[datagen] running bundler data generator for {ver}");
    let status = Command::new(java)
        .args(datagen_args(&server_jar))
        .current_dir(&work)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .status()?;
    if !status.success() {
        bail!(
            "data generator failed ({status}); see {}",
            log_path.display()
        );
    }
    println!("[datagen] output in {}", generated.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_run_bundler_main_with_reports_into_generated() {
        let args: Vec<String> = datagen_args(Path::new("/c/server.jar"))
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        assert_eq!(
            args,
            [
                "-DbundlerMainClass=net.minecraft.data.Main",
                "-jar",
                "/c/server.jar",
                "--reports",
                "--server",
                "--output",
                "../generated",
            ]
        );
    }
}
