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

/// Whether any chunk of `source` (compiled under `edition`) contains
/// `PushLocal`.
fn uses_push_local(source: &str, edition: crate::semantics::edition::Edition) -> bool {
    fn walk(chunk: &crate::vm::bytecode::Chunk) -> bool {
        chunk.code.iter().any(|&i| {
            crate::vm::bytecode::decode_op(i) == crate::vm::bytecode::OpCode::PushLocal as u8
        }) || chunk.prototypes.iter().any(walk)
    }
    let _edition = crate::semantics::edition::scope(Some(edition));
    let chunk = compiler::compile(&parse_program(source)).expect("compile error");
    walk(&chunk)
}

#[test]
fn push_assignment_compiles_to_push_local_in_every_edition() {
    use crate::semantics::edition::Edition;
    let assign = "fn f() {\n let mut xs = []\n xs = push(xs, 1)\n return xs\n}";
    let statement = "fn f() {\n let mut xs = []\n push(xs, 1)\n return xs\n}";
    for edition in Edition::ALL {
        assert!(uses_push_local(assign, *edition), "{edition}");
    }
    // Edition 2027: the free function is pure, so the statement form is an
    // ordinary (discarded) call.
    assert!(uses_push_local(statement, Edition::E2026));
    assert!(!uses_push_local(statement, Edition::E2027));
}

#[test]
fn pushing_object_and_tuple_literals_compiles_to_push_local() {
    use crate::semantics::edition::Edition;
    for value in ["{id: i, tags: [i, 1]}", "(i, \"x\")", "{nested: {a: i}}"] {
        let source = format!(
            "fn f(n) {{\n let mut xs = []\n for i in range(n) {{ xs.push({value}) }}\n return xs\n}}"
        );
        assert!(uses_push_local(&source, Edition::E2026), "{value}");
    }
    // A literal holding a call runs user code: it stays a method call.
    let call = "fn g() { return 1 }\nfn f() {\n let mut xs = []\n xs.push({v: g()})\n return xs\n}";
    assert!(!uses_push_local(call, Edition::E2026));
}

#[test]
fn pushed_object_literals_are_invisible_to_aliases() {
    assert_fn_output(
        r#"
        let mut rows = []
        rows.push({id: 1})
        let before = rows
        rows.push({id: 2, tags: [1, 2]})
        rows.push((3, "x"))
        say len(before)
        say len(rows)
        say rows[1].tags
        say rows[2]
        "#,
        &["1", "3", "[1, 2]", "(3, x)"],
    );
}

#[test]
fn push_assignment_in_place_is_invisible_to_aliases() {
    for edition in crate::semantics::edition::Edition::ALL {
        let _edition = crate::semantics::edition::scope(Some(*edition));
        assert_fn_output(
            r#"
            let mut a = []
            a = push(a, 1)
            let b = a
            a = push(a, 2)
            let holder = [a]
            a = push(a, 3)
            println(b)
            println(holder[0])
            println(a)
            "#,
            &["[1]", "[1, 2]", "[1, 2, 3]"],
        );
    }
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

// ---------------------------------------------------------------------------
// Sets: `s.add(x)` / `s.remove(x)` statements update an owned set in place
// (`UpdateSetLocal`), and the hash index answers lookups.
// ---------------------------------------------------------------------------

fn uses_update_set_local(source: &str) -> bool {
    fn walk(chunk: &crate::vm::bytecode::Chunk) -> bool {
        chunk.code.iter().any(|&i| {
            crate::vm::bytecode::decode_op(i) == crate::vm::bytecode::OpCode::UpdateSetLocal as u8
        }) || chunk.prototypes.iter().any(walk)
    }
    walk(&compiler::compile(&parse_program(source)).expect("compile error"))
}

#[test]
fn set_add_and_remove_statements_compile_to_update_set_local() {
    assert!(uses_update_set_local(
        "fn f() {\n let mut s = set([])\n s.add(1)\n return s\n}"
    ));
    assert!(uses_update_set_local("let mut s = set([1])\ns.remove(1)"));
    // Expression position, an immutable binding or a non-simple argument
    // keep the generic lowering.
    assert!(!uses_update_set_local(
        "fn f() {\n let mut s = set([])\n let t = s.add(1)\n return t\n}"
    ));
    assert!(!uses_update_set_local("let s = set([])\ns.add(1)"));
    assert!(!uses_update_set_local(
        "fn g() { return 1 }\nlet mut s = set([])\ns.add(g())"
    ));
}

#[test]
fn set_updates_in_place_are_invisible_to_aliases() {
    assert_fn_output(
        r#"
        let mut s = set([])
        s.add(1)
        s.add(2)
        let a = s
        s.add(3)
        s.remove(1)
        let holder = [s]
        s.add(4)
        s.remove(2)
        let f = fn() { return len(s) }
        s.add(5)
        say a
        say holder[0]
        say s
        say f()
        "#,
        &["set(1, 2)", "set(2, 3)", "set(3, 4, 5)", "3"],
    );
}

#[test]
fn set_updates_keep_set_semantics_in_place() {
    assert_fn_output(
        r#"
        let mut s = set([])
        for i in range(40) {
            s.add(i % 20)
            s.add(float(i % 20))
        }
        say len(s)
        for i in range(20) {
            if i % 4 != 0 { s.remove(float(i)) }
        }
        say s
        let mut items = []
        for x in s { items.push(x) }
        say items
        s.add(1)
        say [s.has(1.0), s.has(2), len(s)]
        "#,
        &[
            "20",
            "set(0, 4, 8, 12, 16)",
            "[0, 4, 8, 12, 16]",
            "[true, false, 6]",
        ],
    );
}

#[test]
fn update_set_local_with_an_unknown_operation_is_rejected() {
    use crate::vm::bytecode::{encode_abc, Chunk, OpCode};
    let mut chunk = Chunk::new("bad");
    chunk.max_registers = 2;
    chunk.emit_at(encode_abc(OpCode::UpdateSetLocal, 0, 1, 7), 1, 0);
    chunk.emit_at(encode_abc(OpCode::ReturnNull, 0, 0, 0), 1, 0);
    let err = crate::vm::verify::verify_chunk(&chunk).expect_err("must be rejected");
    assert!(err.to_string().contains("unknown operation"), "{}", err);
}
