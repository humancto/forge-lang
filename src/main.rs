mod chat;
mod doc;
mod errors;
mod formatter;
mod interpreter;
/// Forge — Internet-Native Programming Language
/// Go's simplicity. Rust's safety. The internet built in.
mod learn;
mod lexer;
mod lsp;
mod manifest;
mod native;
mod package;
mod parser;
mod repl;
mod runtime;
mod scaffold;
mod stdlib;
mod testing;
mod typechecker;
mod vm;
mod watch;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use interpreter::Interpreter;
use lexer::Lexer;
use parser::ast::{Expr, Program, Stmt};
use parser::Parser as ForgeParser;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
enum FrontendError {
    Lex {
        line: usize,
        col: usize,
        message: String,
    },
    Parse {
        line: usize,
        col: usize,
        message: String,
    },
    Type(Vec<typechecker::TypeWarning>),
}

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

    /// Profile function calls (uses VM, prints report after execution)
    #[arg(long = "profile")]
    profile: bool,

    /// Enforce type annotations as errors (gradual strict mode)
    #[arg(long = "strict")]
    strict: bool,
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
        /// Check formatting without writing (exit code 1 if unformatted)
        #[arg(long)]
        check: bool,
    },
    /// Run tests in the tests/ directory
    Test {
        /// Test directory (defaults to "tests")
        #[arg(default_value = "tests")]
        dir: String,
        /// Filter tests by name pattern
        #[arg(long)]
        filter: Option<String>,
    },
    /// Create a new Forge project
    New {
        /// Project name
        name: String,
    },
    /// Compile Forge source to bytecode
    Build {
        /// Emit a native launcher executable that shells into the Forge runtime
        #[arg(long)]
        native: bool,
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
    /// Watch a file and re-run on changes
    Watch {
        /// Path to a .fg file
        file: PathBuf,
    },
    /// Generate documentation from source files
    Doc {
        /// Files or directories (defaults to current directory)
        paths: Vec<PathBuf>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let use_vm = cli.use_vm || cli.use_jit || cli.profile;
    let use_jit = cli.use_jit;
    let profile = cli.profile;
    let strict = cli.strict;

    if let Some(code) = cli.eval_code {
        let code = code.replace(';', "\n");
        if use_jit && !profile {
            run_jit(&code, "<eval>", strict);
            return;
        }
        run_source(&code, "<eval>", use_vm, profile, strict).await;
        return;
    }

    match cli.command {
        Some(Command::Run { file }) => {
            if file.extension().map(|e| e == "fgc").unwrap_or(false) {
                run_bytecode_file(&file, profile);
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
            if use_jit && !profile {
                run_jit(&source, &path_str, strict);
                return;
            }
            run_source(&source, &path_str, use_vm, profile, strict).await;
        }
        Some(Command::Repl) => {
            repl::run_repl();
        }
        Some(Command::Version) => {
            println!("Forge v{}", VERSION);
            println!("Internet-native programming language");
            println!("Bytecode VM with mark-sweep GC");
        }
        Some(Command::Fmt { files, check }) => {
            formatter::format_files(&files, check);
        }
        Some(Command::Test { dir, filter }) => {
            let test_dir = if dir == "tests" {
                if let Some(m) = manifest::load_manifest() {
                    m.test.directory
                } else {
                    dir
                }
            } else {
                dir
            };
            testing::run_tests(&test_dir, filter.as_deref());
        }
        Some(Command::New { name }) => {
            scaffold::create_project(&name);
        }
        Some(Command::Build { file, native }) => {
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
            if native {
                compile_to_native_launcher(&source, &path_str, &file, strict);
            } else {
                compile_to_bytecode(&source, &path_str, &file, strict);
            }
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
        Some(Command::Watch { file }) => {
            watch::run_watch(&file).await;
        }
        Some(Command::Doc { paths }) => {
            doc::generate_docs(&paths);
        }
        None => {
            repl::run_repl();
        }
    }
}

fn prepare_program(
    source: &str,
    strict: bool,
) -> Result<(Program, Vec<typechecker::TypeWarning>), FrontendError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().map_err(|e| FrontendError::Lex {
        line: e.line,
        col: e.col,
        message: e.message,
    })?;

    let mut parser = ForgeParser::new(tokens);
    let program = parser.parse_program().map_err(|e| FrontendError::Parse {
        line: e.line,
        col: e.col,
        message: e.message,
    })?;

    let mut checker = typechecker::TypeChecker::with_strict(strict);
    let warnings = checker.check(&program);
    if warnings.iter().any(|w| w.is_error) {
        return Err(FrontendError::Type(warnings));
    }

    Ok((program, warnings))
}

fn print_frontend_error(source: &str, filename: &str, err: FrontendError) -> ! {
    match err {
        FrontendError::Lex { line, col, message } | FrontendError::Parse { line, col, message } => {
            eprintln!(
                "{}",
                errors::format_error(source, line, col, &format!("[{}] {}", filename, message))
            );
        }
        FrontendError::Type(warnings) => {
            for warning in warnings {
                let rendered = format!("[{}] {}", filename, warning.message);
                if warning.is_error {
                    eprintln!("{}", errors::format_simple_error(&rendered));
                } else {
                    eprintln!("{}", errors::format_warning(&rendered));
                }
            }
        }
    }
    process::exit(1);
}

fn emit_type_warnings(warnings: &[typechecker::TypeWarning]) {
    for warning in warnings {
        if !warning.is_error {
            eprintln!("{}", errors::format_warning(&warning.message));
        }
    }
}

fn vm_builtin_import(path: &str) -> bool {
    matches!(
        path,
        "math"
            | "fs"
            | "io"
            | "crypto"
            | "db"
            | "pg"
            | "env"
            | "json"
            | "regex"
            | "log"
            | "term"
            | "http"
            | "csv"
            | "exec"
            | "time"
    )
}

fn collect_vm_incompatible_stmt(stmt: &Stmt, issues: &mut BTreeSet<&'static str>) {
    match stmt {
        Stmt::TypeDef { .. } => {
            issues.insert("type definitions");
        }
        Stmt::InterfaceDef { .. } => {
            issues.insert("interface/power definitions");
        }
        Stmt::ImplBlock { methods, .. } => {
            issues.insert("impl/give blocks");
            for method in methods {
                collect_vm_incompatible_stmt(method, issues);
            }
        }
        Stmt::Destructure { pattern: _, value } => {
            collect_vm_incompatible_expr(value, issues);
        }
        Stmt::TryCatch {
            try_body,
            catch_body,
            ..
        } => {
            for stmt in try_body {
                collect_vm_incompatible_stmt(stmt, issues);
            }
            for stmt in catch_body {
                collect_vm_incompatible_stmt(stmt, issues);
            }
        }
        Stmt::SafeBlock { body } => {
            issues.insert("safe blocks");
            for stmt in body {
                collect_vm_incompatible_stmt(stmt, issues);
            }
        }
        Stmt::TimeoutBlock { body, .. } => {
            issues.insert("timeout blocks");
            for stmt in body {
                collect_vm_incompatible_stmt(stmt, issues);
            }
        }
        Stmt::RetryBlock { body, .. } => {
            issues.insert("retry blocks");
            for stmt in body {
                collect_vm_incompatible_stmt(stmt, issues);
            }
        }
        Stmt::ScheduleBlock { body, .. } => {
            issues.insert("schedule blocks");
            for stmt in body {
                collect_vm_incompatible_stmt(stmt, issues);
            }
        }
        Stmt::WatchBlock { body, .. } => {
            issues.insert("watch blocks");
            for stmt in body {
                collect_vm_incompatible_stmt(stmt, issues);
            }
        }
        Stmt::PromptDef { .. } => {
            issues.insert("prompt definitions");
        }
        Stmt::AgentDef { .. } => {
            issues.insert("agent definitions");
        }
        Stmt::DecoratorStmt(_) => {
            issues.insert("decorator-driven runtime features");
        }
        Stmt::Import { path, .. } => {
            if !vm_builtin_import(path) {
                issues.insert("file/package imports");
            }
        }
        Stmt::FnDef {
            body, decorators, ..
        } => {
            if !decorators.is_empty() {
                issues.insert("decorator-driven runtime features");
            }
            for stmt in body {
                collect_vm_incompatible_stmt(stmt, issues);
            }
        }
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_vm_incompatible_stmt(stmt, issues);
            }
            if let Some(else_body) = else_body {
                for stmt in else_body {
                    collect_vm_incompatible_stmt(stmt, issues);
                }
            }
        }
        Stmt::Match { arms, .. } => {
            for arm in arms {
                for stmt in &arm.body {
                    collect_vm_incompatible_stmt(stmt, issues);
                }
            }
        }
        Stmt::For { body, .. }
        | Stmt::While { body, .. }
        | Stmt::Loop { body }
        | Stmt::Spawn { body } => {
            for stmt in body {
                collect_vm_incompatible_stmt(stmt, issues);
            }
        }
        Stmt::Let { value, .. } | Stmt::Expression(value) | Stmt::YieldStmt(value) => {
            collect_vm_incompatible_expr(value, issues)
        }
        Stmt::Assign { target, value } => {
            collect_vm_incompatible_expr(target, issues);
            collect_vm_incompatible_expr(value, issues);
        }
        Stmt::Return(Some(expr)) | Stmt::CheckStmt { expr, .. } => {
            collect_vm_incompatible_expr(expr, issues)
        }
        Stmt::When { subject, arms } => {
            collect_vm_incompatible_expr(subject, issues);
            for arm in arms {
                if let Some(value) = &arm.value {
                    collect_vm_incompatible_expr(value, issues);
                }
                collect_vm_incompatible_expr(&arm.result, issues);
            }
        }
        Stmt::Return(None) | Stmt::Break | Stmt::Continue | Stmt::StructDef { .. } => {}
    }
}

