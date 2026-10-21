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

/// Where a frame's time goes, as the profiler pie shows it.
pub const SECTIONS: [&str; 5] = ["tick", "update", "visuals", "gui", "draw"];
pub const SECTION_TICK: usize = 0;
pub const SECTION_UPDATE: usize = 1;
pub const SECTION_VISUALS: usize = 2;
pub const SECTION_GUI: usize = 3;
pub const SECTION_DRAW: usize = 4;

/// Smoothed milliseconds per frame section.
#[derive(Debug)]
pub struct FrameSections {
    ms: [f32; SECTIONS.len()],
    mark: Instant,
}

impl Default for FrameSections {
    fn default() -> Self {
        Self {
            ms: [0.0; SECTIONS.len()],
            mark: Instant::now(),
        }
    }
}

impl FrameSections {
    pub fn begin(&mut self) {
        self.mark = Instant::now();
    }

    fn add(&mut self, section: usize, ms: f32) {
        self.ms[section] += (ms - self.ms[section]) * 0.05;
    }

    /// Charges the time since the last mark to `section`.
    pub fn lap(&mut self, section: usize) {
        let now = Instant::now();
        self.add(section, (now - self.mark).as_secs_f32() * 1000.0);
        self.mark = now;
    }

    pub fn tick(&mut self, ms: f32) {
        self.add(SECTION_TICK, ms);
    }

    /// Each section's share of the total in percent, largest first.
    pub fn shares(&self) -> Vec<(&'static str, f32)> {
        let total: f32 = self.ms.iter().sum();
        let mut shares: Vec<_> = SECTIONS
            .iter()
            .zip(self.ms)
            .map(|(name, ms)| (*name, if total > 0.0 { ms / total * 100.0 } else { 0.0 }))
            .collect();
        shares.sort_by(|a, b| b.1.total_cmp(&a.1));
        shares
    }
}

/// A pie slice colour from the section's name, like `ResultField#getColor`.
pub fn section_color(name: &str) -> u32 {
    let hash = name.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    0xFF00_0000 | ((hash & 0xAA_AAAA) + 0x44_4444)
}

const MAX_NAME_CHARS: usize = 48;

/// `Minecraft#debugClientMetricsStart`: frame and tick times for ten seconds, kept as a zip.
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

    fn samples_csv(name: &str, samples: &[f32]) -> String {
        let mut text = format!("index,{name}_ms\n");
        for (i, ms) in samples.iter().enumerate() {
            text.push_str(&format!("{i},{ms:.4}\n"));
        }
        text
    }

    /// Writes `debug/profiling/<stamp>-<world>-<version>.zip` under `game_dir` and returns its path.
    pub fn write(
        &self,
        game_dir: &Path,
        stamp: &str,
        input: &ArchiveInput<'_>,
    ) -> std::io::Result<PathBuf> {
        let dir = game_dir.join("debug").join("profiling");
        std::fs::create_dir_all(&dir)?;
        let base = format!("{stamp}-{}-{}", safe_name(input.world), input.version);
        let path = (0..1000)
            .map(|n| match n {
                0 => dir.join(format!("{base}.zip")),
                n => dir.join(format!("{base}_{n}.zip")),
            })
            .find(|p| !p.exists())
            .ok_or_else(|| std::io::Error::other("no free profile name"))?;
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path)?);
        let options = zip::write::SimpleFileOptions::default();
        let mut add = |name: &str, text: &str| -> std::io::Result<()> {
            zip.start_file(name, options)
                .map_err(std::io::Error::other)?;
            std::io::Write::write_all(&mut zip, text.as_bytes())
        };
        add("system.txt", input.system)?;
        add("client/options.txt", input.options)?;
        add("client/metrics.txt", &self.report())?;
        add("client/sections.txt", &sections_text(input.shares))?;
        add(
            "client/frames.csv",
            &Self::samples_csv("frame", &self.frames),
        )?;
        add("client/ticks.csv", &Self::samples_csv("tick", &self.ticks))?;
        zip.finish().map_err(std::io::Error::other)?;
        Ok(path)
    }
}

/// What goes into the profile archive besides the samples.
pub struct ArchiveInput<'a> {
    pub world: &'a str,
    pub version: &'a str,
    pub system: &'a str,
    pub options: &'a str,
    pub shares: &'a [(&'static str, f32)],
}

