//! `Screenshot#grab` (F2): the shown frame, UI included, saved as `screenshots/<date>.png` off-thread.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use mcr_protocol::types::text::{Arg, ClickEvent, Component, Style};
use mcr_render::Capture;

#[derive(Debug)]
pub struct Screenshots {
    dir: PathBuf,
    tx: Sender<Component>,
    rx: Receiver<Component>,
}

impl Screenshots {
    pub fn new(game_dir: &Path) -> Self {
        let (tx, rx) = channel();
        Self {
            dir: game_dir.join("screenshots"),
            tx,
            rx,
        }
    }

    /// Encodes and writes `capture`; the chat message arrives through [`Screenshots::poll`].
    pub fn save(&self, capture: Capture) {
        let dir = self.dir.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let message = match write(&dir, capture) {
                Ok(path) => {
                    let name = path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    let link = Component::text(name).styled(Style {
                        underlined: Some(true),
                        click_event: Some(Box::new(ClickEvent::OpenFile(
                            path.to_string_lossy().into_owned(),
                        ))),
                        ..Style::default()
                    });
                    println!("screenshot: saved {}", path.display());
                    Component::translatable("screenshot.success", vec![Arg::Component(link)])
                }
                Err(e) => {
                    eprintln!("screenshot: {e:#}");
                    Component::translatable("screenshot.failure", vec![Arg::Str(e.to_string())])
                }
            };
            let _ = tx.send(message);
        });
    }

    pub fn poll(&self) -> Option<Component> {
        self.rx.try_recv().ok()
    }
}

fn write(dir: &Path, capture: Capture) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = free_name(dir, &local_timestamp());
    let mut rgba = capture.pixels;
    for px in rgba.as_chunks_mut::<4>().0 {
        if capture.bgra {
            px.swap(0, 2);
        }
        px[3] = 0xFF;
    }
    crate::screenshot::write_png(&path, capture.width, capture.height, &rgba)?;
    Ok(path)
}

/// `Screenshot#getFile`: `name.png`, then `name_2.png` and so on.
fn free_name(dir: &Path, name: &str) -> PathBuf {
    (1..)
        .map(|n| {
            if n == 1 {
                dir.join(format!("{name}.png"))
            } else {
                dir.join(format!("{name}_{n}.png"))
            }
        })
        .find(|p| !p.exists())
        .expect("an unused name")
}

/// `yyyy-MM-dd_HH.mm.ss` in local time, like `Util#getFilenameFormattedDateTime`.
pub fn local_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as libc::time_t;
    // SAFETY: localtime_r only writes the zeroed struct it is given.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        tm
    };
    format!(
        "{:04}-{:02}-{:02}_{:02}.{:02}.{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_count_up_when_taken() {
        let dir = std::env::temp_dir().join(format!("mcr-grab-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = free_name(&dir, "2026-10-19_12.00.00");
        assert!(first.ends_with("2026-10-19_12.00.00.png"));
        std::fs::write(&first, b"x").unwrap();
        assert!(free_name(&dir, "2026-10-19_12.00.00").ends_with("2026-10-19_12.00.00_2.png"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn timestamps_have_the_vanilla_shape() {
        let t = local_timestamp();
        assert_eq!(t.len(), 19);
        assert_eq!(&t[10..11], "_");
    }
}
