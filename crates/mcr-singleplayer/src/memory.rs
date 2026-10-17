use std::process::{Command, Stdio};

pub const MIN_HEAP_MIB: u64 = 2048;
pub const MAX_HEAP_MIB: u64 = 4096;
/// Room above the heap for metaspace, code cache, thread stacks and direct buffers.
pub const SCOPE_EXTRA_MIB: u64 = 1024;
pub const HEAP_ENV: &str = "MCR_SP_HEAP_MIB";

/// A quarter of system memory clamped to 2–4 GiB; a valid `override_mib` (512–16384) wins.
pub fn heap_mib(total_mib: Option<u64>, override_mib: Option<&str>) -> u64 {
    if let Some(v) = override_mib
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| (512..=16384).contains(v))
    {
        return v;
    }
    total_mib.map_or(MIN_HEAP_MIB, |t| (t / 4).clamp(MIN_HEAP_MIB, MAX_HEAP_MIB))
}

pub fn scope_limit_mib(heap: u64) -> u64 {
    heap + SCOPE_EXTRA_MIB
}

pub fn total_memory_mib() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    parse_meminfo(&text)
}

pub fn parse_meminfo(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib / 1024)
}

/// The heap for the server JVM on this machine.
pub fn server_heap_mib() -> u64 {
    heap_mib(total_memory_mib(), std::env::var(HEAP_ENV).ok().as_deref())
}

/// Whether `systemd-run --user --scope` can wrap a process here.
pub fn scope_available() -> bool {
    cfg!(target_os = "linux")
        && Command::new("systemd-run")
            .args(["--user", "--scope", "-q", "true"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_from_ram() {
        assert_eq!(heap_mib(Some(16 * 1024), None), 4096);
        assert_eq!(heap_mib(Some(15 * 1024), None), 3840);
        assert_eq!(heap_mib(Some(4 * 1024), None), 2048);
        assert_eq!(heap_mib(None, None), 2048);
        assert_eq!(heap_mib(Some(64 * 1024), Some("1536")), 1536);
        assert_eq!(heap_mib(Some(64 * 1024), Some("10")), 4096);
        assert_eq!(heap_mib(Some(64 * 1024), Some("x")), 4096);
        assert_eq!(scope_limit_mib(4096), 5120);
    }

    #[test]
    fn meminfo() {
        let text = "MemTotal:       16166144 kB\nMemFree:  1 kB\n";
        assert_eq!(parse_meminfo(text), Some(15787));
        assert_eq!(parse_meminfo("nope"), None);
    }
}
