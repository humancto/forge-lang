//! `VM::call_callback` (`vm::callback`): callbacks from native builtins
//! must behave exactly like `call_value` (results, errors, stack traces,
//! limits, GC rooting) with the JIT off, auto-tiered and eager, while
//! JIT-able callbacks really run natively from the cached selection.

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::vm::compiler;
#[cfg(feature = "jit")]
use crate::vm::jit::tier::JitMode;
use crate::vm::machine::{VMError, VM};

fn compile(source: &str) -> crate::vm::bytecode::Chunk {
    let tokens = Lexer::new(source).tokenize().expect("lex");
    let program = Parser::new(tokens).parse_program().expect("parse");
    compiler::compile(&program).expect("compile")
}

/// Run `source` on a fresh VM; `setup` configures it first.
fn run_with(source: &str, setup: impl FnOnce(&mut VM)) -> (Vec<String>, Option<VMError>, VM) {
    let chunk = compile(source);
    let mut vm = VM::new();
    setup(&mut vm);
    let err = vm.execute(&chunk).err();
    (vm.output.clone(), err, vm)
}

/// Output and error message with the JIT off, after asserting that the
/// auto-tiered and eager JIT, and a GC-stressed run, produce exactly the
/// same.
fn run_all_modes(source: &str) -> (Vec<String>, Option<String>) {
    #[cfg(feature = "jit")]
    let (out, err, _) = run_with(source, |vm| vm.set_jit_mode(JitMode::Off));
    #[cfg(not(feature = "jit"))]
    let (out, err, _) = run_with(source, |_| {});
    let err = err.map(|e| e.message);
    #[cfg(feature = "jit")]
    for mode in [JitMode::Auto, JitMode::Eager] {
        let (o, e, _) = run_with(source, |vm| vm.set_jit_mode(mode));
        assert_eq!(o, out, "{mode:?}: output differs from the JIT-off VM");
        assert_eq!(e.map(|e| e.message), err, "{mode:?}: error differs");
    }
    let (o, e, _) = run_with(source, |vm| vm.gc.set_stress(true));
    assert_eq!(o, out, "GC stress: output differs");
    assert_eq!(e.map(|e| e.message), err, "GC stress: error differs");
    (out, err)
}

fn ok_output(source: &str) -> Vec<String> {
    let (out, err) = run_all_modes(source);
    assert!(err.is_none(), "unexpected error: {err:?}");
    out
}

#[test]
fn array_builtins_with_closures() {
    let out = ok_output(
        r#"
        let xs = range(0, 500)
        let ys = map(xs, fn(x) { return x * 3 })
        let zs = filter(ys, fn(x) { return x % 2 == 0 })
        println(len(zs))
        println(reduce(zs, 0, fn(acc, x) { return acc + x }))
        println(any(xs, fn(x) { return x > 498 }))
        println(all(xs, fn(x) { return x >= 0 }))
        println(find(xs, fn(x) { return x * x > 1000 }))
        let s = sort(map(xs, fn(x) { return (x * 7919) % 501 }), fn(a, b) { return b - a })
        println(s[0])
        println(s[499])
        println(len(flat_map(range(0, 300), fn(x) { return [x, x] })))
        println(sort_by(["ccc", "a", "bb"], fn(w) { return len(w) }))
        println(partition(range(0, 10), fn(x) { return x < 3 }))
        println(group_by(range(0, 6), fn(x) { return x % 2 }))
        "#,
    );
    assert_eq!(
        out,
        vec![
            "250",
            "186750",
            "true",
            "true",
            "32",
            "500",
            "0",
            "600",
            "[a, bb, ccc]",
            "[[0, 1, 2], [3, 4, 5, 6, 7, 8, 9]]",
            "{ \"0\": [0, 2, 4], \"1\": [1, 3, 5] }",
        ]
    );
}

#[test]
fn method_forms_and_stream_stages() {
    let out = ok_output(
        r#"
        let xs = range(0, 400)
        println(xs.map(fn(x) { return x + 1 }).filter(fn(x) { return x % 5 == 0 }).reduce(0, fn(a, x) { return a + x }))
        let total = xs.stream().filter(fn(x) { return x % 3 == 0 }).map(fn(x) { return x * 2 }).reduce(0, fn(a, x) { return a + x })
        println(total)
        println(xs.stream().map(fn(x) { return x * x }).find(fn(x) { return x > 2000 }))
        println(xs.stream().any(fn(x) { return x == 399 }))
        println(xs.stream().all(fn(x) { return x < 399 }))
        let mut seen = 0
        xs.stream().for_each(fn(x) { seen = seen + x })
        println(seen)
        "#,
    );
    assert_eq!(
        out,
        vec!["16200", "53466", "Some(2025)", "true", "false", "79800"]
    );
}

