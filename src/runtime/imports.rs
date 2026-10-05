//! Import-cycle detection shared by every execution engine.
//!
//! # Invariant
//!
//! Both the interpreter (`Stmt::Import`) and the VM (`__forge_import_module`)
//! execute an imported file by recursively running it. Before running a
//! module, an engine must call [`enter_import`] with the resolved file path
//! and keep the returned [`ImportGuard`] alive for the duration of the
//! module's execution. If the file is already being imported further up the
//! chain, `enter_import` fails with a single, readable error:
//!
//! ```text
//! circular import: a.fg -> b.fg -> a.fg
//! ```
//!
//! The chain is tracked per thread (imports run synchronously on the thread
//! that executes the `import` statement) and is unwound by the guard's `Drop`,
//! so errors and early returns cannot leave stale entries behind.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    static IMPORT_STACK: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

/// Keeps a module on the import chain; popped on drop.
#[must_use = "the module stays on the import chain only while the guard lives"]
pub struct ImportGuard {
    depth: usize,
}

impl Drop for ImportGuard {
    fn drop(&mut self) {
        IMPORT_STACK.with(|s| s.borrow_mut().truncate(self.depth));
    }
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn display_path(path: &Path) -> String {
    if let Ok(cwd) = std::env::current_dir() {
        let cwd = canonical(&cwd);
        if let Ok(rel) = path.strip_prefix(&cwd) {
            return rel.display().to_string();
        }
    }
    path.display().to_string()
}

/// Push `path` onto the current thread's import chain, or report a cycle.
pub fn enter_import(path: &Path) -> Result<ImportGuard, String> {
    // Every module load counts against the run's import limit
    // (`runtime::limits`).
    crate::runtime::limits::charge_import()?;
    let path = canonical(path);
    IMPORT_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        if let Some(start) = stack.iter().position(|p| *p == path) {
            let chain: Vec<String> = stack[start..]
                .iter()
                .chain(std::iter::once(&path))
                .map(|p| display_path(p))
                .collect();
            return Err(format!("circular import: {}", chain.join(" -> ")));
        }
        let depth = stack.len();
        stack.push(path);
        Ok(ImportGuard { depth })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_cycle_and_unwinds() {
        let dir = std::env::temp_dir().join(format!("forge_import_cycle_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let a = dir.join("a.fg");
        let b = dir.join("b.fg");
        std::fs::write(&a, "").expect("write a");
        std::fs::write(&b, "").expect("write b");
        {
            let _ga = enter_import(&a).expect("a");
            let _gb = enter_import(&b).expect("b");
            let err = enter_import(&a).err().expect("cycle must be detected");
            assert!(err.starts_with("circular import: "), "{}", err);
            assert!(err.contains("a.fg -> "), "{}", err);
            assert!(err.ends_with("a.fg"), "{}", err);
            assert!(err.contains("b.fg"), "{}", err);
        }
        // Guards dropped: the chain is empty again, so re-entry succeeds.
        let _again = enter_import(&a).expect("chain must unwind on drop");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
