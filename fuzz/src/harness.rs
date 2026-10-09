//! Fuzz-target bodies, shared by the cargo-fuzz targets in
//! `fuzz/fuzz_targets/` and the stable smoke harness `tests/fuzz_smoke.rs`
//! (which includes this file with `#[path]`). Each `check_*` function must
//! return normally for every input; a panic, abort or hang is a bug.

use forge_lang::interpreter::Interpreter;
use forge_lang::lexer::Lexer;
use forge_lang::parser::ast::Program;
use forge_lang::parser::Parser;
use forge_lang::permissions::{self, Capabilities};
use forge_lang::vm::bytecode::{decode_op, Chunk, Constant, OpCode};
use forge_lang::vm::machine::VM;
use forge_lang::vm::{compiler, serialize, verify};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Wall-clock budget for one execution in the differential and bytecode
/// targets. Programs that exceed it are discarded, not reported.
pub const RUN_BUDGET: Duration = Duration::from_secs(2);

/// Lex + parse arbitrary text.
pub fn parse(src: &str) -> Option<Program> {
    parse_result(src).ok()
}

/// Lex + parse, keeping the error message.
pub fn parse_result(src: &str) -> Result<Program, String> {
    let tokens = Lexer::new(src).tokenize().map_err(|e| e.to_string())?;
    Parser::new(tokens)
        .parse_program()
        .map_err(|e| e.to_string())
}

/// Target `parse`: the lexer and parser never panic.
pub fn check_parse(data: &[u8]) {
    let src = String::from_utf8_lossy(data);
    let _ = parse(&src);
}

/// Target `compile`: whatever parses compiles without panicking, and
/// whatever compiles passes the bytecode verifier and survives a
/// serialize/deserialize round trip unchanged.
pub fn check_compile(data: &[u8]) {
    let src = String::from_utf8_lossy(data);
    let Some(program) = parse(&src) else { return };
    for chunk in [
        compiler::compile(&program),
        compiler::compile_repl(&program),
    ]
    .into_iter()
    .flatten()
    {
        if let Err(e) = verify::verify_chunk(&chunk) {
            panic!("compiler output rejected by the verifier: {e}\n--- source ---\n{src}");
        }
        let bytes = serialize::serialize_chunk(&chunk).expect("serialize compiled chunk");
        let restored = match serialize::deserialize_chunk(&bytes) {
            Ok(c) => c,
            Err(e) => panic!("round trip of compiled bytecode failed: {e}\n--- source ---\n{src}"),
        };
        assert_eq!(restored.code, chunk.code, "round trip changed the code");
    }
}

/// Target `bytecode`: deserializing arbitrary bytes never panics or
/// allocates unboundedly, and bytecode the verifier accepts runs on the VM
/// without panicking (under a deny-all permission policy and a time
/// budget; chunks that could block or touch the host are not run).
pub fn check_bytecode(data: &[u8]) {
    let Ok(chunk) = serialize::deserialize_chunk(data) else {
        return;
    };
    if !safe_to_run(&chunk) {
        return;
    }
    let _policy = permissions::scope(Arc::new(Capabilities::deny_all()));
    let mut vm = VM::new();
    let _watchdog = Watchdog::start(vm.cancel_flag(), RUN_BUDGET);
    let _ = vm.execute(&chunk);
}

/// Opcodes that start threads, block, or call out of the process.
fn opcode_is_host_effect(op: OpCode) -> bool {
    matches!(
        op,
        OpCode::Spawn
            | OpCode::Schedule
            | OpCode::Watch
            | OpCode::Await
            | OpCode::Ask
            | OpCode::SquadBegin
            | OpCode::SquadEnd
    )
}

/// Globals reachable only by name (`GetGlobal` needs a string constant), so
/// a chunk without these names cannot block, sleep, read stdin, or reach
/// the host even before the deny-all policy is consulted.
const BLOCKING_OR_HOST_NAMES: &[&str] = &[
    "wait",
    "sleep",
    "input",
    "prompt",
    "receive",
    "try_receive",
    "select",
    "channel",
    "send",
    "exit",
    "sh",
    "shell",
    "sh_lines",
    "sh_json",
    "sh_ok",
    "run_command",
    "pipe_to",
    "fetch",
    "http",
    "io",
    "fs",
    "os",
    "env",
    "db",
    "pg",
    "mysql",
    "time",
    "ws",
    "net",
    "cd",
    "await_timeout",
    "download",
    "crawl",
    "watch",
    "schedule",
    "import",
    "ask",
    "listen",
    "serve",
];

