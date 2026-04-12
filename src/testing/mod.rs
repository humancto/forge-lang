use crate::errors;
use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::parser::ast::*;
use crate::parser::Parser;
use std::path::Path;
use std::time::Instant;

#[cfg(test)]
pub mod parity;

pub fn run_tests(test_dir: &str, filter: Option<&str>, coverage: bool) {
    let dir = Path::new(test_dir);
    if !dir.exists() {
        eprintln!(
            "{}",
            errors::format_simple_error(&format!(
                "test directory '{}' not found. Create it with test files.",
                test_dir
            ))
        );
        std::process::exit(1);
    }

    let mut total = 0;
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut coverage_data: Vec<(String, usize, usize)> = Vec::new();

    println!();

    let dir_entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_simple_error(&format!(
                    "could not read test directory '{}': {}",
                    test_dir, e
                ))
            );
            std::process::exit(1);
        }
    };
    let mut entries: Vec<_> = dir_entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "fg"))
        .collect();
    entries.sort_by_key(|e| e.path());

    if entries.is_empty() {
        println!("  No test files found in '{}'", test_dir);
        println!("  Create .fg files with @test functions");
        println!();
        return;
    }

    for entry in entries {
        let path = entry.path();
        let path_str = path.display().to_string();
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("  Could not read {}: {}", path_str, e);
                continue;
            }
        };

        let mut lexer = Lexer::new(&source);
        let tokens = match lexer.tokenize() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("  \x1B[31mERROR\x1B[0m  {} — {}", path_str, e);
                failed += 1;
                total += 1;
                continue;
            }
        };

        let mut parser = Parser::new(tokens);
        let program = match parser.parse_program() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("  \x1B[31mERROR\x1B[0m  {} — {}", path_str, e);
                failed += 1;
                total += 1;
                continue;
            }
        };

        let test_info = find_test_functions(&program);
        let before_fn = find_hook_function(&program, "before");
        let after_fn = find_hook_function(&program, "after");

        // Apply filter if specified
        let test_fns: Vec<&TestInfo> = test_info
            .iter()
            .filter(|t| {
                if let Some(pat) = filter {
                    t.name.contains(pat)
                } else {
                    true
                }
            })
            .collect();

        if test_fns.is_empty() {
            continue;
        }

        println!("  \x1B[1m{}\x1B[0m", path_str);

        // Run the full program first to define all functions
        let mut interpreter = Interpreter::new();
        if coverage {
            interpreter.coverage = Some(std::collections::HashSet::new());
        }
        if let Err(e) = interpreter.run(&program) {
            eprintln!("    \x1B[31mERROR\x1B[0m  setup — {}", e.message);
            failed += 1;
            total += 1;
            continue;
        }

        for test in &test_fns {
            total += 1;

            // Handle @skip
            if test.skip {
                skipped += 1;
                println!("    \x1B[33mSKIP\x1B[0m  {}", test.name);
                continue;
            }

            let start = Instant::now();

            // Run @before hook
            if let Some(ref before_name) = before_fn {
                if let Some(f) = interpreter.env.get(before_name) {
                    if let Err(e) = interpreter.call_function(f, vec![]) {
                        failed += 1;
                        println!(
                            "    \x1B[31mFAIL\x1B[0m  {} — @before hook failed: {}",
                            test.name, e.message
                        );
                        continue;
                    }
                }
            }

            let func = interpreter.env.get(&test.name);
            let result = match func {
                Some(f) => interpreter.call_function(f, vec![]),
                None => {
                    failed += 1;
                    println!(
                        "    \x1B[31mFAIL\x1B[0m  {} — function not found",
                        test.name
                    );
                    continue;
                }
            };

            // Run @after hook regardless of test result
            if let Some(ref after_name) = after_fn {
                if let Some(f) = interpreter.env.get(after_name) {
                    let _ = interpreter.call_function(f, vec![]);
                }
            }

            let duration = start.elapsed().as_millis();

            match result {
                Ok(_) => {
                    passed += 1;
                    println!(
                        "    \x1B[32mok\x1B[0m    {} \x1B[90m({}ms)\x1B[0m",
                        test.name, duration
                    );
                }
                Err(e) => {
                    failed += 1;
                    println!(
                        "    \x1B[31mFAIL\x1B[0m  {} \x1B[90m({}ms)\x1B[0m",
                        test.name, duration
                    );
                    println!("          {}", e.message);
                }
            }
        }
        if let Some(ref cov) = interpreter.coverage {
            let executable_set = executable_line_set(&source);
            let executed = cov.intersection(&executable_set).count();
            coverage_data.push((path_str.clone(), executable_set.len(), executed));
        }

        println!();
    }

    let skip_msg = if skipped > 0 {
        format!(", {} skipped", skipped)
    } else {
        String::new()
    };
    println!(
        "  \x1B[1m{} passed, {} failed{}, {} total\x1B[0m",
        passed, failed, skip_msg, total
    );
    println!();

    if coverage && !coverage_data.is_empty() {
        println!("  \x1B[1mCoverage\x1B[0m");
        println!();
        let mut total_executable = 0usize;
        let mut total_executed = 0usize;
        for (file, executable, executed) in &coverage_data {
            total_executable += executable;
            total_executed += executed;
            let pct = if *executable > 0 {
                *executed as f64 / *executable as f64 * 100.0
            } else {
                100.0
            };
            let color = if pct >= 80.0 {
                "\x1B[32m"
            } else if pct >= 50.0 {
                "\x1B[33m"
            } else {
                "\x1B[31m"
            };
            println!(
                "    {}{:5.1}%\x1B[0m  {} ({}/{})",
                color, pct, file, executed, executable
            );
        }
        let overall = if total_executable > 0 {
            total_executed as f64 / total_executable as f64 * 100.0
        } else {
            100.0
        };
        println!();
        let overall_color = if overall >= 80.0 {
            "\x1B[32m"
        } else if overall >= 50.0 {
            "\x1B[33m"
        } else {
            "\x1B[31m"
        };
        println!(
            "  {}Overall: {:.1}%\x1B[0m ({}/{})",
            overall_color, overall, total_executed, total_executable
        );
        println!();
    }

    if failed > 0 {
        std::process::exit(1);
    }
}

