use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::parser::Parser;
/// Forge REPL — Interactive Shell
/// Read-Eval-Print Loop for Forge.
use std::io::{self, BufRead, Write};

const BANNER: &str = r#"
  ╔═══════════════════════════════════════╗
  ║   FORGE v0.1.0 — Internet-Native     ║
  ║   Type 'help' for commands            ║
  ║   Type 'exit' or Ctrl+D to quit      ║
  ╚═══════════════════════════════════════╝
"#;

pub fn run_repl() {
    println!("{}", BANNER);

    let mut interpreter = Interpreter::new();
    let stdin = io::stdin();
    let mut input_buffer = String::new();
    let mut multiline = false;
    let mut brace_depth: i32 = 0;

    loop {
        // Prompt
        if multiline {
            print!("  ... ");
        } else {
            print!("forge> ");
        }
        io::stdout().flush().unwrap();

        // Read line
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!("\nGoodbye!");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                continue;
            }
        }

        let trimmed = line.trim();

        // Commands
        if !multiline {
            match trimmed {
                "exit" | "quit" => {
                    println!("Goodbye!");
                    break;
                }
                "help" => {
                    print_help();
                    continue;
                }
                "clear" => {
                    // Clear screen (ANSI escape)
                    print!("\x1B[2J\x1B[1;1H");
                    continue;
                }
                "env" => {
                    println!("[debug] Environment state:");
                    // Print all defined variables
                    if let Some(val) = interpreter.env.get("_last") {
                        println!("  _last = {}", val);
                    }
                    continue;
                }
                "" => continue,
                _ => {}
            }
        }

        // Track brace depth for multiline input
        for ch in trimmed.chars() {
            match ch {
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                _ => {}
            }
        }

        input_buffer.push_str(&line);

        if brace_depth > 0 {
            multiline = true;
            continue;
        }

        // Complete input — evaluate
        multiline = false;
        brace_depth = 0;
        let source = input_buffer.trim().to_string();
        input_buffer.clear();

        if source.is_empty() {
            continue;
        }

        // Lex
        let mut lexer = Lexer::new(&source);
        let tokens = match lexer.tokenize() {
            Ok(tokens) => tokens,
            Err(e) => {
                eprintln!("\x1B[31m{}\x1B[0m", e);
                continue;
            }
        };

        // Parse
        let mut parser = Parser::new(tokens);
        let program = match parser.parse_program() {
            Ok(prog) => prog,
            Err(e) => {
                eprintln!("\x1B[31m{}\x1B[0m", e);
                continue;
            }
        };

        // Evaluate
        match interpreter.run_repl(&program) {
            Ok(value) => {
                // Print result (unless it's Null from a statement)
                match &value {
                    crate::interpreter::Value::Null => {}
                    _ => println!("\x1B[32m=> {}\x1B[0m", value),
                }
                interpreter.env.define("_last".to_string(), value);
            }
            Err(e) => {
                eprintln!("\x1B[31m{}\x1B[0m", e);
            }
        }
    }
}

fn print_help() {
    println!(
        r#"
  Forge REPL Commands:
    help      Show this message
    clear     Clear the screen
    env       Show environment state
    exit      Quit the REPL

  Forge Basics:
    let x = 42              Variable binding
    let name = "Forge"      String with interpolation
    fn add(a, b) {{ a + b }}  Function definition
    println("Hello!")       Print to console
    [1, 2, 3]              Array literal
    {{ key: "value" }}        Object literal

  Coming Soon:
    fetch("https://...")    HTTP client
    @server(port: 8080)     HTTP server
    spawn {{ ... }}           Concurrency
"#
    );
}
