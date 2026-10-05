//! Tree-walking interpreter hot paths.
//!
//! Each case is a small Forge program exercising one path that has
//! historically regressed to super-linear time: call overhead (fib),
//! repeated string concatenation, `push` in a loop, deep recursion, and
//! set building. Sizes are kept small so `cargo bench` stays fast; the
//! full-size programs live in `benchmarks/interp/` and are timed with
//! `tools/bench_interp.sh`.
//!
//! Run: `cargo bench --bench interpreter_hot_paths`

use criterion::{criterion_group, Criterion};
use forge_lang::interpreter::Interpreter;
use forge_lang::lexer::Lexer;
use forge_lang::parser::ast::Program;
use forge_lang::parser::Parser;
use std::hint::black_box;

fn parse(source: &str) -> Program {
    let tokens = Lexer::new(source)
        .tokenize()
        .expect("bench source should lex");
    Parser::new(tokens)
        .parse_program()
        .expect("bench source should parse")
}

fn run(program: &Program) {
    let mut interp = Interpreter::new();
    interp.run(program).expect("bench source should run");
    black_box(interp);
}

const FIB: &str = r#"
fn fib(n) {
    if n <= 1 { return n }
    return fib(n - 1) + fib(n - 2)
}
let r = fib(20)
"#;

const STRING_BUILD: &str = r#"
let mut s = ""
let mut i = 0
while i < 20000 {
    s = s + "x"
    i = i + 1
}
"#;

const PUSH_METHOD: &str = r#"
let mut a = []
let mut i = 0
while i < 20000 {
    a.push(i)
    i = i + 1
}
"#;

const PUSH_REASSIGN: &str = r#"
let mut a = []
let mut i = 0
while i < 20000 {
    a = push(a, i)
    i = i + 1
}
"#;

const DEEP_RECURSION: &str = r#"
fn down(n) {
    if n == 0 { return 0 }
    return 1 + down(n - 1)
}
let r = down(3000)
"#;

const SET_BUILD: &str = r#"
let mut s = set([])
let mut i = 0
while i < 2000 {
    s.add(i)
    i = i + 1
}
"#;

fn bench_hot_paths(c: &mut Criterion) {
    let cases = [
        ("interp/fib_20", FIB),
        ("interp/string_build_20k", STRING_BUILD),
        ("interp/push_method_20k", PUSH_METHOD),
        ("interp/push_reassign_20k", PUSH_REASSIGN),
        ("interp/deep_recursion_3000", DEEP_RECURSION),
        ("interp/set_build_2k", SET_BUILD),
    ];
    let mut group = c.benchmark_group("interpreter");
    group.sample_size(10);
    for (name, src) in cases {
        let program = parse(src);
        group.bench_function(name, |b| b.iter(|| run(&program)));
    }
    group.finish();
}

fn main_with_stack() {
    // Deep recursion needs the same large stack the CLI gives programs.
    let stack = forge_lang::runtime::recursion::MAIN_STACK_SIZE;
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            forge_lang::runtime::recursion::register_thread_stack(stack);
            benches();
            Criterion::default().configure_from_args().final_summary();
        })
        .expect("spawn bench thread")
        .join()
        .expect("bench thread panicked");
}

criterion_group!(benches, bench_hot_paths);

fn main() {
    main_with_stack();
}
