use crate::errors;
use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::parser::ast::*;
use crate::parser::Parser;
use crate::vm;
use std::path::Path;
use std::time::Instant;

#[cfg(test)]
pub mod parity;

/// Which execution engine(s) `forge test` runs each test file on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Engine {
    /// Bytecode VM (the default engine for `forge run`). Files the VM cannot
    /// run yet fall back to the interpreter, exactly like `forge run`.
    Vm,
    /// Tree-walking interpreter (`--interp`).
    Interp,
    /// Run every file on both engines; a failure on either fails the run.
    Both,
}

/// A single concrete engine a file is executed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    Vm,
    Interp,
}

impl Backend {
    fn label(self) -> &'static str {
        match self {
            Backend::Vm => "vm",
            Backend::Interp => "interp",
        }
    }
}

/// Options for [`run_tests`].
pub struct TestOptions<'a> {
    pub filter: Option<&'a str>,
    pub coverage: bool,
    pub engine: Engine,
    /// Returns `Err(reason)` when a program uses constructs the VM cannot
    /// run yet; such files fall back to the interpreter (same rule as
    /// `forge run`).
    pub vm_compat: &'a dyn Fn(&Program) -> Result<(), String>,
    /// Per-test (and per-file setup) wall-clock limit. A test that exceeds
    /// it cannot be interrupted safely, so the whole run aborts with a
    /// failure naming the hung test. `None` disables the watchdog.
    pub timeout: Option<std::time::Duration>,
}

/// Aborts the process when the currently armed test overruns its deadline.
///
/// Neither engine can be pre-empted from outside, so a hung test (e.g. an
/// infinite loop caused by an engine bug) would otherwise stall `forge test`
/// and CI forever.
struct Watchdog {
    slot: std::sync::Arc<std::sync::Mutex<Option<(Instant, String)>>>,
}

