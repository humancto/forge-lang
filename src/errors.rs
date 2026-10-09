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

/// How program diagnostics (syntax, type and runtime errors) are written to
/// stderr: `forge run --error-format json` / `forge check --format json`
/// emit one JSON object per line for editors and agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum ErrorFormat {
    #[default]
    Human,
    Json,
}

static ERROR_FORMAT: std::sync::OnceLock<ErrorFormat> = std::sync::OnceLock::new();

/// Select the diagnostic format for this process (first call wins).
#[allow(dead_code)]
pub fn set_error_format(format: ErrorFormat) {
    let _ = ERROR_FORMAT.set(format);
}

#[allow(dead_code)]
pub fn error_format() -> ErrorFormat {
    ERROR_FORMAT.get().copied().unwrap_or_default()
}

/// Which stage of running a program produced a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Phase {
    Syntax,
    Type,
    Runtime,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Phase::Syntax => "syntax",
            Phase::Type => "type",
            Phase::Runtime => "runtime",
        }
    }
}

/// One program diagnostic, independent of how it is rendered.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct ProgramDiagnostic {
    /// `E0009` / `T0006`.
    pub code: String,
    pub is_error: bool,
    /// Headline (first line of the message, without the hint).
    pub message: String,
    /// The file as shown to the user (see [`display_path`]).
    pub file: String,
    /// 1-based; 0 when the position is unknown.
    pub line: usize,
    pub col: usize,
    /// Highlighted width in chars (at least 1).
    pub len: usize,
    pub hint: Option<String>,
    pub phase: Phase,
}

#[allow(dead_code)]
impl ProgramDiagnostic {
    /// A runtime error message (as raised by either engine) at `line:col`.
    /// The code comes from the shared table (`semantics::errors`); the hint
    /// is the message's own `hint:` line, else the code's default hint.
    pub fn runtime(message: &str, file: &str, line: usize, col: usize) -> Self {
        use crate::semantics::errors;
        let class = errors::classify(message);
        Self {
            code: class.code.to_string(),
            is_error: true,
            message: errors::headline(message).to_string(),
            file: file.to_string(),
            line,
            col,
            len: 1,
            hint: Some(errors::hint_for(message).to_string()),
            phase: Phase::Runtime,
        }
    }

    /// A lexer (`E0001`) or parser (`E0002`) error.
    pub fn syntax(lexer: bool, message: &str, file: &str, line: usize, col: usize) -> Self {
        use crate::semantics::errors;
        let class = errors::lookup(if lexer { "E0001" } else { "E0002" })
            .expect("BUG: syntax codes are in the table");
        Self {
            code: class.code.to_string(),
            is_error: true,
            message: errors::headline(message).to_string(),
            file: file.to_string(),
            line,
            col,
            len: 1,
            hint: Some(
                errors::message_hint(message)
                    .unwrap_or(class.hint)
                    .to_string(),
            ),
            phase: Phase::Syntax,
        }
    }

    /// The JSON object emitted by `--error-format json` (one per line):
    /// `{code, severity, message, file, line, col, hint, phase}`. `line`
    /// and `col` are null when the position is unknown.
    pub fn to_json(&self) -> String {
        let position = |n: usize| {
            if n == 0 {
                serde_json::Value::Null
            } else {
                serde_json::Value::from(n)
            }
        };
        serde_json::json!({
            "code": self.code,
            "severity": if self.is_error { "error" } else { "warning" },
            "message": self.message,
            "file": self.file,
            "line": position(self.line),
            "col": position(if self.line == 0 { 0 } else { self.col }),
            "hint": self.hint,
            "phase": self.phase.as_str(),
        })
        .to_string()
    }

    /// Human rendering: a source snippet headed `[code] message`, with the
    /// hint as help and, for errors, a pointer to `forge explain`.
    pub fn to_human(&self, source: &str) -> String {
        self.to_human_with_color(source, color_enabled())
    }

