use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::table::UPCALLS;

pub const STATUS_OK: i32 = 0;
pub const REGISTER_COUNT_MISMATCH: i32 = -2;
pub const REGISTER_TWICE: i32 = -3;
pub const REGISTER_WRONG_THREAD: i32 = -4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UpcallError {
    #[error("upcalls may only run on the mcr-java thread inside a downcall")]
    WrongContext,
    #[error("upcall {0} is not registered")]
    NotRegistered(&'static str),
    #[error("upcall {name} failed with status {status}")]
    Failed { name: &'static str, status: i32 },
}

thread_local! {
    static JAVA_THREAD: Cell<bool> = const { Cell::new(false) };
    static DOWNCALL_DEPTH: Cell<u32> = const { Cell::new(0) };
}

const UPCALL_COUNT: usize = UPCALLS.len();
static STUBS: [AtomicUsize; UPCALL_COUNT] = [const { AtomicUsize::new(0) }; UPCALL_COUNT];
static REGISTERED: AtomicBool = AtomicBool::new(false);

pub(crate) fn mark_java_thread() {
    JAVA_THREAD.with(|t| t.set(true));
}

pub fn on_java_thread() -> bool {
    JAVA_THREAD.with(Cell::get)
}

#[derive(Debug)]
pub(crate) struct DowncallScope(());

impl DowncallScope {
    pub(crate) fn enter() -> Self {
        DOWNCALL_DEPTH.with(|d| d.set(d.get() + 1));
        Self(())
    }
}

impl Drop for DowncallScope {
    fn drop(&mut self) {
        DOWNCALL_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

fn in_upcall_context() -> bool {
    on_java_thread() && DOWNCALL_DEPTH.with(Cell::get) > 0
}

pub(crate) extern "C" fn register_upcalls(addr: i64, count: i32) -> i32 {
    if !on_java_thread() {
        return REGISTER_WRONG_THREAD;
    }
    if usize::try_from(count) != Ok(UPCALL_COUNT) || addr == 0 {
        return REGISTER_COUNT_MISMATCH;
    }
    if REGISTERED.swap(true, Ordering::AcqRel) {
        return REGISTER_TWICE;
    }
    let base = addr as usize as *const u64;
    for (i, slot) in STUBS.iter().enumerate() {
        // SAFETY: Java passes a global-arena segment holding `count` longs, and count was checked.
        let stub = unsafe { base.add(i).read() };
        slot.store(stub as usize, Ordering::Release);
    }
    STATUS_OK
}

fn stub(index: usize) -> Result<usize, UpcallError> {
    if !in_upcall_context() {
        return Err(UpcallError::WrongContext);
    }
    match STUBS[index].load(Ordering::Acquire) {
        0 => Err(UpcallError::NotRegistered(UPCALLS[index].name)),
        p => Ok(p),
    }
}

fn status(index: usize, status: i32) -> Result<(), UpcallError> {
    if status == STATUS_OK {
        Ok(())
    } else {
        Err(UpcallError::Failed {
            name: UPCALLS[index].name,
            status,
        })
    }
}

pub fn selftest_ping() -> Result<(), UpcallError> {
    const I: usize = 0;
    let p = stub(I)?;
    // SAFETY: Java registered an upcall stub for descriptor ()I at this index, and the context check
    // guarantees we are on the attached mcr-java thread inside a downcall.
    let f: extern "C" fn() -> i32 = unsafe { std::mem::transmute(p) };
    status(I, f())
}

pub fn selftest_add(addr: i64, delta: i64) -> Result<(), UpcallError> {
    const I: usize = 1;
    let p = stub(I)?;
    // SAFETY: as in `selftest_ping`, for descriptor (JJ)I.
    let f: extern "C" fn(i64, i64) -> i32 = unsafe { std::mem::transmute(p) };
    status(I, f(addr, delta))
}

pub fn selftest_throw() -> Result<(), UpcallError> {
    const I: usize = 2;
    let p = stub(I)?;
    // SAFETY: as in `selftest_ping`, for descriptor ()I.
    let f: extern "C" fn() -> i32 = unsafe { std::mem::transmute(p) };
    status(I, f())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_match_the_upcall_list() {
        assert_eq!(UPCALLS[0].name, "selftest_ping");
        assert_eq!(UPCALLS[1].name, "selftest_add");
        assert_eq!(UPCALLS[2].name, "selftest_throw");
    }

    #[test]
    fn upcalls_outside_a_downcall_are_refused() {
        assert_eq!(selftest_ping(), Err(UpcallError::WrongContext));
        let _scope = DowncallScope::enter();
        assert_eq!(selftest_ping(), Err(UpcallError::WrongContext));
    }

    #[test]
    fn registration_off_the_java_thread_is_refused() {
        let stubs = [1u64; UPCALL_COUNT];
        let addr = stubs.as_ptr() as i64;
        assert_eq!(
            register_upcalls(addr, UPCALL_COUNT as i32),
            REGISTER_WRONG_THREAD
        );
    }

    #[test]
    fn unregistered_upcall_reports_its_name() {
        std::thread::spawn(|| {
            mark_java_thread();
            let _scope = DowncallScope::enter();
            assert_eq!(
                selftest_ping(),
                Err(UpcallError::NotRegistered("selftest_ping"))
            );
            assert_eq!(register_upcalls(8, 99), REGISTER_COUNT_MISMATCH);
        })
        .join()
        .unwrap();
    }
}
