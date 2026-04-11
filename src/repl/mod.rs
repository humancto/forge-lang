use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::parser::Parser;
/// Forge REPL — Interactive Shell
/// Read-Eval-Print Loop for Forge, powered by rustyline.
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::{Config, Editor, Helper, Hinter, Validator};
use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn banner() -> String {
    format!(
        r#"
  ╔═══════════════════════════════════════╗
  ║   FORGE v{:<6}— Internet-Native     ║
  ║   Type 'help' for commands            ║
  ║   Type 'exit' or Ctrl+D to quit      ║
  ╚═══════════════════════════════════════╝
"#,
        VERSION
    )
}

const BUILTINS: &[&str] = &[
    "print",
    "println",
    "len",
    "type",
    "typeof",
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
    "reduce",
    "sort",
    "reverse",
    "find",
    "flat_map",
    "any",
    "all",
    "sample",
    "shuffle",
    "sum",
    "min_of",
    "max_of",
    "unique",
    "zip",
    "flatten",
    "group_by",
    "chunk",
    "slice",
    "partition",
    "has_key",
    "get",
    "pick",
    "omit",
    "merge",
    "entries",
    "from_entries",
    "diff",
    "split",
    "join",
    "replace",
    "starts_with",
    "ends_with",
    "substring",
    "index_of",
    "pad_start",
    "pad_end",
    "capitalize",
    "title",
    "slugify",
    "snake_case",
    "camel_case",
    "Ok",
    "Err",
    "is_ok",
    "is_err",
    "unwrap",
    "unwrap_or",
    "Some",
    "None",
    "is_some",
    "is_none",
    "json",
    "fetch",
    "time",
    "uuid",
    "say",
    "yell",
    "whisper",
    "wait",
    "assert",
    "assert_eq",
    "assert_ne",
    "assert_throws",
    "channel",
    "send",
    "receive",
    "sh",
    "shell",
    "sh_lines",
    "sh_json",
    "sh_ok",
    "which",
    "cwd",
    "exit",
    "input",
    "sus",
    "bruh",
    "bet",
    "no_cap",
    "ick",
    "cook",
    "yolo",
    "ghost",
    "slay",
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

const MODULES: &[&str] = &[
    "math", "fs", "io", "crypto", "db", "pg", "mysql", "env", "json", "regex", "log", "http",
    "csv", "term", "time", "jwt", "npc", "exec",
];

#[derive(Helper, Validator, Hinter)]
struct ForgeHelper {
    user_names: Arc<Mutex<Vec<String>>>,
}

impl Completer for ForgeHelper {
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
        let mut seen = HashSet::new();

        // Static builtins, keywords, modules
        for word in BUILTINS.iter().chain(KEYWORDS.iter()).chain(MODULES.iter()) {
            if word.starts_with(prefix) && seen.insert(word.to_string()) {
                candidates.push(Pair {
                    display: word.to_string(),
                    replacement: word.to_string(),
                });
            }
        }

        // User-defined names from the interpreter environment
        if let Ok(names) = self.user_names.lock() {
            for name in names.iter() {
                if name.starts_with(prefix) && seen.insert(name.clone()) {
                    candidates.push(Pair {
                        display: name.clone(),
                        replacement: name.clone(),
                    });
                }
            }
        }

        Ok((start, candidates))
    }
}

impl Highlighter for ForgeHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let mut result = String::with_capacity(line.len() * 2);
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            // String literals
            if chars[i] == '"' {
                result.push_str("\x1B[33m\""); // yellow
                i += 1;
                while i < len && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < len {
                        result.push(chars[i]);
                        result.push(chars[i + 1]);
                        i += 2;
                    } else {
                        result.push(chars[i]);
                        i += 1;
                    }
                }
                if i < len {
                    result.push('"');
                    i += 1;
                }
                result.push_str("\x1B[0m");
                continue;
            }

            // Single-quoted strings
            if chars[i] == '\'' {
                result.push_str("\x1B[33m'"); // yellow
                i += 1;
                while i < len && chars[i] != '\'' {
                    if chars[i] == '\\' && i + 1 < len {
                        result.push(chars[i]);
                        result.push(chars[i + 1]);
                        i += 2;
                    } else {
                        result.push(chars[i]);
                        i += 1;
                    }
                }
                if i < len {
                    result.push('\'');
                    i += 1;
                }
                result.push_str("\x1B[0m");
                continue;
            }

            // Numbers (allow at most one decimal point)
            if chars[i].is_ascii_digit() {
                result.push_str("\x1B[36m"); // cyan
                let mut has_dot = false;
                while i < len {
                    if chars[i].is_ascii_digit() {
                        result.push(chars[i]);
                        i += 1;
                    } else if chars[i] == '.'
                        && !has_dot
                        && i + 1 < len
                        && chars[i + 1].is_ascii_digit()
                    {
                        has_dot = true;
                        result.push(chars[i]);
                        i += 1;
                    } else {
                        break;
                    }
                }
                result.push_str("\x1B[0m");
                continue;
            }

            // Comments
            if chars[i] == '/' && i + 1 < len && chars[i + 1] == '/' {
                result.push_str("\x1B[90m"); // dim
                while i < len {
                    result.push(chars[i]);
                    i += 1;
                }
                result.push_str("\x1B[0m");
                continue;
            }

            // Identifiers and keywords
            if chars[i].is_alphabetic() || chars[i] == '_' {
                let start = i;
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                if word == "true" || word == "false" {
                    result.push_str("\x1B[36m"); // cyan for booleans
                    result.push_str(&word);
                    result.push_str("\x1B[0m");
                } else if KEYWORDS.contains(&word.as_str()) {
                    result.push_str("\x1B[35m"); // magenta for keywords
                    result.push_str(&word);
                    result.push_str("\x1B[0m");
                } else if BUILTINS.contains(&word.as_str()) {
                    result.push_str("\x1B[34m"); // blue for builtins
                    result.push_str(&word);
                    result.push_str("\x1B[0m");
                } else if MODULES.contains(&word.as_str()) {
                    result.push_str("\x1B[32m"); // green for modules
                    result.push_str(&word);
                    result.push_str("\x1B[0m");
                } else {
                    result.push_str(&word);
                }
                continue;
            }

            result.push(chars[i]);
            i += 1;
        }

        Cow::Owned(result)
    }

    fn highlight_char(
        &self,
        _line: &str,
        _pos: usize,
        _kind: rustyline::highlight::CmdKind,
    ) -> bool {
        true
    }
}

pub fn run_repl() {
    println!("{}", banner());

    let config = Config::builder().auto_add_history(true).build();
    let mut rl = match Editor::with_config(config) {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("Error: could not initialize REPL: {}", e);
            std::process::exit(1);
        }
    };

    let user_names: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    rl.set_helper(Some(ForgeHelper {
        user_names: Arc::clone(&user_names),
    }));

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
                            let names = interpreter.env.all_names();
                            if names.is_empty() {
                                println!("  (no variables defined)");
                            } else {
                                for name in &names {
                                    if let Some(val) = interpreter.env.get(name) {
                                        println!("  {} = {}", name, val);
                                    }
                                }
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
                            println!("Forge v{}", VERSION);
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

                // Update tab-completion with current environment names
                if let Ok(mut names) = user_names.lock() {
                    *names = interpreter.env.all_names();
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
    env         Show all defined variables
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

  Modules: math, fs, io, crypto, db, pg, mysql, env, json, regex, log, http, csv, term, jwt, exec, npc
"#
    );
}
