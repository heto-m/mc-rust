use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, LineWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::layout::Layout;
use crate::memory::scope_limit_mib;

pub const READY_TIMEOUT: Duration = Duration::from_secs(300);
pub const STOP_GRACE: Duration = Duration::from_secs(60);
/// How long dropping a running server may block while it saves.
pub const DROP_GRACE: Duration = Duration::from_secs(30);
const MAX_LINE: usize = 4096;
const MAX_LOG: u64 = 64 << 20;
const MAX_CONSOLE: usize = 256;
const TAIL_LINES: usize = 8;
const TAIL_CHARS: usize = 200;
const POLL: Duration = Duration::from_millis(50);

/// How to run one server process.
#[derive(Clone, Debug)]
pub struct Spec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    /// Receives every output line, stdout and stderr interleaved.
    pub log: PathBuf,
    pub pid_file: Option<PathBuf>,
    /// Per-launch token written next to the pid, proving later that a pid is this launch's server.
    pub token: String,
    pub ready_timeout: Duration,
}

/// The parts of a vanilla server launch that vary.
#[derive(Clone, Debug)]
pub struct VanillaLaunch<'a> {
    pub java: &'a Path,
    pub jar: &'a Path,
    pub folder: &'a str,
    pub port: u16,
    pub heap_mib: u64,
    /// Wrap the JVM in a `systemd-run --user --scope` with a memory limit.
    pub scope: bool,
    pub bonus_chest: bool,
}

