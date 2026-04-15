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

#[test]
fn vm_map_empty() {
    let output = vm_output("say len(map())");
    assert_eq!(output, vec!["0"]);
}

#[test]
fn vm_map_from_pairs() {
    let output = vm_output("let m = map([(\"a\", 1), (\"b\", 2)])\nsay m.len()");
    assert_eq!(output, vec!["2"]);
}

#[test]
fn vm_map_from_array_of_arrays() {
    let output = vm_output("let m = map([[\"a\", 1], [\"b\", 2]])\nsay m.len()");
    assert_eq!(output, vec!["2"]);
}

#[test]
fn vm_map_get_hit() {
    let output = vm_output("say map([(\"a\", 1), (\"b\", 2)]).get(\"b\")");
    assert_eq!(output, vec!["2"]);
}

#[test]
fn vm_map_get_miss() {
    let output = vm_output("say map([(\"a\", 1)]).get(\"nope\")");
    assert_eq!(output, vec!["null"]);
}

#[test]
fn vm_map_has() {
    let output = vm_output("let m = map([(\"a\", 1)])\nsay m.has(\"a\")\nsay m.has(\"b\")");
    assert_eq!(output, vec!["true", "false"]);
}

#[test]
fn vm_map_set_new_key() {
    let output = vm_output("let m = map().set(\"a\", 1).set(\"b\", 2)\nsay m.len()");
    assert_eq!(output, vec!["2"]);
}

#[test]
fn vm_map_set_overwrite_preserves_order() {
    let output = vm_output(
        "let m = map([(\"a\", 1), (\"b\", 2), (\"c\", 3)]).set(\"a\", 99)\nsay m.keys()\nsay m.values()",
    );
    assert_eq!(output, vec!["[a, b, c]", "[99, 2, 3]"]);
}

#[test]
fn vm_map_remove() {
    let output = vm_output(
        "let m = map([(\"a\", 1), (\"b\", 2)]).remove(\"a\")\nsay m.len()\nsay m.has(\"a\")",
    );
    assert_eq!(output, vec!["1", "false"]);
}

#[test]
fn vm_map_remove_missing_is_noop() {
    let output = vm_output("let m = map([(\"a\", 1)]).remove(\"z\")\nsay m.len()");
    assert_eq!(output, vec!["1"]);
}

#[test]
fn vm_map_keys_values() {
    let output = vm_output("let m = map([(\"a\", 1), (\"b\", 2)])\nsay m.keys()\nsay m.values()");
    assert_eq!(output, vec!["[a, b]", "[1, 2]"]);
}

#[test]
fn vm_map_len_builtin() {
    let output = vm_output("say len(map([(\"a\", 1), (\"b\", 2), (\"c\", 3)]))");
    assert_eq!(output, vec!["3"]);
}

#[test]
fn vm_map_contains_key() {
    let output = vm_output("say contains(map([(\"a\", 1)]), \"a\")");
    assert_eq!(output, vec!["true"]);
}

#[test]
fn vm_map_to_array() {
    let output = vm_output("say map([(\"a\", 1), (\"b\", 2)]).to_array()");
    assert_eq!(output, vec!["[(a, 1), (b, 2)]"]);
}

#[test]
fn vm_map_typeof() {
    let output = vm_output("say typeof(map())");
    assert_eq!(output, vec!["Map"]);
}

#[test]
fn vm_map_display() {
    let output = vm_output("say map([(\"a\", 1), (\"b\", 2)])");
    assert_eq!(output, vec!["Map(a => 1, b => 2)"]);
}

#[test]
fn vm_map_int_float_key_collision() {
    let output = vm_output("let m = map([(1, \"a\")]).set(1.0, \"b\")\nsay m.len()\nsay m.get(1)");
    assert_eq!(output, vec!["1", "b"]);
}

#[test]
fn vm_map_for_k_v_iteration() {
    let output = vm_output(
        "let m = map([(\"a\", 1), (\"b\", 2)])\nfor k, v in m { say k + \"=\" + str(v) }",
    );
    assert_eq!(output, vec!["a=1", "b=2"]);
}

#[test]
fn vm_map_for_single_var_yields_tuple() {
    let output = vm_output("let m = map([(\"x\", 10), (\"y\", 20)])\nfor p in m { say p }");
    assert_eq!(output, vec!["(x, 10)", "(y, 20)"]);
}

#[test]
fn vm_object_for_k_v_parity() {
    let output =
        vm_output("let o = {name: \"alice\", age: 30}\nfor k, v in o { say k + \"=\" + str(v) }");
    assert_eq!(output, vec!["name=alice", "age=30"]);
}

#[test]
fn vm_map_equality_order_independent() {
    let output = vm_output("say map([(\"a\", 1), (\"b\", 2)]) == map([(\"b\", 2), (\"a\", 1)])");
    assert_eq!(output, vec!["true"]);
}

#[test]
fn vm_map_inequality() {
    let output = vm_output("say map([(\"a\", 1)]) == map([(\"a\", 2)])");
    assert_eq!(output, vec!["false"]);
}

#[test]
fn vm_map_nested_equality() {
    let output = vm_output(
        "let a = map([(\"inner\", map([(\"x\", 1)]))])\nlet b = map([(\"inner\", map([(\"x\", 1)]))])\nsay a == b",
    );
    assert_eq!(output, vec!["true"]);
}

#[test]
fn vm_map_in_set_dedups() {
    let output = vm_output(
        "let s = set([map([(\"a\", 1)]), map([(\"a\", 1)]), map([(\"a\", 2)])])\nsay len(s)",
    );
    assert_eq!(output, vec!["2"]);
}

#[test]
fn vm_map_json_stringify_string_keys() {
    let output = vm_output("say json.stringify(map([(\"name\", \"alice\")]))");
    assert_eq!(output, vec!["{\"name\": \"alice\"}"]);
}

#[test]
fn vm_map_frozen_set_rejected() {
    let program = parse_program("let m = freeze(map([(\"a\", 1)]))\nm.set(\"b\", 2)");
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    let err = vm.execute(&chunk).expect_err("expected frozen rejection");
    assert!(err.message.contains("frozen map"));
}

#[test]
fn vm_map_truthiness() {
    let output =
        vm_output("if map([(\"a\", 1)]) { say \"nonempty\" }\nif map() { say \"never\" } else { say \"empty\" }");
    assert_eq!(output, vec!["nonempty", "empty"]);
}
