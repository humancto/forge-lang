mod cancellation;
mod chat;
mod dap;
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
mod permissions;
mod parser;
mod publish;
mod registry;
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

    /// Use the bytecode VM (this is now the default). Kept for backwards
    /// compatibility — has no effect since VM is already the default engine.
    #[arg(long = "vm")]
    use_vm: bool,

    /// Use the tree-walking interpreter instead of the VM. Required for
    /// decorator-driven HTTP servers (@server, @get, etc.). The VM
    /// auto-falls back to the interpreter when decorators are detected.
    #[arg(long = "interp")]
    use_interp: bool,

    /// JIT-compile numeric leaf functions via Cranelift on top of --vm.
    /// Only Int/Float arithmetic and comparisons are supported: any function
    /// that touches strings, arrays, objects, closures, or builtins falls
    /// back to the bytecode interpreter automatically. Best for tight math
    /// loops; for everything else --vm alone is usually enough.
    #[arg(long = "jit")]
    use_jit: bool,

    /// Profile function calls (uses VM, prints report after execution)
    #[arg(long = "profile")]
    profile: bool,

    /// Enforce type annotations as errors (gradual strict mode)
    #[arg(long = "strict")]
    strict: bool,

    /// Allow shell execution (sh, shell, run_command, sh_lines, sh_json, sh_ok, pipe_to).
    /// Without this flag, these builtins return a permission error.
    #[arg(long = "allow-run")]
    allow_run: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Run a Forge source file (.fg) or compiled bytecode (.fgc)
    Run {
        /// Path to a .fg or .fgc file (reads entry from forge.toml if omitted)
        file: Option<PathBuf>,
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
        /// Show line coverage report after tests
        #[arg(long)]
        coverage: bool,
    },
    /// Create a new Forge project
    New {
        /// Project name
        name: String,
    },
    /// Compile Forge source to bytecode
    Build {
        /// Emit a native launcher that embeds source and shells into the Forge runtime
        #[arg(long, conflicts_with = "aot")]
        native: bool,
        /// Compile to bytecode and embed in a native binary (no source exposure)
        #[arg(long, conflicts_with = "native")]
        aot: bool,
        /// Source file to compile
        file: PathBuf,
    },
    /// Install a Forge package from git URL or local path
    Install {
        /// Git URL or local path
        source: String,
    },
    /// Add a dependency to forge.toml and install it
    Add {
        /// Package name or name@version (e.g., "router" or "router@^1.0")
        package: String,
    },
    /// Update all dependencies to latest compatible versions
    Update,
    /// Publish the current project to the local registry
    Publish {
        /// Show what would be packaged without publishing
        #[arg(long)]
        dry_run: bool,
        /// Custom registry path (defaults to ~/.forge/registry/)
        #[arg(long)]
        registry: Option<String>,
    },
    /// Search the package registry
    Search {
        /// Search query (matches name and description)
        query: Option<String>,
    },
    /// Start the Language Server Protocol server
    Lsp,
    /// Start the Debug Adapter Protocol server
    Dap,
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
    let use_jit = cli.use_jit;
    #[cfg(not(feature = "jit"))]
    if use_jit {
        eprintln!("error: --jit requires the 'jit' feature (install with: cargo install forge-lang --features jit)");
        std::process::exit(1);
    }
    let use_vm = !cli.use_interp || cli.use_jit || cli.profile;
    let profile = cli.profile;
    let strict = cli.strict;
    // REPL and -e are user-invoked contexts — always allow shell execution.
    // For file execution (forge run), require explicit --allow-run.
    let is_interactive = cli.eval_code.is_some()
        || matches!(cli.command, Some(Command::Repl) | None);
    permissions::set_allow_run(cli.allow_run || is_interactive);

    if let Some(code) = cli.eval_code {
        let code = code.replace(';', "\n");
        #[cfg(feature = "jit")]
        if use_jit && !profile {
            run_jit(&code, "<eval>", strict);
            return;
        }
        run_source(&code, "<eval>", use_vm, profile, strict).await;
        return;
    }

    match cli.command {
        Some(Command::Run { file }) => {
            let file = match file {
                Some(f) => f,
                None => {
                    if let Some(m) = manifest::load_manifest() {
                        if m.project.entry.is_empty() {
                            eprintln!(
                                "{}",
                                errors::format_simple_error(
                                    "forge.toml found but no 'entry' field set. Add entry = \"src/main.fg\" to [project] or specify a file: forge run <file>"
                                )
                            );
                            process::exit(1);
                        }
                        PathBuf::from(&m.project.entry)
                    } else {
                        eprintln!(
                            "{}",
                            errors::format_simple_error(
                                "no file specified and no forge.toml found. Usage: forge run <file>"
                            )
                        );
                        process::exit(1);
                    }
                }
            };
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
            #[cfg(feature = "jit")]
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
        Some(Command::Test {
            dir,
            filter,
            coverage,
        }) => {
            let test_dir = if dir == "tests" {
                if let Some(m) = manifest::load_manifest() {
                    m.test.directory
                } else {
                    dir
                }
            } else {
                dir
            };
            testing::run_tests(&test_dir, filter.as_deref(), coverage);
        }
        Some(Command::New { name }) => {
            scaffold::create_project(&name);
        }
        Some(Command::Build { file, native, aot }) => {
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
            if aot {
                compile_to_native_aot(&source, &path_str, &file, strict);
            } else if native {
                compile_to_native_launcher(&source, &path_str, &file, strict);
            } else {
                compile_to_bytecode(&source, &path_str, &file, strict);
            }
        }
        Some(Command::Install { source }) => {
            package::install(&source);
        }
        Some(Command::Add { package: pkg }) => {
            match manifest::parse_package_spec(&pkg) {
                Ok((name, version)) => {
                    let mut m = manifest::load_manifest().unwrap_or_default();
                    let action = if m.dependencies.contains_key(&name) {
                        "Updated"
                    } else {
                        "Added"
                    };
                    m.dependencies
                        .insert(name.clone(), manifest::DependencySpec::Version(version.clone()));
                    if let Err(e) = manifest::save_manifest(&m) {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                    println!(
                        "  {} {} = \"{}\" to forge.toml",
                        action, name, version
                    );
                    package::install_from_manifest();
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Update) => {
            package::update();
        }
        Some(Command::Publish { dry_run, registry }) => {
            publish::publish(dry_run, registry.as_deref());
        }
        Some(Command::Search { query }) => {
            let q = query.as_deref().unwrap_or("");
            match registry::fetch_index() {
                Ok(index) => {
                    let results = registry::search_packages(q, &index);
                    if results.is_empty() {
                        if q.is_empty() {
                            println!("No packages found in registry.");
                        } else {
                            println!("No packages found matching '{}'.", q);
                        }
                    } else {
                        println!(
                            "{:<20} {:<10} {}",
                            "NAME", "VERSION", "DESCRIPTION"
                        );
                        println!("{}", "-".repeat(60));
                        for pkg in &results {
                            println!(
                                "{:<20} {:<10} {}",
                                pkg.name,
                                if pkg.latest.is_empty() { "-" } else { &pkg.latest },
                                pkg.description
                            );
                        }
                        println!("\n{} package(s) found.", results.len());
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Lsp) => {
            lsp::run_lsp();
        }
        Some(Command::Dap) => {
            dap::run_dap();
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
                let rendered = if warning.line > 0 {
                    format!("[{}:{}] {}", filename, warning.line, warning.message)
                } else {
                    format!("[{}] {}", filename, warning.message)
                };
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
            if warning.line > 0 {
                eprintln!(
                    "{}",
                    errors::format_warning(&format!("line {}: {}", warning.line, warning.message))
                );
            } else {
                eprintln!("{}", errors::format_warning(&warning.message));
            }
        }
    }
}

fn collect_vm_incompatible_stmt(stmt: &Stmt, issues: &mut BTreeSet<&'static str>) {
    match stmt {
        Stmt::TypeDef { .. } => {}
        Stmt::InterfaceDef { .. } => {}
        Stmt::ImplBlock { methods, .. } => {
            for method in methods {
                collect_vm_incompatible_stmt(&method.stmt, issues);
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
            for s in try_body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
            for s in catch_body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::SafeBlock { body } => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::TimeoutBlock { body, .. } => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::RetryBlock { count, body } => {
            collect_vm_incompatible_expr(count, issues);
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::ScheduleBlock { body, .. } => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::WatchBlock { body, .. } => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::PromptDef { .. } => {}
        Stmt::AgentDef { .. } => {}
        Stmt::DecoratorStmt(_) => {
            issues.insert("decorator-driven runtime features");
        }
        Stmt::Import { .. } => {}
        Stmt::FnDef {
            body, decorators, ..
        } => {
            if decorators
                .iter()
                .any(|decorator| !is_vm_metadata_decorator(&decorator.name))
            {
                issues.insert("decorator-driven runtime features");
            }
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
            if let Some(else_body) = else_body {
                for s in else_body {
                    collect_vm_incompatible_stmt(&s.stmt, issues);
                }
            }
        }
        Stmt::Match { arms, .. } => {
            for arm in arms {
                for s in &arm.body {
                    collect_vm_incompatible_stmt(&s.stmt, issues);
                }
            }
        }
        Stmt::For { body, .. }
        | Stmt::While { body, .. }
        | Stmt::Loop { body }
        | Stmt::Spawn { body }
        | Stmt::Squad { body } => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
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

fn is_vm_metadata_decorator(name: &str) -> bool {
    matches!(name, "test" | "skip" | "before" | "after")
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
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
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
            collect_vm_incompatible_expr(source, issues);
            collect_vm_incompatible_expr(value, issues);
        }
        Expr::PipeChain { source, steps } => {
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
        Expr::Must(expr) | Expr::Ask(expr) | Expr::Freeze(expr) | Expr::Await(expr) => {
            collect_vm_incompatible_expr(expr, issues);
        }
        Expr::Spread(expr) => collect_vm_incompatible_expr(expr, issues),
        Expr::Spawn(body) | Expr::Squad(body) => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Expr::Tuple(items) => {
            for item in items {
                collect_vm_incompatible_expr(item, issues);
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

    // Auto-fallback: if VM is requested but program uses decorators, fall back to interpreter
    let effective_vm = if use_vm {
        match ensure_vm_compatible(&program, "VM") {
            Ok(()) => true,
            Err(message) => {
                eprintln!("  Info: falling back to interpreter ({})", message);
                false
            }
        }
    } else {
        false
    };

    if effective_vm {
        if profile {
            match vm::run_with_profiling(&program) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("{}", errors::format_simple_error(&e.to_string()));
                    process::exit(1);
                }
            }
        } else {
            match vm::run(&program) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("{}", errors::format_simple_error(&e.to_string()));
                    process::exit(1);
                }
            }
        }
    } else {
        let mut interpreter = Interpreter::new();
        interpreter.source = Some(source.to_string());
        let path = std::path::Path::new(filename);
        if path.exists() {
            interpreter.source_file =
                Some(std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
        }
        interpreter.set_defer_host_runtime(true);
        match interpreter.run(&program) {
            Ok(_) => {}
            Err(e) => {
                if e.line > 0 {
                    eprintln!(
                        "{}",
                        errors::format_error(
                            source,
                            e.line,
                            if e.col > 0 { e.col } else { 1 },
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

        let runtime_plan = runtime::metadata::extract_runtime_plan(&program);
        if let Err(e) = runtime::host::launch(interpreter, &runtime_plan).await {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    }
}

#[cfg(feature = "jit")]
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

    // Create the VM first so we can pre-allocate string constants into GC
    // for functions that need runtime bridges (string/collection/global ops).
    let mut vm = vm::machine::VM::new();

    for (i, proto) in chunk.prototypes.iter().enumerate() {
        let name = if proto.name.is_empty() {
            format!("fn_{}", i)
        } else {
            proto.name.clone()
        };
        let type_info = vm::jit::type_analysis::analyze(proto);
        let needs_vm_ptr = type_info.has_string_ops
            || type_info.has_collection_ops
            || type_info.has_global_ops;

        // Pre-allocate string constants into GC so their GcRef indices
        // can be baked into JIT code for runtime bridge calls.
        let string_refs = if needs_vm_ptr {
            let refs: Vec<Option<i64>> = proto
                .constants
                .iter()
                .map(|c| match c {
                    vm::bytecode::Constant::Str(s) => {
                        let r = vm.gc.alloc_string(s.clone());
                        vm.jit_roots.push(r);
                        Some(r.0 as i64)
                    }
                    _ => None,
                })
                .collect();
            Some(refs)
        } else {
            None
        };

        match jit.compile_function(proto, &name, string_refs.as_ref()) {
            Ok(ptr) => {
                eprintln!(
                    "  JIT compiled: {} ({} instructions -> native)",
                    name,
                    proto.code.len()
                );
                vm.jit_cache.insert(
                    name,
                    vm::machine::JitEntry {
                        ptr,
                        uses_float: type_info.has_float,
                        has_string_ops: type_info.has_string_ops,
                        has_collection_ops: type_info.has_collection_ops,
                        has_global_ops: type_info.has_global_ops,
                        returns_obj: matches!(
                            type_info.return_type,
                            vm::jit::type_analysis::RegType::StringRef
                                | vm::jit::type_analysis::RegType::ObjRef
                        ),
                        returns_float: matches!(
                            type_info.return_type,
                            vm::jit::type_analysis::RegType::Float
                        ),
                    },
                );
            }
            Err(e) => {
                eprintln!("  JIT skip: {} ({})", name, e);
            }
        }
    }

    match vm.execute(&chunk) {
        Ok(_) => {}
        Err(e) => {
            // Use the full Display impl so the stack trace (function +
            // source line) gets printed, not just the bare message.
            eprintln!("{}", errors::format_simple_error(&e.to_string()));
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

fn compile_to_native_aot(source: &str, filename: &str, file_path: &PathBuf, strict: bool) {
    let (program, warnings) = match prepare_program(source, strict) {
        Ok(prepared) => prepared,
        Err(err) => print_frontend_error(source, filename, err),
    };
    emit_type_warnings(&warnings);

    if let Err(message) = ensure_vm_compatible(&program, "AOT build") {
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

    let bytecode = match vm::serialize::serialize_chunk(&chunk) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    };

    match native::build_native_aot(&bytecode, file_path) {
        Ok(output_path) => {
            let standalone = native::find_libforge_dir().is_some();
            let runtime_msg = if standalone {
                "standalone (libforge linked)"
            } else {
                "Forge VM required at execution time"
            };
            println!(
                "Built AOT binary {} -> {}\n  bytecode embedded ({} bytes, no source exposure)\n  runtime: {}",
                filename,
                output_path.display(),
                bytecode.len(),
                runtime_msg
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
            // Use the full Display impl so the stack trace (function +
            // source line) gets printed, not just the bare message.
            eprintln!("{}", errors::format_simple_error(&e.to_string()));
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
    fn vm_incompatibilities_allow_interface_and_impl_blocks() {
        let source = r#"
        thing Robot { id: Int }
        power Speakable { fn speak() -> String }
        give Robot {
            fn speak(it) { return "beep" }
        }
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        let issues = vm_incompatibilities(&program);
        assert!(!issues.contains(&"interface/power definitions"));
        assert!(!issues.contains(&"impl/give blocks"));
    }

    #[test]
    fn vm_incompatibilities_allow_type_definitions() {
        let source = r#"
        type Color = Red | Green | Blue
        let color = Red
        color
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        let issues = vm_incompatibilities(&program);
        assert!(!issues.contains(&"type definitions"));
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

    #[test]
    fn vm_incompatibilities_allow_safe_blocks() {
        let source = r#"
        let mut status = "ok"
        safe {
            let crash = 1 / 0
            status = "bad"
        }
        status
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_retry_blocks() {
        let source = r#"
        let mut attempts = 0
        retry 3 times {
            attempts += 1
            if attempts < 3 {
                let crash = 1 / 0
            }
        }
        attempts
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_timeout_blocks() {
        let source = r#"
        timeout 1 seconds {
            println("slow")
        }
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_file_imports() {
        let import_path = format!("/tmp/forge_vm_import_check_{}.fg", std::process::id());
        std::fs::write(&import_path, r#"let meaning = 42"#).expect("write import fixture");

        let source = format!(
            r#"
            import "{}"
            meaning
            "#,
            import_path
        );

        let (program, _) = prepare_program(&source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());

        std::fs::remove_file(&import_path).ok();
    }

    #[test]
    fn vm_incompatibilities_allow_where_filters() {
        let source = r#"
        let users = [{ age: 17 }, { age: 30 }]
        users where age >= 18
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_pipe_chains() {
        let source = r#"
        let users = [{ name: "Bob", active: true }]
        users >> keep where active >> sort by name >> take 1
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_prompt_definitions() {
        let source = r#"
        prompt summarize(text) {
            system: "You are concise"
            user: "Summarize: {text}"
        }
        let kind = type(summarize)
        kind
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_agent_definitions() {
        let source = r#"
        agent researcher(topic) {
            tools: ["search", "read"]
            goal: "Research {topic}"
            max_steps: 5
        }
        let kind = type(researcher)
        kind
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_test_decorators() {
        let source = r#"
        @test
        fn smoke() { return 42 }
        smoke()
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_reject_server_route_decorators() {
        let source = r#"
        @server(port: 8080)
        @get("/hello")
        fn hello() { return "hi" }
        "#;

        let (program, _) = prepare_program(source, false).expect("program should parse");
        assert!(vm_incompatibilities(&program).contains(&"decorator-driven runtime features"));
    }
}