impl Spec {
    pub fn vanilla(layout: &Layout, launch: &VanillaLaunch<'_>) -> Option<Self> {
        let cwd = layout.server_dir(launch.folder)?;
        let mut args: Vec<OsString> = Vec::new();
        let program = if launch.scope {
            for a in ["--user", "--scope", "-q", "-p"] {
                args.push(a.into());
            }
            args.push(format!("MemoryMax={}M", scope_limit_mib(launch.heap_mib)).into());
            args.push("-p".into());
            args.push("MemorySwapMax=0".into());
            args.push("--".into());
            args.push(launch.java.into());
            PathBuf::from("systemd-run")
        } else {
            launch.java.to_path_buf()
        };
        args.push(format!("-Xmx{}M", launch.heap_mib).into());
        args.push(format!("-Xms{}M", launch.heap_mib.min(512)).into());
        args.push("-Djava.awt.headless=true".into());
        let world = layout.world(launch.folder)?;
        let token = crate::stale::new_token();
        args.push(crate::stale::world_marker(&world));
        args.push(format!("{}{token}", crate::stale::TOKEN_ARG).into());
        let mut repo = OsString::from("-DbundlerRepoDir=");
        repo.push(layout.bundle_dir());
        args.push(repo);
        args.push("-jar".into());
        args.push(launch.jar.into());
        args.push("--universe".into());
        args.push(layout.saves().into());
        args.push("--world".into());
        args.push(launch.folder.into());
        args.push("--port".into());
        args.push(launch.port.to_string().into());
        if launch.bonus_chest {
            args.push("--bonusChest".into());
        }
        args.push("nogui".into());
        Some(Self {
            program,
            args,
            log: cwd.join("logs").join("console.log"),
            pid_file: Some(cwd.join(crate::stale::PID_FILE)),
            token,
            cwd,
            ready_timeout: READY_TIMEOUT,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Loading the level; a percentage once "Preparing spawn area" lines appear.
    Preparing(Option<u8>),
    Ready,
    Exited {
        code: Option<i32>,
        /// A stop was asked for, so this exit is expected.
        requested: bool,
        /// Why mcr killed it, or the last output lines of an unexpected exit.
        reason: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LineEvent {
    Done,
    Spawn(u8),
    Loading,
}

/// What one server output line tells the supervisor.
fn parse_line(line: &str) -> Option<LineEvent> {
    let message = match line.find("]: ") {
        Some(i) if line.starts_with('[') => &line[i + 3..],
        _ => line,
    };
    if message.starts_with("Done (") && message.contains(")! For help") {
        return Some(LineEvent::Done);
    }
    if let Some(rest) = message.strip_prefix("Preparing spawn area: ") {
        let digits: String = rest
            .chars()
            .take_while(char::is_ascii_digit)
            .take(3)
            .collect();
        return digits
            .parse::<u16>()
            .ok()
            .map(|p| LineEvent::Spawn(p.min(100) as u8));
    }
    (message.starts_with("Preparing level") || message.starts_with("Selecting global world spawn"))
        .then_some(LineEvent::Loading)
}

fn sanitize(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .map(|c| if c == '\t' { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect()
}

/// Reads one line of at most `max` bytes; the rest of a longer line is consumed and dropped.
fn read_line_bounded(
    reader: &mut impl BufRead,
    out: &mut Vec<u8>,
    max: usize,
) -> io::Result<usize> {
    let mut total = 0;
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            return Ok(total);
        }
        let (used, found) = match buf.iter().position(|&b| b == b'\n') {
            Some(i) => (i + 1, true),
            None => (buf.len(), false),
        };
        let room = max.saturating_sub(out.len());
        let keep = if found { used - 1 } else { used };
        out.extend_from_slice(&buf[..keep.min(room)]);
        reader.consume(used);
        total += used;
        if found {
            return Ok(total);
        }
    }
}

struct Log {
    out: Option<LineWriter<File>>,
    written: u64,
}

impl Log {
    fn open(path: &Path) -> Self {
        let out = path
            .parent()
            .map_or(Ok(()), crate::safe::ensure_dir)
            .and_then(|()| crate::safe::create_log(path))
            .map(LineWriter::new)
            .map_err(|e| tracing::warn!("server log {}: {e}", path.display()))
            .ok();
        Self { out, written: 0 }
    }

    fn line(&mut self, text: &str) {
        let Some(out) = &mut self.out else { return };
        self.written += text.len() as u64 + 1;
        if self.written > MAX_LOG {
            let _ = writeln!(out, "[mcr] log truncated at {MAX_LOG} bytes");
            self.out = None;
            return;
        }
        if writeln!(out, "{text}").is_err() {
            self.out = None;
        }
    }
}

enum Msg {
    Line(String, Option<LineEvent>),
    Console(String),
    Stop(Duration),
    Kill,
}

fn reader(stream: impl Read, log: Arc<Mutex<Log>>, tx: Sender<Msg>) {
    let mut reader = BufReader::with_capacity(16 * 1024, stream);
    let mut line = Vec::with_capacity(256);
    loop {
        line.clear();
        match read_line_bounded(&mut reader, &mut line, MAX_LINE) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let text = sanitize(&line);
        if let Ok(mut log) = log.lock() {
            log.line(&text);
        }
        let event = parse_line(&text);
        let short: String = text.chars().take(TAIL_CHARS).collect();
        if tx.send(Msg::Line(short, event)).is_err() {
            return;
        }
    }
}

#[cfg(target_os = "linux")]
fn child_setup(parent: u32) -> io::Result<()> {
    // SAFETY: prctl, getppid and _exit are async-signal-safe and touch no Rust state.
    unsafe {
        if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
            return Err(io::Error::last_os_error());
        }
        if libc::getppid() as u32 != parent {
            libc::_exit(1);
        }
    }
    Ok(())
}

fn configure(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
        #[cfg(target_os = "linux")]
        {
            let parent = std::process::id();
            // SAFETY: the hook only makes async-signal-safe calls (see child_setup).
            unsafe {
                cmd.pre_exec(move || child_setup(parent));
            }
        }
    }
    #[cfg(not(unix))]
    let _ = cmd;
}

fn kill_group(child: &mut Child) {
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(child.id()) {
        // SAFETY: kill has no memory-safety preconditions; the group is the unreaped child's own.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

fn send_console(stdin: &mut Option<ChildStdin>, line: &str) {
    if let Some(pipe) = stdin
        && writeln!(pipe, "{line}")
            .and_then(|()| pipe.flush())
            .is_err()
    {
        *stdin = None;
    }
}

fn spawn(spec: &Spec) -> io::Result<Child> {
    crate::safe::ensure_dir(&spec.cwd).map_err(io::Error::other)?;
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure(&mut cmd);
    cmd.spawn()
}

fn supervise(
    spec: Spec,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    events: Sender<Event>,
    started: SyncSender<io::Result<u32>>,
) {
    let mut child = match spawn(&spec) {
        Ok(child) => child,
        Err(e) => {
            let _ = started.send(Err(e));
            return;
        }
    };
    if let Some(path) = &spec.pid_file {
        let pid = format!("{} {}\n", child.id(), spec.token);
        if let Err(e) = crate::safe::write_file(path, pid.as_bytes()) {
            tracing::warn!("server pid file: {e}");
        }
    }
    let _ = started.send(Ok(child.id()));
    let log = Arc::new(Mutex::new(Log::open(&spec.log)));
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        let (log, tx) = (log.clone(), tx.clone());
        readers.push(std::thread::spawn(move || reader(out, log, tx)));
    }
    if let Some(err) = child.stderr.take() {
        let (log, tx) = (log.clone(), tx.clone());
        readers.push(std::thread::spawn(move || reader(err, log, tx)));
    }
    drop(tx);
    let mut stdin = child.stdin.take();
    let begun = Instant::now();
    let mut ready = false;
    let mut requested = false;
    let mut kill_at: Option<Instant> = None;
    let mut reason: Option<String> = None;
    let mut tail: VecDeque<String> = VecDeque::with_capacity(TAIL_LINES);
    let code = loop {
        match rx.recv_timeout(POLL) {
            Ok(Msg::Line(text, event)) => {
                if tail.len() == TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(text);
                let out = match event {
                    Some(LineEvent::Done) if !ready => {
                        ready = true;
                        Some(Event::Ready)
                    }
                    Some(LineEvent::Spawn(p)) if !ready => Some(Event::Preparing(Some(p))),
                    Some(LineEvent::Loading) if !ready => Some(Event::Preparing(None)),
                    _ => None,
                };
                if let Some(e) = out {
                    let _ = events.send(e);
                }
            }
            Ok(Msg::Console(line)) => send_console(&mut stdin, &line),
            Ok(Msg::Stop(grace)) => {
                if !requested {
                    requested = true;
                    send_console(&mut stdin, "save-all flush");
                    send_console(&mut stdin, "stop");
                    kill_at = Some(Instant::now() + grace);
                }
            }
            Ok(Msg::Kill) => {
                requested = true;
                kill_group(&mut child);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => std::thread::sleep(POLL),
        }
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {}
            Err(e) => {
                reason = Some(format!("lost track of the server process: {e}"));
                kill_group(&mut child);
                break None;
            }
        }
        if !ready && kill_at.is_none() && begun.elapsed() > spec.ready_timeout {
            reason = Some(format!(
                "the server did not finish starting within {} s",
                spec.ready_timeout.as_secs()
            ));
            kill_group(&mut child);
            kill_at = Some(Instant::now() + Duration::from_secs(5));
        }
        if kill_at.is_some_and(|t| Instant::now() >= t) {
            if reason.is_none() {
                reason = Some("the server did not stop in time and was killed".into());
            }
            kill_group(&mut child);
            kill_at = Some(Instant::now() + Duration::from_secs(5));
        }
    };
    drop(stdin);
    for r in readers {
        let _ = r.join();
    }
    while let Ok(Msg::Line(text, _)) = rx.try_recv() {
        if tail.len() == TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(text);
    }
    if let Some(path) = &spec.pid_file {
        let _ = fs::remove_file(path);
    }
    if reason.is_none() && !requested {
        let lines: Vec<&str> = tail.iter().map(String::as_str).collect();
        reason = Some(lines.join("\n"));
    }
    let _ = events.send(Event::Exited {
        code,
        requested,
        reason,
    });
}

/// A server process owned by a supervisor thread.
#[derive(Debug)]
pub struct ManagedServer {
    tx: Sender<Msg>,
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
    pid: u32,
    exited: bool,
}

impl std::fmt::Debug for Msg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Line(..) => "Line",
            Self::Console(_) => "Console",
            Self::Stop(_) => "Stop",
            Self::Kill => "Kill",
        })
    }
}

