//! Clocks and sleeping for the portable core.
//!
//! On every OS target this is exactly `std::time` and `std::thread::sleep`.
//! On `wasm32-unknown-unknown` (the browser playground) the standard
//! library has no clock: `Instant::now()` and `SystemTime::now()` panic and
//! `thread::sleep` is unsupported. There the same names come from
//! `web-time` (backed by `performance.now()` / `Date.now()`), and [`sleep`]
//! spins on the monotonic clock; the playground runs programs in a Web
//! Worker, so a spin never blocks the page.
//!
//! Invariant: code compiled into the core (interpreter, VM, pure stdlib)
//! reads the clock and sleeps only through this module. Host-only modules
//! (server, HTTP client, databases, CLI) may keep using `std::time`.

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use web_time::{Instant, SystemTime, UNIX_EPOCH};

use std::time::Duration;

/// Block the current thread for `duration`.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[inline]
pub fn sleep(duration: Duration) {
    std::thread::sleep(duration);
}

/// Block the current thread for `duration` (busy-wait: the browser has no
/// blocking sleep outside `Atomics.wait` on shared memory).
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub fn sleep(duration: Duration) {
    let start = Instant::now();
    while start.elapsed() < duration {
        std::hint::spin_loop();
    }
}

/// `rx.recv_timeout(timeout)`. On `wasm32-unknown-unknown` (one thread, no
/// std clock) nothing can arrive while we wait, so it is a non-blocking
/// poll that reports `Timeout` when the channel is empty (cancellable waits
/// then stop, see [`HAS_THREADS`]).
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[inline]
pub fn recv_timeout<T>(
    rx: &std::sync::mpsc::Receiver<T>,
    timeout: Duration,
) -> Result<T, std::sync::mpsc::RecvTimeoutError> {
    rx.recv_timeout(timeout)
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub fn recv_timeout<T>(
    rx: &std::sync::mpsc::Receiver<T>,
    _timeout: Duration,
) -> Result<T, std::sync::mpsc::RecvTimeoutError> {
    use std::sync::mpsc::{RecvTimeoutError, TryRecvError};
    rx.try_recv().map_err(|e| match e {
        TryRecvError::Empty => RecvTimeoutError::Timeout,
        TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
    })
}

/// Whether another thread can ever complete a blocking wait. False on
/// `wasm32-unknown-unknown` (one thread): a wait that is not ready on its
/// first poll would wait forever, so cancellable waits fail with
/// [`WAITS_FOREVER`] instead of spinning.
pub const HAS_THREADS: bool = !cfg!(all(target_arch = "wasm32", target_os = "unknown"));

/// Error text for a wait that can never complete (see [`HAS_THREADS`]).
pub const WAITS_FOREVER: &str =
    "this would wait forever: there are no other tasks to wake it in the browser playground";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_waits_at_least_the_duration() {
        let start = Instant::now();
        sleep(Duration::from_millis(5));
        assert!(start.elapsed() >= Duration::from_millis(5));
    }

    #[test]
    fn system_time_is_after_the_epoch() {
        assert!(SystemTime::now().duration_since(UNIX_EPOCH).is_ok());
    }
}