impl Watchdog {
    fn start(limit: std::time::Duration) -> Watchdog {
        let slot: std::sync::Arc<std::sync::Mutex<Option<(Instant, String)>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let watched = std::sync::Arc::clone(&slot);
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let expired = match watched.lock() {
                Ok(guard) => guard
                    .as_ref()
                    .filter(|(started, _)| started.elapsed() > limit)
                    .map(|(_, label)| label.clone()),
                Err(_) => None,
            };
            if let Some(label) = expired {
                println!(
                    "    \x1B[31mFAIL\x1B[0m  {} — timed out after {}s; aborting the test run",
                    label,
                    limit.as_secs_f64()
                );
                eprintln!(
                    "error: test '{}' exceeded the {}s limit (raise it with --timeout <secs>, 0 disables)",
                    label,
                    limit.as_secs_f64()
                );
                std::process::exit(1);
            }
        });
        Watchdog { slot }
    }

    fn arm(watchdog: Option<&Watchdog>, label: String) {
        if let Some(w) = watchdog {
            if let Ok(mut guard) = w.slot.lock() {
                *guard = Some((Instant::now(), label));
            }
        }
    }

    fn disarm(watchdog: Option<&Watchdog>) {
        if let Some(w) = watchdog {
            if let Ok(mut guard) = w.slot.lock() {
                *guard = None;
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub total: usize,
}

impl Tally {
    fn add(&mut self, other: Tally) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.skipped += other.skipped;
        self.total += other.total;
    }
}

/// A loaded program on one engine, ready to have its test functions called.
enum Session {
    Interp(Box<Interpreter>),
    Vm(Box<vm::machine::VM>),
}

impl Session {
    fn start(backend: Backend, program: &Program, coverage: bool) -> Result<Session, String> {
        match backend {
            Backend::Interp => {
                let mut interpreter = Interpreter::new();
                if coverage {
                    interpreter.coverage = Some(std::collections::HashSet::new());
                }
                interpreter.run(program).map_err(|e| e.message)?;
                Ok(Session::Interp(Box::new(interpreter)))
            }
            Backend::Vm => {
                let chunk = vm::compiler::compile(program).map_err(|e| e.message)?;
                let mut machine = vm::machine::VM::new();
                machine.execute(&chunk).map_err(|e| e.to_string())?;
                Ok(Session::Vm(Box::new(machine)))
            }
        }
    }

    /// Call a zero-argument global function. `None` if it is not defined.
    fn call(&mut self, name: &str) -> Option<Result<(), String>> {
        match self {
            Session::Interp(interpreter) => {
                let f = interpreter.env.get(name)?;
                Some(
                    interpreter
                        .call_function(f, vec![])
                        .map(|_| ())
                        .map_err(|e| e.message),
                )
            }
            Session::Vm(machine) => {
                let f = machine.globals.get(name).cloned()?;
                Some(
                    machine
                        .call_value(f, vec![])
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                )
            }
        }
    }

    fn coverage(&self) -> Option<&std::collections::HashSet<usize>> {
        match self {
            Session::Interp(interpreter) => interpreter.coverage.as_ref(),
            Session::Vm(_) => None,
        }
    }
}

/// Resolve which concrete backends a file runs on.
///
/// Returns the backends plus an optional note explaining a VM fallback.
fn backends_for(
    engine: Engine,
    program: &Program,
    vm_compat: &dyn Fn(&Program) -> Result<(), String>,
) -> (Vec<Backend>, Option<String>) {
    match engine {
        Engine::Interp => (vec![Backend::Interp], None),
        Engine::Vm => match vm_compat(program) {
            Ok(()) => (vec![Backend::Vm], None),
            Err(reason) => (
                vec![Backend::Interp],
                Some(format!("falling back to interpreter ({})", reason)),
            ),
        },
        Engine::Both => match vm_compat(program) {
            Ok(()) => (vec![Backend::Interp, Backend::Vm], None),
            Err(reason) => (
                vec![Backend::Interp],
                Some(format!("vm run skipped, interpreter only ({})", reason)),
            ),
        },
    }
}

pub fn run_tests(test_dir: &str, opts: &TestOptions) {
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

    let coverage = opts.coverage;
    if coverage && opts.engine == Engine::Vm {
        eprintln!("  Info: --coverage is only supported on the interpreter; collecting coverage with --engine interp");
    }
    let engine = if coverage && opts.engine == Engine::Vm {
        Engine::Interp
    } else {
        opts.engine
    };
    // With `--engine both`, coverage is gathered from the interpreter pass.

    let mut tallies: Vec<(Backend, Tally)> = Vec::new();
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

    let watchdog = opts.timeout.map(Watchdog::start);

    let mut record = |backend: Backend, tally: Tally| {
        if let Some((_, t)) = tallies.iter_mut().find(|(b, _)| *b == backend) {
            t.add(tally);
        } else {
            tallies.push((backend, tally));
        }
    };

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

        let program = match parse_source(&source) {
            Ok(p) => p,
            Err(message) => {
                eprintln!("  \x1B[31mERROR\x1B[0m  {} — {}", path_str, message);
                let backend = if engine == Engine::Interp {
                    Backend::Interp
                } else {
                    Backend::Vm
                };
                record(
                    backend,
                    Tally {
                        failed: 1,
                        total: 1,
                        ..Tally::default()
                    },
                );
                continue;
            }
        };

        let test_info = find_test_functions(&program);
        let before_fn = find_hook_function(&program, "before");
        let after_fn = find_hook_function(&program, "after");

        let test_fns: Vec<&TestInfo> = test_info
            .iter()
            .filter(|t| opts.filter.map_or(true, |pat| t.name.contains(pat)))
            .collect();

        if test_fns.is_empty() {
            continue;
        }

        let (backends, note) = backends_for(engine, &program, opts.vm_compat);
        let show_label = engine != Engine::Interp;

        for backend in backends {
            if show_label {
                println!(
                    "  \x1B[1m{}\x1B[0m \x1B[90m[{}]\x1B[0m",
                    path_str,
                    backend.label()
                );
            } else {
                println!("  \x1B[1m{}\x1B[0m", path_str);
            }
            if let Some(ref note) = note {
                println!("    \x1B[90mInfo: {}\x1B[0m", note);
            }

            let collect_coverage = coverage && backend == Backend::Interp;
            let (tally, session) = run_file_on(
                backend,
                &program,
                &test_fns,
                (before_fn.as_deref(), after_fn.as_deref()),
                collect_coverage,
                (watchdog.as_ref(), &path_str),
            );
            record(backend, tally);

            if let Some(cov) = session.as_ref().and_then(|s| s.coverage()) {
                let executable_set = executable_line_set(&source);
                let executed = cov.intersection(&executable_set).count();
                coverage_data.push((path_str.clone(), executable_set.len(), executed));
            }
            println!();
        }
    }

    let mut any_failed = false;
    for (backend, tally) in &tallies {
        any_failed |= tally.failed > 0;
        let skip_msg = if tally.skipped > 0 {
            format!(", {} skipped", tally.skipped)
        } else {
            String::new()
        };
        let prefix = if tallies.len() > 1 || engine != Engine::Interp {
            format!("[{}] ", backend.label())
        } else {
            String::new()
        };
        println!(
            "  \x1B[1m{}{} passed, {} failed{}, {} total\x1B[0m",
            prefix, tally.passed, tally.failed, skip_msg, tally.total
        );
    }
    if tallies.is_empty() {
        println!("  \x1B[1m0 passed, 0 failed, 0 total\x1B[0m");
    }
    println!();

    if coverage && !coverage_data.is_empty() {
        print_coverage(&coverage_data);
    }

    if any_failed {
        std::process::exit(1);
    }
}

