use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::jdk::resolve_java;
use crate::paths::Layout;
use crate::soak_churn::{self, CYCLE_SECS};

pub const SAMPLE_SECS: u64 = 10;
const SERVER_START: Duration = Duration::from_secs(300);
const JOIN_TIMEOUT: Duration = Duration::from_secs(120);
const GPU_KEYS: [&str; 8] = [
    "gpu_buffers",
    "gpu_buffer_bytes",
    "gpu_textures",
    "gpu_texture_bytes",
    "gpu_bind_groups",
    "entity_layers",
    "entities",
    "chunks",
];

#[derive(Debug, Clone)]
pub struct Options {
    pub mcr: PathBuf,
    pub minutes: f64,
    pub warmup_minutes: f64,
    pub port: u16,
    pub max_slope_mib_per_min: f64,
    pub wave: u32,
    pub disconnects: u32,
    pub mods: Option<PathBuf>,
    pub out: Option<PathBuf>,
    pub mcr_memory: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    pub secs: f64,
    pub rss_kib: u64,
    pub threads: u64,
    pub gpu: BTreeMap<String, i64>,
    pub java_heap_kib: Option<u64>,
}

pub fn run(layout: &Layout, ver: &str, opts: &Options) -> Result<()> {
    if server_running() {
        bail!("a server.jar is already running; the soak needs the machine to itself");
    }
    let mcr = std::path::absolute(&opts.mcr)?;
    if !mcr.is_file() {
        bail!(
            "{} is missing; build it first (cargo build -p mcr-app)",
            mcr.display()
        );
    }
    let out = match &opts.out {
        Some(dir) => dir.clone(),
        None => layout.soak_dir().join(stamp()),
    };
    fs::create_dir_all(&out)?;
    let out = fs::canonicalize(&out)?;
    println!("soak output in {}", out.display());

    let mut server = Server::start(layout, ver, opts.port, &out)?;
    let result = soak(&mut server, &mcr, opts, &out);
    server.stop();
    let samples = result?;
    let verdict = analyze(
        &samples,
        opts.warmup_minutes * 60.0,
        opts.max_slope_mib_per_min,
    );
    let report = verdict.report(&samples, &server_errors(&out.join("server.log")));
    fs::write(out.join("summary.txt"), &report)?;
    println!("{report}");
    if !verdict.passed() {
        bail!("soak failed: {}", verdict.failures.join("; "));
    }
    Ok(())
}

