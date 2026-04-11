use std::path::{Path, PathBuf};

pub fn format_files(files: &[PathBuf], check: bool) {
    let targets = if files.is_empty() {
        find_forge_files(".")
    } else {
        files.to_vec()
    };

    if targets.is_empty() {
        println!("No .fg files found");
        return;
    }

    let mut formatted = 0;
    let mut unformatted = 0;
    for path in &targets {
        match std::fs::read_to_string(path) {
            Ok(source) => {
                let result = format_source(&source);
                if result != source {
                    if check {
                        println!("  would format  {}", path.display());
                        unformatted += 1;
                    } else {
                        if let Err(e) = std::fs::write(path, &result) {
                            eprintln!("  error      {} — {}", path.display(), e);
                            continue;
                        }
                        println!("  formatted  {}", path.display());
                        formatted += 1;
                    }
                } else {
                    println!("  unchanged  {}", path.display());
                }
            }
            Err(e) => {
                eprintln!("  error      {} — {}", path.display(), e);
            }
        }
    }
    println!();
    if check {
        if unformatted > 0 {
            println!("  {} file(s) need formatting", unformatted);
            std::process::exit(1);
        } else {
            println!("  All files formatted correctly");
        }
    } else {
        println!("  {} file(s) formatted", formatted);
    }
}

fn find_forge_files(dir: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_fg_files(Path::new(dir), &mut files);
    files.sort();
    files
}

fn collect_fg_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if !name.starts_with('.') && name != "target" && name != "node_modules" {
                    collect_fg_files(&path, files);
                }
            } else if path.extension().is_some_and(|e| e == "fg") {
                files.push(path);
            }
        }
    }
}

/// Count leading close braces/brackets at the start of a line (before any other content).
fn count_leading_closes(line: &str) -> i32 {
    let mut count = 0i32;
    for c in line.chars() {
        match c {
            '}' | ']' | ')' => count += 1,
            ' ' | '\t' => continue,
            _ => break,
        }
    }
    count
}

/// Count braces in a line, ignoring those inside strings and comments.
fn count_braces(line: &str) -> (i32, i32) {
    let mut opens = 0i32;
    let mut closes = 0i32;
    let mut in_string = false;
    let mut string_char = '"';
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        // Handle line comments — stop counting
        if !in_string && c == '/' && chars.peek() == Some(&'/') {
            break;
        }

        // Handle string start/end
        if !in_string && (c == '"' || c == '\'') {
            in_string = true;
            string_char = c;
            continue;
        }
        if in_string {
            if c == '\\' {
                // Skip escaped character
                chars.next();
                continue;
            }
            if c == string_char {
                in_string = false;
            }
            continue;
        }

        // Count braces, brackets, and parens outside strings
        if c == '{' || c == '[' || c == '(' {
            opens += 1;
        } else if c == '}' || c == ']' || c == ')' {
            closes += 1;
        }
    }

    (opens, closes)
}

fn format_source(source: &str) -> String {
    let mut output = String::new();
    let mut indent_level: i32 = 0;
    let mut prev_blank = false;

    for line in source.lines() {
        let trimmed = line.trim();

        // Collapse multiple blank lines into one
        if trimmed.is_empty() {
            if !prev_blank {
                output.push('\n');
                prev_blank = true;
            }
            continue;
        }
        prev_blank = false;

        let (opens, closes) = count_braces(trimmed);
        let leading_closes = count_leading_closes(trimmed);

        // Decrease indent for leading close braces (before writing the line)
        indent_level -= leading_closes;
        if indent_level < 0 {
            indent_level = 0;
        }

        let indent = "    ".repeat(indent_level as usize);
        output.push_str(&indent);
        output.push_str(trimmed);
        output.push('\n');

        // Adjust indent for remaining braces (opens minus non-leading closes)
        let trailing_closes = closes - leading_closes;
        indent_level += opens - trailing_closes;
        if indent_level < 0 {
            indent_level = 0;
        }
    }

    // Ensure trailing newline
    if !output.ends_with('\n') {
        output.push('\n');
    }

    // Remove trailing blank lines
    while output.ends_with("\n\n") {
        output.pop();
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_basic_indentation() {
        let input = "fn greet(name) {\nprintln(name)\n}\n";
        let result = format_source(input);
        assert!(result.contains("    println(name)"));
    }

    #[test]
    fn preserves_correct_indentation() {
        let input = "let x = 42\nlet y = 10\n";
        let result = format_source(input);
        assert_eq!(result, input);
    }

    #[test]
    fn ignores_braces_in_strings() {
        let input = "let s = \"hello { world }\"\nsay s\n";
        let result = format_source(input);
        // Braces inside strings should NOT affect indentation
        assert_eq!(result, input);
    }

    #[test]
    fn ignores_braces_in_comments() {
        let input = "let x = 1 // this { brace\nsay x\n";
        let result = format_source(input);
        assert_eq!(result, input);
    }

    #[test]
    fn handles_else_blocks() {
        let input = "if true {\nsay \"yes\"\n} else {\nsay \"no\"\n}\n";
        let result = format_source(input);
        assert!(result.contains("} else {"));
        assert!(result.contains("    say \"yes\""));
        assert!(result.contains("    say \"no\""));
    }

    #[test]
    fn strips_trailing_whitespace() {
        let input = "let x = 42   \nlet y = 10  \n";
        let result = format_source(input);
        assert_eq!(result, "let x = 42\nlet y = 10\n");
    }

    #[test]
    fn collapses_multiple_blank_lines() {
        let input = "let x = 1\n\n\n\nlet y = 2\n";
        let result = format_source(input);
        assert_eq!(result, "let x = 1\n\nlet y = 2\n");
    }

    #[test]
    fn handles_nested_braces() {
        let input = "fn outer() {\nif true {\nsay \"nested\"\n}\n}\n";
        let result = format_source(input);
        assert!(result.contains("    if true {"));
        assert!(result.contains("        say \"nested\""));
        assert!(result.contains("    }"));
    }

    #[test]
    fn count_braces_ignores_strings() {
        assert_eq!(count_braces("let x = \"{\""), (0, 0));
        assert_eq!(count_braces("if true {"), (1, 0));
        assert_eq!(count_braces("}"), (0, 1));
        assert_eq!(count_braces("} else {"), (1, 1));
        assert_eq!(count_braces("let s = \"} else {\""), (0, 0));
    }

    #[test]
    fn count_braces_includes_brackets() {
        assert_eq!(count_braces("let a = ["), (1, 0));
        assert_eq!(count_braces("]"), (0, 1));
        assert_eq!(count_braces("[{"), (2, 0));
        assert_eq!(count_braces("}]"), (0, 2));
    }

    #[test]
    fn handles_bracket_indentation() {
        let input = "let a = [\n1,\n2,\n3\n]\n";
        let result = format_source(input);
        assert_eq!(result, "let a = [\n    1,\n    2,\n    3\n]\n");
    }

    #[test]
    fn handles_paren_continuation() {
        let input = "let result = some_function(\narg1,\narg2,\narg3\n)\n";
        let result = format_source(input);
        assert_eq!(
            result,
            "let result = some_function(\n    arg1,\n    arg2,\n    arg3\n)\n"
        );
    }

    #[test]
    fn count_braces_includes_parens() {
        assert_eq!(count_braces("fn call("), (1, 0));
        assert_eq!(count_braces(")"), (0, 1));
        assert_eq!(count_braces("fn call(arg) {"), (2, 1));
    }
}
