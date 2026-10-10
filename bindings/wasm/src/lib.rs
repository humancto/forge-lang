//! Forge in the browser.
//!
//! The portable Forge core (`forge-lang` without its `host` feature)
//! compiled to `wasm32-unknown-unknown`, plus the playground's API:
//!
//! * [`run`] — execute a program on the bytecode VM (falling back to the
//!   interpreter for constructs the VM compiler does not support, exactly
//!   like `forge run`) or on a chosen engine, with captured output, an
//!   output cap and a deterministic execution budget so an infinite loop
//!   ends with an error instead of freezing the tab;
//! * [`check`] — lex, parse and type-check without running (the same
//!   diagnostics as `forge check` / the LSP);
//! * [`format`] — `forge fmt`.
//!
//! Everything here is plain Rust and is unit-tested natively
//! (`cargo test`). The `wasm` module at the bottom is the thin
//! `wasm-bindgen` layer: it exchanges JSON strings with the JS wrapper
//! (`docs/playground/forge.js`), which turns them into objects.
//!
//! Host capabilities (files, network, databases, processes, threads) do
//! not exist in the browser: they fail at run time with "… is not available
//! in the browser playground", never with a compile error in user code.
//! Concurrency constructs that need threads are rejected before the
//! program starts, with their position (see [`unsupported_constructs`]).

use forge_lang::clock::Instant;
use forge_lang::interpreter::Interpreter;
use forge_lang::lexer::token::Token;
use forge_lang::lexer::Lexer;
use forge_lang::parser::ast::Program;
use forge_lang::parser::Parser;
use forge_lang::runtime::stdio::{self, Stream};
use forge_lang::vm;
use serde::{Deserialize, Serialize};

// Lets the resource limits measure the interpreter's memory (the VM
// accounts its own heap).
#[global_allocator]
static ALLOCATOR: forge_lang::CountingAllocator = forge_lang::CountingAllocator;

/// Bytes of WebAssembly shadow stack (see build.rs; keep in sync).
pub const STACK_SIZE: usize = 8 * 1024 * 1024;

/// Forge call-depth limits in the browser. Every WebAssembly frame also
/// uses the JS engine's native stack (about 1 MB in V8, which a page cannot
/// raise), so recursion runs out far earlier than on the CLI's 1 GiB
/// thread. Measured in Node 22 (V8): plain recursion overflowed the JS
/// stack at ~1,900 Forge calls on the VM and ~900 on the interpreter. The
/// limits keep about half of that as headroom; unusually large frames
/// (recursion through `map` callbacks) can still overflow the JS stack,
/// which `forge.js` reports as a recursion error and recovers from by
/// starting a new instance.
pub const MAX_CALL_DEPTH_VM: usize = 1_000;
pub const MAX_CALL_DEPTH_INTERP: usize = 450;

/// Default VM instruction budget: about 2-3 s of work in a desktop browser.
pub const DEFAULT_MAX_INSTRUCTIONS: u64 = 100_000_000;

/// Default interpreter step budget (statements, loop iterations, calls):
/// about 2 s in a desktop browser.
pub const DEFAULT_MAX_STEPS: u64 = 15_000_000;

/// Innermost stack frames kept in an error's `trace` (deep recursion would
/// otherwise produce thousands).
pub const MAX_TRACE_FRAMES: usize = 32;

/// Default memory limit for one run.
pub const DEFAULT_MAX_MEMORY: usize = 256 * 1024 * 1024;

/// Default cap on captured output.
pub const DEFAULT_MAX_OUTPUT: usize = 1024 * 1024;

/// Which engine runs the program.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// The VM, or the interpreter when the VM compiler reports a construct
    /// as unsupported (what `forge run` does).
    #[default]
    Auto,
    Vm,
    #[serde(alias = "interpreter")]
    Interp,
}

/// Options for [`run`]. Every field is optional in JSON.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RunOptions {
    pub engine: Engine,
    /// VM instruction budget; `None` = [`DEFAULT_MAX_INSTRUCTIONS`].
    pub max_instructions: Option<u64>,
    /// Interpreter step budget; `None` = [`DEFAULT_MAX_STEPS`].
    pub max_steps: Option<u64>,
    /// Memory limit in bytes; `None` = [`DEFAULT_MAX_MEMORY`].
    pub max_memory: Option<usize>,
    /// Output cap in bytes; `None` = [`DEFAULT_MAX_OUTPUT`].
    pub max_output: Option<usize>,
}

