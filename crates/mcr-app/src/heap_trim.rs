//! Hands the C heap's free pages back to the system once a loading burst is over: glibc keeps
//! freed memory in its per-thread arenas, which after Java's resource load and the join's chunk
//! meshing held about 600 MB in Medieval Immersed.

use std::time::{Duration, Instant};

/// Activity must have stopped this long before a trim.
const QUIET: Duration = Duration::from_secs(2);
/// At most one trim this often.
const MIN_GAP: Duration = Duration::from_secs(20);

#[derive(Debug, Default)]
pub struct HeapTrim {
    activity: u64,
    quiet_since: Option<Instant>,
    pending: bool,
    last: Option<Instant>,
}

impl HeapTrim {
    /// A milestone that freed a lot (a resource load, Java's title): trims now, unless a trim
    /// just ran.
    pub fn milestone(&mut self, now: Instant) -> bool {
        if self.last.is_some_and(|t| now.duration_since(t) < QUIET) {
            self.request(now);
            return false;
        }
        self.last = Some(now);
        trim();
        true
    }

    fn request(&mut self, now: Instant) {
        self.pending = true;
        self.quiet_since = Some(now);
    }

    /// `activity` is any counter that moves while loading (sections meshed); trims once it stood
    /// still for [`QUIET`] after moving.
    pub fn poll(&mut self, activity: u64, now: Instant) -> bool {
        if activity != self.activity {
            self.activity = activity;
            self.request(now);
            return false;
        }
        let quiet = self
            .quiet_since
            .is_some_and(|t| now.duration_since(t) >= QUIET);
        let spaced = self.last.is_none_or(|t| now.duration_since(t) >= MIN_GAP);
        if !(self.pending && quiet && spaced) {
            return false;
        }
        self.pending = false;
        self.last = Some(now);
        trim();
        true
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn trim() {
    let _ = std::thread::Builder::new()
        .name("mcr-heap-trim".into())
        .spawn(|| {
            // SAFETY: malloc_trim only releases free memory; it is safe from any thread.
            unsafe { libc::malloc_trim(0) };
        });
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn trim() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_once_activity_stops_and_spaces_trims() {
        let t0 = Instant::now();
        let mut h = HeapTrim::default();
        assert!(!h.poll(0, t0), "nothing happened yet");
        assert!(!h.poll(5, t0));
        assert!(!h.poll(5, t0 + Duration::from_secs(1)));
        assert!(h.poll(5, t0 + Duration::from_secs(3)));
        assert!(
            !h.poll(5, t0 + Duration::from_secs(4)),
            "one trim per burst"
        );
        assert!(!h.poll(9, t0 + Duration::from_secs(5)));
        assert!(
            !h.poll(9, t0 + Duration::from_secs(8)),
            "too soon after the last"
        );
        assert!(h.poll(9, t0 + Duration::from_secs(24)));
    }

    #[test]
    fn a_milestone_trims_at_once_unless_one_just_ran() {
        let t0 = Instant::now();
        let mut h = HeapTrim::default();
        assert!(h.milestone(t0));
        assert!(!h.milestone(t0 + Duration::from_secs(1)));
        assert!(!h.poll(0, t0 + Duration::from_secs(2)));
        assert!(h.poll(0, t0 + Duration::from_secs(21)));
    }
}
