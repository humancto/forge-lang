mod errors;
mod interpreter;
/// Forge — Internet-Native Programming Language
/// Go's simplicity. Rust's safety. The internet built in.
mod lexer;
mod parser;
mod repl;
mod runtime;

use std::env;
use std::fs;
use std::process;

use interpreter::Interpreter;
use lexer::Lexer;
use parser::Parser;

const VERSION: &str = "0.1.0";

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        // No arguments — launch REPL
        repl::run_repl();
        return;
    }

    match args[1].as_str() {
        "run" => {
            if args.len() < 3 {
                eprintln!("Usage: forge run <file.fg>");
                process::exit(1);
            }
            run_file(&args[2]).await;
        }
        "repl" => {
            repl::run_repl();
        }
        "version" | "--version" | "-v" => {
            println!("Forge v{}", VERSION);
            println!("Internet-native programming language");
        }
        "help" | "--help" | "-h" => {
            print_usage();
        }
        // If the argument ends in .fg, treat it as a file
        arg if arg.ends_with(".fg") => {
            run_file(arg).await;
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_usage();
            process::exit(1);
        }
    }
}

async fn run_file(path: &str) {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_simple_error(&format!("could not read '{}': {}", path, e))
            );
            process::exit(1);
        }
    };

    // Lex
    let mut lexer = Lexer::new(&source);
    let tokens = match lexer.tokenize() {
        Ok(tokens) => tokens,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_error(&source, e.line, e.col, &e.message)
            );
            process::exit(1);
        }
    };

    // Parse
    let mut parser = Parser::new(tokens);
    let program = match parser.parse_program() {
        Ok(prog) => prog,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_error(&source, e.line, e.col, &e.message)
            );
            process::exit(1);
        }
    };

    // Execute
    let mut interpreter = Interpreter::new();
    match interpreter.run(&program) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    }

    // Check if program defines a server
    let server_config = runtime::server::extract_server_config(&program);
    let routes = runtime::server::extract_routes(&program);

    if let Some(config) = server_config {
        if routes.is_empty() {
            eprintln!(
                "{}",
                errors::format_simple_error(
                    "@server defined but no route handlers found. Add @get/@post functions."
                )
            );
            process::exit(1);
        }
        if let Err(e) = runtime::server::start_server(interpreter, &config, &routes).await {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    }
}

fn print_usage() {
    println!(
        r#"
Forge v{} — Internet-Native Programming Language

USAGE:
    forge                    Start the REPL
    forge run <file.fg>      Run a Forge program
    forge repl               Start the REPL (explicit)
    forge version            Show version info
    forge help               Show this message

EXAMPLES:
    forge run hello.fg       Run a Forge source file
    forge                    Enter interactive mode

COMING SOON:
    forge new <name>         Create a new Forge project
    forge build              Compile to bytecode
    forge test               Run tests
    forge fmt                Format source code
"#,
        VERSION
    );
}
// TEMP
