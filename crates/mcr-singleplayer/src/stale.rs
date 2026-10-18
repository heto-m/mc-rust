use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::{Result, SpError};

pub const PID_FILE: &str = "server.pid";
/// A JVM saving a large world on SIGTERM can take a while; SIGKILL mid-save risks the world.
const TERM_WAIT: Duration = Duration::from_secs(60);
const KILL_WAIT: Duration = Duration::from_secs(3);

pub const TOKEN_ARG: &str = "-Dmcr.sp.token=";
pub const WORLD_ARG: &str = "-Dmcr.sp.world=";

/// A random 128-bit hex token for one launch.
pub fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let half = |salt: u64| {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(salt);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        h.write_u32(std::process::id());
        h.finish()
    };
    format!("{:016x}{:016x}", half(1), half(2))
}

/// `-Dmcr.sp.world=<canonical world path>`, the JVM argument naming the world a server belongs to.
pub fn world_marker(world: &Path) -> std::ffi::OsString {
    let path = fs::canonicalize(world)
        .or_else(|_| std::path::absolute(world))
        .unwrap_or_else(|_| world.to_path_buf());
    let mut arg = std::ffi::OsString::from(WORLD_ARG);
    arg.push(path);
    arg
}

/// The pid and launch token from a pid file; the file comes from the world folder and is untrusted.
pub fn read_pid_file(path: &Path) -> Option<(u32, Option<String>)> {
    let mut text = String::new();
    crate::safe::open_read(path)
        .ok()?
        .take(128)
        .read_to_string(&mut text)
        .ok()?;
    let mut parts = text.split_whitespace();
    let pid = parts.next()?.parse().ok().filter(|&p| p > 1)?;
    let token = parts
        .next()
        .filter(|t| t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_owned);
    Some((pid, token))
}

pub fn read_pid(path: &Path) -> Option<u32> {
    read_pid_file(path).map(|(pid, _)| pid)
}

/// Whether a NUL-separated command line runs a Minecraft server jar.
pub fn is_server_cmdline(cmdline: &[u8]) -> bool {
    cmdline.split(|&b| b == 0).any(|arg| {
        let name = arg.rsplit(|&b| b == b'/').next().unwrap_or(arg);
        name == b"server.jar" || (name.starts_with(b"minecraft_server") && name.ends_with(b".jar"))
    })
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 only checks that the process exists.
    let r = unsafe { libc::kill(pid, 0) };
    let exists = r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
    exists && !zombie(pid as u32)
}

#[cfg(target_os = "linux")]
fn zombie(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| s.rsplit_once(") ").map(|(_, rest)| rest.starts_with('Z')))
        .unwrap_or(false)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn zombie(_pid: u32) -> bool {
    false
}

#[cfg(not(unix))]
fn alive(_pid: u32) -> bool {
    false
}

#[cfg(target_os = "linux")]
fn cmdline(pid: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    fs::File::open(format!("/proc/{pid}/cmdline"))
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

#[cfg(not(target_os = "linux"))]
fn cmdline(_pid: u32) -> Option<Vec<u8>> {
    None
}

#[cfg(target_os = "linux")]
fn same_user(pid: u32) -> bool {
    let Ok(status) = fs::read_to_string(format!("/proc/{pid}/status")) else {
        return false;
    };
    // SAFETY: getuid has no preconditions.
    let me = unsafe { libc::getuid() };
    status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|l| l.split_whitespace().next())
        .and_then(|uid| uid.parse::<u32>().ok())
        == Some(me)
}

/// Whether `pid` provably is a server mcr started for `world`: same user, a server jar, this
/// world's marker and, when known, this launch's token.
#[cfg(target_os = "linux")]
pub fn proven_server(pid: u32, world: &Path, token: Option<&str>) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Some(cmdline) = cmdline(pid) else {
        return false;
    };
    let marker = world_marker(world);
    let has = |want: &[u8]| cmdline.split(|&b| b == 0).any(|a| a == want);
    same_user(pid)
        && is_server_cmdline(&cmdline)
        && has(marker.as_bytes())
        && token.is_none_or(|t| has(format!("{TOKEN_ARG}{t}").as_bytes()))
}

