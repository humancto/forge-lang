//! Editor services that need no host runtime: diagnostics and formatting.
//!
//! These are the library entry points for tools that embed Forge without
//! the CLI — the browser playground (`bindings/wasm`) in particular. They
//! are pure functions of the source text and build for every target,
//! including `wasm32-unknown-unknown`.

/// One problem found by [`check_source`]. Positions are 1-based and counted
/// in `char`s (the lexer's convention); 0 when unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub column: usize,
    /// End of the highlighted range (exclusive). Equal to the start for a
    /// point diagnostic (lex/parse errors, statement-level checks).
    pub end_line: usize,
    pub end_column: usize,
    pub is_error: bool,
    /// Type-checker code (`T0006`); `None` for lex and parse errors.
    pub code: Option<String>,
    pub message: String,
    /// Extra explanation (`did you mean ...`), when the checker has one.
    pub help: Option<String>,
}

/// Lex, parse and type-check `source` without running it. Lex and parse
/// errors stop at the first one; type-checker findings are all reported,
/// in source order. Imports of files are not resolved (there is no file).
pub fn check_source(source: &str) -> Vec<Diagnostic> {
    use crate::typechecker::{analyze, CheckOptions, FrontendError};
    match analyze(source, &CheckOptions::default()) {
        Ok(analysis) => analysis
            .diagnostics
            .into_iter()
            .map(|d| Diagnostic {
                line: d.span.start.line,
                column: d.span.start.col,
                end_line: d.span.end.line.max(d.span.start.line),
                end_column: if d.span.end.line > d.span.start.line {
                    d.span.end.col
                } else {
                    d.span.end.col.max(d.span.start.col)
                },
                is_error: d.is_error(),
                code: Some(d.code.as_str().to_string()),
                message: d.message.clone(),
                help: d.help.clone(),
            })
            .collect(),
        Err(FrontendError::Lex { line, col, message })
        | Err(FrontendError::Parse { line, col, message }) => vec![Diagnostic {
            line,
            column: col,
            end_line: line,
            end_column: col,
            is_error: true,
            code: None,
            message,
            help: None,
        }],
    }
}

/// Format `source` the way `forge fmt` does. Source that does not lex is
/// only re-indented, so formatting never fails.
pub fn format_source(source: &str) -> String {
    crate::formatter::format_source(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_source_has_no_diagnostics() {
        assert!(check_source("let x = 1\nsay x\n").is_empty());
    }

    #[test]
    fn parse_errors_are_reported_with_a_position() {
        let d = check_source("let = 5\n");
        assert_eq!(d.len(), 1);
        assert!(d[0].is_error);
        assert_eq!(d[0].code, None);
        assert_eq!(d[0].line, 1);
    }

    #[test]
    fn type_errors_carry_a_code_and_range() {
        let d = check_source("fn f(a: Int) -> Int { return a }\nf(\"x\")\n");
        let first = d.first().expect("a type diagnostic");
        assert!(first.code.as_deref().is_some_and(|c| c.starts_with('T')));
        assert_eq!(first.line, 2);
        assert!(first.end_column >= first.column);
    }

    #[test]
    fn format_is_idempotent() {
        let once = format_source("fn  f( a ){\nreturn a}\n");
        assert_eq!(format_source(&once), once);
    }
}
