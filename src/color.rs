//! Terminal color policy for everything Forge prints *about* a program —
//! diagnostics, `forge test` results, the REPL, `forge learn`, banners and
//! the stdlib's own chrome (`log.*` lines, `term.success`, `term.table`
//! headers, ...).
//!
//! One rule, decided once per stream:
//! 1. `NO_COLOR` set to a non-empty value disables color (<https://no-color.org>).
//! 2. `FORCE_COLOR` or `CLICOLOR_FORCE` set to a non-empty value other than
//!    `0` forces color (CI log viewers that render ANSI).
//! 3. Otherwise color is used only when that stream is a terminal.
//!
//! Values a *program* builds on purpose are data, not chrome, and are never
//! touched: `term.red("x")` always returns the ANSI-wrapped string, and
//! `say term.red("x")` prints it as-is.
//!
//! Chrome is written with [`cprintln!`]/[`ceprintln!`] (and the `print`
//! variants): they format as usual and strip SGR color sequences when
//! color is off for that stream, so call sites keep their inline escapes.
//! They write through `runtime::stdio`, so a host capture sees them.

use std::borrow::Cow;
use std::io::IsTerminal;
use std::sync::OnceLock;

/// An output stream whose color decision is tracked separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// Whether text written to `stream` should carry ANSI color.
/// Computed once per stream per process.
pub fn enabled(stream: Stream) -> bool {
    static STDOUT: OnceLock<bool> = OnceLock::new();
    static STDERR: OnceLock<bool> = OnceLock::new();
    let (cell, is_tty): (&OnceLock<bool>, fn() -> bool) = match stream {
        Stream::Stdout => (&STDOUT, || std::io::stdout().is_terminal()),
        Stream::Stderr => (&STDERR, || std::io::stderr().is_terminal()),
    };
    *cell.get_or_init(|| {
        let var = |name: &str| std::env::var_os(name).map(|v| v.to_string_lossy().into_owned());
        decide(
            var("NO_COLOR").as_deref(),
            var("FORCE_COLOR").as_deref(),
            var("CLICOLOR_FORCE").as_deref(),
            is_tty(),
        )
    })
}

/// Pure color decision, separated from the environment for testing.
pub(crate) fn decide(
    no_color: Option<&str>,
    force_color: Option<&str>,
    clicolor_force: Option<&str>,
    is_tty: bool,
) -> bool {
    let set = |v: Option<&str>| v.is_some_and(|s| !s.is_empty());
    let forced = |v: Option<&str>| v.is_some_and(|s| !s.is_empty() && s != "0");
    if set(no_color) {
        return false;
    }
    if forced(force_color) || forced(clicolor_force) {
        return true;
    }
    is_tty
}

/// `text` wrapped in the SGR sequence `code` (e.g. `"1;31"`) when color is
/// on for `stream`, otherwise `text` unchanged.
#[allow(dead_code)]
pub fn paint(stream: Stream, code: &str, text: &str) -> String {
    if enabled(stream) {
        format!("\x1B[{}m{}\x1B[0m", code, text)
    } else {
        text.to_string()
    }
}

/// `text` ready for `stream`: unchanged when color is on, with every SGR
/// (`ESC [ ... m`) sequence removed when it is off. Cursor-control
/// sequences (clear screen, erase line) are left alone.
pub fn sanitize(stream: Stream, text: &str) -> Cow<'_, str> {
    if enabled(stream) {
        Cow::Borrowed(text)
    } else {
        strip_sgr(text)
    }
}

/// Remove SGR color/style sequences (`ESC [ <digits;...> m`).
pub fn strip_sgr(text: &str) -> Cow<'_, str> {
    if !text.contains('\x1B') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('\x1B') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let params = tail.strip_prefix("\x1B[").map(|after| {
            after
                .bytes()
                .take_while(|b| b.is_ascii_digit() || *b == b';')
                .count()
        });
        match params {
            Some(n) if tail.as_bytes().get(2 + n) == Some(&b'm') => rest = &tail[3 + n..],
            _ => {
                out.push('\x1B');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// `println!` for CLI chrome on stdout: strips color when stdout is not a
/// color terminal (see the module docs).
#[allow(unused_macros)]
macro_rules! cprintln {
    () => { $crate::runtime::stdio::out_line("") };
    ($($arg:tt)*) => {
        $crate::runtime::stdio::out_line(&$crate::color::sanitize($crate::color::Stream::Stdout, &format!($($arg)*)))
    };
}

/// `print!` for CLI chrome on stdout.
#[allow(unused_macros)]
macro_rules! cprint {
    ($($arg:tt)*) => {
        $crate::runtime::stdio::out(&$crate::color::sanitize($crate::color::Stream::Stdout, &format!($($arg)*)))
    };
}

/// `eprintln!` for CLI chrome on stderr.
#[allow(unused_macros)]
macro_rules! ceprintln {
    () => { $crate::runtime::stdio::err_line("") };
    ($($arg:tt)*) => {
        $crate::runtime::stdio::err_line(&$crate::color::sanitize($crate::color::Stream::Stderr, &format!($($arg)*)))
    };
}

/// `eprint!` for CLI chrome on stderr.
#[allow(unused_macros)]
macro_rules! ceprint {
    ($($arg:tt)*) => {
        $crate::runtime::stdio::err(&$crate::color::sanitize($crate::color::Stream::Stderr, &format!($($arg)*)))
    };
}

#[allow(unused_imports)]
pub(crate) use {ceprint, ceprintln, cprint, cprintln};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_color_wins() {
        assert!(!decide(Some("1"), Some("1"), Some("1"), true));
    }

    #[test]
    fn empty_no_color_is_ignored() {
        assert!(decide(Some(""), None, None, true));
    }

    #[test]
    fn force_color_overrides_non_tty() {
        assert!(decide(None, Some("1"), None, false));
        assert!(decide(None, None, Some("1"), false));
        assert!(!decide(None, Some("0"), None, false));
    }

    #[test]
    fn defaults_to_tty_detection() {
        assert!(decide(None, None, None, true));
        assert!(!decide(None, None, None, false));
    }

    #[test]
    fn strip_sgr_removes_only_color_sequences() {
        assert_eq!(strip_sgr("\x1B[1;31mFAIL\x1B[0m  x"), "FAIL  x");
        assert_eq!(strip_sgr("\x1B[38;5;196mred\x1B[0m"), "red");
        assert_eq!(strip_sgr("\x1B[mplain"), "plain");
        // Cursor control and stray escapes survive.
        assert_eq!(strip_sgr("\x1B[2J\x1B[1;1H"), "\x1B[2J\x1B[1;1H");
        assert_eq!(strip_sgr("a\x1Bb"), "a\x1Bb");
        assert_eq!(strip_sgr("ünïcödé ✓"), "ünïcödé ✓");
        assert!(matches!(strip_sgr("no escapes"), Cow::Borrowed(_)));
    }
}
