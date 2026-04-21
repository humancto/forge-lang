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

fn run_on_vm(source: &str) -> Vec<String> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.execute(&chunk).expect("vm error");
    vm.output.clone()
}

fn run_on_vm_value(source: &str) -> String {
    let program = parse_program(source);
    let chunk = compiler::compile_repl(&program).expect("compile error");
    let mut vm = VM::new();
    let value = vm.execute(&chunk).expect("vm error");
    value.display(&vm.gc)
}

fn run_on_vm_err(source: &str) -> String {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    let err = vm.execute(&chunk).expect_err("expected vm error");
    err.message
}

#[test]
fn vm_squad_basic_two_spawns() {
    let out = run_on_vm(
        r#"
        let results = squad {
            spawn { 1 + 1 }
            spawn { 2 + 2 }
        }
        say results
    "#,
    );
    assert_eq!(out, vec!["[2, 4]"]);
}

#[test]
fn vm_squad_single_spawn() {
    let out = run_on_vm(
        r#"
        let results = squad {
            spawn { 42 }
        }
        say results
    "#,
    );
    assert_eq!(out, vec!["[42]"]);
}

#[test]
fn vm_squad_empty_body() {
    let out = run_on_vm(
        r#"
        let results = squad { }
        say results
    "#,
    );
    assert_eq!(out, vec!["[]"]);
}

#[test]
fn vm_squad_preserves_order() {
    let out = run_on_vm(
        r#"
        let results = squad {
            spawn { "first" }
            spawn { "second" }
            spawn { "third" }
        }
        say results
    "#,
    );
    assert_eq!(out, vec!["[first, second, third]"]);
}

#[test]
fn vm_squad_error_propagation() {
    let msg = run_on_vm_err(
        r#"
        let results = squad {
            spawn { 1 + 1 }
            spawn { must null }
        }
        say results
    "#,
    );
    assert!(
        msg.contains("squad task error"),
        "expected squad error, got: {}",
        msg
    );
}

#[test]
fn vm_squad_as_expression() {
    let out = run_on_vm(
        r#"
        let count = len(squad {
            spawn { 10 }
            spawn { 20 }
            spawn { 30 }
        })
        say count
    "#,
    );
    assert_eq!(out, vec!["3"]);
}

#[test]
fn vm_squad_non_spawn_setup() {
    let out = run_on_vm(
        r#"
        let results = squad {
            let x = 10
            spawn { x + 1 }
            spawn { x + 2 }
        }
        say results
    "#,
    );
    assert_eq!(out, vec!["[11, 12]"]);
}

#[test]
fn vm_squad_statement_form() {
    let out = run_on_vm(
        r#"
        squad {
            spawn { 1 + 1 }
        }
        say "done"
    "#,
    );
    assert_eq!(out, vec!["done"]);
}

#[test]
fn vm_squad_string_results() {
    let out = run_on_vm(
        r#"
        let results = squad {
            spawn { "hello" }
            spawn { "world" }
        }
        say join(results, " ")
    "#,
    );
    assert_eq!(out, vec!["hello world"]);
}

#[test]
fn vm_squad_many_spawns() {
    let out = run_on_vm(
        r#"
        let results = squad {
            spawn { 1 }
            spawn { 2 }
            spawn { 3 }
            spawn { 4 }
            spawn { 5 }
        }
        say sum(results)
    "#,
    );
    assert_eq!(out, vec!["15"]);
}

#[test]
fn vm_squad_with_return_in_spawn() {
    let out = run_on_vm(
        r#"
        let results = squad {
            spawn { return 99 }
            spawn { return 100 }
        }
        say results
    "#,
    );
    assert_eq!(out, vec!["[99, 100]"]);
}

#[test]
fn vm_squad_cancellation_stops_loop() {
    // A long-running loop task should be cancelled when a sibling errors.
    // If cancellation doesn't work, this test will hang (timeout).
    let msg = run_on_vm_err(
        r#"
        squad {
            spawn {
                let mut i = 0
                while i < 10000000 {
                    i = i + 1
                }
                i
            }
            spawn { must null }
        }
    "#,
    );
    assert!(
        msg.contains("squad task error"),
        "expected squad error, got: {}",
        msg
    );
}
