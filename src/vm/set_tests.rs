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