fn collect_vm_incompatible_expr(expr: &Expr, issues: &mut BTreeSet<&'static str>) {
    match expr {
        Expr::BinOp { left, right, .. } => {
            collect_vm_incompatible_expr(left, issues);
            collect_vm_incompatible_expr(right, issues);
        }
        Expr::UnaryOp { operand, .. } | Expr::Try(operand) => {
            collect_vm_incompatible_expr(operand, issues)
        }
        Expr::FieldAccess { object, .. } => collect_vm_incompatible_expr(object, issues),
        Expr::Index { object, index } => {
            collect_vm_incompatible_expr(object, issues);
            collect_vm_incompatible_expr(index, issues);
        }
        Expr::Call { function, args } => {
            collect_vm_incompatible_expr(function, issues);
            for arg in args {
                collect_vm_incompatible_expr(arg, issues);
            }
        }
        Expr::Pipeline { value, function } => {
            collect_vm_incompatible_expr(value, issues);
            collect_vm_incompatible_expr(function, issues);
        }
        Expr::Lambda { body, .. } | Expr::Block(body) => {
            for stmt in body {
                collect_vm_incompatible_stmt(stmt, issues);
            }
        }
        Expr::Object(fields) | Expr::StructInit { fields, .. } => {
            for (_, value) in fields {
                collect_vm_incompatible_expr(value, issues);
            }
        }
        Expr::Array(items) => {
            for item in items {
                collect_vm_incompatible_expr(item, issues);
            }
        }
        Expr::StringInterp(parts) => {
            for part in parts {
                if let parser::ast::StringPart::Expr(expr) = part {
                    collect_vm_incompatible_expr(expr, issues);
                }
            }
        }
        Expr::MethodCall { object, args, .. } => {
            collect_vm_incompatible_expr(object, issues);
            for arg in args {
                collect_vm_incompatible_expr(arg, issues);
            }
        }
        Expr::WhereFilter { source, value, .. } => {
            issues.insert("where filters");
            collect_vm_incompatible_expr(source, issues);
            collect_vm_incompatible_expr(value, issues);
        }
        Expr::PipeChain { source, steps } => {
            issues.insert("pipe chains");
            collect_vm_incompatible_expr(source, issues);
            for step in steps {
                match step {
                    parser::ast::PipeStep::Keep(expr)
                    | parser::ast::PipeStep::Take(expr)
                    | parser::ast::PipeStep::Apply(expr) => {
                        collect_vm_incompatible_expr(expr, issues);
                    }
                    parser::ast::PipeStep::Sort(_) => {}
                }
            }
        }
        Expr::Await(expr)
        | Expr::Freeze(expr)
        | Expr::Spread(expr)
        | Expr::Must(expr)
        | Expr::Ask(expr) => collect_vm_incompatible_expr(expr, issues),
        Expr::Spawn(body) => {
            for stmt in body {
                collect_vm_incompatible_stmt(stmt, issues);
            }
        }
        Expr::Int(_) | Expr::Float(_) | Expr::StringLit(_) | Expr::Bool(_) | Expr::Ident(_) => {}
    }
}

