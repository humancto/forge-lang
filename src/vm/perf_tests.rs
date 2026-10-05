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

// ---------------------------------------------------------------------------
// In-place updates of uniquely owned locals (`AddLocal`, `PushLocal`,
// `PopLocal`): no other binding may observe them.
// ---------------------------------------------------------------------------

fn run_output(source: &str, stress: bool) -> Vec<String> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.gc.set_stress(stress);
    vm.execute(&chunk).expect("vm error");
    vm.output.clone()
}

/// Runs `body` inside a function (so its variables are locals) with and
/// without GC stress, and checks both against `expected`.
fn assert_fn_output(body: &str, expected: &[&str]) {
    let source = format!("fn main_test() {{\n{body}\n}}\nmain_test()\n");
    for stress in [false, true] {
        assert_eq!(run_output(&source, stress), expected, "stress={stress}");
    }
}

#[test]
fn push_in_place_is_invisible_to_aliases() {
    assert_fn_output(
        r#"
        let mut a = []
        a.push(1)
        a.push(2)
        let b = a
        a.push(3)
        push(a, 4)
        say b
        say a
        let mut c = a
        c.push(5)
        say a
        say c
        "#,
        &["[1, 2]", "[1, 2, 3, 4]", "[1, 2, 3, 4]", "[1, 2, 3, 4, 5]"],
    );
}

#[test]
fn push_in_place_is_invisible_to_containers_and_callees() {
    assert_fn_output(
        r#"
        let mut a = [0]
        a.push(1)
        let holder = { items: a }
        let list = [a]
        a.push(2)
        say holder.items
        say list[0]
        fn pass_through(xs) {
            return xs
        }
        let kept = pass_through(a)
        a.push(3)
        say kept
        say a
        "#,
        &["[0, 1]", "[0, 1]", "[0, 1, 2]", "[0, 1, 2, 3]"],
    );
}

#[test]
fn push_in_place_is_invisible_to_closures() {
    assert_fn_output(
        r#"
        let mut a = []
        a.push(1)
        let snapshot = fn() { return len(a) }
        a.push(2)
        a.push(3)
        say snapshot()
        say len(a)
        "#,
        &["3", "3"],
    );
}

#[test]
fn push_result_used_as_value_keeps_value_semantics() {
    assert_fn_output(
        r#"
        let mut a = [1]
        a.push(2)
        let b = a.push(3)
        a.push(4)
        say b
        say a
        "#,
        &["[1, 2, 3]", "[1, 2, 3, 4]"],
    );
}

#[test]
fn pop_in_place_is_invisible_to_aliases() {
    assert_fn_output(
        r#"
        let mut a = [1, 2, 3]
        a.push(4)
        let b = a
        let x = a.pop()
        let y = pop(a)
        say x
        say y
        say a
        say b
        let mut e = []
        say e.pop()
        say e
        "#,
        &["4", "3", "[1, 2]", "[1, 2, 3, 4]", "null", "[]"],
    );
}

#[test]
fn string_append_in_place_is_invisible_to_aliases() {
    assert_fn_output(
        r#"
        let mut s = "a"
        s = s + "b"
        s += "c"
        let t = s
        s = s + "d"
        let list = [s]
        s += 1
        s = s + true
        say t
        say list[0]
        say s
        let mut u = s
        u += "!"
        say s
        say u
        s = s + s
        say s
        "#,
        &[
            "abc",
            "abcd",
            "abcd1true",
            "abcd1true",
            "abcd1true!",
            "abcd1trueabcd1true",
        ],
    );
}

#[test]
fn add_local_keeps_numeric_and_error_semantics() {
    assert_fn_output(
        r#"
        let mut i = 1
        i = i + 2
        i += 0.5
        say i
        let mut big = 140737488355327
        big += 1
        say big
        let mut m = 9223372036854775807
        m += 1
        say m
        let mut q = 1
        q = q + "x"
        say q
        "#,
        &["3.5", "140737488355328", "9223372036854776000", "1x"],
    );
}

#[test]
fn add_local_on_captured_local_updates_the_closure_cell() {
    assert_fn_output(
        r#"
        let mut s = ""
        let mut n = 0
        let read = fn() { return "{s}/{n}" }
        s = s + "x"
        s += "y"
        n += 2
        say read()
        "#,
        &["xy/2"],
    );
}

#[test]
fn interned_strings_are_not_mutated_by_in_place_append() {
    assert_fn_output(
        r#"
        let mut s = "ab"
        s += "c"
        s += "d"
        let lit = "abc"
        say lit
        say s
        say s == "abcd"
        "#,
        &["abc", "abcd", "true"],
    );
}
