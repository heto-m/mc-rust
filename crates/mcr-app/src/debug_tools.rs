//! The files F3 chords write: the crash report, profiling results and dumped sheets.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const PROFILING_SECONDS: u64 = 10;
const MAX_SAMPLES: usize = 200_000;

pub fn crash_report_text(stamp: &str) -> String {
    format!(
        "---- mcr Crash Report ----\nTime: {stamp}\nDescription: Manually triggered debug crash\n\n\
         java.lang.Throwable: Manually triggered debug crash\n"
    )
}

/// `KeyboardHandler#tick`'s deliberate crash: the report is written, then the client panics.
pub fn crash(game_dir: &Path) -> ! {
    let stamp = crate::grab::local_timestamp();
    let dir = game_dir.join("crash-reports");
    let path = dir.join(format!("crash-{stamp}-client.txt"));
    let written = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&path, crash_report_text(&stamp)));
    match written {
        Ok(()) => eprintln!("Saved crash report to {}", path.display()),
        Err(e) => eprintln!("Could not save the crash report: {e}"),
    }
    panic!("Manually triggered debug crash");
}

/// `Minecraft#debugClientMetricsStart`: frame and tick times for ten seconds.
#[derive(Debug)]
pub struct Profiling {
    started: Instant,
    frames: Vec<f32>,
    ticks: Vec<f32>,
}

impl Profiling {
    pub fn start() -> Self {
        Self {
            started: Instant::now(),
            frames: Vec::new(),
            ticks: Vec::new(),
        }
    }

    pub fn frame(&mut self, ms: f32) {
        if self.frames.len() < MAX_SAMPLES {
            self.frames.push(ms);
        }
    }

    pub fn tick(&mut self, ms: f32) {
        if self.ticks.len() < MAX_SAMPLES {
            self.ticks.push(ms);
        }
    }

    pub fn finished(&self) -> bool {
        self.started.elapsed() >= Duration::from_secs(PROFILING_SECONDS)
    }

    pub fn seconds(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    pub fn tick_count(&self) -> usize {
        self.ticks.len()
    }

    fn summary(name: &str, samples: &[f32]) -> String {
        if samples.is_empty() {
            return format!("{name}: no samples\n");
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f32::total_cmp);
        let mean = sorted.iter().sum::<f32>() / sorted.len() as f32;
        let at = |q: f32| sorted[((sorted.len() - 1) as f32 * q) as usize];
        format!(
            "{name}: {} samples, mean {mean:.3} ms, median {:.3} ms, p99 {:.3} ms, max {:.3} ms\n",
            sorted.len(),
            at(0.5),
            at(0.99),
            at(1.0)
        )
    }

    pub fn report(&self) -> String {
        let mut text = format!("mcr client metrics over {:.2} s\n", self.seconds());
        text.push_str(&Self::summary("frame time", &self.frames));
        text.push_str(&Self::summary("tick time", &self.ticks));
        text
    }

    /// Writes `debug/profiling/<stamp>.txt` under `game_dir`.
    pub fn write(&self, game_dir: &Path, stamp: &str) -> std::io::Result<PathBuf> {
        let dir = game_dir.join("debug").join("profiling");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{stamp}.txt"));
        std::fs::write(&path, self.report())?;
        Ok(path)
    }
}

/// One texture sheet as RGBA.
pub struct Sheet<'a> {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

/// `TextureManager#dumpAllSheets`: one PNG per sheet under `debug/dynamic_textures_<stamp>`.
pub fn dump_sheets(game_dir: &Path, stamp: &str, sheets: &[Sheet<'_>]) -> std::io::Result<PathBuf> {
    let dir = game_dir
        .join("debug")
        .join(format!("dynamic_textures_{stamp}"));
    std::fs::create_dir_all(&dir)?;
    for sheet in sheets {
        crate::screenshot::write_png(
            &dir.join(format!("{}.png", sheet.name)),
            sheet.width,
            sheet.height,
            sheet.rgba,
        )
        .map_err(std::io::Error::other)?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mcr-dbg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_crash_report_names_the_deliberate_crash() {
        let text = crash_report_text("2026-10-21_12.00.00");
        assert!(text.contains("Description: Manually triggered debug crash"));
    }

    #[test]
    fn profiling_summarises_and_writes_a_file() {
        let mut p = Profiling::start();
        for ms in [1.0, 2.0, 3.0, 4.0] {
            p.frame(ms);
        }
        p.tick(10.0);
        assert!(!p.finished());
        let report = p.report();
        assert!(report.contains("frame time: 4 samples, mean 2.500 ms"));
        assert!(report.contains("tick time: 1 samples"));
        let dir = temp("profile");
        let path = p.write(&dir, "2026-10-21_12.00.00").unwrap();
        assert!(path.ends_with("debug/profiling/2026-10-21_12.00.00.txt"));
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sheets_are_dumped_as_pngs() {
        let dir = temp("sheets");
        let rgba = [255u8; 16];
        let sheets = [Sheet {
            name: "gui_0".into(),
            width: 2,
            height: 2,
            rgba: &rgba,
        }];
        let out = dump_sheets(&dir, "s", &sheets).unwrap();
        assert!(out.join("gui_0.png").is_file());
        let _ = std::fs::remove_dir_all(dir);
    }
}
