use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::errors;
use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::parser::Parser;

pub async fn run_watch(file: &Path) {
    let path_str = file.display().to_string();

    if !file.exists() {
        eprintln!(
            "{}",
            errors::format_simple_error(&format!("file not found: {}", path_str))
        );
        std::process::exit(1);
    }

    println!();
    println!("  \x1B[1;36m👁 Watching\x1B[0m {}", path_str);
    println!("  \x1B[90mPress Ctrl+C to stop\x1B[0m");
    println!();

    let mut last_modified = get_mtime(file);
    run_file(file, &path_str);

    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let current_mtime = get_mtime(file);
        if current_mtime != last_modified {
            last_modified = current_mtime;
            println!("\x1B[2J\x1B[H");
            println!("  \x1B[1;36m↻\x1B[0m File changed, re-running...");
            println!();
            run_file(file, &path_str);
        }
    }
}

fn get_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

fn run_file(file: &Path, path_str: &str) {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_simple_error(&format!("could not read '{}': {}", path_str, e))
            );
            return;
        }
    };

    let mut lexer = Lexer::new(&source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_error(&source, e.line, e.col, &e.message)
            );
            return;
        }
    };

    let mut parser = Parser::new(tokens);
    let program = match parser.parse_program() {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_error(&source, e.line, e.col, &e.message)
            );
            return;
        }
    };

    let mut interpreter = Interpreter::new();
    match interpreter.run(&program) {
        Ok(_) => {
            println!();
            println!(
                "  \x1B[32m✓\x1B[0m Completed at {}",
                chrono::Local::now().format("%H:%M:%S")
            );
        }
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e.message));
        }
    }
}
