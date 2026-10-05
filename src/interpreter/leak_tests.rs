//! Leak tests for the scope cycle collector (`heap.rs`).
//!
//! Each test counts the scopes created on its thread (`heap::probe`) and
//! requires the count to return to zero once every interpreter involved
//! has been dropped — or, for long runs, to stay bounded while running.

use super::heap::probe;
use super::*;
use crate::lexer::Lexer;
use crate::parser::Parser;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::{Duration, Instant};

fn parse(src: &str) -> Program {
    Parser::new(Lexer::new(src).tokenize().expect("lex"))
        .parse_program()
        .expect("parse")
}

/// Uninstalls the probe even when an assertion fails.
struct Probe(Arc<AtomicIsize>);

impl Probe {
    fn install() -> Self {
        Probe(probe::install())
    }
    fn live(&self) -> isize {
        self.0.load(Ordering::SeqCst)
    }
    /// Wait for task threads (which may drop the last interpreter of a
    /// heap a moment after the program finishes) to wind down.
    fn settle_to(&self, target: isize) -> isize {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.live() != target && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        self.live()
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        probe::uninstall();
    }
}

fn run(interp: &mut Interpreter, src: &str) -> Value {
    interp.run_repl(&parse(src)).expect("program should run")
}

/// Run `src` on a fresh interpreter `times` times; every scope must be
/// gone afterwards.
#[track_caller]
fn assert_no_leak(src: &str, times: usize) {
    let probe = Probe::install();
    for _ in 0..times {
        let mut interp = Interpreter::new();
        run(&mut interp, src);
    }
    assert_eq!(probe.settle_to(0), 0, "scopes leaked by:\n{src}");
}

const RECURSIVE: &str =
    "fn fact(n) { if n <= 1 { return 1 }\n return n * fact(n - 1) }\nlet x = fact(10)";

#[test]
fn baseline_without_functions_never_leaked() {
    assert_no_leak("let x = 1 + 2\nlet ys = [1, 2, 3]", 5);
}

#[test]
fn recursive_function_is_reclaimed() {
    assert_no_leak(RECURSIVE, 20);
}

#[test]
fn mutual_recursion_is_reclaimed() {
    assert_no_leak(
        "fn is_even(n) { if n == 0 { return true }\n return is_odd(n - 1) }\n\
         fn is_odd(n) { if n == 0 { return false }\n return is_even(n - 1) }\n\
         let r = is_even(10)",
        10,
    );
}

#[test]
fn escaping_closure_counter_is_reclaimed() {
    assert_no_leak(
        "fn make_counter() {\n let mut count = 0\n return fn() { count = count + 1\n return count } }\n\
         let c = make_counter()\nc()\nc()\nlet v = c()",
        10,
    );
}

#[test]
fn lambdas_created_in_loops_are_reclaimed() {
    assert_no_leak(
        "let mut fs = []\n\
         for i in range(0, 20) {\n let f = fn() { return i }\n fs.push(f)\n let y = f() }\n\
         let mut k = 0\nwhile k < 10 { let g = fn(x) { return x + k }\n k = g(1) }",
        5,
    );
}

#[test]
fn inner_functions_and_methods_are_reclaimed() {
    assert_no_leak(
        "fn outer(n) {\n fn inner(k) { if k == 0 { return 0 }\n return inner(k - 1) }\n return inner(n) }\n\
         for i in range(0, 10) { outer(3) }\n\
         struct Point { x: Int, y: Int }\n\
         impl Point { fn sum(it) { return it.x + it.y } }\n\
         type Shape = Circle(float) | Square(float)\n\
         impl Shape { fn area(it) { match it { Circle(r) => return r * r\n Square(s) => return s * s } } }\n\
         let p = Point { x: 1, y: 2 }\nlet s = p.sum()\nlet a = Circle(2.0).area()",
        5,
    );
}

#[test]
fn streams_holding_lambdas_are_reclaimed() {
    assert_no_leak(
        "let s = [1, 2, 3].stream().map(fn(x) { return x * 2 })\n\
         let t = [4, 5].stream().filter(fn(x) { return x > 4 })\nlet n = t.count()",
        5,
    );
}

#[test]
fn spawned_tasks_and_squads_are_reclaimed() {
    assert_no_leak(
        "fn work(n) { if n == 0 { return 0 }\n return work(n - 1) }\n\
         let mut count = 0\nlet bump = fn() { count = count + 1\n return count }\n\
         squad {\n spawn { bump() }\n spawn { work(5) }\n }\n\
         let h = spawn { fn local(k) { return k }\n return local(work(3)) }\n\
         let r = await h",
        5,
    );
}