fn vm_incompatibilities(program: &Program) -> Vec<&'static str> {
    let mut issues = BTreeSet::new();
    for stmt in &program.statements {
        collect_vm_incompatible_stmt(&stmt.stmt, &mut issues);
    }
    issues.into_iter().collect()
}

fn ensure_vm_compatible(program: &Program, mode: &str) -> Result<(), String> {
    let issues = vm_incompatibilities(program);
    if issues.is_empty() {
        return Ok(());
    }

    Err(format!(
        "{} mode does not support this program yet. Unsupported constructs: {}.\n  hint: run without {} for full language support",
        mode,
        issues.join(", "),
        mode
    ))
}

async fn run_source(source: &str, filename: &str, use_vm: bool, profile: bool, strict: bool) {
    let (program, warnings) = match prepare_program(source, strict) {
        Ok(prepared) => prepared,
        Err(err) => print_frontend_error(source, filename, err),
    };
    emit_type_warnings(&warnings);

    if use_vm {
        if let Err(message) = ensure_vm_compatible(&program, "--vm") {
            eprintln!("{}", errors::format_simple_error(&message));
            process::exit(1);
        }
        if profile {
            match vm::run_with_profiling(&program) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("{}", errors::format_simple_error(&e.message));
                    process::exit(1);
                }
            }
        } else {
            match vm::run(&program) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("{}", errors::format_simple_error(&e.message));
                    process::exit(1);
                }
            }
        }
    } else {
        let mut interpreter = Interpreter::new();
        interpreter.source = Some(source.to_string());
        match interpreter.run(&program) {
            Ok(_) => {}
            Err(e) => {
                if e.line > 0 {
                    eprintln!(
                        "{}",
                        errors::format_error(
                            source,
                            e.line,
                            1,
                            &format!("[{}] {}", filename, e.message)
                        )
                    );
                } else {
                    eprintln!(
                        "{}",
                        errors::format_simple_error(&format!("[{}] {}", filename, e.message))
                    );
                }
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

fn run_jit(source: &str, filename: &str, strict: bool) {
    let (program, warnings) = match prepare_program(source, strict) {
        Ok(prepared) => prepared,
        Err(err) => print_frontend_error(source, filename, err),
    };
    emit_type_warnings(&warnings);
    if let Err(message) = ensure_vm_compatible(&program, "--jit") {
        eprintln!("{}", errors::format_simple_error(&message));
        process::exit(1);
    }

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
            Ok(_ptr) => {
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
            let type_info = vm::jit::type_analysis::analyze(proto);
            vm.jit_cache.insert(
                name,
                vm::machine::JitEntry {
                    ptr,
                    uses_float: type_info.has_float,
                },
            );
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

fn compile_to_bytecode(source: &str, filename: &str, file_path: &PathBuf, strict: bool) {
    let (program, warnings) = match prepare_program(source, strict) {
        Ok(prepared) => prepared,
        Err(err) => print_frontend_error(source, filename, err),
    };
    emit_type_warnings(&warnings);
    if let Err(message) = ensure_vm_compatible(&program, "bytecode build") {
        eprintln!("{}", errors::format_simple_error(&message));
        process::exit(1);
    }

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

fn compile_to_native_launcher(source: &str, filename: &str, file_path: &PathBuf, strict: bool) {
    let (_, warnings) = match prepare_program(source, strict) {
        Ok(prepared) => prepared,
        Err(err) => print_frontend_error(source, filename, err),
    };
    emit_type_warnings(&warnings);

    match native::build_native_launcher(source, file_path) {
        Ok(output_path) => {
            println!(
                "Built native launcher {} -> {}\n  runtime: Forge interpreter/VM required at execution time",
                filename,
                output_path.display()
            );
        }
        Err(message) => {
            eprintln!("{}", errors::format_simple_error(&message));
            process::exit(1);
        }
    }
}

fn run_bytecode_file(file_path: &PathBuf, profile: bool) {
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

    let mut vm = if profile {
        vm::machine::VM::with_profiling()
    } else {
        vm::machine::VM::new()
    };
    match vm.execute(&chunk) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    }
    if profile {
        vm.profiler.print_report();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parity_corpus_supported_cases() {
        let cases = crate::testing::parity::load_supported_cases();
        assert!(!cases.is_empty(), "expected supported parity fixtures");
        for case in &cases {
            crate::testing::parity::assert_supported_case(case);
        }
    }

    #[test]
    fn parity_corpus_vm_rejection_cases() {
        let cases = crate::testing::parity::load_vm_rejection_cases();
        assert!(!cases.is_empty(), "expected VM rejection parity fixtures");

        for case in &cases {
            let (program, _) = prepare_program(&case.source, false)
                .unwrap_or_else(|err| panic!("{} should parse: {:?}", case.path.display(), err));
            let error = ensure_vm_compatible(&program, "parity corpus")
                .expect_err(&format!("{} should be rejected by VM", case.path.display()));
            assert!(
                error.contains(&case.expected_error),
                "{} rejection mismatch: expected substring '{}', got '{}'",
                case.path.display(),
                case.expected_error,
                error
            );
        }
    }

    #[test]
    fn prepare_program_rejects_strict_type_errors() {
        let source = r#"
        fn needs_int(x: Int) { return x }
        needs_int("oops")
        "#;

        match prepare_program(source, true) {
            Err(FrontendError::Type(warnings)) => {
                assert!(warnings.iter().any(|w| w.is_error));
                assert!(warnings.iter().any(|w| w.message.contains("expected Int")));
            }
            other => panic!("expected type error, got {:?}", other),
        }
    }

    #[test]
    fn prepare_program_keeps_non_strict_warnings_non_fatal() {
        let source = r#"
        fn needs_int(x: Int) { return x }
        needs_int("oops")
        "#;

        let (_, warnings) = prepare_program(source, false).expect("program should prepare");
        assert!(warnings.iter().any(|w| !w.is_error));
        assert!(warnings.iter().any(|w| w.message.contains("expected Int")));
    }

    #[test]
    fn vm_incompatibilities_detect_interface_and_impl_blocks() {
        let source = r#"
        thing Robot { id: Int }
        power Speakable { fn speak() -> String }
        give Robot {
            fn speak(it) { return "beep" }
        }
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        let issues = vm_incompatibilities(&program);
        assert!(issues.contains(&"interface/power definitions"));
        assert!(issues.contains(&"impl/give blocks"));
    }

    #[test]
    fn vm_incompatibilities_ignore_basic_programs() {
        let source = r#"
        fn add(a, b) { return a + b }
        let sum = add(20, 22)
        println(sum)
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_object_destructuring() {
        let source = r#"
        let user = { name: "Forge", age: 4 }
        unpack { name, age } from user
        name
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_try_catch() {
        let source = r#"
        let status = "ok"
        try {
            let crash = 1 / 0
        } catch err {
            status = err.type
        }
        status
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_array_rest_destructuring() {
        let source = r#"
        let items = [1, 2, 3]
        unpack [first, ...rest] from items
        first
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }
}
