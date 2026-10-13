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

/// Longest single sleep of [`sleep_interruptible`]: how late a cancellation
/// or `timeout` deadline can be noticed by a sleeping program.
pub const SLEEP_SLICE: Duration = Duration::from_millis(50);

/// Sleep for `duration` (the `wait` builtin on both engines), calling
/// `interrupted` before every slice of at most [`SLEEP_SLICE`] and once at
/// the end, so cancellation and deadlines stop the sleep early.
///
/// The slices are measured against one deadline on the monotonic clock, not
/// added up: an OS whose timers oversleep (macOS CI wakes a short sleep
/// tens of milliseconds late) makes the whole `wait` late by at most one
/// oversleep, not one per slice. Counting slices made a `wait(0.1)` cost
/// two oversleeps on the VM (50 ms slices) against one on the interpreter
/// (100 ms slices), which made a channel pipeline on the VM as slow as a
/// serial one on macOS (eval/V2_FINDINGS.md, finding 27).
pub fn sleep_interruptible<E>(
    duration: Duration,
    mut interrupted: impl FnMut() -> Result<(), E>,
) -> Result<(), E> {
    // `None`: too far in the future to represent, i.e. forever.
    let deadline = Instant::now().checked_add(duration);
    loop {
        interrupted()?;
        let slice = match deadline {
            Some(deadline) => {
                let now = Instant::now();
                if now >= deadline {
                    return Ok(());
                }
                (deadline - now).min(SLEEP_SLICE)
            }
            None => SLEEP_SLICE,
        };
        sleep(slice);
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
    fn sleep_interruptible_sleeps_the_duration_and_stops_when_interrupted() {
        let start = Instant::now();
        let mut polls = 0;
        let done: Result<(), ()> = sleep_interruptible(Duration::from_millis(120), || {
            polls += 1;
            Ok(())
        });
        assert!(done.is_ok());
        assert!(start.elapsed() >= Duration::from_millis(120));
        // At least one poll per slice, plus the final one.
        assert!(polls >= 3, "{}", polls);

        let start = Instant::now();
        let mut polls = 0;
        let stopped = sleep_interruptible(Duration::from_secs(60), || {
            polls += 1;
            if polls == 2 {
                Err("cancelled")
            } else {
                Ok(())
            }
        });
        assert_eq!(stopped, Err("cancelled"));
        assert!(start.elapsed() < Duration::from_secs(5));

        // A duration too large for the clock sleeps "forever" (until
        // interrupted) instead of overflowing.
        let mut polls = 0;
        let stopped = sleep_interruptible(Duration::MAX, || {
            polls += 1;
            if polls == 2 {
                Err(())
            } else {
                Ok(())
            }
        });
        assert_eq!(stopped, Err(()));
    }

    #[test]
    fn system_time_is_after_the_epoch() {
        assert!(SystemTime::now().duration_since(UNIX_EPOCH).is_ok());
    }
}
