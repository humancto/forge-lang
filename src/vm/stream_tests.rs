// VM Stream tests (M9.4 iterator protocol)
//
// Mirrors the interpreter stream test suite so the VM backend stays in
// parity. Covers all sources, combinators, terminals, edge cases called
// out in .planning/iterator-protocol.plan.md (empty sources, take(0),
// skip > len, zip unequal, chain empty, filter-all-out, map-to-null,
// single-use redrain, short-circuit any/all, error propagation, upvalue
// capture, sum int/float promotion, and long pipelines).

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

fn vm_output(source: &str) -> Vec<String> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.execute(&chunk).expect("execution error");
    vm.output.clone()
}

fn vm_run(source: &str) -> Result<(), VMError> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.execute(&chunk)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

#[test]
fn vm_stream_array_collect() {
    assert_eq!(
        vm_output("say [1, 2, 3].stream().collect()"),
        vec!["[1, 2, 3]"]
    );
}

#[test]
fn vm_stream_tuple_collect() {
    assert_eq!(
        vm_output("say (1, 2, 3).stream().collect()"),
        vec!["[1, 2, 3]"]
    );
}

#[test]
fn vm_stream_set_count() {
    assert_eq!(vm_output("say set([1, 2, 3]).stream().count()"), vec!["3"]);
}