/// One piece of output, in the order it was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutputChunk {
    /// `"stdout"` or `"stderr"`.
    pub stream: &'static str,
    pub text: String,
}

/// A stack frame of a runtime error (innermost first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TraceFrame {
    pub function: String,
    pub line: usize,
}

/// Why a run failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorKind {
    /// The source does not lex or parse.
    Syntax,
    /// The program uses something the browser cannot do (threads, servers).
    Unsupported,
    /// The VM compiler rejected the program.
    Compile,
    /// The program failed while running.
    Runtime,
    /// The execution budget ran out.
    Limit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorInfo {
    pub kind: ErrorKind,
    pub message: String,
    /// 1-based; 0 when unknown.
    pub line: usize,
    /// 1-based; 0 when unknown.
    pub col: usize,
    /// Stable error code, when the failing stage has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<TraceFrame>,
}

/// What [`run`] produced.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunResult {
    pub ok: bool,
    /// Everything written to stdout.
    pub stdout: String,
    /// Everything written to stderr (`sus`, `log.*`, `term.*` chrome).
    pub stderr: String,
    /// stdout and stderr interleaved in write order.
    pub output: Vec<OutputChunk>,
    /// The output cap was reached; later output was dropped.
    pub truncated: bool,
    pub error: Option<ErrorInfo>,
    /// The engine that ran the program: `"vm"` or `"interp"` (empty when
    /// it never started).
    pub engine: &'static str,
    /// Wall-clock time spent, in milliseconds (informational).
    pub elapsed_ms: f64,
}

/// One diagnostic from [`check`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckDiagnostic {
    /// `"error"` or `"warning"`.
    pub severity: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

/// A construct the browser cannot run, found by [`unsupported_constructs`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    pub what: &'static str,
    pub line: usize,
    pub col: usize,
}

/// Constructs that need OS threads or a server socket. They are rejected
/// before running (with a position) instead of failing mid-run.
pub fn unsupported_constructs(source: &str, program: &Program) -> Vec<Unsupported> {
    let mut found = Vec::new();
    if let Ok(tokens) = Lexer::new(source).tokenize() {
        for t in tokens {
            let what = match t.token {
                Token::Spawn => "`spawn` (threads)",
                Token::Squad => "`squad` (threads)",
                Token::Timeout => "`timeout` (threads)",
                Token::Schedule => "`schedule`",
                Token::Watch => "`watch`",
                _ => continue,
            };
            found.push(Unsupported {
                what,
                line: t.line,
                col: t.col,
            });
        }
    }
    if forge_lang::runtime::metadata::extract_runtime_plan(program)
        .server
        .is_some()
    {
        found.push(Unsupported {
            what: "`@server` (HTTP servers)",
            line: 0,
            col: 0,
        });
    }
    found
}

fn parse(source: &str) -> Result<Program, ErrorInfo> {
    let syntax = |message: String, line: usize, col: usize| ErrorInfo {
        kind: ErrorKind::Syntax,
        message,
        line,
        col,
        code: None,
        trace: Vec::new(),
    };
    let tokens = Lexer::new(source)
        .tokenize()
        .map_err(|e| syntax(e.message, e.line, e.col))?;
    Parser::new(tokens)
        .parse_program()
        .map_err(|e| syntax(e.message, e.line, e.col))
}

