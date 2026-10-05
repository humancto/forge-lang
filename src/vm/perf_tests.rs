//! Semantics guards for VM performance work: every fast path here must be
//! unobservable. Covers safe-point polling of `timeout` deadlines.

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::vm::compiler;
use crate::vm::machine::{VMError, VM};

fn parse_program(source: &str) -> crate::parser::ast::Program {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer error");
    let mut parser = Parser::new(tokens);
    parser.parse_program().expect("parse error")
}

/// Value of the program's last expression, rendered (REPL mode).
fn run_value(source: &str) -> Result<String, VMError> {
    let program = parse_program(source);
    let chunk = compiler::compile_repl(&program).expect("compile error");
    let mut vm = VM::new();
    let value = vm.execute(&chunk)?;
    Ok(value.display(&vm.gc))
}

// ---------------------------------------------------------------------------
// Safe-point polling of `timeout` deadlines
// ---------------------------------------------------------------------------

#[test]
fn timeout_interrupts_a_cpu_bound_loop() {
    let start = std::time::Instant::now();
    let err = run_value(
        r#"
        timeout 1 seconds {
            let mut i = 0
            while true {
                i = i + 1
            }
        }
        "#,
    )
    .expect_err("an endless loop inside `timeout` must be interrupted");
    assert!(
        err.message
            .contains("timeout: operation exceeded 1 second limit"),
        "{}",
        err.message
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn expired_timeout_fires_before_its_body_runs() {
    // A zero-second scope is already expired when it is pushed; the body
    // must not run (deadlines are polled at the first instruction after
    // `PushTimeout`, not only every SAFEPOINT_INTERVAL instructions).
    let out = run_value(
        r#"
        let mut ran = false
        let mut msg = ""
        try {
            timeout 0 seconds {
                ran = true
            }
        } catch e {
            msg = "caught"
        }
        "{ran}/{msg}"
        "#,
    )
    .expect("program runs");
    assert_eq!(out, "false/caught");
}

#[test]
fn timeout_catch_path_does_not_refire() {
    // After a timeout fires, its catch path pops the guard and raises once;
    // the error is catchable and execution continues normally.
    let out = run_value(
        r#"
        let mut n = 0
        try {
            timeout 1 seconds {
                while true {
                    n = n + 1
                }
            }
        } catch e {
            n = -1
        }
        let mut after = 0
        let mut i = 0
        while i < 5000 {
            after = after + 1
            i = i + 1
        }
        "{n}/{after}"
        "#,
    )
    .expect("program runs");
    assert_eq!(out, "-1/5000");
}
