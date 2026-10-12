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

fn vm_output(source: &str) -> Vec<String> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.execute(&chunk).expect("execution error");
    vm.output.clone()
}

fn vm_run(source: &str) -> Result<(), crate::vm::machine::VMError> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.execute(&chunk)?;
    Ok(())
}

#[test]
fn vm_set_from_array() {
    let output = vm_output("say len(set([1, 2, 3]))");
    assert_eq!(output, vec!["3"]);
}

#[test]
fn vm_set_from_tuple() {
    let output = vm_output("say len(set((1, 2, 3)))");
    assert_eq!(output, vec!["3"]);
}

#[test]
fn vm_set_empty() {
    let output = vm_output("say len(set())");
    assert_eq!(output, vec!["0"]);
}

#[test]
fn vm_set_dedup() {
    let output = vm_output("say len(set([1, 1, 2, 2, 3, 3]))");
    assert_eq!(output, vec!["3"]);
}

#[test]
fn vm_set_has_true() {
    let output = vm_output("say set([1, 2, 3]).has(2)");
    assert_eq!(output, vec!["true"]);
}

#[test]
fn vm_set_has_false() {
    let output = vm_output("say set([1, 2, 3]).has(99)");
    assert_eq!(output, vec!["false"]);
}

#[test]
fn vm_set_add() {
    let output = vm_output("let s = set([1, 2]).add(3)\nsay len(s)");
    assert_eq!(output, vec!["3"]);
}

#[test]
fn vm_set_add_duplicate() {
    let output = vm_output("let s = set([1, 2]).add(1)\nsay len(s)");
    assert_eq!(output, vec!["2"]);
}

#[test]
fn vm_set_remove() {
    let output = vm_output("let s = set([1, 2, 3]).remove(2)\nsay len(s)");
    assert_eq!(output, vec!["2"]);
}

#[test]
fn vm_set_remove_missing() {
    let output = vm_output("let s = set([1, 2]).remove(99)\nsay len(s)");
    assert_eq!(output, vec!["2"]);
}

#[test]
fn vm_set_contains_builtin() {
    let output = vm_output("say contains(set([1, 2, 3]), 2)");
    assert_eq!(output, vec!["true"]);
}

#[test]
fn vm_set_union() {
    let output = vm_output("say len(set([1, 2, 3]).union(set([3, 4, 5])))");
    assert_eq!(output, vec!["5"]);
}

#[test]
fn vm_set_intersect() {
    let output = vm_output("say len(set([1, 2, 3]).intersect(set([2, 3, 4])))");
    assert_eq!(output, vec!["2"]);
}

#[test]
fn vm_set_diff() {
    let output = vm_output("say len(set([1, 2, 3]).diff(set([2, 3])))");
    assert_eq!(output, vec!["1"]);
}

#[test]
fn vm_set_equality_order_independent() {
    let output = vm_output("say set([1, 2, 3]) == set([3, 2, 1])");
    assert_eq!(output, vec!["true"]);
}

#[test]
fn vm_set_inequality() {
    let output = vm_output("say set([1, 2]) == set([1, 2, 3])");
    assert_eq!(output, vec!["false"]);
}

#[test]
fn vm_set_iteration() {
    let source = r#"
        let s = set([10, 20, 30])
        let mut total = 0
        for x in s {
            total = total + x
        }
        say total
    "#;
    let output = vm_output(source);
    assert_eq!(output, vec!["60"]);
}

#[test]
fn vm_set_display() {
    let output = vm_output("say set([1, 2, 3])");
    assert_eq!(output, vec!["set(1, 2, 3)"]);
}

#[test]
fn vm_set_to_array() {
    let output = vm_output("say len(set([1, 2, 3]).to_array())");
    assert_eq!(output, vec!["3"]);
}

#[test]
fn vm_set_typeof() {
    let output = vm_output(r#"say type(set([1, 2]))"#);
    assert_eq!(output, vec!["Set"]);
}

#[test]
fn vm_set_string_elements() {
    let output = vm_output(r#"say len(set(["a", "b", "a", "c"]))"#);
    assert_eq!(output, vec!["3"]);
}

#[test]
fn vm_set_index_assign_rejected() {
    let result = vm_run("let mut s = set([1, 2, 3])\ns[0] = 99");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("cannot index-assign a set"));
}

#[test]
fn vm_set_is_truthy() {
    let output = vm_output(
        r#"
        let mut r = "empty"
        let s = set([1])
        if s { r = "non-empty" }
        say r
    "#,
    );
    assert_eq!(output, vec!["non-empty"]);
}

/// Values covering every kind of set member, with pairs that compare equal
/// across representations (the interpreter's tests have the same corpus).
const SET_HASH_CORPUS: &str = r#"[1, 1.0, 0, -0.0, 0.0, math.sqrt(-1.0), "a", "b", [1, 2], [1.0, 2], [2, 1], (1, 2), {a: 1, b: 2}, {b: 2.0, a: 1}, {a: 1}, set([1, 2]), set([2.0, 1]), set([3]), Some(1), Some(1.0), None, Ok(1), Ok(1.0), Err(1), freeze([1]), [1], null, true, false, 9007199254740992, 9007199254740993, 9007199254740992.0, [[1, [2, 3.0]]], [[1, [2, 3]]], map([[1, 2]]), map([[1.0, 2]])]"#;

/// Values that `set_eq` considers equal must hash equally, or a set lookup
/// would miss them (`Value::set_hash`, `semantics::ordered_set`).
#[test]
fn equal_values_have_equal_set_hashes() {
    use crate::vm::value::{ObjKind, VmSet};
    let program = parse_program(&format!("let corpus = {}\ncorpus", SET_HASH_CORPUS));
    let chunk = compiler::compile_repl(&program).expect("compile error");
    let mut vm = VM::new();
    let result = vm.execute(&chunk).expect("execution error");
    let values: Vec<_> = match result.as_obj().and_then(|r| vm.gc.get(r)).map(|o| &o.kind) {
        Some(ObjKind::Array(items)) => items.clone(),
        _ => panic!("corpus is an array"),
    };
    let gc = &vm.gc;
    let mut equal_pairs = 0;
    for a in &values {
        for b in &values {
            if a.set_eq(b, gc) {
                equal_pairs += 1;
                assert_eq!(
                    a.set_hash(gc),
                    b.set_hash(gc),
                    "{} and {}",
                    a.display(gc),
                    b.display(gc)
                );
            }
        }
    }
    assert!(
        equal_pairs > values.len() + 10,
        "{} equal pairs",
        equal_pairs
    );
    // Sets built from these values agree with the linear definition.
    let set = VmSet::collect_dedup(values.iter().copied(), gc);
    for v in &values {
        assert!(set.has_in(v, gc), "{}", v.display(gc));
    }
    let mut linear: Vec<crate::vm::value::Value> = Vec::new();
    for v in &values {
        if !linear.iter().any(|x| x.set_eq(v, gc)) {
            linear.push(*v);
        }
    }
    assert_eq!(set.len(), linear.len());
    for (x, y) in set.iter().zip(&linear) {
        assert!(x.set_eq(y, gc), "{} vs {}", x.display(gc), y.display(gc));
    }
}