#[test]
fn signature_changes_and_overflow_deopt() {
    // Ints, then floats, then strings through the same callback; then an
    // i64 overflow after the callback is hot (native code deopts, the VM
    // promotes to float).
    let out = ok_output(
        r#"
        let f = fn(x) { return x + x }
        let mut items = []
        for i in range(0, 300) { items.push(i) }
        for i in range(0, 300) { items.push(i * 0.5) }
        items.push("ab")
        items.push(4611686018427387904)
        items.push(7)
        let r = map(items, f)
        println(r[299])
        println(r[599])
        println(r[600])
        println(r[601])
        println(r[602])
        "#,
    );
    assert_eq!(out, vec!["598", "299", "abab", "9223372036854776000", "14"]);
}

#[test]
fn guards_see_captures_reassigned_by_another_stage() {
    // The second stage calls a captured function; the first stage (not
    // JIT-able: it assigns a capture) replaces that function half way.
    // Native code for stage two must never keep calling the old one.
    let out = ok_output(
        r#"
        fn run() {
            let mut h = fn(y) { return y + 1 }
            let swap = fn(y) { return y * 100 }
            let out = range(0, 600).stream().map(fn(x) {
                if x == 300 { h = swap }
                return x
            }).map(fn(x) { return h(x) }).collect()
            println(out[299])
            println(out[300])
            println(out[599])
        }
        run()
        "#,
    );
    assert_eq!(out, vec!["300", "30000", "59900"]);
}

#[test]
fn user_defined_map_shadows_the_builtin() {
    let out = ok_output(
        r#"
        fn map(xs, f) { return "mine: " + str(len(xs)) }
        println(map([1, 2, 3], fn(x) { return x }))
        println(filter([1, 2, 3], fn(x) { return x > 1 }))
        "#,
    );
    assert_eq!(out, vec!["mine: 3", "[2, 3]"]);
}

#[test]
fn more_closures_than_the_memo_holds() {
    let out = ok_output(
        &r#"
        let r = range(0, 300).stream()
            .map(fn(x) { return x + 1 })
            .map(fn(x) { return x * 2 })
            .filter(fn(x) { return x % 3 != 0 })
            .map(fn(x) { return x - 1 })
            .map(fn(x) { return x * x })
            .filter(fn(x) { return x % 2 == 1 })
            .reduce(0, fn(a, x) { return a + x })
        println(r)
        "#
        .replace("\n            .", "."),
    );
    assert_eq!(out.len(), 1);
}

#[test]
fn non_closure_callbacks() {
    let out = ok_output(
        r#"
        println(map(["1", "22", "333"], int))
        println(map([1, 2], str))
        println(map(["a", "bb"], len))
        "#,
    );
    assert_eq!(out, vec!["[1, 22, 333]", "[1, 2]", "[1, 2]"]);
}

#[test]
fn lenient_arity_nested_and_mutating_callbacks() {
    let out = ok_output(
        r#"
        println(map([1, 2], fn(x, i) { return [x, i] }))
        println(map([1, 2], fn() { return 0 }))
        println(map([[1, 2], [3]], fn(row) { return map(row, fn(v) { return v * 10 }) }))
        let mut count = 0
        let ys = map(range(0, 300), fn(x) { count = count + 1
            return x })
        println(count)
        println(len(ys))
        "#,
    );
    assert_eq!(
        out,
        vec![
            "[[1, null], [2, null]]",
            "[0, 0]",
            "[[10, 20], [30]]",
            "300",
            "300"
        ]
    );
}