fn parse_source(source: &str) -> Result<Program, String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
    let mut parser = Parser::new(tokens);
    parser.parse_program().map_err(|e| e.to_string())
}

/// Run every selected test of one file on one backend.
fn run_file_on(
    backend: Backend,
    program: &Program,
    tests: &[&TestInfo],
    (before_fn, after_fn): (Option<&str>, Option<&str>),
    coverage: bool,
    (watchdog, path): (Option<&Watchdog>, &str),
) -> (Tally, Option<Session>) {
    let mut tally = Tally::default();

    // Run the full program first to define all functions.
    Watchdog::arm(watchdog, format!("{} (setup) [{}]", path, backend.label()));
    let started = Session::start(backend, program, coverage);
    Watchdog::disarm(watchdog);
    let mut session = match started {
        Ok(s) => s,
        Err(message) => {
            eprintln!("    \x1B[31mERROR\x1B[0m  setup — {}", message);
            tally.failed += 1;
            tally.total += 1;
            return (tally, None);
        }
    };

    for test in tests {
        tally.total += 1;

        if test.skip {
            tally.skipped += 1;
            println!("    \x1B[33mSKIP\x1B[0m  {}", test.name);
            continue;
        }

        let start = Instant::now();
        Watchdog::arm(
            watchdog,
            format!("{} ({} [{}])", test.name, path, backend.label()),
        );

        if let Some(before_name) = before_fn {
            if let Some(Err(message)) = session.call(before_name) {
                Watchdog::disarm(watchdog);
                tally.failed += 1;
                println!(
                    "    \x1B[31mFAIL\x1B[0m  {} — @before hook failed: {}",
                    test.name, message
                );
                continue;
            }
        }

        let result = session.call(&test.name);

        // Run @after hook regardless of test result
        if let Some(after_name) = after_fn {
            let _ = session.call(after_name);
        }
        Watchdog::disarm(watchdog);

        let duration = start.elapsed().as_millis();

        match result {
            None => {
                tally.failed += 1;
                println!(
                    "    \x1B[31mFAIL\x1B[0m  {} — function not found",
                    test.name
                );
            }
            Some(Ok(())) => {
                tally.passed += 1;
                println!(
                    "    \x1B[32mok\x1B[0m    {} \x1B[90m({}ms)\x1B[0m",
                    test.name, duration
                );
            }
            Some(Err(message)) => {
                tally.failed += 1;
                println!(
                    "    \x1B[31mFAIL\x1B[0m  {} \x1B[90m({}ms)\x1B[0m",
                    test.name, duration
                );
                println!("          {}", message);
            }
        }
    }

    (tally, Some(session))
}

