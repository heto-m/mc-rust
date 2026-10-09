use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const READY_TIMEOUT: Duration = Duration::from_secs(180);
const STOP_TIMEOUT: Duration = Duration::from_secs(30);
const COMMAND_GAP: Duration = Duration::from_millis(400);

#[derive(Debug)]
pub struct Server {
    child: Child,
    console: Console,
}

#[derive(Debug, Clone)]
pub struct Console(Arc<Mutex<ChildStdin>>);

impl Console {
    pub fn send(&self, line: &str) -> bool {
        let Ok(mut stdin) = self.0.lock() else {
            return false;
        };
        writeln!(stdin, "{line}").is_ok() && stdin.flush().is_ok()
    }
}

impl Server {
    pub fn spawn(port: u16, log: &Path) -> Result<Self> {
        let _ = std::fs::remove_dir_all("run/server-26.3/world");
        let mut child = Command::new("cargo")
            .args(["xtask", "server", "--nogui", "--port", &port.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("spawning `cargo xtask server`")?;
        let stdout = child.stdout.take().context("server stdout")?;
        let stdin = child.stdin.take().context("server stdin")?;
        let mut log = std::fs::File::create(log)?;
        let (ready_tx, ready_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut ready = Some(ready_tx);
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = writeln!(log, "{line}");
                if line.contains("Done (")
                    && line.contains("For help")
                    && let Some(tx) = ready.take()
                {
                    let _ = tx.send(());
                }
            }
        });
        let mut server = Self {
            child,
            console: Console(Arc::new(Mutex::new(stdin))),
        };
        if ready_rx.recv_timeout(READY_TIMEOUT).is_err() {
            server.stop();
            bail!("server did not report Done within {READY_TIMEOUT:?}");
        }
        server.console.send("whitelist off");
        Ok(server)
    }

    pub fn console(&self) -> Console {
        self.console.clone()
    }

    pub fn stop(&mut self) {
        self.console.send("stop");
        let deadline = std::time::Instant::now() + STOP_TIMEOUT;
        while std::time::Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug, PartialEq)]
pub enum Step {
    Command(String),
    Sleep(Duration),
}

pub fn parse_script(text: &str, player: &str) -> Vec<Step> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.strip_prefix("sleep ").map(str::parse::<f64>) {
            Some(Ok(secs)) => Step::Sleep(Duration::from_secs_f64(secs)),
            _ => Step::Command(l.replace("{name}", player)),
        })
        .collect()
}

pub fn run_script(console: &Console, steps: Vec<Step>) {
    for step in steps {
        match step {
            Step::Sleep(d) => std::thread::sleep(d),
            Step::Command(c) => {
                if !console.send(&c) {
                    return;
                }
                std::thread::sleep(COMMAND_GAP);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_parsing() {
        let steps = parse_script("# c\n\nop {name}\nsleep 1.5\ntime set day\n", "Cap");
        assert_eq!(
            steps,
            vec![
                Step::Command("op Cap".into()),
                Step::Sleep(Duration::from_millis(1500)),
                Step::Command("time set day".into()),
            ]
        );
    }
}
