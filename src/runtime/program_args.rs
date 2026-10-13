//! The program's own command line: what `io.args()` returns.
//!
//! `forge run main.fg a b` gives the program `["main.fg", "a", "b"]` (the
//! script path as given, then everything after it), like a C `argv` or
//! Python's `sys.argv`: forge's own flags and the path of the `forge`
//! binary are never part of it. A standalone binary built with `forge
//! build` sees its own `argv`.
//!
//! The arguments are set once for the process by its entry point
//! ([`set_process`]: the CLI's `run`, the standalone entry points) and can
//! be overridden per thread ([`scope`]): a sandbox run sees only what its
//! host passed (`Sandbox::args`, MCP `run_forge`'s `args`), never the host
//! process's command line. The override is carried into threads the
//! program starts by `permissions::inherit`, like the policy.

use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

static PROCESS: OnceLock<Arc<[String]>> = OnceLock::new();

thread_local! {
    static SCOPED: RefCell<Option<Arc<[String]>>> = const { RefCell::new(None) };
}

/// Set the process's program arguments (script path first). Only the first
/// call has an effect.
pub fn set_process(args: Vec<String>) {
    let _ = PROCESS.set(args.into());
}

/// The arguments of the program running on this thread: the thread's
/// override, else the process's, else none.
pub fn current() -> Arc<[String]> {
    current_override()
        .or_else(|| PROCESS.get().cloned())
        .unwrap_or_else(|| Arc::from(Vec::new()))
}

/// This thread's override, if any (what `permissions::inherit` carries).
pub fn current_override() -> Option<Arc<[String]>> {
    SCOPED.with(|s| s.borrow().clone())
}

/// Restores the previous override when dropped.
pub struct ArgsGuard {
    previous: Option<Arc<[String]>>,
}

impl Drop for ArgsGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        SCOPED.with(|s| *s.borrow_mut() = previous);
    }
}

/// Run this thread under `args` until the guard drops (`None` keeps the
/// process's arguments).
pub fn scope(args: Option<Arc<[String]>>) -> ArgsGuard {
    let previous = SCOPED.with(|s| std::mem::replace(&mut *s.borrow_mut(), args));
    ArgsGuard { previous }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_args_override_and_restore() {
        let before = current();
        {
            let _g = scope(Some(Arc::from(vec!["a.fg".to_string(), "x".to_string()])));
            assert_eq!(&*current(), &["a.fg".to_string(), "x".to_string()]);
            {
                let _inner = scope(Some(Arc::from(Vec::new())));
                assert!(current().is_empty());
            }
            assert_eq!(current().len(), 2);
        }
        assert_eq!(current(), before);
    }

    #[test]
    fn inherited_threads_keep_the_override() {
        let _g = scope(Some(Arc::from(vec!["t.fg".to_string()])));
        let seen = crate::permissions::spawn(|| current().to_vec())
            .join()
            .expect("thread");
        assert_eq!(seen, vec!["t.fg".to_string()]);
    }
}