fn sections_text(shares: &[(&'static str, f32)]) -> String {
    let mut text = String::from("root\n");
    for (name, percent) in shares {
        text.push_str(&format!("  {name} {percent:.2}%\n"));
    }
    text
}

/// A file-name-safe world or server name.
fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .take(MAX_NAME_CHARS)
        .collect();
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '.') {
        "unknown".into()
    } else {
        cleaned
    }
}

const ENTRIES_FILE: &str = "debug-profile.json";
const MAX_ENTRIES_BYTES: u64 = 64 * 1024;

/// `debug-profile.json` under `game_dir`; a missing or bad file is the default profile.
pub fn load_entries(game_dir: &Path) -> mcr_ui::DebugEntries {
    use std::io::Read;
    let read = std::fs::File::open(game_dir.join(ENTRIES_FILE)).and_then(|f| {
        let mut text = String::new();
        f.take(MAX_ENTRIES_BYTES).read_to_string(&mut text)?;
        Ok(text)
    });
    let Ok(text) = read else {
        return mcr_ui::DebugEntries::default();
    };
    mcr_ui::DebugEntries::parse(&text).unwrap_or_else(|| {
        eprintln!("debug: could not read {ENTRIES_FILE}, using the default profile");
        mcr_ui::DebugEntries::default()
    })
}

pub fn save_entries(game_dir: &Path, entries: &mcr_ui::DebugEntries) {
    if let Err(e) = std::fs::write(game_dir.join(ENTRIES_FILE), entries.to_json()) {
        eprintln!("debug: could not save {ENTRIES_FILE}: {e}");
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
    fn profiling_summarises_and_writes_a_zip() {
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
        let shares = [("draw", 75.0), ("tick", 25.0)];
        let input = ArchiveInput {
            world: "My World/../x",
            version: "26.3",
            system: "OS: test",
            options: "fov:70",
            shares: &shares,
        };
        let path = p.write(&dir, "2026-10-21_12.00.00", &input).unwrap();
        assert!(path.ends_with("debug/profiling/2026-10-21_12.00.00-My_World_.._x-26.3.zip"));
        let again = p.write(&dir, "2026-10-21_12.00.00", &input).unwrap();
        assert_ne!(path, again, "never overwrites a profile");
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let mut names: Vec<String> = archive
            .file_names()
            .map(|n| n.unwrap().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "client/frames.csv",
                "client/metrics.txt",
                "client/options.txt",
                "client/sections.txt",
                "client/ticks.csv",
                "system.txt"
            ]
        );
        let mut text = String::new();
        std::io::Read::read_to_string(
            &mut archive.by_name("client/sections.txt").unwrap(),
            &mut text,
        )
        .unwrap();
        assert!(text.contains("draw 75.00%"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn section_shares_sum_to_a_hundred_and_sort_descending() {
        let mut s = FrameSections::default();
        assert!(s.shares().iter().all(|(_, p)| *p == 0.0));
        for _ in 0..200 {
            s.add(SECTION_DRAW, 6.0);
            s.add(SECTION_GUI, 2.0);
            s.tick(2.0);
        }
        let shares = s.shares();
        assert_eq!(shares[0].0, "draw");
        let total: f32 = shares.iter().map(|(_, p)| p).sum();
        assert!((total - 100.0).abs() < 0.01);
        assert!((shares[0].1 - 60.0).abs() < 0.5, "{shares:?}");
        assert_eq!(section_color("draw"), section_color("draw"));
        assert_eq!(section_color("draw") >> 24, 0xFF);
    }

    #[test]
    fn profile_names_cannot_escape_the_folder() {
        assert_eq!(safe_name("../etc"), ".._etc");
        assert_eq!(safe_name(".."), "unknown");
        assert_eq!(safe_name(""), "unknown");
        assert!(safe_name(&"x".repeat(500)).len() <= MAX_NAME_CHARS);
    }

    #[test]
    fn entries_persist_and_bad_files_fall_back() {
        let dir = temp("entries");
        assert_eq!(load_entries(&dir), mcr_ui::DebugEntries::default());
        let mut entries = mcr_ui::DebugEntries::default();
        entries.set_status("biome", mcr_ui::EntryStatus::AlwaysOn);
        save_entries(&dir, &entries);
        assert_eq!(load_entries(&dir), entries);
        std::fs::write(dir.join(ENTRIES_FILE), "{{{").unwrap();
        assert_eq!(load_entries(&dir), mcr_ui::DebugEntries::default());
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
