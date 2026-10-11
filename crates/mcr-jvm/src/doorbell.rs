use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Shared layout: the `u32` sequence word sits at offset 0 so Java can read it directly.
#[repr(C, align(64))]
#[derive(Debug)]
pub(crate) struct Bell {
    seq: AtomicU32,
    waiters: AtomicU32,
    #[cfg(not(target_os = "linux"))]
    lock: std::sync::Mutex<()>,
    #[cfg(not(target_os = "linux"))]
    cond: std::sync::Condvar,
}

/// A wake-up word: consumers read [`seq`](Doorbell::seq), re-check their condition, then
/// [`wait`](Doorbell::wait) on the value they saw; producers [`notify`](Doorbell::notify).
#[derive(Debug)]
pub struct Doorbell {
    bell: Box<Bell>,
}

impl Default for Doorbell {
    fn default() -> Self {
        Self::new()
    }
}

impl Doorbell {
    pub fn new() -> Self {
        Self {
            bell: Box::new(Bell {
                seq: AtomicU32::new(0),
                waiters: AtomicU32::new(0),
                #[cfg(not(target_os = "linux"))]
                lock: std::sync::Mutex::new(()),
                #[cfg(not(target_os = "linux"))]
                cond: std::sync::Condvar::new(),
            }),
        }
    }

    /// Address handed to Java; valid for as long as this doorbell lives.
    pub fn addr(&self) -> i64 {
        &raw const *self.bell as i64
    }

    pub fn seq(&self) -> u32 {
        self.bell.seq.load(Ordering::Acquire)
    }

    pub fn notify(&self) {
        self.bell.notify();
    }

    /// Returns `true` once the sequence differs from `seen`, `false` on timeout.
    pub fn wait(&self, seen: u32, timeout: Option<Duration>) -> bool {
        self.bell.wait(seen, timeout)
    }
}

impl Bell {
    fn notify(&self) {
        self.seq.fetch_add(1, Ordering::SeqCst);
        if self.waiters.load(Ordering::SeqCst) > 0 {
            self.wake_all();
        }
    }

    fn wait(&self, seen: u32, timeout: Option<Duration>) -> bool {
        let deadline = timeout.map(|t| Instant::now() + t);
        self.waiters.fetch_add(1, Ordering::SeqCst);
        let changed = loop {
            if self.seq.load(Ordering::SeqCst) != seen {
                break true;
            }
            let remaining = match deadline {
                Some(d) => match d.checked_duration_since(Instant::now()) {
                    Some(r) if !r.is_zero() => Some(r),
                    _ => break false,
                },
                None => None,
            };
            self.block(seen, remaining);
        };
        self.waiters.fetch_sub(1, Ordering::SeqCst);
        changed
    }

    #[cfg(target_os = "linux")]
    fn block(&self, seen: u32, timeout: Option<Duration>) {
        let ts = timeout.map(|t| libc::timespec {
            tv_sec: t.as_secs().min(i64::MAX as u64) as libc::time_t,
            tv_nsec: t.subsec_nanos() as libc::c_long,
        });
        let ts_ptr = ts
            .as_ref()
            .map_or(std::ptr::null(), |t| t as *const libc::timespec);
        // SAFETY: FUTEX_WAIT on a live, aligned u32; the kernel only compares and sleeps.
        unsafe {
            libc::syscall(
                libc::SYS_futex,
                self.seq.as_ptr(),
                libc::FUTEX_WAIT | libc::FUTEX_PRIVATE_FLAG,
                seen,
                ts_ptr,
            );
        }
    }

    #[cfg(target_os = "linux")]
    fn wake_all(&self) {
        // SAFETY: FUTEX_WAKE on a live, aligned u32.
        unsafe {
            libc::syscall(
                libc::SYS_futex,
                self.seq.as_ptr(),
                libc::FUTEX_WAKE | libc::FUTEX_PRIVATE_FLAG,
                i32::MAX,
            );
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn block(&self, seen: u32, timeout: Option<Duration>) {
        let guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.seq.load(Ordering::SeqCst) != seen {
            return;
        }
        let _guard = match timeout {
            Some(t) => {
                self.cond
                    .wait_timeout(guard, t)
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .0
            }
            None => self
                .cond
                .wait(guard)
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        };
    }

    #[cfg(not(target_os = "linux"))]
    fn wake_all(&self) {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.cond.notify_all();
    }
}

/// # Safety
/// `addr` must come from [`Doorbell::addr`] of a doorbell that is still alive.
unsafe fn bell_at<'a>(addr: i64) -> &'a Bell {
    // SAFETY: guaranteed by the caller.
    unsafe { &*(addr as usize as *const Bell) }
}

pub(crate) extern "C" fn doorbell_wait(addr: i64, seen: i32, timeout_nanos: i64) -> i32 {
    let timeout = u64::try_from(timeout_nanos).ok().map(Duration::from_nanos);
    // SAFETY: Java only passes doorbell addresses Rust handed out, which outlive the VM's use of them.
    let bell = unsafe { bell_at(addr) };
    i32::from(bell.wait(seen as u32, timeout))
}

pub(crate) extern "C" fn doorbell_notify(addr: i64) {
    // SAFETY: as in `doorbell_wait`.
    unsafe { bell_at(addr) }.notify();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn wait_returns_immediately_on_stale_seq() {
        let bell = Doorbell::new();
        let seen = bell.seq();
        bell.notify();
        assert!(bell.wait(seen, Some(Duration::from_secs(5))));
    }

    #[test]
    fn wait_times_out() {
        let bell = Doorbell::new();
        let start = Instant::now();
        assert!(!bell.wait(bell.seq(), Some(Duration::from_millis(20))));
        assert!(start.elapsed() >= Duration::from_millis(20));
    }

    #[test]
    fn notify_wakes_a_blocked_waiter() {
        let bell = Arc::new(Doorbell::new());
        let woke = Arc::new(AtomicBool::new(false));
        let seen = bell.seq();
        let t = {
            let (bell, woke) = (bell.clone(), woke.clone());
            std::thread::spawn(move || {
                assert!(bell.wait(seen, Some(Duration::from_secs(10))));
                woke.store(true, Ordering::SeqCst);
            })
        };
        std::thread::sleep(Duration::from_millis(20));
        bell.notify();
        t.join().unwrap();
        assert!(woke.load(Ordering::SeqCst));
    }

    #[test]
    fn ffi_entry_points_match_methods() {
        let bell = Doorbell::new();
        let seen = bell.seq() as i32;
        doorbell_notify(bell.addr());
        assert_eq!(doorbell_wait(bell.addr(), seen, 0), 1);
        assert_eq!(doorbell_wait(bell.addr(), seen + 1, 1_000), 0);
        assert_eq!(bell.addr() % 64, 0);
    }
}
