//! Regression tests for VM memory/runtime safety:
//! GC rooting across native callbacks (with GC stress mode), big-integer
//! construction, recursion limits, import cycles and Result matching.

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

fn run_vm(source: &str, stress: bool) -> Result<Vec<String>, VMError> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.gc.set_stress(stress);
    vm.execute(&chunk)?;
    Ok(vm.output.clone())
}

fn stress_output(source: &str) -> Vec<String> {
    run_vm(source, true).expect("vm error under GC stress")
}

/// Value of the program's last expression, rendered (REPL mode).
fn run_vm_value(source: &str, stress: bool) -> Result<String, VMError> {
    let program = parse_program(source);
    let chunk = compiler::compile_repl(&program).expect("compile error");
    let mut vm = VM::new();
    vm.gc.set_stress(stress);
    let value = vm.execute(&chunk)?;
    Ok(value.display(&vm.gc))
}

// ---------------------------------------------------------------------------
// GC rooting across native callbacks
// ---------------------------------------------------------------------------

#[test]
fn gc_map_results_survive_collection_in_callbacks() {
    // Every callback allocates; under stress mode a GC runs after each
    // allocation, so unrooted results of earlier callbacks would be freed.
    let out = stress_output(
        r#"
        let r = map(range(0, 200), fn(x) { return "item-" + str(x) + "-padding" })
        say r[0]
        say r[199]
        let mut ok = true
        for i in range(0, 200) { if r[i] != "item-" + str(i) + "-padding" { ok = false } }
        say ok
        "#,
    );
    assert_eq!(out, vec!["item-0-padding", "item-199-padding", "true"]);
}

#[test]
fn gc_map_objects_survive_collection_in_callbacks() {
    let out = stress_output(
        r#"
        let objs = map(range(0, 100), fn(i) { return {id: i, tags: [i, i + 1]} })
        say objs[5]
        say objs[99].tags[1]
        "#,
    );
    assert_eq!(out, vec![r#"{ "id": 5, "tags": [5, 6] }"#, "100"]);
}

#[test]
fn gc_callback_builtins_under_stress() {
    let out = stress_output(
        r#"
        let xs = map(range(0, 30), fn(x) { return "s" + str(x) })
        say len(filter(xs, fn(s) { let t = s + "!"
 return len(t) > 3 }))
        say reduce(xs, "", fn(acc, s) { return acc + s })
        let sorted = sort(range(0, 40), fn(a, b) { let t = "x" + str(a)
 return b - a })
        say sorted[0]
        say find(xs, fn(s) { let t = s + "?"
 return s == "s17" })
        say len(flat_map(range(0, 10), fn(i) { return ["a" + str(i), "b" + str(i)] }))
        let parts = partition(xs, fn(s) { let t = s + "?"
 return len(s) == 2 })
        say len(parts[0])
        let groups = group_by(xs, fn(s) { return "k" + str(len(s)) })
        say len(groups.k3)
        let sorted_keys = sort_by(xs, fn(s) { return "key-" + s })
        say sorted_keys[0]
        let mut n = 0
        for_each(xs, fn(s) { let t = s + "."
 n = n + 1 })
        say any(xs, fn(s) { return s + "" == "s29" })
        say all(xs, fn(s) { return len(s + "") >= 2 })
        "#,
    );
    assert_eq!(
        out,
        vec![
            "20",
            "s0s1s2s3s4s5s6s7s8s9s10s11s12s13s14s15s16s17s18s19s20s21s22s23s24s25s26s27s28s29",
            "39",
            "s17",
            "20",
            "10",
            "20",
            "s0",
            "true",
            "true",
        ]
    );
}

#[test]
fn gc_snapshot_survives_callback_mutating_source() {
    // The callback drops the source array's last references to its items;
    // the builtin's snapshot must keep them alive.
    let out = stress_output(
        r#"
        let mut src = map(range(0, 20), fn(i) { return "v" + str(i) })
        let out = map(src, fn(s) {
            src = []
            let junk = map(range(0, 5), fn(j) { return "junk" + str(j) })
            return s + "!"
        })
        say out[0]
        say out[19]
        "#,
    );
    assert_eq!(out, vec!["v0!", "v19!"]);
}

#[test]
fn gc_stream_pipeline_under_stress() {
    let out = stress_output(
        r#"
        let r = range(0, 50).stream().map(fn(x) { return "n" + str(x) }).filter(fn(s) { return len(s + "") == 3 }).collect()
        say len(r)
        say r[0]
        say r[39]
        "#,
    );
    assert_eq!(out, vec!["40", "n10", "n49"]);
}

/// Run every supported parity fixture with the GC collecting at every safe
/// point and require the same result as a normal run. Any value a builtin
/// holds without rooting shows up here as a mismatch or a `<freed>` value.
#[test]
fn gc_stress_matches_normal_run_on_parity_fixtures() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/supported");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("read parity fixtures")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "fg"))
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "no parity fixtures found");
    let mut checked = 0;
    for path in entries {
        let source = std::fs::read_to_string(&path).expect("read fixture");
        let normal = run_vm_value(&source, false);
        let stressed = run_vm_value(&source, true);
        match (normal, stressed) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a, b, "GC stress changed the result of {}", path.display());
                assert!(!b.contains("<freed>"), "{}: {}", path.display(), b);
                checked += 1;
            }
            (Err(a), Err(b)) => assert_eq!(a.message, b.message, "{}", path.display()),
            (a, b) => panic!(
                "{}: normal={:?} stressed={:?}",
                path.display(),
                a.map_err(|e| e.message),
                b.map_err(|e| e.message)
            ),
        }
    }
    assert!(checked > 10, "only {} fixtures checked", checked);
}