/// Run `source` and capture what it prints.
pub fn run(source: &str, options: &RunOptions) -> RunResult {
    let start = Instant::now();
    // Each run starts near the top of the stack the module reserved
    // (natively, e.g. under `cargo test`, the thread's own stack applies).
    #[cfg(target_arch = "wasm32")]
    forge_lang::runtime::recursion::register_thread_stack(STACK_SIZE);

    let capture = stdio::capture(options.max_output.unwrap_or(DEFAULT_MAX_OUTPUT));
    let (engine, error) = match parse(source) {
        Err(e) => ("", Some(e)),
        Ok(program) => match unsupported_constructs(source, &program).first() {
            Some(u) => (
                "",
                Some(ErrorInfo {
                    kind: ErrorKind::Unsupported,
                    message: forge_lang::runtime::unavailable_message(u.what),
                    line: u.line,
                    col: u.col,
                    code: None,
                    trace: Vec::new(),
                }),
            ),
            None => execute(source, &program, options),
        },
    };
    let captured = capture.finish();
    let error = error.map(|mut e| {
        e.message = browser_message(&e.message);
        e
    });

    RunResult {
        ok: error.is_none(),
        stdout: captured.text(Stream::Stdout),
        stderr: captured.text(Stream::Stderr),
        output: captured
            .chunks
            .into_iter()
            .map(|(stream, text)| OutputChunk {
                stream: match stream {
                    Stream::Stdout => "stdout",
                    Stream::Stderr => "stderr",
                },
                text,
            })
            .collect(),
        truncated: captured.truncated,
        error,
        engine,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    }
}

fn execute(
    source: &str,
    program: &Program,
    options: &RunOptions,
) -> (&'static str, Option<ErrorInfo>) {
    let chunk = match options.engine {
        Engine::Interp => None,
        Engine::Vm | Engine::Auto => match vm::compiler::compile(program) {
            Ok(chunk) => Some(chunk),
            Err(e) if options.engine == Engine::Auto && e.is_unsupported() => None,
            Err(e) => {
                return (
                    "vm",
                    Some(ErrorInfo {
                        kind: ErrorKind::Compile,
                        message: e.message,
                        line: 0,
                        col: 0,
                        code: None,
                        trace: Vec::new(),
                    }),
                )
            }
        },
    };
    use forge_lang::runtime::limits::{self, Budget, Limits};
    use forge_lang::runtime::recursion::set_max_depth;
    // Deterministic fuel (one unit per VM instruction, or per interpreter
    // statement/call/loop iteration) and a memory ceiling, through the same
    // resource limits as `forge run --max-fuel/--max-memory` and the
    // sandbox. Exhaustion is fatal: `try`/`safe`/`retry` cannot catch it.
    let (engine, max_depth, fuel) = match chunk {
        Some(_) => (
            "vm",
            MAX_CALL_DEPTH_VM,
            options.max_instructions.unwrap_or(DEFAULT_MAX_INSTRUCTIONS),
        ),
        None => (
            "interp",
            MAX_CALL_DEPTH_INTERP,
            options.max_steps.unwrap_or(DEFAULT_MAX_STEPS),
        ),
    };
    set_max_depth(max_depth);
    let budget = Budget::new(Limits {
        max_fuel: Some(fuel),
        max_memory: Some(options.max_memory.unwrap_or(DEFAULT_MAX_MEMORY)),
        ..Limits::default()
    });
    let result = {
        let _limits = limits::scope(Some(budget.clone()));
        match &chunk {
            Some(chunk) => run_vm(chunk),
            None => run_interp(source, program),
        }
    };
    let error = result.err().map(|mut e| {
        if let Some(message) = budget.trip_message() {
            // A swallowed trip still ends the run as a limit.
            e.kind = ErrorKind::Limit;
            if limits::classify(&e.message).is_none() {
                e.message = message;
            }
        } else if limits::classify(&e.message).is_some() {
            e.kind = ErrorKind::Limit;
        }
        e
    });
    (engine, error)
}

fn run_vm(chunk: &vm::bytecode::Chunk) -> Result<(), ErrorInfo> {
    let mut machine = vm::machine::VM::new();
    let err = match machine.execute(chunk) {
        Ok(_) => return Ok(()),
        Err(err) => err,
    };
    let at = err.stack_trace.iter().find(|f| f.line > 0);
    Err(ErrorInfo {
        kind: ErrorKind::Runtime,
        code: Some(forge_lang::tooling::runtime_error_code(&err.message).to_string()),
        message: err.message.clone(),
        line: at.map_or(0, |f| f.line),
        col: at.map_or(0, |f| f.col),
        trace: err
            .stack_trace
            .iter()
            .take(MAX_TRACE_FRAMES)
            .map(|f| TraceFrame {
                function: f.function.clone(),
                line: f.line,
            })
            .collect(),
    })
}