struct TestInfo {
    name: String,
    skip: bool,
}

fn find_test_functions(program: &Program) -> Vec<TestInfo> {
    let mut tests = Vec::new();
    for spanned in &program.statements {
        if let Stmt::FnDef {
            name, decorators, ..
        } = &spanned.stmt
        {
            let mut is_test = false;
            let mut is_skip = false;
            for dec in decorators {
                if dec.name == "test" {
                    is_test = true;
                }
                if dec.name == "skip" {
                    is_skip = true;
                }
            }
            if is_test {
                tests.push(TestInfo {
                    name: name.clone(),
                    skip: is_skip,
                });
            }
        }
    }
    tests
}

/// Build a set of 1-indexed line numbers considered executable.
/// Excludes blank lines, single-line comments, multi-line comment blocks, and lone braces.
fn executable_line_set(source: &str) -> std::collections::HashSet<usize> {
    let mut set = std::collections::HashSet::new();
    let mut in_block_comment = false;
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if in_block_comment {
            if let Some(pos) = trimmed.find("*/") {
                in_block_comment = false;
                // If there's code after the closing */, count this line
                let after = trimmed[pos + 2..].trim();
                if !after.is_empty() {
                    set.insert(i + 1);
                }
            }
            continue;
        }
        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed == "}" {
            continue;
        }
        set.insert(i + 1); // 1-indexed to match parser line numbers
    }
    set
}

fn find_hook_function(program: &Program, hook_name: &str) -> Option<String> {
    for spanned in &program.statements {
        if let Stmt::FnDef {
            name, decorators, ..
        } = &spanned.stmt
        {
            for dec in decorators {
                if dec.name == hook_name {
                    return Some(name.clone());
                }
            }
        }
    }
    None
}
