//! Recursion limits shared by every execution engine.
//!
//! # Invariants
//!
//! * Deep recursion in Forge code must surface as a catchable runtime error,
//!   never as a native stack overflow (which aborts the whole process).
//! * Every engine (interpreter, bytecode VM) checks the same two limits on
//!   each Forge-level call and reports them with [`depth_exceeded_message`],
//!   so the error text is identical everywhere:
//!   1. a configurable **call-depth limit** ([`max_depth`]), default
//!      [`DEFAULT_MAX_DEPTH`], overridable with `FORGE_MAX_DEPTH=<n>` or the
//!      `--max-depth <n>` CLI flag;
//!   2. a **native stack guard** ([`native_stack_exhausted`]) that trips when
//!      the current thread is within a red zone of its stack end, whatever
//!      the depth. This is the backstop for threads with small stacks
//!      (spawned tasks, server handlers) and for frames that use more stack
//!      than expected.
//! * The CLI runs programs on a thread with [`MAIN_STACK_SIZE`] bytes of stack
//!   (registered via [`register_thread_stack`]) so the default depth limit is
//!   reachable for ordinary recursion. Runtime worker/blocking threads
//!   ([`configure_runtime`]) and interpreter task threads ([`spawn_worker`])
//!   get [`WORKER_STACK_SIZE`]. Threads that never register are assumed to
//!   have [`ASSUMED_STACK_SIZE`] (Rust's and tokio's default).

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Default maximum Forge call depth.
pub const DEFAULT_MAX_DEPTH: usize = 10_000;

/// Stack size for the thread the CLI runs programs on. Only touched pages are
/// committed, so a large reservation is cheap.
pub const MAIN_STACK_SIZE: usize = 1024 * 1024 * 1024;

/// Stack size for threads that run Forge code besides the CLI main thread:
/// the tokio runtime's worker and blocking-pool threads (HTTP handlers run
/// on the blocking pool) and interpreter `spawn`/`timeout` threads. Like
/// [`MAIN_STACK_SIZE`] it is only reserved address space; pages are
/// committed as recursion actually touches them.
pub const WORKER_STACK_SIZE: usize = 256 * 1024 * 1024;

/// Stack size assumed for threads that did not call [`register_thread_stack`].
pub const ASSUMED_STACK_SIZE: usize = 2 * 1024 * 1024;

/// Upper bound on the red zone kept free below the guard.
const MAX_RED_ZONE: usize = 1024 * 1024;

/// 0 = not yet resolved from the environment.
static MAX_DEPTH: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Lowest stack address (exclusive of the red zone) this thread may use
    /// before the guard trips. 0 = not initialised.
    static STACK_LIMIT: Cell<usize> = const { Cell::new(0) };
}

/// The active call-depth limit.
pub fn max_depth() -> usize {
    let cur = MAX_DEPTH.load(Ordering::Relaxed);
    if cur != 0 {
        return cur;
    }
    let resolved = std::env::var("FORGE_MAX_DEPTH")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_MAX_DEPTH);
    MAX_DEPTH.store(resolved, Ordering::Relaxed);
    resolved
}

/// Override the call-depth limit (e.g. from `--max-depth`). Zero is ignored.
pub fn set_max_depth(n: usize) {
    if n > 0 {
        MAX_DEPTH.store(n, Ordering::Relaxed);
    }
}

#[inline(never)]
fn approx_stack_pointer() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

fn red_zone(size: usize) -> usize {
    (size / 4).min(MAX_RED_ZONE)
}

/// Record the current thread's stack size. Call this first thing on a thread
/// you spawned with an explicit `stack_size`.
pub fn register_thread_stack(size: usize) {
    let sp = approx_stack_pointer();
    let limit = sp.saturating_sub(size.saturating_sub(red_zone(size)));
    STACK_LIMIT.with(|l| l.set(limit));
}

/// Give every thread of a tokio runtime [`WORKER_STACK_SIZE`] bytes of
/// stack and register it with the guard. This covers the blocking pool,
/// so HTTP handlers (run via `spawn_blocking`) get the same recursion
/// headroom as other Forge code instead of tokio's 2 MiB default.
#[cfg(feature = "host")]
pub fn configure_runtime(builder: &mut tokio::runtime::Builder) -> &mut tokio::runtime::Builder {
    builder
        .thread_stack_size(WORKER_STACK_SIZE)
        .on_thread_start(|| register_thread_stack(WORKER_STACK_SIZE))
}

