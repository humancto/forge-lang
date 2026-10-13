//! Index reads of a local whose key is an index or field read or a call
//! (`GetIndexLocal`, or a `PeekLocal` snapshot when the key may call):
//! the object stays owned, so the following write updates it in place,
//! and the value read is the one the variable had before the key ran.

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::vm::compiler;
use crate::vm::machine::VM;

fn parse_program(source: &str) -> crate::parser::ast::Program {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer error");
    let mut parser = Parser::new(tokens);
    parser.parse_program().expect("parse error")
}

fn run_output(source: &str, stress: bool) -> Vec<String> {
    let chunk = compiler::compile(&parse_program(source)).expect("compile error");
    let mut vm = VM::new();
    vm.gc.set_stress(stress);
    vm.execute(&chunk).expect("vm error");
    vm.output.clone()
}

/// The program's output on the interpreter, for parity.
fn interpreter_output(source: &str) -> Vec<String> {
    use crate::runtime::stdio::{capture, Stream};
    let guard = capture(1 << 20);
    crate::interpreter::Interpreter::new()
        .run(&parse_program(source))
        .expect("interpreter error");
    guard
        .finish()
        .text(Stream::Stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// Runs `source` with and without GC stress, and on the interpreter: all
/// three must print `expected`.
fn assert_output(source: &str, expected: &[&str]) {
    for stress in [false, true] {
        assert_eq!(
            run_output(source, stress),
            expected,
            "stress={stress}\n{source}"
        );
    }
    assert_eq!(
        interpreter_output(source),
        expected,
        "interpreter\n{source}"
    );
}

/// Objects allocated while running `source` (small programs: no
/// collection runs, so the count only grows).
fn allocations(source: &str) -> usize {
    let chunk = compiler::compile(&parse_program(source)).expect("compile error");
    let mut vm = VM::new();
    let before = vm.gc.alloc_count;
    vm.execute(&chunk).expect("vm error");
    vm.gc.alloc_count - before
}

/// Extra allocations of `program(n)` with `n = 1500` over `n = 100`: about
/// 1400 or more when every step copies the container.
fn extra_allocations(program: impl Fn(usize) -> String) -> usize {
    allocations(&program(1500)).saturating_sub(allocations(&program(100)))
}

#[test]
fn index_reads_with_computed_keys_keep_ownership_and_values() {
    assert_output(
        r#"
        let names = ["a", "b", "c"]
        let it = { sku: "b" }
        let mut o = { a: 0, b: 0, c: 0 }
        let w = o
        for i in range(0, 3) {
            o[names[i]] += 1
            o[it.sku] += 1
            o["k" + str(i)] = i
            o[str(i)] = has_key(o, str(i))
            o[str(i)] = o[str(i)] == false
        }
        println(w)
        println(o)
        "#,
        &[
            r#"{ "a": 0, "b": 0, "c": 0 }"#,
            r#"{ "a": 1, "b": 4, "c": 1, "k0": 0, "0": true, "k1": 1, "1": true, "k2": 2, "2": true }"#,
        ],
    );
}

#[test]
fn a_call_in_the_key_reads_the_snapshot() {
    // The key's call replaces the captured object: the read uses the value
    // the variable had before the key ran, like the generic read.
    assert_output(
        r#"
        let mut o = { k: 1 }
        fn key() {
            o = { k: 2 }
            return "k"
        }
        fn grow() {
            o["n"] = 5
            return "k"
        }
        let a = o[key()]
        let b = o[grow()]
        println(a)
        println(b)
        println(o)
        "#,
        &["1", "2", r#"{ "k": 2, "n": 5 }"#],
    );
}

#[test]
fn computed_keys_do_not_copy_per_step() {
    let keyed = |n: usize| {
        format!(
            "fn f(n) {{\n let names = map(range(0, 50), fn(i) {{ return \"k{{i}}\" }})\n let mut o = {{}}\n for k in names {{\n o[k] = 0\n }}\n for i in range(0, n) {{\n o[names[i % 50]] += 1\n }}\n return o\n}}\nf({n})\n"
        )
    };
    // No copy of the object per step.
    assert!(
        extra_allocations(keyed) < 200,
        "{}",
        extra_allocations(keyed)
    );
}