fn soak(server: &mut Server, mcr: &Path, opts: &Options, out: &Path) -> Result<Vec<Sample>> {
    let total = Duration::from_secs_f64(opts.minutes * 60.0);
    let mut client = Client::start(mcr, opts, out, total)?;
    let joined = Instant::now();
    while !client.joined() {
        if let Some(status) = client.child.try_wait()? {
            bail!("mcr exited before joining: {status}");
        }
        if joined.elapsed() > JOIN_TIMEOUT {
            client.kill();
            bail!("mcr did not join within {JOIN_TIMEOUT:?}");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    for cmd in soak_churn::SETUP {
        server.send(cmd)?;
    }
    println!("mcr joined; churning for {:.1} min", opts.minutes);
    let cycles = (total.as_secs() as u32).div_ceil(CYCLE_SECS);
    let warmup_cycles = ((opts.warmup_minutes * 60.0) as u32).div_ceil(CYCLE_SECS);
    let disconnects = soak_churn::disconnect_cycles(opts.disconnects, cycles, warmup_cycles);
    let csv_path = out.join("samples.csv");
    let mut csv = File::create(&csv_path)?;
    writeln!(csv, "{}", csv_header())?;
    let mut samples = Vec::new();
    let start = Instant::now();
    let mut next_sample = Duration::ZERO;
    let mut sent_second = None;
    let gc_log = out.join("gc.log");
    while start.elapsed() < total {
        if let Some(status) = client.child.try_wait()? {
            bail!("mcr exited during the soak: {status} (137 means its memory scope killed it)");
        }
        let elapsed = start.elapsed();
        let second = elapsed.as_secs() as u32;
        if sent_second != Some(second) {
            sent_second = Some(second);
            let (cycle, at) = (second / CYCLE_SECS, second % CYCLE_SECS);
            for cmd in soak_churn::commands(cycle, at, opts.wave) {
                server.send(&cmd)?;
            }
            if at == CYCLE_SECS - 5
                && let Some(&(_, kind)) = disconnects.iter().find(|&&(c, _)| c == cycle)
            {
                println!("{:.0}s: {kind:?}", elapsed.as_secs_f64());
                server.send(&soak_churn::disconnect_command(kind, opts.port))?;
            }
        }
        if elapsed >= next_sample {
            next_sample += Duration::from_secs(SAMPLE_SECS);
            let sample = Sample {
                secs: elapsed.as_secs_f64(),
                gpu: client.latest_stats(),
                java_heap_kib: opts.mods.as_ref().and_then(|_| last_heap_after_gc(&gc_log)),
                ..proc_sample(client.child.id()).unwrap_or_default()
            };
            writeln!(csv, "{}", csv_row(&sample))?;
            csv.flush()?;
            samples.push(sample);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    client.finish();
    println!("wrote {}", csv_path.display());
    Ok(samples)
}

struct Server {
    child: Child,
    stdin: ChildStdin,
}

impl Server {
    fn start(layout: &Layout, ver: &str, port: u16, out: &Path) -> Result<Self> {
        let jar = layout.server_jar(ver);
        if !jar.is_file() {
            bail!(
                "{} is missing; run `cargo xtask fetch` first",
                jar.display()
            );
        }
        let dir = layout.soak_server_dir(ver);
        let world = dir.join("world");
        if world.exists() {
            fs::remove_dir_all(&world).context("removing the previous soak world")?;
        }
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("eula.txt"), "eula=true\n")?;
        fs::write(dir.join("server.properties"), soak_properties(port))?;
        let log = out.join("server.log");
        let java = resolve_java(layout)?;
        let mut cmd = crate::server::memory_capped(&java);
        cmd.args(["-Xms256M", "-Xmx1536M", "-XX:+UseSerialGC", "-jar"])
            .arg(&jar)
            .arg("--nogui")
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(File::create(&log)?)
            .stderr(Stdio::null());
        let mut child = cmd.spawn().context("starting the vanilla server")?;
        let stdin = child.stdin.take().context("server stdin")?;
        fs::write(out.join("server.pid"), child.id().to_string())?;
        let mut server = Self { child, stdin };
        let started = Instant::now();
        loop {
            let text = fs::read_to_string(&log).unwrap_or_default();
            if text.contains("Done (") {
                break;
            }
            if let Some(status) = server.child.try_wait()? {
                bail!("server exited during startup: {status}");
            }
            if started.elapsed() > SERVER_START {
                server.stop();
                bail!("server did not start within {SERVER_START:?}");
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        println!("server up on port {port}");
        Ok(server)
    }

    fn send(&mut self, cmd: &str) -> Result<()> {
        writeln!(self.stdin, "{cmd}")?;
        self.stdin.flush()?;
        Ok(())
    }

    fn stop(&mut self) {
        if let Ok(Some(_)) = self.child.try_wait() {
            return;
        }
        let _ = self.send("stop");
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(60) {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Client {
    child: Child,
    stats: Arc<Mutex<ClientLog>>,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.kill();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Default)]
struct ClientLog {
    joined: bool,
    stats: BTreeMap<String, i64>,
}

impl Client {
    fn start(mcr: &Path, opts: &Options, out: &Path, total: Duration) -> Result<Self> {
        let game_dir = out.join("client");
        fs::create_dir_all(&game_dir)?;
        fs::write(game_dir.join("options.txt"), "soundCategory_master:0.0\n")?;
        let mut cmd = Command::new("systemd-run");
        cmd.args(["--user", "--scope", "-q", "-p"])
            .arg(format!("MemoryMax={}", opts.mcr_memory))
            .args(["-p", "MemorySwapMax=0"])
            .arg(mcr)
            .arg("--connect")
            .arg(format!("127.0.0.1:{}", opts.port))
            .args(["--name", soak_churn::PLAYER, "--size", "854x480"])
            .arg("--game-dir")
            .arg(&game_dir)
            .arg("--soak")
            .arg((total.as_secs() + 120).to_string());
        if let Some(mods) = &opts.mods {
            fs::write(
                game_dir.join("mcr-mods.json"),
                "{\"jvm\":{\"heap_mib\":1024}}\n",
            )?;
            cmd.arg("--mods").arg(mods).env(
                "JAVA_TOOL_OPTIONS",
                format!("-Xlog:gc:file={}", out.join("gc.log").display()),
            );
        }
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(File::create(out.join("mcr.err"))?)
            .spawn()
            .context("starting mcr")?;
        fs::write(out.join("mcr.pid"), child.id().to_string())?;
        let stdout = child.stdout.take().context("mcr stdout")?;
        let stats = Arc::new(Mutex::new(ClientLog::default()));
        let shared = stats.clone();
        let mut log = File::create(out.join("mcr.log"))?;
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = writeln!(log, "{line}");
                let Ok(mut state) = shared.lock() else { return };
                if line.starts_with("joined as") {
                    state.joined = true;
                }
                if let Some(stats) = parse_stats(&line) {
                    state.stats = stats;
                }
            }
        });
        Ok(Self { child, stats })
    }

    fn joined(&self) -> bool {
        self.stats.lock().is_ok_and(|s| s.joined)
    }

    fn latest_stats(&self) -> BTreeMap<String, i64> {
        self.stats
            .lock()
            .map(|s| s.stats.clone())
            .unwrap_or_default()
    }

    fn finish(&mut self) {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(5) {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        self.kill();
    }

    fn kill(&mut self) {
        if let Ok(Some(_)) = self.child.try_wait() {
            return;
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn soak_properties(port: u16) -> String {
    let props = [
        ("online-mode", "false".to_owned()),
        ("white-list", "false".into()),
        ("enforce-whitelist", "false".into()),
        ("server-port", port.to_string()),
        ("pause-when-empty-seconds", "-1".into()),
        ("accept-transfers", "true".into()),
        ("view-distance", "8".into()),
        ("simulation-distance", "6".into()),
        ("spawn-protection", "0".into()),
        ("gamemode", "creative".into()),
        ("force-gamemode", "true".into()),
        ("level-seed", "mcr-soak".into()),
        ("max-tick-time", "-1".into()),
        ("allow-flight", "true".into()),
        ("motd", "mcr soak".into()),
    ];
    props.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
}

fn server_running() -> bool {
    let Ok(procs) = fs::read_dir("/proc") else {
        return false;
    };
    procs
        .flatten()
        .any(|p| fs::read(p.path().join("cmdline")).is_ok_and(|raw| is_server_cmdline(&raw)))
}

/// A java process with a `server.jar` argument; shells merely mentioning one do not count.
pub fn is_server_cmdline(raw: &[u8]) -> bool {
    let mut args = raw.split(|&b| b == 0).map(String::from_utf8_lossy);
    args.next().is_some_and(|exe| exe.ends_with("java")) && args.any(|a| a.ends_with("server.jar"))
}

fn stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("soak-{secs}")
}

pub fn parse_stats(line: &str) -> Option<BTreeMap<String, i64>> {
    let rest = line.strip_prefix("soak-stats ")?;
    Some(
        rest.split_whitespace()
            .filter_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                Some((k.to_owned(), v.parse().ok()?))
            })
            .collect(),
    )
}

pub fn parse_proc_status(text: &str) -> Option<(u64, u64)> {
    let field = |name: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|v| v.split_whitespace().next())
            .and_then(|v| v.parse::<u64>().ok())
    };
    Some((field("VmRSS:")?, field("Threads:")?))
}

fn proc_sample(pid: u32) -> Option<Sample> {
    let text = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let (rss_kib, threads) = parse_proc_status(&text)?;
    Some(Sample {
        rss_kib,
        threads,
        ..Sample::default()
    })
}

/// Heap after the latest collection from a `-Xlog:gc` line such as `Pause Young (...) 80M->20M(256M) 3.1ms`.
pub fn heap_after_gc(line: &str) -> Option<u64> {
    let arrow = line.rfind("->")?;
    let after = &line[arrow + 2..];
    let end = after.find('(')?;
    let value = &after[..end];
    let (num, unit) = value.split_at(value.len().checked_sub(1)?);
    let n: u64 = num.parse().ok()?;
    match unit {
        "K" => Some(n),
        "M" => Some(n * 1024),
        "G" => Some(n * 1024 * 1024),
        _ => None,
    }
}

fn last_heap_after_gc(path: &Path) -> Option<u64> {
    let text = fs::read_to_string(path).ok()?;
    text.lines().rev().find_map(heap_after_gc)
}

fn server_errors(log: &Path) -> Vec<String> {
    let text = fs::read_to_string(log).unwrap_or_default();
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| {
            [
                "<--[HERE]",
                "Unknown or incomplete command",
                "Failed to decode",
            ]
            .iter()
            .any(|m| l.contains(m))
        })
        .map(|l| l.split_once("] ").map_or(l, |(_, rest)| rest).to_owned())
        .collect();
    lines.sort();
    lines.dedup();
    lines.truncate(20);
    lines
}

fn csv_header() -> String {
    let mut cols = vec!["secs", "rss_kib", "threads", "java_heap_kib"];
    cols.extend(GPU_KEYS);
    cols.join(",")
}

fn csv_row(s: &Sample) -> String {
    let mut cols = vec![
        format!("{:.0}", s.secs),
        s.rss_kib.to_string(),
        s.threads.to_string(),
        s.java_heap_kib.map(|h| h.to_string()).unwrap_or_default(),
    ];
    cols.extend(
        GPU_KEYS
            .iter()
            .map(|k| s.gpu.get(*k).map(i64::to_string).unwrap_or_default()),
    );
    cols.join(",")
}

pub fn slope(xs: &[f64], ys: &[f64]) -> f64 {
    let n = xs.len().min(ys.len()) as f64;
    if n < 2.0 {
        return 0.0;
    }
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let (mut num, mut den) = (0.0, 0.0);
    for (x, y) in xs.iter().zip(ys) {
        num += (x - mx) * (y - my);
        den += (x - mx) * (x - mx);
    }
    if den == 0.0 { 0.0 } else { num / den }
}

#[derive(Debug, Default)]
pub struct Verdict {
    pub rss_slope_mib_per_min: f64,
    pub gpu_slope_mib_per_min: f64,
    pub heap_slope_mib_per_min: Option<f64>,
    pub threads_start: u64,
    pub threads_end: u64,
    pub failures: Vec<String>,
}

impl Verdict {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }

    fn report(&self, samples: &[Sample], server_errors: &[String]) -> String {
        let mut text = String::new();
        let first = samples.first();
        let last = samples.last();
        let mib = |kib: u64| kib as f64 / 1024.0;
        if let (Some(a), Some(b)) = (first, last) {
            text += &format!(
                "samples {} over {:.1} min; rss {:.0} -> {:.0} MiB (peak {:.0}); threads {} -> {}\n",
                samples.len(),
                (b.secs - a.secs) / 60.0,
                mib(a.rss_kib),
                mib(b.rss_kib),
                mib(samples.iter().map(|s| s.rss_kib).max().unwrap_or(0)),
                a.threads,
                b.threads,
            );
            let gpu = |s: &Sample| gpu_bytes(s) as f64 / (1 << 20) as f64;
            text += &format!("gpu {:.0} -> {:.0} MiB\n", gpu(a), gpu(b));
        }
        text += &format!(
            "after warm-up: rss slope {:.3} MiB/min, gpu slope {:.3} MiB/min, threads {} -> {}",
            self.rss_slope_mib_per_min,
            self.gpu_slope_mib_per_min,
            self.threads_start,
            self.threads_end
        );
        if let Some(heap) = self.heap_slope_mib_per_min {
            text += &format!(", java heap-after-gc slope {heap:.3} MiB/min");
        }
        text += "\n";
        if !server_errors.is_empty() {
            text += "server rejected commands:\n";
            for line in server_errors {
                text += &format!("  {line}\n");
            }
        }
        text += if self.passed() { "PASS\n" } else { "FAIL\n" };
        for f in &self.failures {
            text += &format!("  {f}\n");
        }
        text
    }
}

fn gpu_bytes(s: &Sample) -> i64 {
    s.gpu.get("gpu_buffer_bytes").copied().unwrap_or(0)
        + s.gpu.get("gpu_texture_bytes").copied().unwrap_or(0)
}

fn median(mut v: Vec<u64>) -> u64 {
    v.sort_unstable();
    v.get(v.len() / 2).copied().unwrap_or(0)
}

pub fn analyze(samples: &[Sample], warmup_secs: f64, max_slope: f64) -> Verdict {
    let steady: Vec<&Sample> = samples.iter().filter(|s| s.secs >= warmup_secs).collect();
    let mut verdict = Verdict::default();
    if steady.len() < 6 {
        verdict
            .failures
            .push(format!("only {} samples after warm-up", steady.len()));
        return verdict;
    }
    let mins: Vec<f64> = steady.iter().map(|s| s.secs / 60.0).collect();
    let mib = |v: f64| v / 1024.0;
    let rss: Vec<f64> = steady.iter().map(|s| mib(s.rss_kib as f64)).collect();
    verdict.rss_slope_mib_per_min = slope(&mins, &rss);
    let gpu: Vec<f64> = steady
        .iter()
        .map(|s| gpu_bytes(s) as f64 / (1 << 20) as f64)
        .collect();
    verdict.gpu_slope_mib_per_min = slope(&mins, &gpu);
    let heap: Vec<(f64, f64)> = steady
        .iter()
        .filter_map(|s| Some((s.secs / 60.0, mib(s.java_heap_kib? as f64))))
        .collect();
    if heap.len() >= 6 {
        let (x, y): (Vec<f64>, Vec<f64>) = heap.into_iter().unzip();
        verdict.heap_slope_mib_per_min = Some(slope(&x, &y));
    }
    let third = steady.len() / 3;
    verdict.threads_start = median(steady[..third].iter().map(|s| s.threads).collect());
    verdict.threads_end = median(
        steady[steady.len() - third..]
            .iter()
            .map(|s| s.threads)
            .collect(),
    );
    if verdict.rss_slope_mib_per_min > max_slope {
        verdict.failures.push(format!(
            "rss grows {:.3} MiB/min (limit {max_slope})",
            verdict.rss_slope_mib_per_min
        ));
    }
    if verdict.gpu_slope_mib_per_min > max_slope {
        verdict.failures.push(format!(
            "gpu memory grows {:.3} MiB/min (limit {max_slope})",
            verdict.gpu_slope_mib_per_min
        ));
    }
    if let Some(heap) = verdict.heap_slope_mib_per_min
        && heap > max_slope
    {
        verdict.failures.push(format!(
            "java heap after gc grows {heap:.3} MiB/min (limit {max_slope})"
        ));
    }
    if verdict.threads_end > verdict.threads_start {
        verdict.failures.push(format!(
            "threads grow from {} to {}",
            verdict.threads_start, verdict.threads_end
        ));
    }
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(f: impl Fn(f64) -> (u64, u64)) -> Vec<Sample> {
        (0..120)
            .map(|i| {
                let secs = i as f64 * 10.0;
                let (rss_kib, threads) = f(secs);
                Sample {
                    secs,
                    rss_kib,
                    threads,
                    ..Sample::default()
                }
            })
            .collect()
    }

    #[test]
    fn only_java_running_a_server_jar_counts() {
        assert!(is_server_cmdline(
            b"/jdk/bin/java\0-Xmx1G\0-jar\0/x/server.jar\0--nogui\0"
        ));
        assert!(!is_server_cmdline(
            b"/bin/zsh\0-c\0until ! pgrep -f server.jar\0"
        ));
        assert!(!is_server_cmdline(b"/jdk/bin/java\0-jar\0client.jar\0"));
    }

    #[test]
    fn slope_of_a_line() {
        let xs = [0.0, 1.0, 2.0, 3.0];
        let ys = [1.0, 3.0, 5.0, 7.0];
        assert!((slope(&xs, &ys) - 2.0).abs() < 1e-9);
        assert_eq!(slope(&[1.0], &[1.0]), 0.0);
    }

    #[test]
    fn flat_rss_with_noise_passes() {
        let s = samples(|t| (500_000 + ((t as u64 * 7919) % 20_000), 40));
        let v = analyze(&s, 300.0, 1.0);
        assert!(v.passed(), "{:?}", v.failures);
    }

    #[test]
    fn steady_growth_fails() {
        let s = samples(|t| (500_000 + (t / 60.0 * 2048.0) as u64, 40));
        let v = analyze(&s, 300.0, 1.0);
        assert!(!v.passed());
        assert!((v.rss_slope_mib_per_min - 2.0).abs() < 0.01);
    }

    #[test]
    fn warmup_growth_is_ignored() {
        let s = samples(|t| (if t < 300.0 { t as u64 * 1000 } else { 300_000 }, 40));
        assert!(analyze(&s, 300.0, 1.0).passed());
    }

    #[test]
    fn thread_growth_fails() {
        let s = samples(|t| (500_000, 40 + (t / 100.0) as u64));
        let v = analyze(&s, 300.0, 1.0);
        assert!(v.failures.iter().any(|f| f.contains("threads")));
    }

    #[test]
    fn parses_proc_status_and_stats_lines() {
        let status = "Name:\tmcr\nVmRSS:\t  123456 kB\nThreads:\t37\n";
        assert_eq!(parse_proc_status(status), Some((123_456, 37)));
        let stats = parse_stats("soak-stats gpu_buffers=12 gpu_buffer_bytes=4096 bad=x").unwrap();
        assert_eq!(stats.get("gpu_buffers"), Some(&12));
        assert_eq!(stats.get("bad"), None);
        assert!(parse_stats("joined as Soak").is_none());
    }

    #[test]
    fn parses_gc_log_heap() {
        let line = "[12.345s][info][gc] GC(7) Pause Young (Normal) (G1 Evacuation Pause) 81M->23M(256M) 3.120ms";
        assert_eq!(heap_after_gc(line), Some(23 * 1024));
        assert_eq!(heap_after_gc("[gc] Using G1"), None);
    }

    #[test]
    fn csv_row_matches_header() {
        let s = Sample {
            secs: 10.0,
            rss_kib: 1,
            threads: 2,
            ..Sample::default()
        };
        assert_eq!(
            csv_row(&s).split(',').count(),
            csv_header().split(',').count()
        );
    }
}