// ---------------------------------------------------------------------------
// Big integers never panic
// ---------------------------------------------------------------------------

#[test]
fn big_int_results_from_builtins_do_not_panic() {
    let out = run_vm(
        r#"
        say math.pow(2, 50)
        say math.pow(2, 62)
        say type(math.pow(2, 63))
        say math.abs(-140737488355329)
        say type(math.abs(-9223372036854775807 - 1))
        say math.max(140737488355328, 1)
        say math.min(-140737488355328, 1)
        say math.clamp(140737488355329, 0, 140737488355330)
        say [140737488355328].stream().sum()
        say type([9223372036854775807, 1].stream().sum())
        say type(sum([9223372036854775807, 1]))
        say range(140737488355328, 140737488355330)[1]
        say math.floor(140737488355328.5)
        "#,
        false,
    )
    .expect("vm error");
    assert_eq!(
        out,
        vec![
            "1125899906842624",
            "4611686018427387904",
            "Float",
            "140737488355329",
            "Float",
            "140737488355328",
            "-140737488355328",
            "140737488355329",
            "140737488355328",
            "Float",
            "Float",
            "140737488355329",
            "140737488355328",
        ]
    );
}

#[test]
fn math_overflow_policy_matches_interpreter() {
    use crate::interpreter::Value as IV;
    use crate::stdlib::math::call;
    assert_eq!(
        call("math.pow", vec![IV::Int(2), IV::Int(63)]).expect("pow"),
        IV::Float(9_223_372_036_854_775_808.0)
    );
    assert_eq!(
        call("math.pow", vec![IV::Int(2), IV::Int(62)]).expect("pow"),
        IV::Int(1 << 62)
    );
    assert!(matches!(
        call("math.abs", vec![IV::Int(i64::MIN)]).expect("abs"),
        IV::Float(_)
    ));
    assert!(matches!(
        call("math.floor", vec![IV::Float(1e300)]).expect("floor"),
        IV::Float(_)
    ));
    let r = call(
        "math.random_int",
        vec![IV::Int(i64::MIN), IV::Int(i64::MAX)],
    );
    assert!(matches!(r, Ok(IV::Int(_))));
}

// ---------------------------------------------------------------------------
// Recursion limits
// ---------------------------------------------------------------------------

#[test]
fn runaway_recursion_is_a_catchable_error() {
    // The inner closure keeps the function off the auto-JIT path (JIT'd
    // self-recursion runs natively and is not depth-checked).
    let out = run_vm(
        r#"
        fn inf(n) { let f = fn() { return n }
 return "a" + inf(n + 1) }
        try { inf(0) } catch e { say e.message }
        say "alive"
        "#,
        false,
    )
    .expect("vm error");
    assert!(
        out[0].starts_with("maximum recursion depth exceeded"),
        "{:?}",
        out
    );
    assert_eq!(out[1], "alive");
}

#[test]
fn uncaught_recursion_error_uses_shared_message() {
    let err = run_vm(
        "fn inf(n) { let f = fn() { return n }\n return \"a\" + inf(n + 1) }\ninf(0)",
        false,
    )
    .expect_err("must fail");
    assert!(
        err.message.starts_with("maximum recursion depth exceeded"),
        "{}",
        err.message
    );
}

// ---------------------------------------------------------------------------
// Import cycles
// ---------------------------------------------------------------------------

#[test]
fn import_cycle_is_reported_once() {
    let dir = std::env::temp_dir().join(format!("forge_vm_cycle_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let a = dir.join("a.fg");
    let b = dir.join("b.fg");
    std::fs::write(
        &a,
        format!("import \"{}\"\nfn fa() {{ return 1 }}\n", b.display()),
    )
    .expect("write a");
    std::fs::write(
        &b,
        format!("import \"{}\"\nfn fb() {{ return 2 }}\n", a.display()),
    )
    .expect("write b");
    let err = run_vm(&format!("import \"{}\"\nsay fa()", a.display()), false)
        .expect_err("cycle must fail");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        err.message.starts_with("circular import: "),
        "{}",
        err.message
    );
    assert!(err.message.contains("a.fg -> "), "{}", err.message);
    assert!(err.message.contains("b.fg -> "), "{}", err.message);
    assert_eq!(
        err.message.matches("circular import").count(),
        1,
        "{}",
        err.message
    );
}
