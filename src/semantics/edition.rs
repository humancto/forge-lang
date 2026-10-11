//! Language editions (docs/STABILITY.md).
//!
//! An edition is the unit in which Forge may make a source-breaking change:
//! a project opts in with `edition = "..."` under `[project]` in
//! `forge.toml` (or `forge --edition ...`), and every edition stays
//! supported, so upgrading the toolchain never changes a program's meaning.
//!
//! The edition in effect is read where a program is *prepared* — when a
//! parser, compiler or interpreter is created — never per operation, so it
//! costs nothing at run time. Resolution: a per-thread override (installed
//! by [`scope`]; sandboxes use it so concurrent runs can differ), else the
//! process default (set once by the CLI with [`set_default`]), else
//! [`Edition::DEFAULT`]. Threads that run Forge code inherit the override
//! through `permissions::inherit`.
//!
//! Gate a breaking change on `edition::current() >= Edition::E2027` at the
//! point where behaviour differs, record the edition in whatever the engine
//! keeps for the run (chunk, interpreter), and list the change in
//! docs/editions/2027.md.

use std::cell::Cell;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Edition {
    E2026,
    /// In development: its rules may still change until it is announced
    /// stable (docs/editions/2027.md).
    E2027,
}

impl Edition {
    pub const DEFAULT: Edition = Edition::E2026;
    pub const ALL: &'static [Edition] = &[Edition::E2026, Edition::E2027];

    pub fn as_str(self) -> &'static str {
        match self {
            Edition::E2026 => "2026",
            Edition::E2027 => "2027",
        }
    }

    /// Whether this edition's rules may still change.
    pub fn is_preview(self) -> bool {
        matches!(self, Edition::E2027)
    }

    pub fn parse(text: &str) -> Result<Edition, String> {
        Edition::ALL
            .iter()
            .copied()
            .find(|e| e.as_str() == text)
            .ok_or_else(|| {
                let known: Vec<&str> = Edition::ALL.iter().map(|e| e.as_str()).collect();
                format!(
                    "unknown edition \"{}\" (known editions: {}); this Forge v{} may be too old for the project",
                    text,
                    known.join(", "),
                    env!("CARGO_PKG_VERSION")
                )
            })
    }

    fn to_u8(self) -> u8 {
        match self {
            Edition::E2026 => 1,
            Edition::E2027 => 2,
        }
    }

    fn from_u8(n: u8) -> Option<Edition> {
        match n {
            1 => Some(Edition::E2026),
            2 => Some(Edition::E2027),
            _ => None,
        }
    }
}

impl std::fmt::Display for Edition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 0 = unset (use [`Edition::DEFAULT`]).
static PROCESS_DEFAULT: AtomicU8 = AtomicU8::new(0);

thread_local! {
    static OVERRIDE: Cell<Option<Edition>> = const { Cell::new(None) };
}

/// Set the edition for every thread without an override. The CLI calls
/// this once, before any program is parsed.
pub fn set_default(edition: Edition) {
    PROCESS_DEFAULT.store(edition.to_u8(), Ordering::Relaxed);
}

/// The edition in effect on this thread.
pub fn current() -> Edition {
    OVERRIDE
        .with(|o| o.get())
        .or_else(|| Edition::from_u8(PROCESS_DEFAULT.load(Ordering::Relaxed)))
        .unwrap_or(Edition::DEFAULT)
}

/// This thread's override, if any (what `permissions::inherit` carries).
pub fn current_override() -> Option<Edition> {
    OVERRIDE.with(|o| o.get())
}

/// Use `edition` on this thread (`None`: fall back to the process default)
/// until the guard is dropped.
pub fn scope(edition: Option<Edition>) -> ScopeGuard {
    let previous = OVERRIDE.with(|o| o.replace(edition));
    ScopeGuard { previous }
}

#[must_use = "the edition is only in effect while the guard lives"]
pub struct ScopeGuard {
    previous: Option<Edition>,
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        let previous = self.previous;
        let _ = OVERRIDE.try_with(|o| o.set(previous));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editions_parse_and_order() {
        assert_eq!(Edition::parse("2026"), Ok(Edition::E2026));
        assert_eq!(Edition::parse("2027"), Ok(Edition::E2027));
        assert!(Edition::parse("2030")
            .unwrap_err()
            .contains("unknown edition"));
        assert!(Edition::E2027 > Edition::E2026);
        assert!(Edition::E2027.is_preview());
        assert!(!Edition::E2026.is_preview());
    }

    #[test]
    fn scope_overrides_and_restores() {
        let before = current();
        {
            let _g = scope(Some(Edition::E2027));
            assert_eq!(current(), Edition::E2027);
            {
                let _inner = scope(Some(Edition::E2026));
                assert_eq!(current(), Edition::E2026);
            }
            assert_eq!(current(), Edition::E2027);
        }
        assert_eq!(current(), before);
    }
}
