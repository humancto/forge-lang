pub mod bytecode;
pub mod compiler;
pub mod frame;
pub mod gc;
pub mod green;
pub mod jit;
pub mod machine;
pub mod serialize;
pub mod value;

use crate::parser::ast::Program;
use machine::{VMError, VM};

/// Compile and execute a Forge program using the bytecode VM.
pub fn run(program: &Program) -> Result<(), VMError> {
    let chunk = compiler::compile(program).map_err(|e| VMError::new(&e.message))?;
    let mut vm = VM::new();
    vm.execute(&chunk)?;
    Ok(())
}

/// Compile and execute with profiling enabled. Prints a report after execution.
pub fn run_with_profiling(program: &Program) -> Result<(), VMError> {
    let chunk = compiler::compile(program).map_err(|e| VMError::new(&e.message))?;
    let mut vm = VM::with_profiling();
    vm.execute(&chunk)?;
    vm.profiler.print_report();
    Ok(())
}

/// Compile and execute in REPL mode (returns the last value).
#[allow(dead_code)]
pub fn run_repl(vm: &mut VM, program: &Program) -> Result<value::Value, VMError> {
    let chunk = compiler::compile(program).map_err(|e| VMError::new(&e.message))?;
    vm.execute(&chunk)
}

#[cfg(test)]
mod parity_tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn run_on_vm(source: &str) -> Vec<String> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("lexer error");
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().expect("parse error");
        let chunk = compiler::compile(&program).expect("compile error");
        let mut vm = VM::new();
        vm.execute(&chunk).expect("vm error");
        vm.output.clone()
    }

    #[test]
    fn parity_arithmetic() {
        let out = run_on_vm("println(2 + 3)\nprintln(10 - 4)\nprintln(6 * 7)\nprintln(15 / 3)");
        assert_eq!(out, vec!["5", "6", "42", "5"]);
    }

    #[test]
    fn parity_variables() {
        let out = run_on_vm("let x = 42\nprintln(x)\nlet y = x + 8\nprintln(y)");
        assert_eq!(out, vec!["42", "50"]);
    }

    #[test]
    fn parity_mutable_variables() {
        let out = run_on_vm("let mut x = 0\nx = 10\nprintln(x)\nx = x + 5\nprintln(x)");
        assert_eq!(out, vec!["10", "15"]);
    }

    #[test]
    fn parity_string_interpolation() {
        let out = run_on_vm("let name = \"Forge\"\nprintln(\"Hello, {name}!\")");
        assert_eq!(out, vec!["Hello, Forge!"]);
    }

    #[test]
    fn parity_if_else() {
        let out =
            run_on_vm("let x = 10\nif x > 5 { println(\"big\") } else { println(\"small\") }");
        assert_eq!(out, vec!["big"]);
    }

    #[test]
    fn parity_if_else_false_branch() {
        let out = run_on_vm("let x = 3\nif x > 5 { println(\"big\") } else { println(\"small\") }");
        assert_eq!(out, vec!["small"]);
    }

    #[test]
    fn parity_while_loop() {
        let out = run_on_vm(
            "let mut i = 0\nlet mut sum = 0\nwhile i < 5 { sum = sum + i\ni = i + 1 }\nprintln(sum)",
        );
        assert_eq!(out, vec!["10"]);
    }

    #[test]
    fn parity_for_loop() {
        let out = run_on_vm("let items = [10, 20, 30]\nfor item in items { println(item) }");
        assert_eq!(out, vec!["10", "20", "30"]);
    }

    #[test]
    fn parity_function_call() {
        let out = run_on_vm("fn add(a, b) { return a + b }\nlet r = add(3, 4)\nprintln(r)");
        assert_eq!(out, vec!["7"]);
    }

    #[test]
    fn parity_recursion() {
        let out = run_on_vm(
            "fn fib(n) { if n <= 1 { return n } return fib(n - 1) + fib(n - 2) }\nprintln(fib(10))",
        );
        assert_eq!(out, vec!["55"]);
    }

    #[test]
    fn parity_closure_capture() {
        let out = run_on_vm(
            "fn make_adder(n) { return fn(x) { return x + n } }\nlet add5 = make_adder(5)\nprintln(add5(3))",
        );
        assert_eq!(out, vec!["8"]);
    }

    #[test]
    fn parity_closure_multiple_captures() {
        let out = run_on_vm(
            "fn make_adder(n) { return fn(x) { return x + n } }\nlet a5 = make_adder(5)\nlet a10 = make_adder(10)\nprintln(a5(1))\nprintln(a10(1))",
        );
        assert_eq!(out, vec!["6", "11"]);
    }

    #[test]
    fn parity_higher_order() {
        let out = run_on_vm(
            "fn apply(f, v) { return f(v) }\nfn double(x) { return x * 2 }\nprintln(apply(double, 21))",
        );
        assert_eq!(out, vec!["42"]);
    }

    #[test]
    fn parity_nested_if() {
        let out = run_on_vm(
            "let x = 15\nif x > 10 { if x > 20 { println(\"huge\") } else { println(\"big\") } } else { println(\"small\") }",
        );
        assert_eq!(out, vec!["big"]);
    }

    #[test]
    fn parity_boolean_ops() {
        let out = run_on_vm("println(true && false)\nprintln(true || false)\nprintln(!true)");
        assert_eq!(out, vec!["false", "true", "false"]);
    }

    #[test]
    fn parity_comparison() {
        let out = run_on_vm(
            "println(5 == 5)\nprintln(5 != 3)\nprintln(3 < 5)\nprintln(5 > 3)\nprintln(3 <= 3)\nprintln(5 >= 6)",
        );
        assert_eq!(out, vec!["true", "true", "true", "true", "true", "false"]);
    }

    #[test]
    fn parity_array_creation() {
        let out = run_on_vm("let a = [1, 2, 3]\nprintln(a)");
        assert_eq!(out, vec!["[1, 2, 3]"]);
    }

    #[test]
    fn parity_object_creation() {
        let out =
            run_on_vm("let o = { name: \"Odin\", level: 99 }\nprintln(o.name)\nprintln(o.level)");
        assert_eq!(out, vec!["Odin", "99"]);
    }

    #[test]
    fn parity_string_concat() {
        let out = run_on_vm("let a = \"hello\"\nlet b = \" world\"\nprintln(a + b)");
        assert_eq!(out, vec!["hello world"]);
    }

    #[test]
    fn parity_factorial() {
        let out = run_on_vm(
            "fn fact(n) { if n <= 1 { return 1 } return n * fact(n - 1) }\nprintln(fact(10))",
        );
        assert_eq!(out, vec!["3628800"]);
    }

    #[test]
    fn parity_break_in_loop() {
        let out =
            run_on_vm("let mut i = 0\nwhile true { if i >= 3 { break } println(i)\ni = i + 1 }");
        assert_eq!(out, vec!["0", "1", "2"]);
    }

    #[test]
    fn parity_continue_in_loop() {
        let out = run_on_vm(
            "let mut i = 0\nwhile i < 5 { i = i + 1\nif i == 3 { continue } println(i) }",
        );
        assert_eq!(out, vec!["1", "2", "4", "5"]);
    }

    #[test]
    fn parity_negation() {
        let out = run_on_vm("println(-5)\nprintln(-(3 + 4))");
        assert_eq!(out, vec!["-5", "-7"]);
    }

    #[test]
    fn parity_modulo() {
        let out = run_on_vm("println(10 % 3)\nprintln(7 % 2)");
        assert_eq!(out, vec!["1", "1"]);
    }

    #[test]
    fn parity_null_value() {
        let out = run_on_vm("println(null)");
        assert_eq!(out, vec!["null"]);
    }

    #[test]
    fn parity_float_arithmetic() {
        let out = run_on_vm("println(3.14 + 2.86)");
        assert_eq!(out, vec!["6"]);
    }

    #[test]
    fn parity_nested_function_calls() {
        let out = run_on_vm(
            "fn add(a, b) { return a + b }\nfn mul(a, b) { return a * b }\nprintln(add(mul(2, 3), mul(4, 5)))",
        );
        assert_eq!(out, vec!["26"]);
    }

    #[test]
    fn parity_empty_function() {
        let out = run_on_vm("fn noop() { }\nnoop()\nprintln(\"after\")");
        assert_eq!(out, vec!["after"]);
    }

    #[test]
    fn parity_array_index() {
        let out = run_on_vm("let a = [10, 20, 30]\nprintln(a[0])\nprintln(a[2])");
        assert_eq!(out, vec!["10", "30"]);
    }

    #[test]
    fn parity_match_literal() {
        let out = run_on_vm(
            "let x = 2\nmatch x { 1 => { println(\"one\") } 2 => { println(\"two\") } _ => { println(\"other\") } }",
        );
        assert_eq!(out, vec!["two"]);
    }

    #[test]
    fn parity_match_wildcard() {
        let out = run_on_vm(
            "let x = 99\nmatch x { 1 => { println(\"one\") } _ => { println(\"other\") } }",
        );
        assert_eq!(out, vec!["other"]);
    }

    #[test]
    fn parity_bytecode_roundtrip() {
        let source = "fn sq(n) { return n * n }\nprintln(sq(7))";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let chunk = compiler::compile(&program).unwrap();

        let bytes = serialize::serialize_chunk(&chunk).unwrap();
        let restored = serialize::deserialize_chunk(&bytes).unwrap();

        let mut vm = VM::new();
        vm.execute(&restored).unwrap();
        assert_eq!(vm.output, vec!["49"]);
    }

    #[test]
    fn parity_closure_roundtrip() {
        let source = "fn make_mul(n) { return fn(x) { return x * n } }\nlet triple = make_mul(3)\nprintln(triple(7))";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let chunk = compiler::compile(&program).unwrap();

        let bytes = serialize::serialize_chunk(&chunk).unwrap();
        let restored = serialize::deserialize_chunk(&bytes).unwrap();

        let mut vm = VM::new();
        vm.execute(&restored).unwrap();
        assert_eq!(vm.output, vec!["21"]);
    }
}