#[test]
fn imported_modules_are_reclaimed_and_keep_working() {
    let path = std::env::temp_dir().join(format!(
        "forge_leak_import_{}_{:?}.fg",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(
        &path,
        "fn helper(n) { if n == 0 { return 0 }\n return helper(n - 1) + 1 }\n\
         let helpers = { f: fn(x) { return helper(x) } }",
    )
    .expect("write module");
    let src = format!(
        "import \"{}\"\nlet a = helper(3)\nlet b = helpers.f(2)\nassert_eq(a + b, 5)",
        path.to_string_lossy().replace('\\', "\\\\")
    );
    assert_no_leak(&src, 5);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn server_forks_are_reclaimed_per_request() {
    let probe = Probe::install();
    let mut template = Interpreter::new();
    run(
        &mut template,
        "fn fib(n) { if n < 2 { return n }\n return fib(n - 1) + fib(n - 2) }\n\
         fn make_counter() { let mut c = 0\n return fn() { c = c + 1\n return c } }\n\
         let counter = make_counter()\n\
         fn handler() { return fib(5) + counter() }",
    );
    let after_template = probe.live();
    for _ in 0..50 {
        let mut fork = template.fork_for_serving();
        let handler = fork.env.get("handler").expect("handler");
        let out = fork.call_function(handler, vec![]).expect("call");
        // Isolation: every fork starts from the template's counter state.
        assert_eq!(out, Value::Int(6));
    }
    assert_eq!(probe.live(), after_template, "per-request forks leaked");
    drop(template);
    assert_eq!(probe.settle_to(0), 0, "template leaked");
}

#[test]
fn long_running_loops_collect_periodically() {
    let probe = Probe::install();
    let mut interp = Interpreter::new();
    // Every iteration creates a scope cycle (a recursive inner function
    // and a lambda stored in the loop scope). Without periodic collection
    // the live count grows by thousands.
    run(
        &mut interp,
        "fn step(n) {\n fn go(k) { if k == 0 { return 0 }\n return go(k - 1) }\n return go(n) }\n\
         let mut i = 0\n\
         while i < 20000 { let f = fn() { return i }\n step(1)\n i = i + 1 }",
    );
    let live = probe.live();
    assert!(
        live < 5000,
        "{live} scopes live after 20000 cyclic iterations"
    );
    drop(interp);
    assert_eq!(probe.settle_to(0), 0);
}

#[test]
fn values_that_escape_the_interpreter_keep_working() {
    let mut interp = Interpreter::new();
    run(
        &mut interp,
        "fn fact(n) { if n <= 1 { return 1 }\n return n * fact(n - 1) }\n\
         fn make_counter() { let mut c = 0\n return fn() { c = c + 1\n return c } }\n\
         let counter = make_counter()",
    );
    let fact = interp.env.get("fact").expect("fact");
    let counter = interp.env.get("counter").expect("counter");
    drop(interp);
    // The host still holds them: their scopes must not have been cleared.
    let mut other = Interpreter::new();
    assert_eq!(
        other
            .call_function(fact, vec![Value::Int(5)])
            .expect("fact"),
        Value::Int(120)
    );
    assert_eq!(
        other.call_function(counter.clone(), vec![]).expect("c"),
        Value::Int(1)
    );
    assert_eq!(
        other.call_function(counter, vec![]).expect("c"),
        Value::Int(2)
    );
}

#[test]
fn collection_preserves_live_closures_mid_run() {
    // Force many collections while closures that are only reachable
    // through other closures' captured scopes must stay intact.
    let mut interp = Interpreter::new();
    let v = run(
        &mut interp,
        "fn make_adder(n) { fn add(x) { return x + n }\n return add }\n\
         let mut adders = []\n\
         let mut i = 0\n\
         while i < 3000 { adders.push(make_adder(i))\n let junk = fn() { return i }\n i = i + 1 }\n\
         let mut total = 0\nfor a in adders { total = total + a(1) }\ntotal",
    );
    // sum_{i<3000} (i + 1)
    assert_eq!(v, Value::Int(3000 * 3001 / 2));
}

#[test]
fn collector_reports_what_it_frees() {
    let mut interp = Interpreter::new();
    run(&mut interp, RECURSIVE);
    let heap = interp.heap.clone();
    // Mid-run with the interpreter's own environment live: nothing to free.
    assert_eq!(heap.collect(&interp.env.scopes), 0);
    // Once the roots are gone, the global scope cycle is garbage.
    interp.env.scopes.clear();
    assert!(heap.collect(&[]) >= 1);
}
