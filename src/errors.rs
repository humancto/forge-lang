/// Forge Error Formatting
/// Beautiful, source-mapped error output powered by ariadne.
///
/// All of these render text destined for **stderr**. ANSI color is emitted
/// only when it will be seen as color (see [`color_enabled`]); piped output
/// and `NO_COLOR` environments get plain text.
use ariadne::{Color, Config, Label, Report, ReportKind, Source};
use std::io::IsTerminal;
use std::sync::OnceLock;

/// Whether diagnostics written to stderr should use ANSI color.
///
/// Follows the common conventions, in priority order:
/// 1. `NO_COLOR` set to a non-empty value disables color (<https://no-color.org>).
/// 2. `FORCE_COLOR` or `CLICOLOR_FORCE` set to a non-empty value other than
///    `0` forces color (useful in CI log viewers that render ANSI).
/// 3. Otherwise color is used only when stderr is a terminal.
///
/// Computed once per process.
pub fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let var = |name: &str| std::env::var_os(name).map(|v| v.to_string_lossy().into_owned());
        decide_color(
            var("NO_COLOR").as_deref(),
            var("FORCE_COLOR").as_deref(),
            var("CLICOLOR_FORCE").as_deref(),
            std::io::stderr().is_terminal(),
        )
    })
}

/// Pure color decision, separated from the environment for testing.
fn decide_color(
    no_color: Option<&str>,
    force_color: Option<&str>,
    clicolor_force: Option<&str>,
    stderr_is_tty: bool,
) -> bool {
    let set = |v: Option<&str>| v.is_some_and(|s| !s.is_empty());
    let forced = |v: Option<&str>| v.is_some_and(|s| !s.is_empty() && s != "0");
    if set(no_color) {
        return false;
    }
    if forced(force_color) || forced(clicolor_force) {
        return true;
    }
    stderr_is_tty
}

pub fn format_error(source: &str, line: usize, col: usize, message: &str) -> String {
    format_error_with_color(source, line, col, message, color_enabled())
}

fn format_error_with_color(
    source: &str,
    line: usize,
    col: usize,
    message: &str,
    color: bool,
) -> String {
    let mut buf = Vec::new();

    let offset = line_col_to_offset(source, line, col);

    // Split message from hints so the label only shows the core error
    let label_msg = message.lines().next().unwrap_or(message);

    Report::build(ReportKind::Error, "<source>", offset)
        .with_config(Config::default().with_color(color))
        .with_message(message)
        .with_label(
            Label::new(("<source>", offset..offset + 1))
                .with_message(label_msg)
                .with_color(Color::Red),
        )
        .finish()
        .write(("<source>", Source::from(source)), &mut buf)
        .ok();

    match String::from_utf8(buf) {
        Ok(rendered) if color => coalesce_ansi(&rendered),
        Ok(rendered) => rendered,
        Err(_) => format!("error: {}", message),
    }
}

/// ariadne wraps every character of a highlighted snippet in its own
/// `ESC[..m ... ESC[0m` pair, which bloats output and breaks copy/paste in
/// some terminals. Merge runs so that each colored span gets one start and
/// one reset: a reset immediately followed by the same SGR sequence that was
/// active is dropped.
fn coalesce_ansi(input: &str) -> String {
    const RESET: &str = "\x1B[0m";
    let mut out = String::with_capacity(input.len());
    let mut active: Option<&str> = None;
    let mut pending_reset = false;
    let mut rest = input;

    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix("\x1B[") {
            if let Some(end) = stripped.find('m') {
                let seq = &rest[..end + 3];
                rest = &rest[end + 3..];
                if seq == RESET {
                    if active.is_some() {
                        pending_reset = true;
                    } else {
                        out.push_str(seq);
                    }
                } else if pending_reset && active == Some(seq) {
                    // Same color resumes right after a reset: keep the span open.
                    pending_reset = false;
                } else {
                    if pending_reset {
                        out.push_str(RESET);
                        pending_reset = false;
                    }
                    out.push_str(seq);
                    active = Some(seq);
                }
                continue;
            }
        }
        let ch = rest.chars().next().expect("BUG: non-empty rest has a char");
        if pending_reset {
            out.push_str(RESET);
            pending_reset = false;
            active = None;
        }
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    if pending_reset {
        out.push_str(RESET);
    }
    out
}

