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

fn run_vm(source: &str) -> Result<crate::vm::value::Value, crate::vm::machine::VMError> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.execute(&chunk)
}

fn vm_output(source: &str) -> Vec<String> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.execute(&chunk).expect("execution error");
    vm.output.clone()
}

#[test]
fn vm_tuple_creation() {
    let output = vm_output("say (1, 2, 3)");
    assert_eq!(output, vec!["(1, 2, 3)"]);
}

#[test]
fn vm_tuple_single_element() {
    let output = vm_output("say (42,)");
    assert_eq!(output, vec!["(42)"]);
}

#[test]
fn vm_tuple_mixed_types() {
    let output = vm_output(r#"say (1, "hi", true)"#);
    assert_eq!(output, vec!["(1, hi, true)"]);
}

#[test]
fn vm_tuple_indexing() {
    let output = vm_output("let t = (10, 20, 30)\nsay t[1]");
    assert_eq!(output, vec!["20"]);
}

#[test]
fn vm_tuple_index_out_of_bounds() {
    let result = run_vm("let t = (1, 2)\nsay t[5]");
    assert!(result.is_err());
}

#[test]
fn vm_tuple_immutability() {
    let result = run_vm("let mut t = (1, 2, 3)\nt[0] = 99");
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(msg.contains("cannot mutate a tuple"), "got: {}", msg);
}

#[test]
fn vm_tuple_destructuring() {
    let output = vm_output("let (a, b, c) = (10, 20, 30)\nsay b");
    assert_eq!(output, vec!["20"]);
}

#[test]
fn vm_tuple_destructuring_both_vars() {
    let output = vm_output(
        r#"
        let (x, y) = (100, 200)
        say x
        say y
        "#,
    );
    assert_eq!(output, vec!["100", "200"]);
}

#[test]
fn vm_tuple_as_function_return() {
    let output = vm_output(
        r#"
        fn swap(a, b) {
            return (b, a)
        }
        let (x, y) = swap(1, 2)
        say x
        say y
        "#,
    );
    assert_eq!(output, vec!["2", "1"]);
}

#[test]
fn vm_tuple_equality() {
    let output = vm_output("say (1, 2, 3) == (1, 2, 3)");
    assert_eq!(output, vec!["true"]);
}

#[test]
fn vm_tuple_inequality() {
    let output = vm_output("say (1, 2, 3) == (1, 2, 4)");
    assert_eq!(output, vec!["false"]);
}

#[test]
fn vm_tuple_not_equal() {
    let output = vm_output("say (1, 2) != (3, 4)");
    assert_eq!(output, vec!["true"]);
}

#[test]
fn vm_tuple_len() {
    let output = vm_output("say len((1, 2, 3))");
    assert_eq!(output, vec!["3"]);
}

#[test]
fn vm_tuple_type() {
    let output = vm_output("say type((1, 2))");
    assert_eq!(output, vec!["Tuple"]);
}

#[test]
fn vm_tuple_contains() {
    let output = vm_output("say contains((1, 2, 3), 2)");
    assert_eq!(output, vec!["true"]);
}

#[test]
fn vm_tuple_contains_missing() {
    let output = vm_output("say contains((1, 2, 3), 99)");
    assert_eq!(output, vec!["false"]);
}

#[test]
fn vm_tuple_for_loop() {
    let output = vm_output(
        r#"
        let mut sum = 0
        for x in (10, 20, 30) {
            sum = sum + x
        }
        say sum
        "#,
    );
    assert_eq!(output, vec!["60"]);
}

#[test]
fn vm_tuple_nested() {
    let output = vm_output("let t = ((1, 2), (3, 4))\nsay t[0][1]");
    assert_eq!(output, vec!["2"]);
}

#[test]
fn vm_tuple_grouping_still_works() {
    let output = vm_output("say (2 + 3) * 4");
    assert_eq!(output, vec!["20"]);
}

#[test]
fn vm_tuple_in_string_interpolation() {
    let output = vm_output(
        r#"let t = (1, 2, 3)
say "tuple: {t}""#,
    );
    assert_eq!(output, vec!["tuple: (1, 2, 3)"]);
}
