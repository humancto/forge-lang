//! Per-request fork cost of both serving engines: the interpreter's
//! `fork_for_serving` and the VM's `VmTemplate::fork` (`vm::serve`).

use criterion::{criterion_group, criterion_main, Criterion};
use forge_lang::interpreter::Interpreter;
use forge_lang::lexer::Lexer;
use forge_lang::parser::ast::Program;
use forge_lang::parser::Parser;
use forge_lang::runtime::metadata::top_level_fn_params;
use forge_lang::vm::serve::VmTemplate;
use std::hint::black_box;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn parse(source: &str) -> Program {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("bench source should lex");
    let mut parser = Parser::new(tokens);
    parser.parse_program().expect("bench source should parse")
}

fn parse_and_run(source: &str) -> Interpreter {
    let program = parse(source);
    let mut interp = Interpreter::new();
    interp.run(&program).expect("bench source should run");
    interp
}

fn vm_template(source: &str) -> VmTemplate {
    let program = parse(source);
    let chunk = forge_lang::vm::compiler::compile(&program).expect("bench source should compile");
    let mut vm = forge_lang::vm::machine::VM::new();
    vm.execute(&chunk).expect("bench source should run");
    VmTemplate::new(&vm, top_level_fn_params(&program)).expect("template")
}

const CLOSURES_FIXTURE: &str = r#"
        fn make_counter(seed) {
            let mut count = seed
            return fn() {
                count = count + 1
                return count
            }
        }

        let counter_a = make_counter(0)
        let counter_b = make_counter(100)
        let config = {
            name: "bench",
            nested: {
                items: [1, 2, 3, 4, 5],
                flags: { fast: true, isolated: true }
            }
        }

        fn handler() {
            return {
                a: counter_a(),
                b: counter_b(),
                name: config.name
            }
        }
        "#;

fn fixture_with_closures() -> Interpreter {
    parse_and_run(CLOSURES_FIXTURE)
}

fn bench_fork_for_serving(c: &mut Criterion) {
    let empty = Interpreter::new();
    c.bench_function("fork_for_serving/empty", |b| {
        b.iter(|| black_box(empty.fork_for_serving()))
    });

    let with_closures = fixture_with_closures();
    c.bench_function("fork_for_serving/with_closures", |b| {
        b.iter(|| black_box(with_closures.fork_for_serving()))
    });
}

fn bench_vm_fork(c: &mut Criterion) {
    let cancel = Arc::new(AtomicBool::new(false));
    let empty = vm_template("");
    c.bench_function("vm_fork/empty", |b| {
        b.iter(|| black_box(empty.fork(cancel.clone())))
    });

    let with_closures = vm_template(CLOSURES_FIXTURE);
    c.bench_function("vm_fork/with_closures", |b| {
        b.iter(|| black_box(with_closures.fork(cancel.clone())))
    });
}

criterion_group!(benches, bench_fork_for_serving, bench_vm_fork);
criterion_main!(benches);