impl ManagedServer {
    /// Spawns the process; the supervisor thread forks it, so on Linux it lives as long as the child.
    pub fn start(spec: Spec) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let (events_tx, events) = mpsc::channel();
        let (started_tx, started) = mpsc::sync_channel(1);
        let readers_tx = tx.clone();
        let thread = std::thread::Builder::new()
            .name("mcr-sp-server".into())
            .spawn(move || supervise(spec, readers_tx, rx, events_tx, started_tx))?;
        let pid = match started.recv() {
            Ok(Ok(pid)) => pid,
            Ok(Err(e)) => {
                let _ = thread.join();
                return Err(e);
            }
            Err(_) => {
                let _ = thread.join();
                return Err(io::Error::other("the server supervisor stopped"));
            }
        };
        Ok(Self {
            tx,
            events,
            thread: Some(thread),
            pid,
            exited: false,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn exited(&self) -> bool {
        self.exited
    }

    /// The next event, if any.
    pub fn poll(&mut self) -> Option<Event> {
        let event = self.events.try_recv().ok()?;
        if matches!(event, Event::Exited { .. }) {
            self.exited = true;
        }
        Some(event)
    }

    /// One console command; line breaks and overlong commands are refused.
    pub fn console(&self, line: &str) -> bool {
        let ok =
            !line.is_empty() && line.len() <= MAX_CONSOLE && !line.chars().any(char::is_control);
        ok && self.tx.send(Msg::Console(line.to_owned())).is_ok()
    }

    /// Asks the server to save and stop, killing it after `grace`.
    pub fn stop(&self, grace: Duration) {
        let _ = self.tx.send(Msg::Stop(grace));
    }

    pub fn kill(&self) {
        let _ = self.tx.send(Msg::Kill);
    }

    /// Waits up to `timeout` for the exit event; the supervisor enforces its own deadlines.
    pub fn wait(&mut self, timeout: Duration) -> Option<Event> {
        let deadline = Instant::now() + timeout;
        while !self.exited {
            let left = deadline.checked_duration_since(Instant::now())?;
            match self.events.recv_timeout(left.min(POLL)) {
                Ok(e @ Event::Exited { .. }) => {
                    self.exited = true;
                    return Some(e);
                }
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.exited = true;
                    return None;
                }
            }
        }
        None
    }
}

impl Drop for ManagedServer {
    fn drop(&mut self) {
        if !self.exited {
            self.stop(DROP_GRACE);
            let _ = self.wait(DROP_GRACE + Duration::from_secs(10));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn script(tag: &str, body: &str) -> (PathBuf, Spec) {
        let dir = crate::level::tests::temp_dir(&format!("server-{tag}"));
        let path = dir.join("fake.sh");
        fs::write(&path, body).unwrap();
        let spec = Spec {
            program: PathBuf::from("sh"),
            args: vec![path.into()],
            cwd: dir.clone(),
            log: dir.join("logs/console.log"),
            pid_file: Some(dir.join("server.pid")),
            token: "t".into(),
            ready_timeout: Duration::from_secs(10),
        };
        (dir, spec)
    }

    fn until(
        server: &mut ManagedServer,
        timeout: Duration,
        pred: impl Fn(&Event) -> bool,
    ) -> Event {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(e) = server.poll() {
                if pred(&e) {
                    return e;
                }
                continue;
            }
            assert!(Instant::now() < deadline, "timed out waiting for an event");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    const POLITE: &str = r#"
echo "[12:00:00] [Server thread/INFO]: Starting minecraft server version 26.3"
echo "[12:00:00] [Server thread/INFO]: Preparing level \"w\""
echo "[12:00:01] [Worker/INFO]: Preparing spawn area: 42%"
echo "oops" >&2
echo "[12:00:02] [Server thread/INFO]: Done (1.234s)! For help, type \"help\""
while read line; do
  echo "[12:00:03] [Server thread/INFO]: got $line"
  if [ "$line" = "stop" ]; then echo "Stopping server"; exit 0; fi
done
"#;

    #[test]
    fn start_ready_console_stop() {
        let (dir, mut spec) = script("polite", POLITE);
        spec.ready_timeout = Duration::from_secs(20);
        let mut server = ManagedServer::start(spec).unwrap();
        assert!(dir.join("server.pid").is_file());
        assert_eq!(
            until(&mut server, Duration::from_secs(10), |e| matches!(
                e,
                Event::Preparing(Some(_))
            )),
            Event::Preparing(Some(42))
        );
        until(&mut server, Duration::from_secs(10), |e| *e == Event::Ready);
        assert!(server.console("tick freeze"));
        assert!(!server.console("a\nstop"));
        server.stop(Duration::from_secs(10));
        let exit = server.wait(Duration::from_secs(10)).unwrap();
        assert_eq!(
            exit,
            Event::Exited {
                code: Some(0),
                requested: true,
                reason: None
            }
        );
        let log = fs::read_to_string(dir.join("logs/console.log")).unwrap();
        assert!(log.contains("oops"));
        assert!(log.contains("got tick freeze"));
        assert!(log.contains("got save-all flush"));
        assert!(!dir.join("server.pid").exists());
        drop(server);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn never_ready_is_killed() {
        let (dir, mut spec) = script("slow", "echo starting\nexec sleep 60\n");
        spec.ready_timeout = Duration::from_millis(500);
        let mut server = ManagedServer::start(spec).unwrap();
        let exit = server.wait(Duration::from_secs(10)).unwrap();
        let Event::Exited { reason, .. } = exit else {
            unreachable!()
        };
        assert!(reason.unwrap().contains("did not finish starting"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignoring_stop_gets_killed() {
        let body = "echo 'Done (0.1s)! For help, type \"help\"'\ntrap '' TERM\nwhile read l; do :; done\nexec sleep 60\n";
        let (dir, spec) = script("stubborn", body);
        let mut server = ManagedServer::start(spec).unwrap();
        until(&mut server, Duration::from_secs(10), |e| *e == Event::Ready);
        let pid = server.pid();
        let started = Instant::now();
        server.stop(Duration::from_millis(300));
        let exit = server.wait(Duration::from_secs(10)).unwrap();
        assert!(started.elapsed() < Duration::from_secs(8));
        let Event::Exited {
            requested, reason, ..
        } = exit
        else {
            unreachable!()
        };
        assert!(requested);
        assert!(reason.unwrap().contains("killed"));
        assert!(!Path::new(&format!("/proc/{pid}")).exists() || cfg!(not(target_os = "linux")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_reports_last_lines() {
        let body =
            "echo 'Preparing level \"w\"'\necho '**** FAILED TO BIND TO PORT!' >&2\nexit 3\n";
        let (dir, spec) = script("crash", body);
        let mut server = ManagedServer::start(spec).unwrap();
        let exit = server.wait(Duration::from_secs(10)).unwrap();
        let Event::Exited {
            code,
            requested,
            reason,
        } = exit
        else {
            unreachable!()
        };
        assert_eq!(code, Some(3));
        assert!(!requested);
        assert!(reason.unwrap().contains("FAILED TO BIND"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_program_fails_to_start() {
        let (dir, mut spec) = script("missing", "");
        spec.program = dir.join("does-not-exist");
        assert!(ManagedServer::start(spec).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn drop_stops_the_server() {
        let (dir, spec) = script("drop", POLITE);
        let mut server = ManagedServer::start(spec).unwrap();
        until(&mut server, Duration::from_secs(10), |e| *e == Event::Ready);
        drop(server);
        let log = fs::read_to_string(dir.join("logs/console.log")).unwrap();
        assert!(log.contains("Stopping server"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lines_are_bounded() {
        let mut input: &[u8] = b"short\nxxxxxxxxxx\nend";
        let mut out = Vec::new();
        assert_eq!(read_line_bounded(&mut input, &mut out, 4).unwrap(), 6);
        assert_eq!(out, b"shor");
        out.clear();
        read_line_bounded(&mut input, &mut out, 4).unwrap();
        assert_eq!(out, b"xxxx");
        out.clear();
        read_line_bounded(&mut input, &mut out, 4).unwrap();
        assert_eq!(out, b"end");
        out.clear();
        assert_eq!(read_line_bounded(&mut input, &mut out, 4).unwrap(), 0);
        assert_eq!(sanitize(b"a\x1b[31mb\tc\xff"), "a[31mb c\u{fffd}");
    }

    #[test]
    fn output_parsing() {
        assert_eq!(
            parse_line("[12:00:02] [Server thread/INFO]: Done (3.2s)! For help, type \"help\""),
            Some(LineEvent::Done)
        );
        assert_eq!(
            parse_line("[1] [Worker-Main-3/INFO]: Preparing spawn area: 937%"),
            Some(LineEvent::Spawn(100))
        );
        assert_eq!(
            parse_line("[1] [Server thread/INFO]: Preparing level \"x\""),
            Some(LineEvent::Loading)
        );
        assert_eq!(parse_line("<Steve> Done (1s)! For help"), None);
        assert_eq!(parse_line("[1] [x]: Preparing spawn area: x%"), None);
    }

    #[test]
    fn vanilla_command_line() {
        let layout = Layout::new("/g");
        let launch = VanillaLaunch {
            java: Path::new("/jdk/bin/java"),
            jar: Path::new("/g/versions/26.3/server.jar"),
            folder: "My World",
            port: 41000,
            heap_mib: 1536,
            scope: true,
            bonus_chest: true,
        };
        let spec = Spec::vanilla(&layout, &launch).unwrap();
        let args: Vec<String> = spec
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(spec.program, Path::new("systemd-run"));
        assert!(args.contains(&"MemoryMax=2560M".to_owned()));
        assert!(args.contains(&"-Xmx1536M".to_owned()));
        assert!(args.contains(&"--bonusChest".to_owned()));
        assert_eq!(args.last().unwrap(), "nogui");
        let world = args.iter().position(|a| a == "--world").unwrap();
        assert_eq!(args[world + 1], "My World");
        assert_eq!(spec.cwd, Path::new("/g/saves/My World/mcr-server"));
        let plain = Spec::vanilla(
            &layout,
            &VanillaLaunch {
                scope: false,
                ..launch
            },
        )
        .unwrap();
        assert_eq!(plain.program, Path::new("/jdk/bin/java"));
        assert!(
            Spec::vanilla(
                &layout,
                &VanillaLaunch {
                    folder: "..",
                    ..launch
                }
            )
            .is_none()
        );
    }
}
