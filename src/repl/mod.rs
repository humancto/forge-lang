use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::parser::Parser;
/// Forge REPL — Interactive Shell
/// Read-Eval-Print Loop for Forge, powered by rustyline.
use rustyline::completion::{Completer, Pair};
use rustyline::{Config, Editor, Helper, Highlighter, Hinter, Validator};

const BANNER: &str = r#"
  ╔═══════════════════════════════════════╗
  ║   FORGE v0.3.0 — Internet-Native     ║
  ║   Type 'help' for commands            ║
  ║   Type 'exit' or Ctrl+D to quit      ║
  ╚═══════════════════════════════════════╝
"#;

const BUILTINS: &[&str] = &[
    "print",
    "println",
    "len",
    "type",
    "str",
    "int",
    "float",
    "push",
    "pop",
    "keys",
    "values",
    "contains",
    "range",
    "enumerate",
    "map",
    "filter",
    "Ok",
    "Err",
    "is_ok",
    "is_err",
    "unwrap",
    "unwrap_or",
    "json",
    "fetch",
    "time",
    "uuid",
    "say",
    "yell",
    "whisper",
    "wait",
];

const KEYWORDS: &[&str] = &[
    "let",
    "mut",
    "fn",
    "return",
    "if",
    "else",
    "match",
    "for",
    "in",
    "while",
    "loop",
    "break",
    "continue",
    "struct",
    "type",
    "interface",
    "impl",
    "pub",
    "import",
    "spawn",
    "true",
    "false",
    "set",
    "to",
    "change",
    "define",
    "otherwise",
    "nah",
    "each",
    "repeat",
    "times",
    "grab",
    "from",
    "wait",
    "seconds",
];

#[derive(Helper, Validator, Highlighter, Hinter)]
struct ForgeCompleter;

impl Completer for ForgeCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let start = line[..pos]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let prefix = &line[start..pos];
        if prefix.is_empty() {
            return Ok((pos, vec![]));
        }

        let mut candidates = Vec::new();
        for word in BUILTINS.iter().chain(KEYWORDS.iter()) {
            if word.starts_with(prefix) {
                candidates.push(Pair {
                    display: word.to_string(),
                    replacement: word.to_string(),
                });
            }
        }
        Ok((start, candidates))
    }
}

pub fn run_repl() {
    println!("{}", BANNER);

    let config = Config::builder().auto_add_history(true).build();
    let mut rl = match Editor::with_config(config) {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("Error: could not initialize REPL: {}", e);
            std::process::exit(1);
        }
    };
    rl.set_helper(Some(ForgeCompleter));

    let history_path = dirs_home().join(".forge_history");
    let _ = rl.load_history(&history_path);

    let mut interpreter = Interpreter::new();
    let mut input_buffer = String::new();
    let mut multiline = false;
    let mut brace_depth: i32 = 0;

    loop {
        let prompt = if multiline { "  ... " } else { "forge> " };

        match rl.readline(prompt) {
            Ok(line) => {
                let trimmed = line.trim();

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
                            print!("\x1B[2J\x1B[1;1H");
                            continue;
                        }
                        "env" => {
                            println!("[debug] Environment state:");
                            if let Some(val) = interpreter.env.get("_last") {
                                println!("  _last = {}", val);
                            }
                            continue;
                        }
                        "learn" => {
                            crate::learn::run_learn(None);
                            continue;
                        }
                        s if s.starts_with("learn ") => {
                            let num: Option<usize> = s[6..].trim().parse().ok();
                            crate::learn::run_learn(num);
                            continue;
                        }
                        "version" => {
                            println!("Forge v0.3.0");
                            continue;
                        }
                        "" => continue,
                        _ => {}
                    }
                }

                for ch in trimmed.chars() {
                    match ch {
                        '{' => brace_depth += 1,
                        '}' => brace_depth -= 1,
                        _ => {}
                    }
                }

                input_buffer.push_str(&line);
                input_buffer.push('\n');

                if brace_depth > 0 {
                    multiline = true;
                    continue;
                }

                multiline = false;
                brace_depth = 0;
                let source = input_buffer.trim().to_string();
                input_buffer.clear();

                if source.is_empty() {
                    continue;
                }

                let mut lexer = Lexer::new(&source);
                let tokens = match lexer.tokenize() {
                    Ok(tokens) => tokens,
                    Err(e) => {
                        eprintln!("\x1B[31m{}\x1B[0m", e);
                        continue;
                    }
                };

                let mut parser = Parser::new(tokens);
                let program = match parser.parse_program() {
                    Ok(prog) => prog,
                    Err(e) => {
                        eprintln!("\x1B[31m{}\x1B[0m", e);
                        continue;
                    }
                };

                match interpreter.run_repl(&program) {
                    Ok(value) => {
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
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("^C");
                input_buffer.clear();
                multiline = false;
                brace_depth = 0;
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                println!("\nGoodbye!");
                break;
            }
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                continue;
            }
        }
    }

    let _ = rl.save_history(&history_path);
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

fn print_help() {
    println!(
        r#"
  REPL Commands:
    help        Show this message
    learn       Interactive tutorials (30 lessons)
    learn <n>   Jump to lesson n
    version     Show version
    clear       Clear the screen
    env         Show environment state
    exit        Quit the REPL

  Quick Examples:
    say "Hello!"              Output
    yell "LOUD!"              Uppercase output
    set x to 42               Variable
    set mut y to 0             Mutable variable
    define add(a, b) {{ a + b }}  Function
    [1, 2, 3].sort()          Method chaining
    say term.emoji("fire")     Emoji
    say term.sparkline([1,4,2,8])  Sparkline chart
    say math.pi                Math
    say typeof(42)             Type checking
    say crypto.sha256("hi")    Crypto

  Modules: math, fs, io, crypto, db, env, json, regex, log, http, csv, term
"#
    );
}
