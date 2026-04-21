use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Cooperative cancellation token for structured concurrency (`squad` blocks).
///
/// Shared between a squad's parent thread and all spawned child tasks.
/// When any task fails, the parent sets the token; children check it at
/// safe points (loop heads, function calls, statement boundaries) and
/// return a "task cancelled" error.
#[derive(Clone, Debug)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
