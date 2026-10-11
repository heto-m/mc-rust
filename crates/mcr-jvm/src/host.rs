use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Mutex, PoisonError};

use crate::upcall::DowncallScope;

/// Embedder hook behind the `host_call(op, arg)` downcall. It runs on the calling Java thread
/// (normally `mcr-java`) inside a downcall, so it may make upcalls.
pub type HostCallFn = Box<dyn FnMut(i32, i64) -> i64 + Send>;

/// Returned when no handler is installed or the handler is already running (re-entrant call).
pub const HOST_CALL_UNAVAILABLE: i64 = i64::MIN;
pub const HOST_CALL_PANICKED: i64 = i64::MIN + 1;

static HANDLER: Mutex<Option<HostCallFn>> = Mutex::new(None);

pub(crate) fn install(handler: Option<HostCallFn>) {
    *HANDLER.lock().unwrap_or_else(PoisonError::into_inner) = handler;
}

pub(crate) extern "C" fn host_call(op: i32, arg: i64) -> i64 {
    let _scope = DowncallScope::enter();
    let taken = HANDLER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    let Some(mut handler) = taken else {
        return HOST_CALL_UNAVAILABLE;
    };
    let result = catch_unwind(AssertUnwindSafe(|| handler(op, arg)));
    let mut slot = HANDLER.lock().unwrap_or_else(PoisonError::into_inner);
    if slot.is_none() {
        *slot = Some(handler);
    }
    result.unwrap_or(HOST_CALL_PANICKED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_reports_panics_and_refuses_reentry() {
        install(Some(Box::new(|op, arg| match op {
            1 => arg * 2,
            2 => panic!("handler panic"),
            3 => host_call(1, arg),
            _ => -1,
        })));
        assert_eq!(host_call(1, 21), 42);
        assert_eq!(host_call(2, 0), HOST_CALL_PANICKED);
        assert_eq!(host_call(3, 5), HOST_CALL_UNAVAILABLE);
        assert_eq!(host_call(1, 4), 8);
        install(None);
        assert_eq!(host_call(1, 4), HOST_CALL_UNAVAILABLE);
    }
}
