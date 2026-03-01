mod chat;
mod errors;
mod formatter;
mod interpreter;
/// Forge — Internet-Native Programming Language
/// Go's simplicity. Rust's safety. The internet built in.
mod learn;
mod lexer;
mod lsp;
mod manifest;
mod package;
mod parser;
mod repl;
mod runtime;
mod scaffold;
mod stdlib;
mod testing;
mod typechecker;
mod vm;

use std::fs;
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use interpreter::Interpreter;
use lexer::Lexer;
use parser::Parser as ForgeParser;

const VERSION: &str = "0.2.0";

#[derive(Parser)]
#[command(
    name = "forge",
    version = VERSION,
    about = "Forge — Internet-Native Programming Language",
    long_about = "Go's simplicity. Rust's safety. The internet built in."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Evaluate a Forge expression inline
    #[arg(short = 'e', long = "eval")]
    eval_code: Option<String>,

    /// Use the bytecode VM (experimental, faster but fewer features)
    #[arg(long = "vm")]
    use_vm: bool,

    /// Use JIT compilation for hot functions (requires --vm)
    #[arg(long = "jit")]
    use_jit: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Run a Forge source file (.fg) or compiled bytecode (.fgc)
    Run {
        /// Path to a .fg or .fgc file
        file: PathBuf,
    },
    /// Start the interactive REPL
    Repl,
    /// Show version information
    Version,
    /// Format Forge source files
    Fmt {
        /// Files to format (defaults to all .fg files in current directory)
        files: Vec<PathBuf>,
    },
    /// Run tests in the tests/ directory
    Test {
        /// Test directory (defaults to "tests")
        #[arg(default_value = "tests")]
        dir: String,
    },
    /// Create a new Forge project
    New {
        /// Project name
        name: String,
    },
    /// Compile Forge source to bytecode
    Build {
        /// Source file to compile
        file: PathBuf,
    },
    /// Install a Forge package from git URL or local path
    Install {
        /// Git URL or local path
        source: String,
    },
    /// Start the Language Server Protocol server
    Lsp,
    /// Interactive tutorials to learn Forge
    Learn {
        /// Lesson number (optional)
        lesson: Option<usize>,
    },
    /// Start an AI chat session
    Chat,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let use_vm = cli.use_vm || cli.use_jit;
    let use_jit = cli.use_jit;

    if let Some(code) = cli.eval_code {
        let code = code.replace(';', "\n");
        if use_jit {
            run_jit(&code, "<eval>");
            return;
        }
        run_source(&code, "<eval>", use_vm).await;
        return;
    }

    match cli.command {
        Some(Command::Run { file }) => {
            if file.extension().map(|e| e == "fgc").unwrap_or(false) {
                run_bytecode_file(&file);
                return;
            }
            let path_str = file.display().to_string();
            let source = match fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "{}",
                        errors::format_simple_error(&format!(
                            "could not read '{}': {}",
                            path_str, e
                        ))
                    );
                    process::exit(1);
                }
            };
            if use_jit {
                run_jit(&source, &path_str);
                return;
            }
            run_source(&source, &path_str, use_vm).await;
        }
        Some(Command::Repl) => {
            repl::run_repl();
        }
        Some(Command::Version) => {
            println!("Forge v{}", VERSION);
            println!("Internet-native programming language");
            println!("Bytecode VM with mark-sweep GC");
        }
        Some(Command::Fmt { files }) => {
            formatter::format_files(&files);
        }
        Some(Command::Test { dir }) => {
            let test_dir = if dir == "tests" {
                if let Some(m) = manifest::load_manifest() {
                    m.test.directory
                } else {
                    dir
                }
            } else {
                dir
            };
            testing::run_tests(&test_dir);
        }
        Some(Command::New { name }) => {
            scaffold::create_project(&name);
        }
        Some(Command::Build { file }) => {
            let path_str = file.display().to_string();
            let source = match fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "{}",
                        errors::format_simple_error(&format!(
                            "could not read '{}': {}",
                            path_str, e
                        ))
                    );
                    process::exit(1);
                }
            };
            compile_to_bytecode(&source, &path_str, &file);
        }
        Some(Command::Install { source }) => {
            package::install(&source);
        }
        Some(Command::Lsp) => {
            lsp::run_lsp();
        }
        Some(Command::Learn { lesson }) => {
            learn::run_learn(lesson);
        }
        Some(Command::Chat) => {
            chat::run_chat();
        }
        None => {
            repl::run_repl();
        }
    }
}

