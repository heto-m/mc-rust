use std::env;
use std::fs;
use std::process::Command;

use anyhow::{Result, bail};

use crate::jdk::resolve_java;
use crate::paths::Layout;

const EULA_URL: &str = "https://aka.ms/MinecraftEULA";

#[derive(Debug, PartialEq, Eq)]
pub enum EulaAction {
    Write,
    Keep,
    Refuse,
}

pub fn eula_action(env_accept: Option<&str>, existing_eula: Option<&str>) -> EulaAction {
    if env_accept == Some("1") {
        EulaAction::Write
    } else if existing_eula.is_some_and(|t| t.lines().any(|l| l.trim() == "eula=true")) {
        EulaAction::Keep
    } else {
        EulaAction::Refuse
    }
}

/// Keeps the user's existing settings; only the keys a dev server needs are forced.
pub fn render_properties(existing: Option<&str>, port: u16) -> String {
    let port = port.to_string();
    let forced = [
        ("online-mode", "false"),
        ("server-port", port.as_str()),
        ("pause-when-empty-seconds", "-1"),
        ("white-list", "false"),
        ("enforce-whitelist", "false"),
        ("accept-transfers", "true"),
    ];
    let defaults = [("view-distance", "10"), ("simulation-distance", "10")];
    let mut lines: Vec<String> = existing
        .unwrap_or_default()
        .lines()
        .filter(|l| {
            let key = l.split('=').next().unwrap_or_default().trim();
            !forced.iter().any(|(k, _)| *k == key)
        })
        .map(str::to_owned)
        .collect();
    for (key, value) in defaults {
        if !lines.iter().any(|l| l.split('=').next() == Some(key)) {
            lines.push(format!("{key}={value}"));
        }
    }
    lines.extend(forced.iter().map(|(k, v)| format!("{k}={v}")));
    lines.join("\n") + "\n"
}

pub fn run(layout: &Layout, ver: &str, port: u16, nogui: bool) -> Result<()> {
    let run_dir = layout.server_run_dir(ver);
    fs::create_dir_all(&run_dir)?;
    let eula_path = run_dir.join("eula.txt");
    let existing = fs::read_to_string(&eula_path).ok();
    match eula_action(
        env::var("MCR_ACCEPT_EULA").ok().as_deref(),
        existing.as_deref(),
    ) {
        EulaAction::Write => fs::write(&eula_path, "eula=true\n")?,
        EulaAction::Keep => {}
        EulaAction::Refuse => bail!(
            "you must accept the Minecraft EULA yourself ({EULA_URL}), then set eula=true in {} \
             or re-run with MCR_ACCEPT_EULA=1",
            eula_path.display()
        ),
    }

    let server_jar = layout.server_jar(ver);
    if !server_jar.is_file() {
        bail!(
            "{} is missing; run `cargo xtask fetch --version {ver}` first",
            server_jar.display()
        );
    }
    let properties = run_dir.join("server.properties");
    let current = fs::read_to_string(&properties).ok();
    fs::write(&properties, render_properties(current.as_deref(), port))?;

    let java = resolve_java(layout)?;
    let mut cmd = memory_capped(&java);
    cmd.args(["-Xms256M", "-Xmx1536M", "-XX:+UseSerialGC"])
        .arg("-jar")
        .arg(&server_jar)
        .current_dir(&run_dir);
    if nogui {
        cmd.arg("--nogui");
    }
    let status = cmd.status()?;
    if !status.success() {
        bail!("server exited with {status}");
    }
    Ok(())
}

/// A runaway JVM in its own capped scope dies alone instead of tripping systemd-oomd on the caller's.
fn memory_capped(program: &std::path::Path) -> Command {
    let capped = cfg!(target_os = "linux")
        && Command::new("systemd-run")
            .args(["--user", "--scope", "-q", "true"])
            .status()
            .is_ok_and(|s| s.success());
    if !capped {
        return Command::new(program);
    }
    let mut cmd = Command::new("systemd-run");
    cmd.args([
        "--user",
        "--scope",
        "-q",
        "-p",
        "MemoryMax=2560M",
        "-p",
        "MemorySwapMax=0",
    ])
    .arg(program);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_disable_online_mode_and_set_port() {
        let text = render_properties(None, 25570);
        assert!(text.lines().any(|l| l == "online-mode=false"));
        assert!(text.lines().any(|l| l == "server-port=25570"));
        assert!(text.lines().any(|l| l == "pause-when-empty-seconds=-1"));
        assert!(text.lines().any(|l| l == "view-distance=10"));
    }

    #[test]
    fn properties_keep_user_settings_and_force_dev_keys() {
        let text = render_properties(
            Some("motd=hi\nview-distance=16\nonline-mode=true\nserver-port=1\n"),
            25570,
        );
        assert!(text.lines().any(|l| l == "motd=hi"));
        assert!(text.lines().any(|l| l == "view-distance=16"));
        assert!(!text.lines().any(|l| l == "online-mode=true"));
        assert_eq!(
            text.lines()
                .filter(|l| l.starts_with("server-port="))
                .count(),
            1
        );
    }

    #[test]
    fn eula_refused_without_env_or_prior_acceptance() {
        assert_eq!(eula_action(None, None), EulaAction::Refuse);
        assert_eq!(eula_action(None, Some("eula=false\n")), EulaAction::Refuse);
    }

    #[test]
    fn eula_written_when_env_accepts() {
        assert_eq!(eula_action(Some("1"), None), EulaAction::Write);
        assert_eq!(
            eula_action(Some("1"), Some("eula=false")),
            EulaAction::Write
        );
    }

    #[test]
    fn eula_kept_when_previously_accepted() {
        assert_eq!(
            eula_action(None, Some("#Mon\neula=true\n")),
            EulaAction::Keep
        );
    }
}
