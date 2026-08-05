//! Signal handling for graceful shutdown.
//!
//! Mirrors `server/cpp/src/signal_handler.hpp`.

use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag set by SIGTERM/SIGINT handlers.
pub static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Global flag set by SIGUSR1 handler (self-depart).
pub static SELF_DEPART_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Install signal handlers for SIGTERM, SIGINT, and SIGUSR1.
///
/// SIGTERM/SIGINT set `SHUTDOWN_REQUESTED`.
/// SIGUSR1 sets `SELF_DEPART_REQUESTED`.
#[cfg(unix)]
pub fn install_shutdown_handler() {
    unsafe {
        libc::signal(libc::SIGTERM, signal_handler as libc::sighandler_t);
        libc::signal(libc::SIGINT, signal_handler as libc::sighandler_t);
        libc::signal(libc::SIGUSR1, depart_handler as libc::sighandler_t);
    }
}

#[cfg(unix)]
extern "C" fn signal_handler(_: libc::c_int) {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

#[cfg(unix)]
extern "C" fn depart_handler(_: libc::c_int) {
    SELF_DEPART_REQUESTED.store(true, Ordering::SeqCst);
}

/// Check if shutdown has been requested.
pub fn shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

/// Check if self-depart has been requested.
pub fn self_depart_requested() -> bool {
    SELF_DEPART_REQUESTED.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_helpers_work() {
        // Reset to known state (globals persist across tests).
        SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
        SELF_DEPART_REQUESTED.store(false, Ordering::SeqCst);

        assert!(!shutdown_requested());
        assert!(!self_depart_requested());

        SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
        assert!(shutdown_requested());

        SELF_DEPART_REQUESTED.store(true, Ordering::SeqCst);
        assert!(self_depart_requested());

        // Reset for other tests.
        SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
        SELF_DEPART_REQUESTED.store(false, Ordering::SeqCst);
    }
}
