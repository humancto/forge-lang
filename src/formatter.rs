use std::path::{Path, PathBuf};

pub fn format_files(files: &[PathBuf]) {
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
    for path in &targets {
        match std::fs::read_to_string(path) {
            Ok(source) => {
                let result = format_source(&source);
                if result != source {
                    if let Err(e) = std::fs::write(path, &result) {
                        eprintln!("  error      {} — {}", path.display(), e);
                        continue;
                    }
                    println!("  formatted  {}", path.display());
                    formatted += 1;
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
    println!("  {} file(s) formatted", formatted);
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

fn format_source(source: &str) -> String {
    let mut output = String::new();
    let mut indent_level: i32 = 0;
    let mut prev_blank = false;

    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if !prev_blank {
                output.push('\n');
                prev_blank = true;
            }
            continue;
        }
        prev_blank = false;

        // Decrease indent before lines that start with closing brace
        if trimmed.starts_with('}') || trimmed.starts_with("] ") {
            indent_level -= 1;
            if indent_level < 0 {
                indent_level = 0;
            }
        }

        // Also decrease for lines that are just else/otherwise/nah
        let is_else_line = trimmed.starts_with("} else")
            || trimmed.starts_with("} otherwise")
            || trimmed.starts_with("} nah");

        if is_else_line {
            // These are on the same indent as the if
        } else {
            let indent = "    ".repeat(indent_level as usize);
            output.push_str(&indent);
        }

        if is_else_line {
            let indent = "    ".repeat(indent_level as usize);
            output.push_str(&indent);
        }

        output.push_str(trimmed);
        output.push('\n');

        // Increase indent after lines that end with opening brace
        let opens = trimmed.chars().filter(|c| *c == '{').count();
        let closes = trimmed.chars().filter(|c| *c == '}').count();
        indent_level += opens as i32 - closes as i32;
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
}