#[cfg(not(target_os = "linux"))]
pub fn proven_server(_pid: u32, _world: &Path, _token: Option<&str>) -> bool {
    false
}

#[cfg(unix)]
fn signal(pid: u32, sig: i32) {
    if let Ok(pid) = i32::try_from(pid) {
        // SAFETY: kill has no memory-safety preconditions.
        unsafe {
            libc::kill(pid, sig);
        }
    }
}

fn wait_gone(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while alive(pid) {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

/// SIGTERM (the JVM saves the world in its shutdown hook), then SIGKILL after a bounded wait.
#[cfg(unix)]
pub fn terminate(pid: u32) -> bool {
    signal(pid, libc::SIGTERM);
    if wait_gone(pid, TERM_WAIT) {
        return true;
    }
    signal(pid, libc::SIGKILL);
    wait_gone(pid, KILL_WAIT)
}

#[cfg(not(unix))]
pub fn terminate(_pid: u32) -> bool {
    false
}

/// The pid holding a POSIX record lock on `path` (Java's `FileChannel.tryLock`), if any.
#[cfg(target_os = "linux")]
pub fn lock_owner(path: &Path) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let file = crate::safe::open_read(path).ok()?;
    // SAFETY: flock is plain data; an all-zero value is valid before the fields are set.
    let mut lock: libc::flock = unsafe { std::mem::zeroed() };
    lock.l_type = libc::F_WRLCK as libc::c_short;
    lock.l_whence = libc::SEEK_SET as libc::c_short;
    // SAFETY: F_GETLK only writes into `lock`, which outlives the call.
    let r = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETLK, &mut lock) };
    if r != 0 || lock.l_type == libc::F_UNLCK as libc::c_short {
        return None;
    }
    u32::try_from(lock.l_pid).ok().filter(|&p| p > 0)
}

#[cfg(not(target_os = "linux"))]
pub fn lock_owner(_path: &Path) -> Option<u32> {
    None
}

