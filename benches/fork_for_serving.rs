use criterion::{criterion_group, criterion_main, Criterion};
use forge_lang::interpreter::Interpreter;
use forge_lang::lexer::Lexer;
use forge_lang::parser::Parser;
use std::hint::black_box;

fn parse_and_run(source: &str) -> Interpreter {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("bench source should lex");
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().expect("bench source should parse");

    let mut interp = Interpreter::new();
    interp.run(&program).expect("bench source should run");
    interp
}

fn fixture_with_closures() -> Interpreter {
    parse_and_run(
        r#"
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
        "#,
    )
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

criterion_group!(benches, bench_fork_for_serving);
criterion_main!(benches);
