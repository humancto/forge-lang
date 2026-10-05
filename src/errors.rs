/// Forge Error Formatting
/// Beautiful, source-mapped error output powered by ariadne.
///
/// All of these render text destined for **stderr**. ANSI color is emitted
/// only when it will be seen as color (see [`color_enabled`]); piped output
/// and `NO_COLOR` environments get plain text.
use ariadne::{Color, Config, Label, Report, ReportKind, Source};

/// Whether diagnostics written to stderr should use ANSI color — the
/// shared policy in [`crate::color`] (NO_COLOR, FORCE_COLOR, TTY).
pub fn color_enabled() -> bool {
    crate::color::enabled(crate::color::Stream::Stderr)
}

/// Render `message` with a source snippet pointing at `line:col` of
/// `source`. `origin` names the source in the snippet header
/// (`╭─[ origin:line:col ]`): pass the file path through [`display_path`],
/// or a pseudo-name such as `<eval>`.
pub fn format_error(origin: &str, source: &str, line: usize, col: usize, message: &str) -> String {
    format_error_with_color(origin, source, line, col, message, color_enabled())
}

/// How a source path is shown to the user in diagnostics: relative to the
/// current directory when the file lives under it (`examples/x.fg`, not
/// `/home/me/proj/examples/x.fg` or `./examples/x.fg`), otherwise as given.
/// Pseudo-names such as `<eval>` pass through unchanged.
pub fn display_path(path: &str) -> String {
    let cwd = std::env::current_dir().ok();
    display_path_from(path, cwd.as_deref())
}

fn display_path_from(path: &str, cwd: Option<&std::path::Path>) -> String {
    use std::path::{Component, Path, PathBuf};
    if path.starts_with('<') {
        return path.to_string();
    }
    let given = Path::new(path);
    let absolute = if given.is_absolute() {
        given.to_path_buf()
    } else {
        match cwd {
            Some(cwd) => cwd.join(given),
            None => given.to_path_buf(),
        }
    };
    // Lexically normalise `.` and `..` so `./a/../b.fg` shows as `b.fg`.
    let mut normalised = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalised.pop() {
                    normalised.push("..");
                }
            }
            other => normalised.push(other.as_os_str()),
        }
    }
    match cwd.and_then(|cwd| normalised.strip_prefix(cwd).ok()) {
        Some(rel) if !rel.as_os_str().is_empty() => rel.display().to_string(),
        _ => {
            if given.is_absolute() {
                normalised.display().to_string()
            } else {
                path.trim_start_matches("./").to_string()
            }
        }
    }
}

fn format_error_with_color(
    origin: &str,
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

    Report::build(ReportKind::Error, origin, offset)
        .with_config(Config::default().with_color(color))
        .with_message(message)
        .with_label(
            Label::new((origin, offset..offset + 1))
                .with_message(label_msg)
                .with_color(Color::Red),
        )
        .finish()
        .write((origin, Source::from(source)), &mut buf)
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

/// Character offset of 1-based `line:col` (the lexer counts columns in
/// characters, and ariadne indexes `Source` by character), clamped to the
/// end of the source.
/// How a type-checker diagnostic is rendered (see [`format_diagnostic`]).
/// (Used by the CLI binary; the library build does not render diagnostics.)
#[allow(dead_code)]
pub struct DiagnosticView<'a> {
    /// `T0006` style code.
    pub code: &'a str,
    pub message: &'a str,
    pub help: Option<&'a str>,
    pub line: usize,
    pub col: usize,
    /// Highlighted width in chars (at least 1).
    pub len: usize,
    pub is_error: bool,
}

/// Render a type-checker diagnostic with a source snippet: an error or a
/// warning headed `[code] message`, the span underlined, and the help text
/// (did-you-mean) as a note.
#[allow(dead_code)]
pub fn format_diagnostic(origin: &str, source: &str, d: &DiagnosticView<'_>) -> String {
    format_diagnostic_with_color(origin, source, d, color_enabled())
}