fn line_col_to_offset(source: &str, line: usize, col: usize) -> usize {
    let mut current_line = 1;
    let mut offset = 0;
    for ch in source.chars() {
        if current_line == line {
            if offset + col.saturating_sub(1) <= source.len() {
                return offset + col.saturating_sub(1);
            }
            return offset;
        }
        if ch == '\n' {
            current_line += 1;
        }
        offset += ch.len_utf8();
    }
    offset
}

fn paint(code: &str, text: &str) -> String {
    if color_enabled() {
        format!("\x1B[{}m{}\x1B[0m", code, text)
    } else {
        text.to_string()
    }
}

/// Format a simple error without source context
pub fn format_simple_error(message: &str) -> String {
    format!("{}: {}", paint("1;31", "error"), message)
}

/// Format a warning
#[allow(dead_code)]
pub fn format_warning(message: &str) -> String {
    format!("{}: {}", paint("1;33", "warning"), message)
}

/// Format a success message
#[allow(dead_code)]
pub fn format_success(message: &str) -> String {
    paint("1;32", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_color_wins() {
        assert!(!decide_color(Some("1"), Some("1"), Some("1"), true));
    }

    #[test]
    fn empty_no_color_is_ignored() {
        assert!(decide_color(Some(""), None, None, true));
    }

    #[test]
    fn force_color_overrides_non_tty() {
        assert!(decide_color(None, Some("1"), None, false));
        assert!(decide_color(None, None, Some("1"), false));
        assert!(!decide_color(None, Some("0"), None, false));
    }

    #[test]
    fn defaults_to_tty_detection() {
        assert!(decide_color(None, None, None, true));
        assert!(!decide_color(None, None, None, false));
    }

    #[test]
    fn plain_error_has_no_escape_codes() {
        let out =
            format_error_with_color("let x = 1\nlet y = (\n", 2, 9, "unexpected token", false);
        assert!(!out.contains('\x1B'), "{:?}", out);
        assert!(out.contains("let y = ("));
        assert!(out.contains("unexpected token"));
    }

    #[test]
    fn colored_snippet_is_coalesced_per_span() {
        let source = "let x = 1\nlet yyyyyy = (\n";
        let out = format_error_with_color(source, 2, 14, "unexpected token", true);
        assert!(out.contains('\x1B'));
        // The snippet line must appear as one contiguous run of text,
        // not one escape sequence per character.
        assert!(
            out.contains("let yyyyyy = "),
            "snippet still colored per character: {:?}",
            out
        );
        let snippet_line = out.lines().find(|l| l.contains("yyyyyy")).unwrap();
        let escapes = snippet_line.matches('\x1B').count();
        assert!(
            escapes <= 6,
            "expected one escape pair per span, got {}: {:?}",
            escapes,
            snippet_line
        );
    }

    #[test]
    fn coalesce_merges_identical_adjacent_spans() {
        let input = "\x1B[31ma\x1B[0m\x1B[31mb\x1B[0m\x1B[32mc\x1B[0m d";
        assert_eq!(coalesce_ansi(input), "\x1B[31mab\x1B[0m\x1B[32mc\x1B[0m d");
    }

    #[test]
    fn coalesce_keeps_plain_text_untouched() {
        assert_eq!(coalesce_ansi("plain ✓ text"), "plain ✓ text");
        assert_eq!(coalesce_ansi("a\x1B[0mb"), "a\x1B[0mb");
    }
}
