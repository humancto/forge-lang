use crate::errors;
use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::parser::ast::*;
use crate::parser::Parser;
use std::path::Path;
use std::time::Instant;

pub fn run_tests(test_dir: &str) {
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

        let test_fns = find_test_functions(&program);
        if test_fns.is_empty() {
            continue;
        }

        println!("  \x1B[1m{}\x1B[0m", path_str);

        // Run the full program first to define all functions
        let mut interpreter = Interpreter::new();
        if let Err(e) = interpreter.run(&program) {
            eprintln!("    \x1B[31mERROR\x1B[0m  setup — {}", e.message);
            failed += 1;
            total += 1;
            continue;
        }

        for test_fn_name in &test_fns {
            total += 1;
            let start = Instant::now();

            let func = interpreter.env.get(test_fn_name).cloned();
            let result = match func {
                Some(f) => interpreter.call_function(f, vec![]),
                None => {
                    failed += 1;
                    println!(
                        "    \x1B[31mFAIL\x1B[0m  {} — function not found",
                        test_fn_name
                    );
                    continue;
                }
            };

            let duration = start.elapsed().as_millis();

            match result {
                Ok(_) => {
                    passed += 1;
                    println!(
                        "    \x1B[32mok\x1B[0m    {} \x1B[90m({}ms)\x1B[0m",
                        test_fn_name, duration
                    );
                }
                Err(e) => {
                    failed += 1;
                    println!(
                        "    \x1B[31mFAIL\x1B[0m  {} \x1B[90m({}ms)\x1B[0m",
                        test_fn_name, duration
                    );
                    println!("          {}", e.message);
                }
            }
        }
        println!();
    }

    println!(
        "  \x1B[1m{} passed, {} failed, {} total\x1B[0m",
        passed, failed, total
    );
    println!();

    if failed > 0 {
        std::process::exit(1);
    }
}

fn find_test_functions(program: &Program) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in &program.statements {
        if let Stmt::FnDef {
            name, decorators, ..
        } = stmt
        {
            for dec in decorators {
                if dec.name == "test" {
                    names.push(name.clone());
                    break;
                }
            }
        }
    }
    names
}