#[allow(dead_code)]
fn format_diagnostic_with_color(
    origin: &str,
    source: &str,
    d: &DiagnosticView<'_>,
    color: bool,
) -> String {
    if d.line == 0 {
        let head = format!("[{}] {}", d.code, d.message);
        let text = match d.help {
            Some(help) => format!("{}\n  help: {}", head, help),
            None => head,
        };
        return if d.is_error {
            format_simple_error(&text)
        } else {
            format_warning(&text)
        };
    }
    let offset = line_col_to_offset(source, d.line, d.col);
    let (kind, label_color) = if d.is_error {
        (ReportKind::Error, Color::Red)
    } else {
        (ReportKind::Warning, Color::Yellow)
    };
    let mut report = Report::build(kind, origin, offset)
        .with_config(Config::default().with_color(color))
        .with_code(d.code)
        .with_message(d.message)
        .with_label(
            Label::new((origin, offset..offset + d.len.max(1)))
                .with_message(d.message.lines().next().unwrap_or(d.message))
                .with_color(label_color),
        );
    if let Some(help) = d.help {
        report = report.with_help(help);
    }
    let mut buf = Vec::new();
    report
        .finish()
        .write((origin, Source::from(source)), &mut buf)
        .ok();
    match String::from_utf8(buf) {
        Ok(rendered) if color => coalesce_ansi(&rendered),
        Ok(rendered) => rendered,
        Err(_) => format!("[{}] {}", d.code, d.message),
    }
}

fn line_col_to_offset(source: &str, line: usize, col: usize) -> usize {
    let total = source.chars().count();
    let mut current_line = 1;
    for (offset, ch) in source.chars().enumerate() {
        if current_line == line {
            return (offset + col.saturating_sub(1)).min(total);
        }
        if ch == '\n' {
            current_line += 1;
        }
    }
    total
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
    fn snippet_header_names_the_file() {
        let out = format_error_with_color(
            "examples/demo.fg",
            "let x = 1\nlet y = (\n",
            2,
            9,
            "unexpected token",
            false,
        );
        assert!(out.contains("examples/demo.fg:2:9"), "{}", out);
        assert!(!out.contains("<source>"), "{}", out);
    }

    #[test]
    fn snippet_offsets_count_characters_not_bytes() {
        // `é` is two bytes; the caret must still land under `(`.
        let out = format_error_with_color("t.fg", "let é = (\n", 1, 9, "unexpected token", false);
        assert!(out.contains("t.fg:1:9"), "{}", out);
    }

    #[test]
    fn display_path_is_relative_to_cwd() {
        let cwd = std::path::Path::new("/home/me/proj");
        assert_eq!(
            display_path_from("/home/me/proj/examples/x.fg", Some(cwd)),
            "examples/x.fg"
        );
        assert_eq!(
            display_path_from("./examples/x.fg", Some(cwd)),
            "examples/x.fg"
        );
        assert_eq!(display_path_from("examples/../x.fg", Some(cwd)), "x.fg");
        assert_eq!(
            display_path_from("/elsewhere/y.fg", Some(cwd)),
            "/elsewhere/y.fg"
        );
        assert_eq!(display_path_from("../sib/z.fg", Some(cwd)), "../sib/z.fg");
        assert_eq!(display_path_from("<eval>", Some(cwd)), "<eval>");
        assert_eq!(display_path_from("a.fg", None), "a.fg");
    }

    #[test]
    fn plain_error_has_no_escape_codes() {
        let out = format_error_with_color(
            "t.fg",
            "let x = 1\nlet y = (\n",
            2,
            9,
            "unexpected token",
            false,
        );
        assert!(!out.contains('\x1B'), "{:?}", out);
        assert!(out.contains("let y = ("));
        assert!(out.contains("unexpected token"));
    }

    #[test]
    fn colored_snippet_is_coalesced_per_span() {
        let source = "let x = 1\nlet yyyyyy = (\n";
        let out = format_error_with_color("t.fg", source, 2, 14, "unexpected token", true);
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
