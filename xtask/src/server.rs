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

pub fn render_properties(port: u16) -> String {
    format!("online-mode=false\nserver-port={port}\nview-distance=4\nsimulation-distance=4\n")
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
    fs::write(run_dir.join("server.properties"), render_properties(port))?;

    let java = resolve_java(layout)?;
    let mut cmd = Command::new(java);
    cmd.arg("-jar").arg(&server_jar).current_dir(&run_dir);
    if nogui {
        cmd.arg("--nogui");
    }
    let status = cmd.status()?;
    if !status.success() {
        bail!("server exited with {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_disable_online_mode_and_set_port() {
        let text = render_properties(25570);
        assert!(text.lines().any(|l| l == "online-mode=false"));
        assert!(text.lines().any(|l| l == "server-port=25570"));
        assert!(text.lines().any(|l| l == "view-distance=4"));
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
