//! Stand-ins for the host runtime in builds without the `host` feature (the
//! browser playground). Each item has the signature of its `host` twin so
//! the engines call it unchanged; every entry point fails with
//! [`super::unavailable_message`] instead of doing I/O.

/// Stand-in for `runtime/client.rs` (HTTP client: `fetch`, `grab`, `ask`).
pub mod client {
    use crate::interpreter::Value;
    use std::collections::HashMap;

    #[allow(clippy::too_many_arguments)]
    pub fn fetch_blocking(
        _url: &str,
        _method: &str,
        _body: Option<String>,
        _headers: Option<&HashMap<String, String>>,
        _timeout_secs: Option<u64>,
        _max_redirects: Option<usize>,
        _max_bytes: Option<u64>,
    ) -> Result<Value, String> {
        Err(crate::runtime::unavailable_message("network access"))
    }
}

/// Stand-in for `runtime/host.rs` (`schedule` and `watch` blocks).
pub mod host {
    use crate::interpreter::{Interpreter, RuntimeError};
    use crate::runtime::metadata::{SchedulePlan, WatchPlan};

    pub(crate) fn spawn_schedule(
        _interpreter: &Interpreter,
        _schedule: &SchedulePlan,
    ) -> Result<(), RuntimeError> {
        Err(RuntimeError::new(&crate::runtime::unavailable_message(
            "`schedule`",
        )))
    }

    pub(crate) fn spawn_watch(
        _interpreter: &Interpreter,
        _watch: &WatchPlan,
    ) -> Result<(), RuntimeError> {
        Err(RuntimeError::new(&crate::runtime::unavailable_message(
            "`watch`",
        )))
    }

    pub(crate) fn checked_watch_path(_path: &str) -> Result<String, String> {
        Err(crate::runtime::unavailable_message("`watch`"))
    }
}