    fn to_human_with_color(&self, source: &str, color: bool) -> String {
        let explain = self
            .is_error
            .then(|| format!("run `forge explain {}` for details", self.code));
        if self.line == 0 || source.lines().count() < self.line {
            let mut text = format!("[{}] {}", self.code, self.message);
            if let Some(hint) = &self.hint {
                text.push_str(&format!("\n  help: {}", hint));
            }
            if let Some(explain) = &explain {
                text.push_str(&format!("\n  note: {}", explain));
            }
            return if self.is_error {
                format_simple_error(&text)
            } else {
                format_warning(&text)
            };
        }
        let origin = self.file.as_str();
        let offset = line_col_to_offset(source, self.line, self.col.max(1));
        let (kind, label_color) = if self.is_error {
            (ReportKind::Error, Color::Red)
        } else {
            (ReportKind::Warning, Color::Yellow)
        };
        let mut report = Report::build(kind, origin, offset)
            .with_config(Config::default().with_color(color))
            .with_code(&self.code)
            .with_message(&self.message)
            .with_label(
                Label::new((origin, offset..offset + self.len.max(1)))
                    .with_message(&self.message)
                    .with_color(label_color),
            );
        if let Some(hint) = &self.hint {
            report = report.with_help(hint);
        }
        if let Some(explain) = explain {
            report = report.with_note(explain);
        }
        let mut buf = Vec::new();
        report
            .finish()
            .write((origin, Source::from(source)), &mut buf)
            .ok();
        match String::from_utf8(buf) {
            Ok(rendered) if color => coalesce_ansi(&rendered),
            Ok(rendered) => rendered,
            Err(_) => format!("error[{}]: {}", self.code, self.message),
        }
    }

    /// Render in the process's [`error_format`].
    pub fn render(&self, source: &str) -> String {
        match error_format() {
            ErrorFormat::Json => self.to_json(),
            ErrorFormat::Human => self.to_human(source),
        }
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
        // Separator-neutral: on Windows the relative part is rendered with
        // `\`, which is correct for that platform.
        let rel =
            |p: &str, cwd: Option<&std::path::Path>| display_path_from(p, cwd).replace('\\', "/");
        let cwd = std::path::Path::new("/home/me/proj");
        assert_eq!(
            rel("/home/me/proj/examples/x.fg", Some(cwd)),
            "examples/x.fg"
        );
        assert_eq!(rel("./examples/x.fg", Some(cwd)), "examples/x.fg");
        assert_eq!(rel("examples/../x.fg", Some(cwd)), "x.fg");
        assert_eq!(rel("/elsewhere/y.fg", Some(cwd)), "/elsewhere/y.fg");
        assert_eq!(rel("../sib/z.fg", Some(cwd)), "../sib/z.fg");
        assert_eq!(rel("<eval>", Some(cwd)), "<eval>");
        assert_eq!(rel("a.fg", None), "a.fg");
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
    fn runtime_diagnostics_carry_code_and_hint() {
        let d = ProgramDiagnostic::runtime(
            &crate::semantics::index_out_of_bounds(5, "array", 3),
            "t.fg",
            2,
            5,
        );
        assert_eq!(d.code, "E0009");
        assert_eq!(
            d.message,
            "index out of bounds: index 5 on array of length 3"
        );
        assert_eq!(
            d.hint.as_deref(),
            Some("valid indices are 0 to 2 (or -3 to -1 from the end)")
        );
        let json: serde_json::Value = serde_json::from_str(&d.to_json()).unwrap();
        assert_eq!(json["code"], "E0009");
        assert_eq!(json["severity"], "error");
        assert_eq!(json["file"], "t.fg");
        assert_eq!(json["line"], 2);
        assert_eq!(json["col"], 5);
        assert_eq!(json["phase"], "runtime");
        assert!(json["hint"].as_str().unwrap().starts_with("valid indices"));

        let human = d.to_human_with_color("let a = [1,2,3]\nsay a[5]\n", false);
        assert!(human.contains("[E0009]"), "{}", human);
        assert!(human.contains("t.fg:2:5"), "{}", human);
        assert!(human.contains("valid indices are 0 to 2"), "{}", human);
        assert!(human.contains("forge explain E0009"), "{}", human);

        // Unknown position: null line/col, plain rendering.
        let d = ProgramDiagnostic::runtime("key 'b' not found", "t.fg", 0, 0);
        let json: serde_json::Value = serde_json::from_str(&d.to_json()).unwrap();
        assert!(json["line"].is_null() && json["col"].is_null());
        assert_eq!(json["code"], "E0010");
        assert!(d
            .to_human_with_color("", false)
            .contains("[E0010] key 'b' not found"));
    }

    #[test]
    fn coalesce_keeps_plain_text_untouched() {
        assert_eq!(coalesce_ansi("plain ✓ text"), "plain ✓ text");
        assert_eq!(coalesce_ansi("a\x1B[0mb"), "a\x1B[0mb");
    }
}
