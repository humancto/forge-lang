/// Forge Error Formatting
/// Beautiful, source-mapped error output powered by ariadne.
use ariadne::{Color, Label, Report, ReportKind, Source};

pub fn format_error(source: &str, line: usize, col: usize, message: &str) -> String {
    let mut buf = Vec::new();

    let offset = line_col_to_offset(source, line, col);

    Report::build(ReportKind::Error, "<source>", offset)
        .with_message(message)
        .with_label(
            Label::new(("<source>", offset..offset + 1))
                .with_message(message)
                .with_color(Color::Red),
        )
        .finish()
        .write(("<source>", Source::from(source)), &mut buf)
        .ok();

    String::from_utf8(buf).unwrap_or_else(|_| format!("error: {}", message))
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

/// Format a simple error without source context
pub fn format_simple_error(message: &str) -> String {
    format!("\x1B[1;31merror\x1B[0m: {}", message)
}

/// Format a warning
#[allow(dead_code)]
pub fn format_warning(message: &str) -> String {
    format!("\x1B[1;33mwarning\x1B[0m: {}", message)
}

/// Format a success message
#[allow(dead_code)]
pub fn format_success(message: &str) -> String {
    format!("\x1B[1;32m{}\x1B[0m", message)
}
