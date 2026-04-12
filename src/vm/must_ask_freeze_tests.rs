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

#[test]
fn vm_must_unwraps_ok() {
    let result = run_vm("let x = must Ok(42)\nprintln(x)");
    assert!(result.is_ok());
}

#[test]
fn vm_must_passes_through_plain_value() {
    let result = run_vm("let x = must 99\nprintln(x)");
    assert!(result.is_ok());
}

#[test]
fn vm_must_crashes_on_err() {
    let result = run_vm("let x = must Err(\"boom\")");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("must failed"),
        "expected 'must failed' error, got: {}",
        err.message
    );
}

#[test]
fn vm_must_crashes_on_null() {
    let result = run_vm("let x = must null");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("must failed: got null"));
}

#[test]
fn vm_freeze_wraps_value() {
    let result = run_vm("let x = freeze { a: 1 }\nprintln(x)");
    assert!(result.is_ok());
}

#[test]
fn vm_ask_requires_api_key() {
    // Without API key set, ask should return an error
    std::env::remove_var("FORGE_AI_KEY");
    std::env::remove_var("OPENAI_API_KEY");
    let result = run_vm("let x = ask \"hello\"");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("FORGE_AI_KEY"));
}
