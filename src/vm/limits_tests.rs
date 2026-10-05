//! Resource limits (`runtime::limits`) on the bytecode VM: deterministic
//! fuel, GC-heap memory accounting, size caps, task slots, and that limit
//! errors are fatal (not caught by `try`) while ordinary programs are
//! unaffected.

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::runtime::limits::{self, Budget, Limits, Trip};
use crate::vm::compiler;
use crate::vm::machine::{VMError, VM};
use std::sync::Arc;

struct Run {
    result: Result<(), VMError>,
    output: Vec<String>,
    budget: Arc<Budget>,
    heap_bytes: usize,
}

/// Run `source` on a fresh VM under a fresh budget for `limits`.
fn run(source: &str, limits: Limits) -> Run {
    let tokens = Lexer::new(source).tokenize().expect("lex");
    let program = Parser::new(tokens).parse_program().expect("parse");
    let chunk = compiler::compile(&program).expect("compile");
    let budget = Budget::new(limits);
    let _scope = limits::scope(Some(budget.clone()));
    let mut vm = VM::new();
    let result = vm.execute(&chunk).map(|_| ());
    Run {
        result,
        output: vm.output.clone(),
        budget,
        heap_bytes: vm.gc.memory_bytes(),
    }
}

fn fuel(n: u64) -> Limits {
    Limits {
        max_fuel: Some(n),
        ..Limits::none()
    }
}

fn err_message(r: &Run) -> String {
    match &r.result {
        Err(e) => e.message.clone(),
        Ok(()) => panic!("expected an error, got output {:?}", r.output),
    }
}

const COUNTER: &str = r#"
let mut i = 0
while true {
    i = i + 1
    if i % 100 == 0 { say i }
}
"#;

#[test]
fn fuel_exhaustion_is_typed_and_deterministic() {
    let a = run(COUNTER, fuel(50_000));
    let b = run(COUNTER, fuel(50_000));
    let msg = err_message(&a);
    assert!(msg.starts_with(limits::FUEL_EXHAUSTED), "{msg}");
    assert_eq!(a.budget.tripped(), Some(Trip::Fuel));
    // Same program, same budget: stops at exactly the same instruction.
    assert_eq!(a.output, b.output);
    assert!(!a.output.is_empty());
    // Every unit of fuel was spent, and not one more was settled.
    assert_eq!(a.budget.fuel_used(), 50_000);
    // A bigger budget gets further.
    let c = run(COUNTER, fuel(100_000));
    assert!(c.output.len() > a.output.len());
}

#[test]
fn fuel_counts_exactly_across_small_budgets() {
    // Exhaustion happens at the same output for every budget around a
    // safe-point boundary, and the settled fuel equals the budget.
    for n in [1u64, 2, 1023, 1024, 1025, 2049] {
        let r = run(COUNTER, fuel(n));
        assert!(err_message(&r).starts_with(limits::FUEL_EXHAUSTED));
        assert_eq!(r.budget.fuel_used(), n, "budget {n}");
    }
}

#[test]
fn fuel_error_is_not_catchable() {
    let r = run(
        r#"
        try { while true { } } catch e { say "caught: " + e.message }
        say "after"
        "#,
        fuel(10_000),
    );
    assert!(err_message(&r).starts_with(limits::FUEL_EXHAUSTED));
    assert!(r.output.is_empty(), "{:?}", r.output);
}

#[test]
fn fuel_trip_is_sticky_when_a_builtin_swallows_it() {
    // assert_throws observes the error, but the budget stays tripped and
    // the very next instruction fails again.
    let r = run(
        r#"
        let threw = assert_throws(fn() { while true { } })
        say "after"
        "#,
        fuel(10_000),
    );
    assert!(err_message(&r).starts_with(limits::FUEL_EXHAUSTED));
    assert!(!r.output.contains(&"after".to_string()));
}

#[test]
fn normal_programs_are_unaffected_by_limits() {
    let src = r#"
        fn fib(n) { if n < 2 { return n } return fib(n - 1) + fib(n - 2) }
        let xs = map(range(0, 100), fn(x) { return x * 2 })
        let mut s = ""
        for x in xs { s = s + str(x) }
        try { let y = 1 / 0 } catch e { say "caught" }
        say fib(15)
        say len(s)
    "#;
    let limited = run(
        src,
        Limits {
            max_fuel: Some(10_000_000),
            max_memory: Some(64 << 20),
            max_tasks: Some(4),
            ..Limits::none()
        },
    );
    assert!(limited.result.is_ok(), "{:?}", limited.result.err());
    let free = run(src, Limits::none());
    assert_eq!(limited.output, free.output);
    assert_eq!(limited.output[..2], ["caught", "610"]);
    assert!(limited.budget.fuel_used() > 0);
}

#[test]
fn memory_limit_trips_with_typed_error_after_collecting() {
    let r = run(
        r#"
        fn fill() {
            let mut kept = []
            let mut i = 0
            while true {
                let item = "chunk of text that is kept alive " + str(i)
                kept.push(item)
                i = i + 1
            }
        }
        fill()
        "#,
        Limits {
            max_memory: Some(1 << 20),
            ..Limits::none()
        },
    );
    let msg = err_message(&r);
    assert!(msg.starts_with(limits::MEMORY_LIMIT_EXCEEDED), "{msg}");
    assert_eq!(r.budget.tripped(), Some(Trip::Memory));
}