/// Stops a server left running by an earlier session of this world, or refuses when another program
/// holds its `session.lock`. Only a process proven to be mcr's server for this world is signalled.
pub fn clear_stale(world: &Path, server_dir: &Path) -> Result<bool> {
    let pid_path = server_dir.join(PID_FILE);
    let me = std::process::id();
    let mut stopped = false;
    if let Some((pid, Some(token))) = read_pid_file(&pid_path)
        && pid != me
        && alive(pid)
        && proven_server(pid, world, Some(&token))
    {
        tracing::warn!(
            "stopping a leftover server (pid {pid}) for {}",
            world.display()
        );
        stopped = true;
        if !terminate(pid) {
            return Err(SpError::Refused(format!(
                "an earlier server (pid {pid}) for this world could not be stopped"
            )));
        }
    }
    if let Some(pid) = lock_owner(&world.join("session.lock")).filter(|&p| p != me && alive(p)) {
        if !proven_server(pid, world, None) {
            return Err(SpError::Refused(format!(
                "the world is in use by another program (pid {pid})"
            )));
        }
        stopped = true;
        if !terminate(pid) {
            return Err(SpError::Refused(format!(
                "an earlier server (pid {pid}) for this world could not be stopped"
            )));
        }
    }
    let _ = fs::remove_file(&pid_path);
    Ok(stopped)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn server_command_lines() {
        assert!(is_server_cmdline(
            b"java\0-Xmx1G\0-jar\0/g/versions/26.3/server.jar\0nogui\0"
        ));
        assert!(is_server_cmdline(
            b"java\0-jar\0minecraft_server.1.21.jar\0"
        ));
        assert!(!is_server_cmdline(b"java\0-jar\0client.jar\0"));
        assert!(!is_server_cmdline(b"vim\0server.jar.txt\0"));
    }

    #[test]
    fn pid_files() {
        let dir = crate::level::tests::temp_dir("pid");
        let token = new_token();
        assert_eq!(token.len(), 32);
        assert_ne!(token, new_token());
        fs::write(dir.join(PID_FILE), format!("1234 {token}\n")).unwrap();
        assert_eq!(
            read_pid_file(&dir.join(PID_FILE)),
            Some((1234, Some(token)))
        );
        fs::write(dir.join(PID_FILE), "1234 short").unwrap();
        assert_eq!(read_pid_file(&dir.join(PID_FILE)), Some((1234, None)));
        fs::write(dir.join(PID_FILE), "1").unwrap();
        assert_eq!(read_pid(&dir.join(PID_FILE)), None);
        fs::write(dir.join(PID_FILE), "x".repeat(100)).unwrap();
        assert_eq!(read_pid(&dir.join(PID_FILE)), None);
        let _ = fs::remove_dir_all(&dir);
    }

    fn fake_server(world: &Path, token: &str) -> std::process::Child {
        let child = Command::new("sh")
            .args(["-c", "sleep 5; :", "server.jar"])
            .arg(world_marker(world))
            .arg(format!("{TOKEN_ARG}{token}"))
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        // The pid exists before exec replaces the forked test binary's command line.
        let cmdline = format!("/proc/{}/cmdline", child.id());
        for _ in 0..200 {
            if fs::read(&cmdline).is_ok_and(|c| is_server_cmdline(&c)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        child
    }

    fn running(child: &mut std::process::Child) -> bool {
        child.try_wait().unwrap().is_none()
    }

    #[test]
    fn leftover_server_is_stopped_only_when_proven() {
        let world = crate::level::tests::temp_dir("stale");
        let other = crate::level::tests::temp_dir("stale-other");
        let token = new_token();
        let mut proven = fake_server(&world, &token);
        fs::write(world.join(PID_FILE), format!("{} {token}", proven.id())).unwrap();
        assert!(clear_stale(&world, &world).unwrap());
        assert!(proven.try_wait().unwrap().is_some() || proven.wait().is_ok());
        assert!(!world.join(PID_FILE).exists());

        let mut wrong_token = fake_server(&world, &new_token());
        let mut wrong_world = fake_server(&other, &token);
        let mut stranger = Command::new("sleep").arg("30").spawn().unwrap();
        for (child, tok) in [
            (&mut wrong_token, token.clone()),
            (&mut wrong_world, token.clone()),
            (&mut stranger, token.clone()),
        ] {
            fs::write(world.join(PID_FILE), format!("{} {tok}", child.id())).unwrap();
            clear_stale(&world, &world).unwrap();
            assert!(running(child), "an unproven process is never signalled");
        }
        fs::write(world.join(PID_FILE), format!("{}", wrong_token.id())).unwrap();
        clear_stale(&world, &world).unwrap();
        assert!(running(&mut wrong_token), "no token, no kill");
        for mut child in [wrong_token, wrong_world, stranger] {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&world);
        let _ = fs::remove_dir_all(&other);
    }

    #[test]
    fn session_lock_owner() {
        let dir = crate::level::tests::temp_dir("lock");
        let lock = dir.join("session.lock");
        fs::write(&lock, "").unwrap();
        assert_eq!(lock_owner(&lock), None);
        let script = "import fcntl,sys,time\nf=open(sys.argv[1],'r+')\nfcntl.lockf(f,fcntl.LOCK_EX)\nprint('locked',flush=True)\ntime.sleep(20)\n";
        let Ok(mut holder) = Command::new("python3")
            .args(["-I", "-c", script])
            .arg(&lock)
            .stdout(Stdio::piped())
            .spawn()
        else {
            return;
        };
        let mut line = String::new();
        let _ = std::io::BufRead::read_line(
            &mut std::io::BufReader::new(holder.stdout.take().unwrap()),
            &mut line,
        );
        if line.trim() == "locked" {
            assert_eq!(lock_owner(&lock), Some(holder.id()));
            assert!(
                clear_stale(&dir, &dir).is_err(),
                "a stranger's lock refuses the start"
            );
        }
        let _ = holder.kill();
        let _ = holder.wait();
        let _ = fs::remove_dir_all(&dir);
    }
}