fn run_interp(source: &str, program: &Program) -> Result<(), ErrorInfo> {
    let mut interp = Interpreter::new();
    interp.source = Some(source.to_string());
    let err = match interp.run(program) {
        Ok(_) => return Ok(()),
        Err(err) => err,
    };
    Err(ErrorInfo {
        kind: ErrorKind::Runtime,
        code: Some(forge_lang::tooling::runtime_error_code(&err.message).to_string()),
        message: err.message,
        line: err.line,
        col: err.col,
        trace: Vec::new(),
    })
}

/// The prefix the core puts before a hint inside an error message.
const HINT_PREFIX: &str = "  hint: ";

/// Rewrite one hint for the playground. The core's hints name CLI remedies
/// (`--max-fuel`, `FORGE_MAX_DEPTH`, `--allow-*`, `--interp`) that a browser
/// visitor cannot use; this maps each to what the playground offers, and
/// drops a hint whose only advice is a CLI flag or `forge.toml` setting.
/// `None` means "drop the hint". The core's error text is left unchanged;
/// this only affects what the playground shows.
pub fn browser_hint(hint: &str) -> Option<String> {
    let run_locally = "or run the program locally with `forge run`";
    if hint.contains("--max-fuel") {
        return Some(format!(
            "the playground stops each run after a fixed number of steps; do less work, {run_locally}"
        ));
    }
    if hint.contains("--max-memory") {
        return Some(format!(
            "the playground limits each run's memory; process data in smaller pieces, {run_locally}"
        ));
    }
    if hint.contains("FORGE_MAX_DEPTH") || hint.contains("--max-depth") {
        return Some(
            "check for infinite recursion or restructure to use iteration; the browser allows less recursion than `forge run`"
                .to_string(),
        );
    }
    if hint.contains("--interp") {
        return Some(
            "see the message above; switch the engine to the interpreter for a second opinion if it looks like an engine bug"
                .to_string(),
        );
    }
    // Permission grants (`--allow-*`, `[permissions]`) and any other CLI
    // flag or environment variable cannot be applied in the browser.
    let mentions_cli = hint.contains("--allow")
        || hint.contains("[permissions]")
        || hint.contains("forge.toml")
        || hint.contains("FORGE_")
        || hint.split_whitespace().any(|w| {
            w.trim_start_matches(['(', '`'])
                .strip_prefix("--")
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_lowercase()))
        });
    if mentions_cli {
        None
    } else {
        Some(hint.to_string())
    }
}

/// An error message with every `  hint: ` line passed through
/// [`browser_hint`].
pub fn browser_message(message: &str) -> String {
    if !message.contains(HINT_PREFIX) {
        return message.to_string();
    }
    let mut lines = Vec::new();
    for line in message.split('\n') {
        match line.strip_prefix(HINT_PREFIX) {
            Some(hint) => {
                if let Some(hint) = browser_hint(hint) {
                    lines.push(format!("{HINT_PREFIX}{hint}"));
                }
            }
            None => lines.push(line.to_string()),
        }
    }
    lines.join("\n")
}

/// Lex, parse and type-check `source`.
pub fn check(source: &str) -> Vec<CheckDiagnostic> {
    forge_lang::tooling::check_source(source)
        .into_iter()
        .map(|d| CheckDiagnostic {
            severity: if d.is_error { "error" } else { "warning" },
            message: browser_message(&d.message),
            help: d.help.and_then(|h| browser_hint(&h)),
            code: d.code,
            line: d.line,
            col: d.column,
            end_line: d.end_line,
            end_col: d.end_column,
        })
        .collect()
}

/// Format `source` like `forge fmt`.
pub fn format(source: &str) -> String {
    forge_lang::tooling::format_source(source)
}