/// Spawn a thread for running Forge code with [`WORKER_STACK_SIZE`] bytes
/// of registered stack, carrying the current permission policy (see
/// [`crate::permissions::inherit`]).
pub fn spawn_worker<F, T>(f: F) -> std::io::Result<std::thread::JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let f = crate::permissions::inherit(f);
    std::thread::Builder::new()
        .stack_size(WORKER_STACK_SIZE)
        .spawn(move || {
            register_thread_stack(WORKER_STACK_SIZE);
            f()
        })
}

/// True when the current thread is too close to the end of its stack to
/// safely run another Forge call.
#[inline]
pub fn native_stack_exhausted() -> bool {
    let sp = approx_stack_pointer();
    STACK_LIMIT.with(|l| {
        let mut limit = l.get();
        if limit == 0 {
            // Unregistered thread: assume the default size, measured from
            // the first check (which happens near the top of the stack).
            limit = sp.saturating_sub(ASSUMED_STACK_SIZE - red_zone(ASSUMED_STACK_SIZE));
            l.set(limit);
        }
        sp < limit
    })
}

/// The single error message every engine uses for runaway recursion.
pub fn depth_exceeded_message(depth: usize) -> String {
    format!(
        "maximum recursion depth exceeded (depth {}, limit {})\n  hint: check for infinite recursion or restructure to use iteration; the limit can be raised with FORGE_MAX_DEPTH or --max-depth",
        depth,
        max_depth()
    )
}

// ---------------------------------------------------------------------------
// Value depth
// ---------------------------------------------------------------------------

/// Deepest value nesting that recursive value walkers (VM↔interpreter
/// conversions, display, equality, JSON) will descend into. A script can
/// build a value millions of levels deep in a loop (`a = [a]`); walking it
/// recursively would overflow the native stack and abort the process.
pub const MAX_VALUE_DEPTH: usize = 10_000;

thread_local! {
    static VALUE_DEPTH: Cell<usize> = const { Cell::new(0) };
    static VALUE_TOO_DEEP: Cell<bool> = const { Cell::new(false) };
}

/// One level of a recursive value walk; leaving the level on drop.
#[must_use = "the level is only entered while the guard lives"]
pub struct ValueLevel(());

impl Drop for ValueLevel {
    fn drop(&mut self) {
        VALUE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Enter one more level of nesting in a recursive value walker. `None`
/// means the value is too deep (more than [`MAX_VALUE_DEPTH`] levels, or the
/// native stack is nearly exhausted): the walker must stop descending and
/// produce a placeholder; the condition is recorded for
/// [`take_value_too_deep`], which fallible callers turn into an error.
#[inline]
pub fn enter_value_level() -> Option<ValueLevel> {
    let depth = VALUE_DEPTH.with(|d| {
        let n = d.get() + 1;
        d.set(n);
        n
    });
    if depth > MAX_VALUE_DEPTH || native_stack_exhausted() {
        VALUE_DEPTH.with(|d| d.set(d.get() - 1));
        VALUE_TOO_DEEP.with(|f| f.set(true));
        None
    } else {
        Some(ValueLevel(()))
    }
}

/// Clear and return whether a walker on this thread hit the depth limit.
pub fn take_value_too_deep() -> bool {
    VALUE_TOO_DEEP.with(|f| f.replace(false))
}

/// The error for a value nested too deeply to process.
pub fn value_too_deep_message() -> String {
    format!(
        "value nested too deeply (more than {} levels)",
        MAX_VALUE_DEPTH
    )
}

/// Check both limits for a call that would bring the stack to `depth`.
/// Returns the error message to raise, if any.
#[inline]
pub fn check_call_depth(depth: usize) -> Result<(), String> {
    if depth > max_depth() || native_stack_exhausted() {
        Err(depth_exceeded_message(depth))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_has_stable_prefix() {
        assert!(depth_exceeded_message(5).starts_with("maximum recursion depth exceeded"));
    }

    #[test]
    fn shallow_depth_passes() {
        assert!(check_call_depth(1).is_ok());
    }

    #[test]
    fn guard_trips_before_stack_end() {
        fn recurse(n: usize) -> usize {
            if native_stack_exhausted() {
                return n;
            }
            let pad = [0u8; 4096];
            std::hint::black_box(&pad);
            recurse(n + 1) + 1
        }
        let handle = std::thread::Builder::new()
            .stack_size(4 * 1024 * 1024)
            .spawn(|| {
                register_thread_stack(4 * 1024 * 1024);
                recurse(0)
            })
            .expect("spawn test thread");
        let depth = handle
            .join()
            .expect("guard must stop recursion before overflow");
        assert!(depth > 100, "guard tripped too early: {}", depth);
    }
}
