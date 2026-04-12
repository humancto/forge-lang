use std::sync::atomic::{AtomicBool, Ordering};

static ALLOW_RUN: AtomicBool = AtomicBool::new(false);

/// Enable shell execution permission (called from CLI when --allow-run is set).
pub fn set_allow_run(allowed: bool) {
    ALLOW_RUN.store(allowed, Ordering::SeqCst);
}

/// Check if shell execution is allowed. Returns Ok(()) or an error message.
pub fn check_run_permission() -> Result<(), String> {
    if ALLOW_RUN.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err("Shell execution denied. Use --allow-run to enable sh/shell/run_command.".to_string())
    }
}