#[test]
fn callback_errors_propagate_unchanged() {
    let src = r#"
        let xs = range(0, 300)
        let ys = map(xs, fn(x) {
            if x == 250 { return x / 0 }
            return x + 1
        })
        println("unreachable")
        "#;
    let (out, err) = run_all_modes(src);
    assert!(out.is_empty());
    let msg = err.expect("division by zero must propagate");
    assert!(msg.contains("division by zero"), "{msg}");

    // The stack trace names the callback, as before.
    let (_, err, _) = run_with(src, |_| {});
    let err = err.expect("error");
    assert_eq!(
        err.stack_trace.first().map(|f| f.function.as_str()),
        Some("<lambda>")
    );

    let caught = ok_output(
        r#"
        let mut got = ""
        try {
            map(range(0, 300), fn(x) {
                assert(x != 290, "bad item")
                return x
            })
        } catch e {
            got = e.message
        }
        println(got)
        let inner = map([1, 2, 3], fn(x) {
            try { assert(x != 2, "two") } catch e { return e.message }
            return x
        })
        println(inner)
        "#,
    );
    assert_eq!(
        caught,
        vec![
            "assertion failed: bad item",
            "[1, assertion failed: two, 3]"
        ]
    );
}

#[test]
fn recursion_limit_applies_inside_callbacks() {
    let (out, err) = run_all_modes(
        r#"
        fn down(n) { return map([n], fn(x) { return down(x + 1) }) }
        try { down(0) } catch e { println(e.message) }
        "#,
    );
    assert!(err.is_none(), "{err:?}");
    assert_eq!(out.len(), 1);
    assert!(
        out[0].starts_with("maximum recursion depth exceeded"),
        "{out:?}"
    );
}

#[test]
fn cancellation_is_polled_per_callback() {
    let chunk = compile("fn f(x) { return x }");
    let mut vm = VM::new();
    vm.execute(&chunk).expect("setup");
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    vm.set_cancel_flag(flag.clone());
    let f = *vm.globals.get("f").expect("f is a global");
    let ok = vm.call_callback(f, &[crate::vm::value::Value::bool_val(true)]);
    assert!(ok.is_ok());
    flag.store(true, std::sync::atomic::Ordering::Release);
    let err = vm
        .call_callback(f, &[crate::vm::value::Value::bool_val(true)])
        .expect_err("cancelled");
    assert_eq!(err.message, "task cancelled");
}

#[test]
fn fuel_limits_apply_inside_callbacks() {
    use crate::runtime::limits::{self, Budget, Limits};
    let chunk = compile("let ys = map(range(0, 100000), fn(x) { return x + 1 })\nprintln(len(ys))");
    let mut used = Vec::new();
    for _ in 0..2 {
        let budget = Budget::new(Limits {
            max_fuel: Some(20_000),
            ..Limits::none()
        });
        let _scope = limits::scope(Some(budget.clone()));
        let mut vm = VM::new();
        let err = vm.execute(&chunk).expect_err("fuel must run out");
        assert!(
            err.message.starts_with(limits::FUEL_EXHAUSTED),
            "{}",
            err.message
        );
        assert!(vm.output.is_empty());
        used.push(budget.fuel_used());
    }
    // Deterministic, and every unit was charged.
    assert_eq!(used, vec![20_000, 20_000]);
}

#[cfg(feature = "jit")]
#[test]
fn jit_able_callbacks_run_natively() {
    let src = r#"
        let ys = map(range(0, 2000), fn(x) { return x * 2 })
        let zs = filter(ys, fn(x) { return x % 3 == 0 })
        println(reduce(zs, 0, fn(acc, x) { return acc + x }))
        "#;
    let (out, err, vm) = run_with(src, |vm| vm.set_jit_mode(JitMode::Auto));
    assert!(err.is_none(), "{err:?}");
    assert_eq!(out, ok_output(src));
    // Three lambdas (2000 + 2000 + 667 calls); all but the first
    // HOT_THRESHOLD - 1 calls of each run natively (the call that makes a
    // function hot is the first native one).
    let hot = crate::vm::jit::tier::HOT_THRESHOLD as u64;
    let native = vm.jit.native_runs("<lambda>");
    assert_eq!(native, 2000 + 2000 + 667 - 3 * (hot - 1));
}

#[cfg(feature = "jit")]
#[test]
fn guards_are_rechecked_after_a_timeout_scope_opens() {
    // Inside `timeout` native code is not allowed (deadlines are polled by
    // the VM). The cached selection from before the scope must not be
    // reused inside it.
    let out = ok_output(
        r#"
        let f = fn(x) { return x * 2 }
        let a = map(range(0, 300), f)
        let mut b = []
        timeout 30 seconds {
            b = map(range(0, 300), f)
        }
        println(a[299] + b[299])
        "#,
    );
    assert_eq!(out, vec!["1196"]);
}