async fn run_source(source: &str, filename: &str, use_vm: bool) {
    let mut lexer = Lexer::new(source);
    let tokens = match lexer.tokenize() {
        Ok(tokens) => tokens,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_error(
                    source,
                    e.line,
                    e.col,
                    &format!("[{}] {}", filename, e.message)
                )
            );
            process::exit(1);
        }
    };

    let mut parser = ForgeParser::new(tokens);
    let program = match parser.parse_program() {
        Ok(prog) => prog,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_error(
                    source,
                    e.line,
                    e.col,
                    &format!("[{}] {}", filename, e.message)
                )
            );
            process::exit(1);
        }
    };

    let mut checker = typechecker::TypeChecker::new();
    let warnings = checker.check(&program);
    for w in &warnings {
        eprintln!("{}", errors::format_warning(&w.message));
    }

    if use_vm {
        match vm::run(&program) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("{}", errors::format_simple_error(&e.message));
                process::exit(1);
            }
        }
    } else {
        let mut interpreter = Interpreter::new();
        match interpreter.run(&program) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("{}", errors::format_simple_error(&e.message));
                process::exit(1);
            }
        }

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
}

fn run_jit(source: &str, _filename: &str) {
    let mut lexer = Lexer::new(source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_error(source, e.line, e.col, &e.message)
            );
            process::exit(1);
        }
    };

    let mut parser = ForgeParser::new(tokens);
    let program = match parser.parse_program() {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_error(source, e.line, e.col, &e.message)
            );
            process::exit(1);
        }
    };

    let chunk = match vm::compiler::compile(&program) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    };

    let mut jit = match vm::jit::jit_module::JitCompiler::new() {
        Ok(j) => j,
        Err(e) => {
            eprintln!("JIT init error: {}", e);
            process::exit(1);
        }
    };

    for (i, proto) in chunk.prototypes.iter().enumerate() {
        let name = if proto.name.is_empty() {
            format!("fn_{}", i)
        } else {
            proto.name.clone()
        };
        match jit.compile_function(proto, &name) {
            Ok(ptr) => {
                eprintln!(
                    "  JIT compiled: {} ({} instructions -> native)",
                    name,
                    proto.code.len()
                );
            }
            Err(e) => {
                eprintln!("  JIT skip: {} ({})", name, e);
            }
        }
    }

    let mut vm = vm::machine::VM::new();

    // Populate JIT cache so VM dispatches to native code
    for (i, proto) in chunk.prototypes.iter().enumerate() {
        let name = if proto.name.is_empty() {
            format!("fn_{}", i)
        } else {
            proto.name.clone()
        };
        if let Some(ptr) = jit.get_compiled(&name) {
            vm.jit_cache.insert(name, ptr);
        }
    }

    match vm.execute(&chunk) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    }
}

fn compile_to_bytecode(source: &str, filename: &str, file_path: &PathBuf) {
    let mut lexer = Lexer::new(source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_error(source, e.line, e.col, &e.message)
            );
            process::exit(1);
        }
    };

    let mut parser = ForgeParser::new(tokens);
    let program = match parser.parse_program() {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_error(source, e.line, e.col, &e.message)
            );
            process::exit(1);
        }
    };

    match vm::compiler::compile(&program) {
        Ok(chunk) => {
            let out_path = file_path.with_extension("fgc");
            let bytes = match vm::serialize::serialize_chunk(&chunk) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("{}", errors::format_simple_error(&e.message));
                    process::exit(1);
                }
            };
            if let Err(e) = fs::write(&out_path, &bytes) {
                eprintln!(
                    "{}",
                    errors::format_simple_error(&format!(
                        "could not write '{}': {}",
                        out_path.display(),
                        e
                    ))
                );
                process::exit(1);
            }
            println!(
                "Compiled {} -> {}\n  {} instructions\n  {} constants\n  {} prototypes\n  {} max registers\n  {} bytes",
                filename,
                out_path.display(),
                chunk.code.len(),
                chunk.constants.len(),
                chunk.prototypes.len(),
                chunk.max_registers,
                bytes.len(),
            );
        }
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    }
}

fn run_bytecode_file(file_path: &PathBuf) {
    let bytes = match fs::read(file_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_simple_error(&format!(
                    "could not read '{}': {}",
                    file_path.display(),
                    e
                ))
            );
            process::exit(1);
        }
    };

    let chunk = match vm::serialize::deserialize_chunk(&bytes) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_simple_error(&format!(
                    "invalid bytecode file '{}': {}",
                    file_path.display(),
                    e.message
                ))
            );
            process::exit(1);
        }
    };

    let mut vm = vm::machine::VM::new();
    match vm.execute(&chunk) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    }
}