fn safe_to_run(chunk: &Chunk) -> bool {
    let code_ok = chunk
        .code
        .iter()
        .all(|&inst| OpCode::try_from(decode_op(inst)).is_ok_and(|op| !opcode_is_host_effect(op)));
    let names_ok = chunk.constants.iter().all(|c| match c {
        Constant::Str(s) => !BLOCKING_OR_HOST_NAMES.contains(&s.as_str()),
        _ => true,
    });
    code_ok && names_ok && chunk.prototypes.iter().all(safe_to_run)
}

/// Sets a cancellation flag when the budget runs out, unless dropped first.
pub struct Watchdog {
    done: Arc<(Mutex<bool>, Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Watchdog {
    pub fn start(flag: Arc<AtomicBool>, budget: Duration) -> Self {
        Self::start_many(vec![flag], budget)
    }

    pub fn start_many(flags: Vec<Arc<AtomicBool>>, budget: Duration) -> Self {
        let done = Arc::new((Mutex::new(false), Condvar::new()));
        let d = Arc::clone(&done);
        let thread = std::thread::spawn(move || {
            let (lock, cvar) = &*d;
            let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            let (guard, _) = cvar
                .wait_timeout_while(guard, budget, |finished| !*finished)
                .unwrap_or_else(|e| e.into_inner());
            if !*guard {
                for f in &flags {
                    f.store(true, Ordering::Release);
                }
            }
        });
        Self {
            done,
            thread: Some(thread),
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.done;
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cvar.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// What one engine made of a program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Displayed value of the program's final expression.
    Value(String),
    /// A runtime error (messages are not compared: the engines word some
    /// errors differently).
    Error(String),
    /// The VM compiler does not support the program (it would fall back to
    /// the interpreter), or the budget ran out: nothing to compare.
    Skipped,
}

pub fn run_interpreter(program: &Program) -> Outcome {
    let _policy = permissions::scope(Arc::new(Capabilities::deny_all()));
    let mut interp = Interpreter::new();
    // Swallow any output instead of writing to the fuzzer's stdout.
    interp.output_sink = Some(Arc::new(Mutex::new(Vec::new())));
    let cancel = Arc::new(AtomicBool::new(false));
    interp.cancelled = Arc::clone(&cancel);
    let watchdog = Watchdog::start(Arc::clone(&cancel), RUN_BUDGET);
    let result = interp.run_repl(program);
    drop(watchdog);
    if cancel.load(Ordering::Acquire) {
        return Outcome::Skipped;
    }
    match result {
        Ok(v) => Outcome::Value(v.to_string()),
        Err(e) => Outcome::Error(e.message),
    }
}

pub fn run_vm(program: &Program) -> Outcome {
    let chunk = match compiler::compile_repl(program) {
        Ok(chunk) => chunk,
        Err(e) if e.is_unsupported() => return Outcome::Skipped,
        Err(e) => return Outcome::Error(e.message),
    };
    let _policy = permissions::scope(Arc::new(Capabilities::deny_all()));
    let mut vm = VM::new();
    let cancel = vm.cancel_flag();
    let watchdog = Watchdog::start(Arc::clone(&cancel), RUN_BUDGET);
    let result = vm.execute(&chunk);
    drop(watchdog);
    if cancel.load(Ordering::Acquire) {
        return Outcome::Skipped;
    }
    match result {
        Ok(v) => Outcome::Value(v.display(&vm.gc)),
        Err(e) => Outcome::Error(e.message),
    }
}

/// Run `src` on both engines; `Err` describes a divergence.
pub fn differential(src: &str) -> Result<(), String> {
    let program = match parse_result(src) {
        Ok(p) => p,
        Err(e) => return Err(format!("generated program does not parse: {e}\n{src}")),
    };
    let interp = run_interpreter(&program);
    let vm = run_vm(&program);
    let same = match (&interp, &vm) {
        (Outcome::Skipped, _) | (_, Outcome::Skipped) => true,
        (Outcome::Error(_), Outcome::Error(_)) => true,
        (a, b) => a == b,
    };
    if same {
        Ok(())
    } else {
        Err(format!(
            "interpreter and VM disagree\n--- program ---\n{src}--- interpreter ---\n{interp:?}\n--- vm ---\n{vm:?}"
        ))
    }
}

/// Target `differential`: build a program from `data` and compare engines.
pub fn check_differential(data: &[u8]) {
    let mut u = arbitrary::Unstructured::new(data);
    let Ok(src) = crate::gen::program(&mut u) else {
        return;
    };
    if let Err(report) = differential(&src) {
        panic!("{report}");
    }
}
