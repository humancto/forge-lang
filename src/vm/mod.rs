mod builtins; // VM builtin dispatch — extracted from machine.rs
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
    let chunk = compiler::compile_repl(program).map_err(|e| VMError::new(&e.message))?;
    vm.execute(&chunk)
}

#[cfg(test)]
mod parity_tests {
    use super::*;
    use crate::interpreter::Interpreter;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::vm::jit::jit_module::JitCompiler;
    use crate::vm::jit::type_analysis;
    use crate::vm::machine::JitEntry;

    fn parse_program(source: &str) -> crate::parser::ast::Program {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("lexer error");
        let mut parser = Parser::new(tokens);
        parser.parse_program().expect("parse error")
    }

    fn run_on_interpreter_value(source: &str) -> String {
        let program = parse_program(source);
        let mut interpreter = Interpreter::new();
        let value = interpreter.run_repl(&program).expect("interpreter error");
        value.to_string()
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

    fn run_on_bytecode_value(source: &str) -> String {
        let program = parse_program(source);
        let chunk = compiler::compile_repl(&program).expect("compile error");
        let bytes = serialize::serialize_chunk(&chunk).expect("serialize error");
        let restored = serialize::deserialize_chunk(&bytes).expect("deserialize error");
        let mut vm = VM::new();
        let value = vm.execute(&restored).expect("vm error");
        value.display(&vm.gc)
    }

    fn run_on_jit_value(source: &str) -> String {
        let program = parse_program(source);
        let chunk = compiler::compile_repl(&program).expect("compile error");

        let mut jit = JitCompiler::new().expect("jit init");
        for (i, proto) in chunk.prototypes.iter().enumerate() {
            let name = if proto.name.is_empty() {
                format!("fn_{}", i)
            } else {
                proto.name.clone()
            };
            let info = type_analysis::analyze(proto);
            if !info.has_unsupported_ops {
                let _ = jit.compile_function(proto, &name);
            }
        }

        let mut vm = VM::new();
        for (i, proto) in chunk.prototypes.iter().enumerate() {
            let name = if proto.name.is_empty() {
                format!("fn_{}", i)
            } else {
                proto.name.clone()
            };
            let info = type_analysis::analyze(proto);
            if !info.has_unsupported_ops {
                if let Some(ptr) = jit.get_compiled(&name) {
                    vm.jit_cache.insert(
                        name,
                        JitEntry {
                            ptr,
                            uses_float: info.has_float,
                        },
                    );
                }
            }
        }

        let value = vm.execute(&chunk).expect("jit-assisted vm error");
        value.display(&vm.gc)
    }

    fn assert_cross_backend_value(source: &str, expected: &str) {
        let interp = run_on_interpreter_value(source);
        let vm = run_on_vm_value(source);
        let bytecode = run_on_bytecode_value(source);
        let jit = run_on_jit_value(source);

        assert_eq!(interp, expected);
        assert_eq!(vm, expected);
        assert_eq!(bytecode, expected);
        assert_eq!(jit, expected);
    }

    fn assert_cross_backend_error_contains(source: &str, expected: &str) {
        let program = parse_program(source);

        let mut interpreter = Interpreter::new();
        let interp_err = interpreter
            .run_repl(&program)
            .expect_err("interpreter should error")
            .to_string();

        let chunk = compiler::compile_repl(&program).expect("compile error");

        let mut vm = VM::new();
        let vm_err = vm.execute(&chunk).expect_err("vm should error").to_string();

        let bytes = serialize::serialize_chunk(&chunk).expect("serialize error");
        let restored = serialize::deserialize_chunk(&bytes).expect("deserialize error");
        let mut bytecode_vm = VM::new();
        let bytecode_err = bytecode_vm
            .execute(&restored)
            .expect_err("bytecode should error")
            .to_string();

        let mut jit = JitCompiler::new().expect("jit init error");
        for (index, proto) in chunk.prototypes.iter().enumerate() {
            let name = if proto.name.is_empty() {
                format!("fn_{}", index)
            } else {
                proto.name.clone()
            };
            let info = type_analysis::analyze(proto);
            if !info.has_unsupported_ops {
                let _ = jit.compile_function(proto, &name);
            }
        }
        let mut jit_vm = VM::new();
        for (index, proto) in chunk.prototypes.iter().enumerate() {
            let name = if proto.name.is_empty() {
                format!("fn_{}", index)
            } else {
                proto.name.clone()
            };
            let info = type_analysis::analyze(proto);
            if !info.has_unsupported_ops {
                if let Some(ptr) = jit.get_compiled(&name) {
                    jit_vm.jit_cache.insert(
                        name,
                        JitEntry {
                            ptr,
                            uses_float: info.has_float,
                        },
                    );
                }
            }
        }
        let jit_err = jit_vm
            .execute(&chunk)
            .expect_err("jit-assisted vm should error")
            .to_string();

        assert!(interp_err.contains(expected), "interpreter error: {}", interp_err);
        assert!(vm_err.contains(expected), "vm error: {}", vm_err);
        assert!(
            bytecode_err.contains(expected),
            "bytecode error: {}",
            bytecode_err
        );
        assert!(jit_err.contains(expected), "jit error: {}", jit_err);
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

    #[test]
    fn cross_backend_parity_integer_function() {
        assert_cross_backend_value("fn square(n) { return n * n }\nsquare(9)", "81");
    }

    #[test]
    fn cross_backend_parity_object_destructure() {
        assert_cross_backend_value(
            r#"
            let point = { x: 20, y: 22 }
            unpack { x, y } from point
            fn add(a, b) { return a + b }
            add(x, y)
            "#,
            "42",
        );
    }

    #[test]
    fn cross_backend_parity_array_destructure_without_rest() {
        assert_cross_backend_value(
            r#"
            let values = [10, 32]
            unpack [left, right] from values
            left + right
            "#,
            "42",
        );
    }

    #[test]
    fn cross_backend_parity_array_destructure_with_rest() {
        assert_cross_backend_value(
            r#"
            let values = [1, 2, 3]
            unpack [first, ...rest] from values
            [first, rest[0], rest[1]]
            "#,
            "[1, 2, 3]",
        );
    }

    #[test]
    fn cross_backend_parity_try_catch_nested_call() {
        assert_cross_backend_value(
            r#"
            fn boom() {
                return 1 / 0
            }
            let mut status = "ok"
            try {
                boom()
            } catch err {
                status = err.type
            }
            status
            "#,
            "ArithmeticError",
        );
    }

    #[test]
    fn cross_backend_parity_try_catch_after_loop_continue() {
        assert_cross_backend_value(
            r#"
            let mut outcome = ""
            let mut seen = 0
            while seen < 3 {
                seen += 1
                try {
                    continue
                } catch err {
                    outcome = "bad"
                }
            }
            try {
                let crash = 1 / 0
            } catch err {
                outcome = err.type
            }
            outcome
            "#,
            "ArithmeticError",
        );
    }

    #[test]
    fn cross_backend_parity_safe_block_swallows_error() {
        assert_cross_backend_value(
            r#"
            let mut status = "ok"
            safe {
                let crash = 1 / 0
                status = "bad"
            }
            status
            "#,
            "ok",
        );
    }

    #[test]
    fn cross_backend_parity_retry_block_recovers() {
        assert_cross_backend_value(
            r#"
            let mut attempts = 0
            retry 3 times {
                attempts += 1
                if attempts < 3 {
                    let crash = 1 / 0
                }
            }
            attempts
            "#,
            "3",
        );
    }

    #[test]
    fn cross_backend_parity_retry_block_failure_message() {
        assert_cross_backend_error_contains(
            r#"
            retry 2 times {
                let crash = 1 / 0
            }
            "#,
            "retry failed after 2 attempts",
        );
    }

    #[test]
    fn cross_backend_parity_timeout_block_fast_path() {
        assert_cross_backend_value(
            r#"
            let mut status = "pending"
            timeout 1 seconds {
                status = "done"
            }
            status
            "#,
            "done",
        );
    }

    #[test]
    fn cross_backend_parity_timeout_block_expires() {
        assert_cross_backend_error_contains(
            r#"
            timeout 1 seconds {
                wait(2)
            }
            "#,
            "timeout: operation exceeded 1 second limit",
        );
    }

    #[test]
    fn cross_backend_parity_file_import() {
        assert_cross_backend_value(
            r#"
            import "tests/parity/modules/import_helper.fg"
            helper()
            "#,
            "42",
        );
    }

    #[test]
    fn cross_backend_parity_named_file_import() {
        assert_cross_backend_value(
            r#"
            import { answer } from "tests/parity/modules/import_helper.fg"
            answer
            "#,
            "42",
        );
    }

    #[test]
    fn cross_backend_parity_mutable_closure_counter() {
        assert_cross_backend_value(
            r#"
            fn make_counter() {
                let mut count = 0
                return fn() {
                    count = count + 1
                    return count
                }
            }
            let counter = make_counter()
            let a = counter()
            let b = counter()
            let c = counter()
            [a, b, c]
            "#,
            "[1, 2, 3]",
        );
    }

    #[test]
    fn cross_backend_parity_nested_closure_mutation() {
        assert_cross_backend_value(
            r#"
            fn outer() {
                let mut x = 0
                fn middle() {
                    x = x + 10
                    fn inner() {
                        x = x + 1
                    }
                    inner()
                }
                middle()
                return x
            }
            outer()
            "#,
            "11",
        );
    }

    #[test]
    fn cross_backend_parity_sibling_closures_share_state() {
        assert_cross_backend_value(
            r#"
            fn run_pair() {
                let mut n = 0
                let inc = fn() { n = n + 1 }
                let read = fn() { return n }
                inc()
                return read()
            }
            run_pair()
            "#,
            "1",
        );
    }

    #[test]
    fn cross_backend_parity_give_instance_method() {
        assert_cross_backend_value(
            r#"
            thing Person {
                name: String,
                age: Int
            }
            give Person {
                fn greet(it) {
                    return "Hi, I'm " + it.name
                }
            }
            let p = Person { name: "Alice", age: 30 }
            p.greet()
            "#,
            "Hi, I'm Alice",
        );
    }

    #[test]
    fn cross_backend_parity_give_static_method() {
        assert_cross_backend_value(
            r#"
            thing Person {
                name: String,
                age: Int
            }
            give Person {
                fn infant(name) {
                    return Person { name: name, age: 0 }
                }
            }
            let baby = Person.infant("Bob")
            baby.name
            "#,
            "Bob",
        );
    }

    #[test]
    fn cross_backend_parity_power_satisfies_with_give_methods() {
        assert_cross_backend_value(
            r#"
            thing Robot {
                id: Int
            }
            power Speakable {
                fn speak() -> String
            }
            give Robot {
                fn speak(it) {
                    return "Beep " + str(it.id)
                }
            }
            let r = Robot { id: 42 }
            satisfies(r, Speakable)
            "#,
            "true",
        );
    }

    #[test]
    fn cross_backend_parity_struct_defaults() {
        assert_cross_backend_value(
            r#"
            thing Person {
                name: String = "Anonymous",
                age: Int = 0
            }
            let p = Person {}
            p.name + ":" + str(p.age)
            "#,
            "Anonymous:0",
        );
    }

    #[test]
    fn cross_backend_parity_embedded_field_and_method_delegation() {
        assert_cross_backend_value(
            r#"
            thing Engine {
                hp: Int
            }
            thing Car {
                name: String,
                has engine: Engine
            }
            give Engine {
                fn power(it) {
                    return str(it.hp) + "hp"
                }
            }
            let c = Car {
                name: "Mustang",
                engine: Engine { hp: 450 }
            }
            str(c.hp) + ":" + c.power()
            "#,
            "450:450hp",
        );
    }

    #[test]
    fn cross_backend_parity_adt_unit_variants() {
        assert_cross_backend_value(
            r#"
            type Color = Red | Green | Blue
            let color = Green
            let mut label = ""
            match color {
                Red => { label = "red" }
                Green => { label = "green" }
                Blue => { label = "blue" }
            }
            label
            "#,
            "green",
        );
    }

    #[test]
    fn cross_backend_parity_adt_constructor_fields() {
        assert_cross_backend_value(
            r#"
            type Shape = Circle(Float) | Rect(Float, Float)
            let shape = Rect(3.0, 4.0)
            let mut area = 0.0
            match shape {
                Circle(r) => { area = r }
                Rect(w, h) => { area = w * h }
            }
            area
            "#,
            "12",
        );
    }

    #[test]
    fn vm_power_missing_method_errors() {
        let program = parse_program(
            r#"
            thing Dog {
                name: String
            }
            power Trainable {
                fn sit() -> String
                fn stay() -> String
            }
            give Dog the power Trainable {
                fn sit(it) {
                    return it.name + " sits"
                }
            }
            "#,
        );
        let chunk = compiler::compile_repl(&program).expect("compile error");
        let mut vm = VM::new();
        let err = vm
            .execute(&chunk)
            .expect_err("vm should reject incomplete impl");
        assert!(
            err.message.contains("stay"),
            "expected missing method in error, got: {}",
            err.message
        );
    }

    #[test]
    fn vm_repl_returns_last_non_output_expression() {
        let program = parse_program("1\nprintln(2)\n3");
        let mut vm = VM::new();
        let value = super::run_repl(&mut vm, &program).expect("vm repl error");
        assert_eq!(value.display(&vm.gc), "3");
    }
}

#[cfg(test)]
mod jit_tests {
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::vm::compiler;
    use crate::vm::jit::jit_module::JitCompiler;
    use crate::vm::jit::type_analysis;
    use crate::vm::machine::{JitEntry, VM};

    fn run_jit_function(source: &str) -> Vec<String> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let chunk = compiler::compile(&program).unwrap();

        let mut jit = JitCompiler::new().unwrap();
        for (i, proto) in chunk.prototypes.iter().enumerate() {
            let name = if proto.name.is_empty() {
                format!("fn_{}", i)
            } else {
                proto.name.clone()
            };
            let _ = jit.compile_function(proto, &name);
        }

        let mut vm = VM::new();
        for (i, proto) in chunk.prototypes.iter().enumerate() {
            let name = if proto.name.is_empty() {
                format!("fn_{}", i)
            } else {
                proto.name.clone()
            };
            if let Some(ptr) = jit.get_compiled(&name) {
                let type_info = type_analysis::analyze(proto);
                vm.jit_cache.insert(
                    name,
                    JitEntry {
                        ptr,
                        uses_float: type_info.has_float,
                    },
                );
            }
        }

        vm.execute(&chunk).unwrap();
        vm.output.clone()
    }

    #[test]
    fn jit_fib_integer() {
        let out = run_jit_function(
            "fn fib(n) { if n <= 1 { return n } return fib(n - 1) + fib(n - 2) }\nprintln(fib(10))",
        );
        assert_eq!(out, vec!["55"]);
    }

    #[test]
    fn jit_factorial() {
        let out = run_jit_function(
            "fn fact(n) { if n <= 1 { return 1 } return n * fact(n - 1) }\nprintln(fact(10))",
        );
        assert_eq!(out, vec!["3628800"]);
    }

    #[test]
    fn jit_add_two_args() {
        let out = run_jit_function("fn add(a, b) { return a + b }\nprintln(add(17, 25))");
        assert_eq!(out, vec!["42"]);
    }

    #[test]
    fn jit_subtract() {
        let out = run_jit_function("fn sub(a, b) { return a - b }\nprintln(sub(100, 58))");
        assert_eq!(out, vec!["42"]);
    }

    #[test]
    fn jit_multiply() {
        let out = run_jit_function("fn mul(a, b) { return a * b }\nprintln(mul(6, 7))");
        assert_eq!(out, vec!["42"]);
    }

    #[test]
    fn jit_division() {
        let out = run_jit_function("fn div(a, b) { return a / b }\nprintln(div(84, 2))");
        assert_eq!(out, vec!["42"]);
    }

    #[test]
    fn jit_modulo() {
        let out = run_jit_function("fn modop(a, b) { return a % b }\nprintln(modop(10, 3))");
        assert_eq!(out, vec!["1"]);
    }

    #[test]
    fn jit_negation() {
        let out = run_jit_function("fn neg(x) { return -x }\nprintln(neg(42))");
        assert_eq!(out, vec!["-42"]);
    }

    #[test]
    fn jit_comparison() {
        let out = run_jit_function(
            "fn max(a, b) { if a > b { return a } return b }\nprintln(max(10, 20))\nprintln(max(30, 5))",
        );
        assert_eq!(out, vec!["20", "30"]);
    }

    #[test]
    fn jit_zero_args() {
        let out = run_jit_function("fn answer() { return 42 }\nprintln(answer())");
        assert_eq!(out, vec!["42"]);
    }

    #[test]
    fn jit_nested_calls() {
        let out = run_jit_function(
            "fn sq(n) { return n * n }\nfn sum_sq(a, b) { return sq(a) + sq(b) }\nprintln(sum_sq(3, 4))",
        );
        assert_eq!(out, vec!["25"]);
    }

    #[test]
    fn jit_loop_accumulator() {
        let out = run_jit_function(
            "fn sum_to(n) { let mut s = 0\nlet mut i = 1\nwhile i <= n { s = s + i\ni = i + 1 }\nreturn s }\nprintln(sum_to(100))",
        );
        assert_eq!(out, vec!["5050"]);
    }

    #[test]
    fn jit_boolean_function() {
        let out = run_jit_function(
            "fn is_even(n) { return n % 2 == 0 }\nprintln(is_even(4))\nprintln(is_even(7))",
        );
        // JIT returns int (1/0) for boolean results; VM println shows as 1/0
        assert_eq!(out, vec!["1", "0"]);
    }

    #[test]
    fn jit_float_arithmetic() {
        let out = run_jit_function(
            "fn circle_area(r) { return 3.14159 * r * r }\nprintln(circle_area(10.0))",
        );
        assert_eq!(out, vec!["314.159"]);
    }

    #[test]
    fn jit_float_negation() {
        let out = run_jit_function("fn neg_pi() { return -3.14159 }\nprintln(neg_pi())");
        assert_eq!(out, vec!["-3.14159"]);
    }

    #[test]
    fn jit_rejects_string_function() {
        let mut lexer = Lexer::new("fn greet() { return \"hello\" }");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let chunk = compiler::compile(&program).unwrap();

        let mut jit = JitCompiler::new().unwrap();
        let result = jit.compile_function(&chunk.prototypes[0], "greet");
        assert!(result.is_err());
    }

    #[test]
    fn jit_rejects_array_function() {
        let mut lexer = Lexer::new("fn make_arr() { return [1, 2, 3] }");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let chunk = compiler::compile(&program).unwrap();

        let mut jit = JitCompiler::new().unwrap();
        let result = jit.compile_function(&chunk.prototypes[0], "make_arr");
        assert!(result.is_err());
    }
}