/// The Forge version this module was built from.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The `wasm-bindgen` boundary: JSON in, JSON out. See
/// `docs/playground/forge.js` for the typed JS API built on top.
#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::sync::Mutex;
    use wasm_bindgen::prelude::*;

    /// Message of the last Rust panic. A panic aborts the module (wasm32
    /// has no unwinding), so the JS side reads this after catching the trap
    /// and then starts a fresh instance.
    static LAST_PANIC: Mutex<Option<String>> = Mutex::new(None);

    #[wasm_bindgen(start)]
    pub fn start() {
        std::panic::set_hook(Box::new(|info| {
            let message = match info.payload().downcast_ref::<&str>() {
                Some(s) => s.to_string(),
                None => match info.payload().downcast_ref::<String>() {
                    Some(s) => s.clone(),
                    None => "unknown panic".to_string(),
                },
            };
            let at = info
                .location()
                .map(|l| format!(" ({}:{})", l.file(), l.line()))
                .unwrap_or_default();
            if let Ok(mut slot) = LAST_PANIC.lock() {
                *slot = Some(format!("{}{}", message, at));
            }
        }));
    }

    /// `run(source, optionsJson) -> resultJson`.
    #[wasm_bindgen]
    pub fn run(source: &str, options_json: &str) -> String {
        let options: super::RunOptions = if options_json.trim().is_empty() {
            super::RunOptions::default()
        } else {
            match serde_json::from_str(options_json) {
                Ok(o) => o,
                Err(e) => {
                    return serde_json::json!({ "invalidOptions": e.to_string() }).to_string()
                }
            }
        };
        serde_json::to_string(&super::run(source, &options))
            .unwrap_or_else(|e| serde_json::json!({ "internalError": e.to_string() }).to_string())
    }

    /// `check(source) -> diagnosticsJson`.
    #[wasm_bindgen]
    pub fn check(source: &str) -> String {
        serde_json::to_string(&super::check(source)).unwrap_or_else(|_| "[]".to_string())
    }

    #[wasm_bindgen]
    pub fn format(source: &str) -> String {
        super::format(source)
    }

    #[wasm_bindgen]
    pub fn version() -> String {
        super::version().to_string()
    }

    /// The last panic message, if the module trapped.
    #[wasm_bindgen]
    pub fn last_panic() -> Option<String> {
        LAST_PANIC.lock().ok().and_then(|slot| slot.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(engine: Engine, src: &str) -> RunResult {
        run(
            src,
            &RunOptions {
                engine,
                ..RunOptions::default()
            },
        )
    }

    const ENGINES: [Engine; 3] = [Engine::Auto, Engine::Vm, Engine::Interp];

    #[test]
    fn hello_world_on_every_engine() {
        for engine in ENGINES {
            let r = run_on(engine, "say \"hello\"\nprint(\"a\", \"b\")\nyell \"hi\"\n");
            assert!(r.ok, "{engine:?}: {:?}", r.error);
            assert_eq!(r.stdout, "hello\na bHI\n", "{engine:?}");
            let expected = if engine == Engine::Interp {
                "interp"
            } else {
                "vm"
            };
            assert_eq!(r.engine, expected);
        }
    }

    #[test]
    fn natural_syntax_and_stdlib() {
        let src = r#"
set name to "Forge"
define greet(who) {
    return "Hello, {who}!"
}
say greet(name)
let nums = [1, 2, 3, 4]
say sum(map(nums, fn(x) { return x * x }))
say math.sqrt(16)
say json.stringify({ a: 1 })
say regex.test("abc123", "[0-9]+")
let label = when 15 {
    < 13 -> "kid",
    else -> "older"
}
say label
"#;
        for engine in ENGINES {
            let r = run_on(engine, src);
            assert!(r.ok, "{engine:?}: {:?}", r.error);
            assert_eq!(
                r.stdout, "Hello, Forge!\n30\n4\n{\"a\": 1}\ntrue\nolder\n",
                "{engine:?}"
            );
        }
    }

    #[test]
    fn stderr_is_captured_separately() {
        for engine in ENGINES {
            let r = run_on(engine, "let x = sus(42)\nsay x\n");
            assert!(r.ok, "{engine:?}: {:?}", r.error);
            assert_eq!(r.stdout, "42\n");
            assert!(r.stderr.contains("SUS CHECK"), "{engine:?}: {}", r.stderr);
            assert_eq!(r.output.len(), 2);
            assert_eq!(r.output[0].stream, "stderr");
        }
    }

    #[test]
    fn syntax_errors_have_a_position() {
        let r = run_on(Engine::Auto, "say \"ok\"\nlet = 5\n");
        let e = r.error.expect("syntax error");
        assert_eq!(e.kind, ErrorKind::Syntax);
        assert_eq!(e.line, 2);
        assert!(r.stdout.is_empty(), "nothing runs");
    }

    #[test]
    fn runtime_errors_have_a_position_and_keep_output() {
        for engine in ENGINES {
            let r = run_on(engine, "say \"before\"\nlet x = 1\nlet y = x / 0\n");
            assert!(!r.ok);
            assert_eq!(r.stdout, "before\n", "{engine:?}");
            let e = r.error.expect("runtime error");
            assert_eq!(e.kind, ErrorKind::Runtime, "{engine:?}");
            assert_eq!(e.line, 3, "{engine:?}: {}", e.message);
        }
    }

    #[test]
    fn infinite_loops_hit_the_budget_on_every_engine() {
        for engine in ENGINES {
            let r = run(
                "let mut i = 0\nwhile true { i = i + 1 }\n",
                &RunOptions {
                    engine,
                    max_instructions: Some(1_000_000),
                    max_steps: Some(100_000),
                    ..RunOptions::default()
                },
            );
            let e = r.error.expect("limit");
            assert_eq!(e.kind, ErrorKind::Limit, "{engine:?}: {}", e.message);
            assert!(e.message.contains("fuel exhausted"), "{}", e.message);
        }
    }

    #[test]
    fn the_budget_cannot_be_caught() {
        let src = "while true {\n  try {\n    while true { }\n  } catch e {\n    say \"caught\"\n  }\n}\n";
        for engine in ENGINES {
            let r = run(
                src,
                &RunOptions {
                    engine,
                    max_instructions: Some(100_000),
                    max_steps: Some(10_000),
                    ..RunOptions::default()
                },
            );
            assert_eq!(
                r.error.map(|e| e.kind),
                Some(ErrorKind::Limit),
                "{engine:?}"
            );
            assert!(
                r.stdout.len() <= "caught\n".len(),
                "{engine:?}: {}",
                r.stdout
            );
        }
    }

    #[test]
    fn output_is_capped() {
        let r = run(
            "let mut i = 0\nwhile i < 1000 { say \"0123456789\"\n i = i + 1 }\n",
            &RunOptions {
                max_output: Some(100),
                ..RunOptions::default()
            },
        );
        assert!(r.truncated);
        assert_eq!(r.stdout.len(), 100);
    }

    #[test]
    fn host_capabilities_fail_clearly() {
        let cases = [
            "http.get(\"https://example.com\")",
            "fs.read(\"/etc/passwd\")",
            "sh(\"ls\")",
            "db.open(\":memory:\")",
            "env.get(\"HOME\")",
            "fetch(\"https://example.com\")",
            "os.hostname()",
        ];
        for engine in ENGINES {
            for src in cases {
                let r = run_on(engine, src);
                let e = r
                    .error
                    .unwrap_or_else(|| panic!("{engine:?} {src} should fail"));
                assert!(
                    e.message
                        .contains("not available in the browser playground"),
                    "{engine:?} {src}: {}",
                    e.message
                );
            }
        }
    }

    #[test]
    fn thread_constructs_are_rejected_with_a_position() {
        for (src, line) in [
            ("say 1\nlet h = spawn { return 1 }\n", 2),
            ("squad {\n}\n", 1),
            ("say 1\n\ntimeout 1 seconds { say 2 }\n", 3),
            ("schedule every 1 seconds { say 1 }\n", 1),
        ] {
            let r = run_on(Engine::Auto, src);
            let e = r.error.expect("unsupported");
            assert_eq!(e.kind, ErrorKind::Unsupported, "{src}");
            assert_eq!(e.line, line, "{src}");
            assert!(r.stdout.is_empty(), "{src}: nothing runs");
        }
    }

    #[test]
    fn deep_recursion_is_an_error_not_a_crash() {
        let src = "fn down(n) { return down(n + 1) }\ndown(0)\n";
        for engine in ENGINES {
            let r = run_on(engine, src);
            let e = r.error.expect("depth error");
            assert!(
                e.message.contains("recursion") || e.message.contains("depth"),
                "{engine:?}: {}",
                e.message
            );
        }
    }

    /// No error the playground shows may suggest a CLI flag or variable.
    fn assert_no_cli_remedy(message: &str) {
        for needle in [
            "--max-fuel",
            "--max-memory",
            "--max-depth",
            "FORGE_MAX_DEPTH",
            "--allow",
            "Sandbox::",
        ] {
            assert!(!message.contains(needle), "{needle} in: {message}");
        }
    }

    #[test]
    fn limit_and_depth_hints_are_rewritten_for_the_browser() {
        for engine in ENGINES {
            let r = run(
                "let mut i = 0\nwhile true { i = i + 1 }\n",
                &RunOptions {
                    engine,
                    max_instructions: Some(10_000),
                    max_steps: Some(1_000),
                    ..RunOptions::default()
                },
            );
            let e = r.error.expect("limit");
            assert_eq!(e.kind, ErrorKind::Limit, "{engine:?}");
            assert!(e.message.starts_with("fuel exhausted"), "{}", e.message);
            assert!(
                e.message.contains("  hint: the playground stops each run"),
                "{engine:?}: {}",
                e.message
            );
            assert_no_cli_remedy(&e.message);

            let r = run_on(engine, "fn down(n) { return down(n + 1) }\ndown(0)\n");
            let e = r.error.expect("depth error");
            assert!(
                e.message.contains("the browser allows less recursion"),
                "{engine:?}: {}",
                e.message
            );
            assert_no_cli_remedy(&e.message);
        }
    }

    #[test]
    fn browser_message_maps_each_cli_hint() {
        // The core's own texts (runtime/limits.rs, runtime/recursion.rs,
        // semantics/errors.rs): each one is rewritten or dropped.
        let fuel = forge_lang::runtime::limits::fuel_exhausted_message(5);
        assert!(fuel.contains("--max-fuel"), "core text changed: {fuel}");
        let mapped = browser_message(&fuel);
        assert!(mapped.starts_with(fuel.lines().next().unwrap_or_default()));
        assert_no_cli_remedy(&mapped);

        let memory = forge_lang::runtime::limits::memory_exceeded_message(1 << 20);
        assert!(
            memory.contains("--max-memory"),
            "core text changed: {memory}"
        );
        let mapped = browser_message(&memory);
        assert!(mapped.contains("smaller pieces"), "{mapped}");
        assert_no_cli_remedy(&mapped);

        let denied = "fs.read is not permitted\n  hint: grant the capability with the --allow-* flag named in the message or in forge.toml [permissions]";
        assert_eq!(browser_message(denied), "fs.read is not permitted");

        let engine = browser_hint(
            "see the message above; run with --interp for a second opinion if it looks like an engine bug",
        )
        .expect("kept");
        assert!(engine.contains("switch the engine"), "{engine}");

        // Hints without CLI advice, and messages without hints, are kept.
        let plain = "cannot reassign\n  hint: declare the variable with `let mut` (or `set mut`) to allow reassignment";
        assert_eq!(browser_message(plain), plain);
        assert_eq!(browser_message("division by zero"), "division by zero");
    }

    #[test]
    fn check_reports_diagnostics() {
        assert!(check("let x = 1\nsay x\n").is_empty());
        let d = check("let = 1\n");
        assert_eq!(d[0].severity, "error");
        assert_eq!(d[0].line, 1);
        let typed = check("fn f(a: Int) -> Int { return a }\nf(\"x\")\n");
        assert!(typed[0].code.as_deref().is_some_and(|c| c.starts_with('T')));
    }

    #[test]
    fn format_matches_forge_fmt() {
        assert_eq!(format("let   x =  1\n"), "let x = 1\n");
    }

    #[test]
    fn options_parse_from_camel_case_json() {
        let o: RunOptions =
            serde_json::from_str(r#"{"engine":"interpreter","maxSteps":5,"maxOutput":10}"#)
                .expect("parse");
        assert_eq!(o.engine, Engine::Interp);
        assert_eq!(o.max_steps, Some(5));
        assert_eq!(o.max_output, Some(10));
        let d: RunOptions = serde_json::from_str("{}").expect("parse");
        assert_eq!(d.engine, Engine::Auto);
    }
}