fn print_coverage(coverage_data: &[(String, usize, usize)]) {
    println!("  \x1B[1mCoverage\x1B[0m");
    println!();
    let mut total_executable = 0usize;
    let mut total_executed = 0usize;
    for (file, executable, executed) in coverage_data {
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
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed == "}"
            || trimmed == "{"
            || trimmed == "} else {"
            || trimmed == "} else if"
            || trimmed.starts_with("} else if ")
            || trimmed == "otherwise {"
            || trimmed == "} otherwise {"
            || trimmed.starts_with("@")
        {
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

#[cfg(test)]
mod engine_tests {
    use super::*;

    fn program(src: &str) -> Program {
        parse_source(src).expect("parse")
    }

    #[test]
    fn engine_selection_respects_vm_fallback_rules() {
        let p = program("let x = 1");
        let ok = |_: &Program| Ok(());
        let unsupported = |_: &Program| Err("decorator-driven runtime features".to_string());

        assert_eq!(backends_for(Engine::Vm, &p, &ok).0, vec![Backend::Vm]);
        assert_eq!(
            backends_for(Engine::Interp, &p, &unsupported).0,
            vec![Backend::Interp]
        );
        assert_eq!(
            backends_for(Engine::Both, &p, &ok).0,
            vec![Backend::Interp, Backend::Vm]
        );

        let (vm_fallback, note) = backends_for(Engine::Vm, &p, &unsupported);
        assert_eq!(vm_fallback, vec![Backend::Interp]);
        assert!(note.unwrap().contains("falling back to interpreter"));

        let (both_fallback, note) = backends_for(Engine::Both, &p, &unsupported);
        assert_eq!(both_fallback, vec![Backend::Interp]);
        assert!(note.unwrap().contains("vm run skipped"));
    }

    fn run_on(backend: Backend, src: &str) -> Tally {
        let p = program(src);
        let tests = find_test_functions(&p);
        let refs: Vec<&TestInfo> = tests.iter().collect();
        let before = find_hook_function(&p, "before");
        let after = find_hook_function(&p, "after");
        run_file_on(
            backend,
            &p,
            &refs,
            (before.as_deref(), after.as_deref()),
            false,
            (None, "inline.fg"),
        )
        .0
    }

    const SUITE: &str = r#"
        let mut hits = 0
        @before
        fn setup() { hits = hits + 1 }
        @test
        fn passes() { assert_eq(1 + 1, 2) }
        @test
        fn fails() { assert_eq(1, 2) }
        @test
        @skip
        fn skipped() { assert(false) }
    "#;

    const EXPECTED: Tally = Tally {
        passed: 1,
        failed: 1,
        skipped: 1,
        total: 3,
    };

    #[test]
    fn run_file_on_interp_counts_pass_fail_skip() {
        assert_eq!(run_on(Backend::Interp, SUITE), EXPECTED);
    }

    #[test]
    fn run_file_on_vm_counts_pass_fail_skip() {
        assert_eq!(run_on(Backend::Vm, SUITE), EXPECTED);
    }

    #[test]
    fn run_file_on_reports_setup_errors() {
        for backend in [Backend::Interp, Backend::Vm] {
            let tally = run_on(backend, "let x = undefined_thing()\n@test\nfn t() { }");
            assert_eq!(tally.failed, 1, "{:?}", backend);
            assert_eq!(tally.passed, 0, "{:?}", backend);
        }
    }
}