#[test]
fn vm_stream_map_count() {
    assert_eq!(
        vm_output(r#"say map([("a", 1), ("b", 2)]).stream().count()"#),
        vec!["2"]
    );
}

#[test]
fn vm_stream_string_chars() {
    assert_eq!(
        vm_output(r#"say "abc".stream().collect()"#),
        vec!["[a, b, c]"]
    );
}

#[test]
fn vm_stream_empty_collect() {
    assert_eq!(vm_output("say [].stream().collect()"), vec!["[]"]);
}

// ---------------------------------------------------------------------------
// Combinators
// ---------------------------------------------------------------------------

#[test]
fn vm_stream_filter() {
    assert_eq!(
        vm_output("say [1,2,3,4,5].stream().filter(fn(x) { return x > 2 }).collect()"),
        vec!["[3, 4, 5]"]
    );
}

#[test]
fn vm_stream_filter_all_out() {
    assert_eq!(
        vm_output("say [1,2,3].stream().filter(fn(x) { return x > 100 }).collect()"),
        vec!["[]"]
    );
}

#[test]
fn vm_stream_map_doubled() {
    assert_eq!(
        vm_output("say [1,2,3].stream().map(fn(x) { return x * 2 }).collect()"),
        vec!["[2, 4, 6]"]
    );
}

#[test]
fn vm_stream_map_to_null() {
    assert_eq!(
        vm_output("say [1,2,3].stream().map(fn(x) { return null }).collect()"),
        vec!["[null, null, null]"]
    );
}

#[test]
fn vm_stream_take() {
    assert_eq!(
        vm_output("say [1,2,3,4,5].stream().take(2).collect()"),
        vec!["[1, 2]"]
    );
}

#[test]
fn vm_stream_take_zero() {
    assert_eq!(
        vm_output("say [1,2,3].stream().take(0).collect()"),
        vec!["[]"]
    );
}

#[test]
fn vm_stream_take_more_than_len() {
    assert_eq!(
        vm_output("say [1,2,3].stream().take(99).collect()"),
        vec!["[1, 2, 3]"]
    );
}

#[test]
fn vm_stream_skip() {
    assert_eq!(
        vm_output("say [1,2,3,4,5].stream().skip(2).collect()"),
        vec!["[3, 4, 5]"]
    );
}

#[test]
fn vm_stream_skip_more_than_len() {
    assert_eq!(
        vm_output("say [1,2,3].stream().skip(99).collect()"),
        vec!["[]"]
    );
}

#[test]
fn vm_stream_skip_then_take() {
    assert_eq!(
        vm_output("say [1,2,3,4,5,6].stream().skip(2).take(2).collect()"),
        vec!["[3, 4]"]
    );
}

#[test]
fn vm_stream_chain_basic() {
    assert_eq!(
        vm_output("say [1,2].stream().chain([3,4].stream()).collect()"),
        vec!["[1, 2, 3, 4]"]
    );
}

#[test]
fn vm_stream_chain_empty_first() {
    assert_eq!(
        vm_output("say [].stream().chain([1,2].stream()).collect()"),
        vec!["[1, 2]"]
    );
}

#[test]
fn vm_stream_chain_empty_second() {
    assert_eq!(
        vm_output("say [1,2].stream().chain([].stream()).collect()"),
        vec!["[1, 2]"]
    );
}

#[test]
fn vm_stream_zip_basic() {
    assert_eq!(
        vm_output(r#"say [1,2,3].stream().zip(["a","b","c"].stream()).collect()"#),
        vec!["[(1, a), (2, b), (3, c)]"]
    );
}

#[test]
fn vm_stream_zip_shorter_left() {
    assert_eq!(
        vm_output(r#"say [1,2].stream().zip(["a","b","c"].stream()).collect()"#),
        vec!["[(1, a), (2, b)]"]
    );
}

#[test]
fn vm_stream_zip_shorter_right() {
    assert_eq!(
        vm_output(r#"say [1,2,3].stream().zip(["a","b"].stream()).collect()"#),
        vec!["[(1, a), (2, b)]"]
    );
}

#[test]
fn vm_stream_enumerate() {
    assert_eq!(
        vm_output("say [10, 20, 30].stream().enumerate().collect()"),
        vec!["[(0, 10), (1, 20), (2, 30)]"]
    );
}

// ---------------------------------------------------------------------------
// Terminals
// ---------------------------------------------------------------------------

#[test]
fn vm_stream_to_array() {
    assert_eq!(
        vm_output("say [1,2,3].stream().to_array()"),
        vec!["[1, 2, 3]"]
    );
}

#[test]
fn vm_stream_count_basic() {
    assert_eq!(vm_output("say [1,2,3,4].stream().count()"), vec!["4"]);
}

#[test]
fn vm_stream_count_empty() {
    assert_eq!(vm_output("say [].stream().count()"), vec!["0"]);
}

#[test]
fn vm_stream_sum_int() {
    assert_eq!(vm_output("say [1,2,3,4].stream().sum()"), vec!["10"]);
}

#[test]
fn vm_stream_sum_float_promotion() {
    assert_eq!(vm_output("say [1, 2.5, 3].stream().sum()"), vec!["6.5"]);
}

#[test]
fn vm_stream_sum_empty() {
    assert_eq!(vm_output("say [].stream().sum()"), vec!["0"]);
}

#[test]
fn vm_stream_reduce_with_init() {
    assert_eq!(
        vm_output("say [1,2,3,4].stream().reduce(100, fn(a, b) { return a + b })"),
        vec!["110"]
    );
}

#[test]
fn vm_stream_reduce_empty() {
    assert_eq!(
        vm_output("say [].stream().reduce(42, fn(a, b) { return a + b })"),
        vec!["42"]
    );
}

#[test]
fn vm_stream_first_some() {
    assert_eq!(vm_output("say [1,2,3].stream().first()"), vec!["Some(1)"]);
}

#[test]
fn vm_stream_first_none() {
    assert_eq!(vm_output("say [].stream().first()"), vec!["None"]);
}

#[test]
fn vm_stream_find_some() {
    assert_eq!(
        vm_output("say [1,2,3].stream().find(fn(x) { return x > 1 })"),
        vec!["Some(2)"]
    );
}

#[test]
fn vm_stream_find_none() {
    assert_eq!(
        vm_output("say [1,2,3].stream().find(fn(x) { return x > 99 })"),
        vec!["None"]
    );
}

#[test]
fn vm_stream_any_true() {
    assert_eq!(
        vm_output("say [1,2,3].stream().any(fn(x) { return x > 2 })"),
        vec!["true"]
    );
}

#[test]
fn vm_stream_any_false() {
    assert_eq!(
        vm_output("say [1,2,3].stream().any(fn(x) { return x > 99 })"),
        vec!["false"]
    );
}

#[test]
fn vm_stream_all_true() {
    assert_eq!(
        vm_output("say [1,2,3].stream().all(fn(x) { return x > 0 })"),
        vec!["true"]
    );
}

#[test]
fn vm_stream_all_false() {
    assert_eq!(
        vm_output("say [1,2,3].stream().all(fn(x) { return x > 1 })"),
        vec!["false"]
    );
}

// ---------------------------------------------------------------------------
// Pipelines
// ---------------------------------------------------------------------------

#[test]
fn vm_stream_filter_map_pipeline() {
    assert_eq!(
        vm_output(
            "say [1,2,3,4,5].stream().filter(fn(x) { return x > 2 }).map(fn(x) { return x * 10 }).collect()"
        ),
        vec!["[30, 40, 50]"]
    );
}

#[test]
fn vm_stream_long_pipeline() {
    assert_eq!(
        vm_output(
            "say [1,2,3,4,5,6,7,8,9,10].stream().filter(fn(x) { return x > 2 }).map(fn(x) { return x * 2 }).skip(2).take(3).collect()"
        ),
        vec!["[10, 12, 14]"]
    );
}

#[test]
fn vm_stream_map_filter_count() {
    assert_eq!(
        vm_output(
            "say [1,2,3,4,5,6].stream().map(fn(x) { return x * 2 }).filter(fn(x) { return x > 5 }).count()"
        ),
        vec!["4"]
    );
}

// ---------------------------------------------------------------------------
// Side-effects and short-circuit
// ---------------------------------------------------------------------------

#[test]
fn vm_stream_for_each_side_effect() {
    let output = vm_output(
        r#"
        let mut total = 0
        [1,2,3,4].stream().for_each(fn(x) { total = total + x })
        say total
        "#,
    );
    assert_eq!(output, vec!["10"]);
}

#[test]
fn vm_stream_any_short_circuits() {
    // `any` must stop on the first truthy. With `x >= 3`, we see 1, 2, 3.
    let output = vm_output(
        r#"
        let mut seen = 0
        [1, 2, 3, 4, 5].stream().any(fn(x) {
            seen = seen + 1
            return x >= 3
        })
        say seen
        "#,
    );
    assert_eq!(output, vec!["3"]);
}

#[test]
fn vm_stream_all_short_circuits() {
    // `all` must stop on the first falsy. With `x < 3`, we see 1, 2, 3.
    let output = vm_output(
        r#"
        let mut seen = 0
        [1, 2, 3, 4, 5].stream().all(fn(x) {
            seen = seen + 1
            return x < 3
        })
        say seen
        "#,
    );
    assert_eq!(output, vec!["3"]);
}

#[test]
fn vm_stream_upvalue_capture() {
    // Lambda captures an outer binding and uses it during pull-driven
    // iteration. Makes sure upvalue slots survive across the stream's
    // per-element call sequence.
    let output = vm_output(
        r#"
        let factor = 10
        say [1,2,3].stream().map(fn(x) { return x * factor }).collect()
        "#,
    );
    assert_eq!(output, vec!["[10, 20, 30]"]);
}

// ---------------------------------------------------------------------------
// Single-use / drained / error
// ---------------------------------------------------------------------------

#[test]
fn vm_stream_single_use_redrain_empty() {
    // A stream is single-use: once drained, a second terminal call
    // must yield the empty terminal (collect -> [], count -> 0).
    let output = vm_output(
        r#"
        let s = [1, 2, 3].stream()
        s.collect()
        say s.collect()
        "#,
    );
    assert_eq!(output, vec!["[]"]);
}

#[test]
fn vm_stream_drained_count_is_zero() {
    let output = vm_output(
        r#"
        let s = [1, 2, 3].stream()
        s.collect()
        say s.count()
        "#,
    );
    assert_eq!(output, vec!["0"]);
}

#[test]
fn vm_stream_drained_first_is_none() {
    let output = vm_output(
        r#"
        let s = [1, 2, 3].stream()
        s.collect()
        say s.first()
        "#,
    );
    assert_eq!(output, vec!["None"]);
}

#[test]
fn vm_stream_error_in_closure_propagates() {
    // A closure that errors during a terminal must surface the error.
    let res = vm_run(
        r#"
        [1, 2, 3].stream().for_each(fn(x) { must err("boom") })
        "#,
    );
    assert!(res.is_err(), "expected error, got {:?}", res);
}

#[test]
fn vm_stream_error_poisoning() {
    // The first terminal throws via `must err`, trapped by `safe`.
    // A second terminal call on the *same* stream must also error —
    // the stream is poisoned, not restarted, not silently drained.
    let res = vm_run(
        r#"
        let s = [1, 2, 3].stream()
        safe { s.for_each(fn(x) { must err("boom") }) }
        s.collect()
        "#,
    );
    assert!(
        res.is_err(),
        "expected second terminal to resurface poisoning error, got {:?}",
        res
    );
}

#[test]
fn vm_stream_boundary_rejection_json() {
    // json.stringify crosses the VM↔interpreter bridge via
    // convert_to_interp_val. Passing a Stream across that boundary must
    // error loudly with the bug #6 message, not silently coerce to Null.
    let res = vm_run(r#"json.stringify([1, 2, 3].stream())"#);
    let err = res.expect_err("expected boundary error");
    assert!(
        err.message.contains("Stream cannot cross"),
        "expected boundary message, got: {}",
        err.message
    );
}
