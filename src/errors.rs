/// Forge Error Formatting
/// Beautiful, source-mapped error output.
/// Phase 1: simple colored output. Phase 3: migrate to ariadne.

pub fn format_error(source: &str, line: usize, col: usize, message: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut output = String::new();

    // Error header
    output.push_str(&format!(
        "\x1B[1;31merror\x1B[0m\x1B[1m: {}\x1B[0m\n",
        message
    ));

    // Location
    output.push_str(&format!("  \x1B[1;34m-->\x1B[0m line {}:{}\n", line, col));

    // Source context
    let start = if line > 2 { line - 2 } else { 0 };
    let end = std::cmp::min(line + 1, lines.len());

    output.push_str("  \x1B[1;34m|\x1B[0m\n");

    for i in start..end {
        let line_num = i + 1;
        let prefix = if line_num == line {
            format!("\x1B[1;31m{:>4}\x1B[0m \x1B[1;34m|\x1B[0m ", line_num)
        } else {
            format!("\x1B[1;34m{:>4}\x1B[0m \x1B[1;34m|\x1B[0m ", line_num)
        };

        output.push_str(&prefix);
        output.push_str(lines.get(i).unwrap_or(&""));
        output.push('\n');

        // Underline the error position
        if line_num == line {
            let spaces = " ".repeat(col.saturating_sub(1));
            output.push_str(&format!(
                "     \x1B[1;34m|\x1B[0m \x1B[1;31m{}^\x1B[0m\n",
                spaces
            ));
        }
    }

    output.push_str("  \x1B[1;34m|\x1B[0m\n");

    output
}

/// Format a simple error without source context
pub fn format_simple_error(message: &str) -> String {
    format!("\x1B[1;31merror\x1B[0m: {}", message)
}

/// Format a warning
pub fn format_warning(message: &str) -> String {
    format!("\x1B[1;33mwarning\x1B[0m: {}", message)
}

/// Format a success message
pub fn format_success(message: &str) -> String {
    format!("\x1B[1;32m{}\x1B[0m", message)
}
