//! Sampling heap profiler (`--features heap-profile`): about one allocation per sampled
//! `RATE` bytes keeps its backtrace while live; `MCR_HEAP_PROFILE=<secs>:<path>` writes the live
//! samples, grouped by call stack, after that many seconds.

use std::alloc::{GlobalAlloc, Layout, System};
use std::backtrace::Backtrace;
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Mutex;

const RATE: isize = 256 * 1024;

pub struct Sampler;

struct Sample {
    bytes: usize,
    trace: Backtrace,
}

static LIVE: Mutex<Option<HashMap<usize, Sample>>> = Mutex::new(None);

thread_local! {
    static COUNTDOWN: Cell<isize> = const { Cell::new(RATE) };
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

fn enter() -> bool {
    BUSY.try_with(|b| !b.replace(true)).unwrap_or(false)
}

fn leave() {
    let _ = BUSY.try_with(|b| b.set(false));
}

fn record(ptr: *mut u8, size: usize) {
    if ptr.is_null() || !enter() {
        return;
    }
    let weight = COUNTDOWN.with(|c| {
        let left = c.get() - size as isize;
        if left > 0 {
            c.set(left);
            return 0;
        }
        c.set(RATE);
        size.max(RATE as usize)
    });
    if weight > 0 {
        let trace = Backtrace::force_capture();
        let mut live = LIVE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        live.get_or_insert_with(HashMap::new).insert(
            ptr as usize,
            Sample {
                bytes: weight,
                trace,
            },
        );
    }
    leave();
}

fn forget(ptr: *mut u8) {
    if !enter() {
        return;
    }
    let mut live = LIVE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(map) = live.as_mut() {
        map.remove(&(ptr as usize));
    }
    drop(live);
    leave();
}

// SAFETY: every call forwards to `System` with the caller's arguments; the bookkeeping never
// touches the allocation itself.
unsafe impl GlobalAlloc for Sampler {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        let p = unsafe { System.alloc(layout) };
        record(p, layout.size());
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        let p = unsafe { System.alloc_zeroed(layout) };
        record(p, layout.size());
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        forget(ptr);
        // SAFETY: forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        forget(ptr);
        // SAFETY: forwarded unchanged.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        record(p, new_size);
        p
    }
}

/// Starts the dump thread when `MCR_HEAP_PROFILE` asks for one.
pub fn start() {
    let Ok(spec) = std::env::var("MCR_HEAP_PROFILE") else {
        return;
    };
    let Some((secs, path)) = spec.split_once(':') else {
        return;
    };
    let secs: u64 = secs.parse().unwrap_or(60);
    let path = path.to_owned();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(secs));
        let rss = || {
            std::fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.starts_with("VmRSS:"))
                        .map(|l| l.to_owned())
                })
                .unwrap_or_default()
        };
        // SAFETY: plain glibc introspection calls.
        let info = unsafe { libc::mallinfo2() };
        eprintln!(
            "heap profile: mallinfo2 arena {} MB, in use {} MB, free {} MB, mmapped {} MB, {}",
            info.arena >> 20,
            info.uordblks >> 20,
            info.fordblks >> 20,
            info.hblkhd >> 20,
            rss()
        );
        // SAFETY: as above.
        unsafe { libc::malloc_stats() };
        let t = std::time::Instant::now();
        // SAFETY: as above.
        unsafe { libc::malloc_trim(0) };
        eprintln!(
            "heap profile: after malloc_trim ({:?}) {}",
            t.elapsed(),
            rss()
        );
        if let Err(e) = std::fs::write(&path, dump()) {
            eprintln!("heap profile: {e}");
        } else {
            eprintln!("heap profile: wrote {path}");
        }
    });
}

fn frames(trace: &Backtrace) -> Vec<String> {
    let text = trace.to_string();
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(at) = line.strip_prefix("at ") {
            let short = at
                .rsplit_once("/crates/")
                .map(|(_, p)| p.to_owned())
                .or_else(|| at.rsplit_once("/src/").map(|(_, p)| format!("src/{p}")))
                .unwrap_or_else(|| at.to_owned());
            if let Some(last) = out.last_mut() {
                last.push_str(" @ ");
                last.push_str(&short);
            }
            continue;
        }
        let Some((_, name)) = line.split_once(": ") else {
            continue;
        };
        let name = name.trim();
        let short = name
            .rsplit_once("::h")
            .filter(|(_, h)| h.len() == 16)
            .map_or(name, |(a, _)| a);
        out.push(short.to_owned());
    }
    out.retain(|f| {
        !(f.contains("heap_profile")
            || f.starts_with("std::backtrace")
            || f.starts_with("__rust")
            || f.starts_with("std::sys::backtrace")
            || f.contains("/alloc/src/")
            || f.contains("/core/src/")
            || f.contains("hashbrown")
            || f.contains("rustc/")
            || f.starts_with("record ")
            || f.starts_with("alloc"))
    });
    out
}

fn dump() -> String {
    let _guard = enter();
    let samples: Vec<(usize, String)> = {
        let live = LIVE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        live.as_ref()
            .map(|m| {
                m.values()
                    .map(|s| (s.bytes, frames(&s.trace).join(" < ")))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut by_stack: HashMap<String, usize> = HashMap::new();
    let mut by_top: HashMap<String, usize> = HashMap::new();
    let total: usize = samples.iter().map(|s| s.0).sum();
    for (bytes, stack) in samples {
        let interesting: Vec<&str> = stack.split(" < ").collect();
        let top = interesting
            .iter()
            .take(3)
            .copied()
            .collect::<Vec<_>>()
            .join(" < ");
        *by_top.entry(top).or_default() += bytes;
        *by_stack
            .entry(
                interesting
                    .iter()
                    .take(16)
                    .copied()
                    .collect::<Vec<_>>()
                    .join("\n    < "),
            )
            .or_default() += bytes;
    }
    let mut out = format!(
        "live sampled {:.1} MB\n\n== by top 3 frames\n",
        total as f64 / 1e6
    );
    let mut tops: Vec<_> = by_top.into_iter().collect();
    tops.sort_by_key(|a| std::cmp::Reverse(a.1));
    for (k, v) in tops.iter().take(60) {
        out.push_str(&format!("{:8.1} MB  {k}\n", *v as f64 / 1e6));
    }
    out.push_str("\n== by stack\n");
    let mut stacks: Vec<_> = by_stack.into_iter().collect();
    stacks.sort_by_key(|a| std::cmp::Reverse(a.1));
    for (k, v) in stacks.iter().take(80) {
        out.push_str(&format!("{:8.1} MB  {k}\n\n", *v as f64 / 1e6));
    }
    leave();
    out
}