#[test]
fn garbage_does_not_count_against_the_memory_limit() {
    // Lots of allocation, little of it live: collections keep the heap
    // under the limit.
    let r = run(
        r#"
        let mut total = 0
        for i in range(0, 20000) {
            let tmp = "temporary string number " + str(i)
            total = total + len(tmp)
        }
        say total > 0
        "#,
        Limits {
            max_memory: Some(2 << 20),
            ..Limits::none()
        },
    );
    assert!(r.result.is_ok(), "{:?}", r.result.err());
    assert_eq!(r.output, vec!["true"]);
    assert!(r.heap_bytes < 2 << 20);
}

#[test]
fn memory_error_is_not_catchable() {
    let r = run(
        r#"
        fn fill() {
            let mut kept = []
            let mut i = 0
            while true {
                let item = "kept alive kept alive kept alive " + str(i)
                kept.push(item)
                i = i + 1
            }
        }
        try { fill() } catch e { say "caught" }
        say "after"
        "#,
        Limits {
            max_memory: Some(1 << 20),
            ..Limits::none()
        },
    );
    assert!(err_message(&r).starts_with(limits::MEMORY_LIMIT_EXCEEDED));
    assert!(r.output.is_empty(), "{:?}", r.output);
}

#[test]
fn size_caps_reject_huge_values_before_allocating() {
    let caps = Limits {
        max_string_bytes: Some(1000),
        max_collection_len: Some(1000),
        ..Limits::none()
    };
    for src in [
        r#"let s = repeat_str("x", 1000000000000)"#,
        r#"let r = range(0, 1000000000000)"#,
        r#"let r = range(1000000000000)"#,
        r#"let p = pad_start("x", 1000000000000)"#,
        r#"let mut s = "x"
           while true { s = s + s }"#,
        r#"let mut s = ""
           while true { s = s + "abcdefgh" }"#,
        r#"let mut a = []
           while true { a.push(1) }"#,
    ] {
        let r = run(src, caps.clone());
        let msg = err_message(&r);
        assert!(
            msg.starts_with(limits::RESOURCE_LIMIT_EXCEEDED),
            "{src}: {msg}"
        );
    }
    // Size-cap errors are ordinary errors: a program may catch them.
    let r = run(
        r#"
        try { let r = range(0, 5000) } catch e { say "too big" }
        say len(range(0, 1000))
        "#,
        caps,
    );
    assert!(r.result.is_ok(), "{:?}", r.result.err());
    assert_eq!(r.output, vec!["too big", "1000"]);
}

#[test]
fn memory_limit_implies_size_caps() {
    let r = run(
        r#"let s = repeat_str("abc", 100000000)"#,
        Limits {
            max_memory: Some(1 << 20),
            ..Limits::none()
        },
    );
    assert!(err_message(&r).starts_with(limits::RESOURCE_LIMIT_EXCEEDED));
}

#[test]
fn task_limit_caps_concurrent_spawns() {
    let r = run(
        r#"
        let a = spawn { wait(0.3) return 1 }
        let b = spawn { return 2 }
        "#,
        Limits {
            max_tasks: Some(1),
            ..Limits::none()
        },
    );
    let msg = err_message(&r);
    assert!(msg.contains("too many tasks (limit 1)"), "{msg}");
    // Sequential tasks are fine: the slot is free once a task finished.
    let r = run(
        r#"
        let a = spawn { return 1 }
        say await a
        let b = spawn { return 2 }
        say await b
        "#,
        Limits {
            max_tasks: Some(1),
            ..Limits::none()
        },
    );
    assert!(r.result.is_ok(), "{:?}", r.result.err());
    assert_eq!(r.output, vec!["1", "2"]);
}

#[test]
fn spawned_tasks_spend_the_parent_budget() {
    let r = run(
        r#"
        let h = spawn { let mut i = 0
                        while true { i = i + 1 } }
        await h
        let mut j = 0
        while true { j = j + 1 }
        "#,
        fuel(100_000),
    );
    let msg = err_message(&r);
    assert!(msg.contains(limits::FUEL_EXHAUSTED), "{msg}");
    assert_eq!(r.budget.tripped(), Some(Trip::Fuel));
}

#[test]
fn host_survives_and_unlimited_vm_is_unchanged_after_a_trip() {
    let r = run(COUNTER, fuel(1_000));
    assert!(r.result.is_err());
    // No budget is left behind on this thread.
    assert!(limits::current().is_none());
    let free = run("say 1 + 1", Limits::none());
    assert!(free.result.is_ok());
    assert_eq!(free.output, vec!["2"]);
}

#[cfg(feature = "jit")]
#[test]
fn fuel_keeps_hot_functions_out_of_the_jit() {
    use crate::vm::jit::tier::JitMode;
    let src = r#"
        fn spin(n) { let mut i = 0
                     while i < n { i = i + 1 }
                     return i }
        say spin(100000000000)
    "#;
    let tokens = Lexer::new(src).tokenize().expect("lex");
    let program = Parser::new(tokens).parse_program().expect("parse");
    let chunk = compiler::compile(&program).expect("compile");
    let budget = Budget::new(fuel(200_000));
    let _scope = limits::scope(Some(budget.clone()));
    let mut vm = VM::new();
    vm.set_jit_mode(JitMode::Eager);
    let err = vm.execute(&chunk).expect_err("fuel must stop the loop");
    assert!(err.message.starts_with(limits::FUEL_EXHAUSTED), "{err}");
    assert_eq!(budget.fuel_used(), 200_000);
}
