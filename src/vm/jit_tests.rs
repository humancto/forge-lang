use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::vm::compiler;
use crate::vm::jit::jit_module::JitCompiler;
use crate::vm::jit::type_analysis;
use crate::vm::machine::{JitEntry, VM};

use crate::vm::bytecode::{encode_abc, encode_abx, Chunk, Constant, OpCode};
use crate::vm::jit::ir_builder::{self, StringRefs};
use crate::vm::value::{GcRef, ObjKind};

fn run_jit_function(source: &str) -> Vec<String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let chunk = compiler::compile(&program).unwrap();

    let mut vm = VM::new();
    let mut jit = JitCompiler::new().unwrap();

    for (i, proto) in chunk.prototypes.iter().enumerate() {
        let name = if proto.name.is_empty() {
            format!("fn_{}", i)
        } else {
            proto.name.clone()
        };
        let type_info = type_analysis::analyze(proto);
        // Pre-allocate string constants for string-capable functions
        let string_refs: Option<StringRefs> = if type_info.has_string_ops {
            Some(
                proto
                    .constants
                    .iter()
                    .map(|c| match c {
                        Constant::Str(s) => Some(vm.gc.alloc_string(s.clone()).0 as i64),
                        _ => None,
                    })
                    .collect(),
            )
        } else {
            None
        };
        let _ = jit.compile_function(proto, &name, string_refs.as_ref());
    }

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
                    has_string_ops: type_info.has_string_ops,
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
    let result = jit.compile_function(&chunk.prototypes[0], "greet", None);
    // String-only functions are now supported (no float mix)
    assert!(result.is_ok());
}

#[test]
fn jit_rejects_array_function() {
    let mut lexer = Lexer::new("fn make_arr() { return [1, 2, 3] }");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let chunk = compiler::compile(&program).unwrap();

    let mut jit = JitCompiler::new().unwrap();
    let result = jit.compile_function(&chunk.prototypes[0], "make_arr", None);
    assert!(result.is_err());
}

// ----- And/Or logical semantics -----

#[test]
fn jit_logical_and() {
    // 2 && 3 should produce 1 (true), not 2 (bitwise)
    let out = run_jit_function("fn test(a, b) { return a && b }\nprintln(test(2, 3))");
    assert_eq!(out, vec!["1"]);
}

#[test]
fn jit_logical_and_falsy() {
    let out = run_jit_function("fn test(a, b) { return a && b }\nprintln(test(0, 3))");
    assert_eq!(out, vec!["0"]);
}

#[test]
fn jit_logical_or() {
    // 2 || 0 should produce 1 (true), not 2 (bitwise)
    let out = run_jit_function("fn test(a, b) { return a || b }\nprintln(test(2, 0))");
    assert_eq!(out, vec!["1"]);
}

#[test]
fn jit_logical_or_both_false() {
    let out = run_jit_function("fn test(a, b) { return a || b }\nprintln(test(0, 0))");
    assert_eq!(out, vec!["0"]);
}

#[test]
fn jit_logical_not() {
    let out = run_jit_function("fn test(x) { return !x }\nprintln(test(0))\nprintln(test(42))");
    assert_eq!(out, vec!["1", "0"]);
}

// ----- Multi-argument functions (4+) -----

#[test]
fn jit_four_args() {
    let out =
        run_jit_function("fn sum4(a, b, c, d) { return a + b + c + d }\nprintln(sum4(1, 2, 3, 4))");
    assert_eq!(out, vec!["10"]);
}

#[test]
fn jit_five_args() {
    let out = run_jit_function(
        "fn sum5(a, b, c, d, e) { return a + b + c + d + e }\nprintln(sum5(1, 2, 3, 4, 5))",
    );
    assert_eq!(out, vec!["15"]);
}

#[test]
fn jit_six_args() {
    let out = run_jit_function(
        "fn sum6(a, b, c, d, e, f) { return a + b + c + d + e + f }\nprintln(sum6(1, 2, 3, 4, 5, 6))",
    );
    assert_eq!(out, vec!["21"]);
}

#[test]
fn jit_four_args_float() {
    let out = run_jit_function(
        "fn sum4f(a, b, c, d) { return a + b + c + d + 0.5 }\nprintln(sum4f(1, 2, 3, 4))",
    );
    assert_eq!(out, vec!["10.5"]);
}

// ----- Float operations -----

#[test]
fn jit_float_division() {
    // Use 0.0 to force float mode in type analysis
    let out = run_jit_function("fn fdiv(a, b) { return (a + 0.0) / b }\nprintln(fdiv(7.0, 2.0))");
    assert_eq!(out, vec!["3.5"]);
}

#[test]
fn jit_float_modulo() {
    let out = run_jit_function("fn fmod(a, b) { return (a + 0.0) % b }\nprintln(fmod(7.5, 2.0))");
    assert_eq!(out, vec!["1.5"]);
}

#[test]
fn jit_float_comparison() {
    let out = run_jit_function(
        "fn fmax(a, b) { if a > b + 0.0 { return a } return b }\nprintln(fmax(1.5, 2.5))\nprintln(fmax(3.5, 0.5))",
    );
    assert_eq!(out, vec!["2.5", "3.5"]);
}

#[test]
fn jit_float_equality() {
    let out = run_jit_function(
        "fn feq(a, b) { return a + 0.0 == b }\nprintln(feq(1.5, 1.5))\nprintln(feq(1.5, 2.5))",
    );
    assert_eq!(out, vec!["1", "0"]);
}

#[test]
fn jit_float_and_or() {
    // Use 0.0 constant to force float mode
    let out = run_jit_function(
        "fn fand(a, b) { return (a + 0.0) && (b + 0.0) }\nfn foor(a, b) { return (a + 0.0) || (b + 0.0) }\nprintln(fand(1.5, 2.5))\nprintln(fand(0.0, 2.5))\nprintln(foor(0.0, 0.0))\nprintln(foor(0.0, 1.5))",
    );
    assert_eq!(out, vec!["1", "0", "0", "1"]);
}

#[test]
fn jit_mixed_int_float_args() {
    // When function has float constants, all args are promoted to f64
    let out = run_jit_function("fn scale(x) { return x * 2.5 }\nprintln(scale(4))");
    assert_eq!(out, vec!["10"]);
}

// ----- Recursive + complex -----

#[test]
fn jit_fib_30() {
    let out = run_jit_function(
        "fn fib(n) { if n <= 1 { return n } return fib(n - 1) + fib(n - 2) }\nprintln(fib(30))",
    );
    assert_eq!(out, vec!["832040"]);
}

#[test]
fn jit_gcd() {
    let out = run_jit_function(
        "fn gcd(a, b) { if b == 0 { return a } return gcd(b, a % b) }\nprintln(gcd(48, 18))",
    );
    assert_eq!(out, vec!["6"]);
}

#[test]
fn jit_power() {
    let out = run_jit_function(
        "fn pow_rec(base, exp) { if exp == 0 { return 1 } return base * pow_rec(base, exp - 1) }\nprintln(pow_rec(2, 10))",
    );
    assert_eq!(out, vec!["1024"]);
}

#[test]
fn jit_collatz_steps() {
    let out = run_jit_function(
        "fn collatz(n) { if n == 1 { return 0 } if n % 2 == 0 { return 1 + collatz(n / 2) } return 1 + collatz(3 * n + 1) }\nprintln(collatz(27))",
    );
    assert_eq!(out, vec!["111"]);
}

#[test]
fn jit_nested_conditionals() {
    let out = run_jit_function(
        "fn classify(n) { if n < 0 { return -1 } if n == 0 { return 0 } return 1 }\nprintln(classify(-5))\nprintln(classify(0))\nprintln(classify(42))",
    );
    assert_eq!(out, vec!["-1", "0", "1"]);
}

#[test]
fn jit_while_loop_countdown() {
    let out = run_jit_function(
        "fn countdown(n) { let mut total = 0\nwhile n > 0 { total = total + n\nn = n - 1 }\nreturn total }\nprintln(countdown(10))",
    );
    assert_eq!(out, vec!["55"]);
}

#[test]
fn jit_boolean_chain() {
    // Test chaining logical operators
    let out = run_jit_function(
        "fn test(a, b, c) { return a && b && c }\nprintln(test(1, 1, 1))\nprintln(test(1, 0, 1))",
    );
    assert_eq!(out, vec!["1", "0"]);
}

#[test]
fn jit_all_comparisons() {
    let out = run_jit_function(
        "fn cmp(a, b) { \
        if a == b { return 1 } \
        if a != b { return 2 } \
        return 0 }\n\
        println(cmp(5, 5))\n\
        println(cmp(5, 3))",
    );
    assert_eq!(out, vec!["1", "2"]);
}

#[test]
fn jit_lte_gte() {
    let out = run_jit_function(
        "fn test_lte(a, b) { return a <= b }\n\
        fn test_gte(a, b) { return a >= b }\n\
        println(test_lte(3, 5))\n\
        println(test_lte(5, 5))\n\
        println(test_lte(7, 5))\n\
        println(test_gte(3, 5))\n\
        println(test_gte(5, 5))\n\
        println(test_gte(7, 5))",
    );
    assert_eq!(out, vec!["1", "1", "0", "0", "1", "1"]);
}

// ----- VMError stack trace tests -----
//
// Before this work the compiler emitted every instruction with line=0,
// so VMError.stack_trace either stayed empty or reported "(line 0)" for
// every frame. These tests pin the new behaviour: real source lines
// surface in the trace, and frames stack up across function calls.

fn compile_source(source: &str) -> crate::vm::bytecode::Chunk {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lex");
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().expect("parse");
    compiler::compile(&program).expect("compile")
}

#[test]
fn vm_error_stack_trace_reports_top_level_line() {
    // Three blank lines so the failing statement is on line 4 — we want
    // to confirm the trace doesn't always report line 1.
    let chunk = compile_source("\n\n\nlet arr = [1, 2, 3]\nprintln(arr[100])\n");
    let mut vm = VM::new();
    let err = vm.execute(&chunk).expect_err("should fail");

    assert!(err.message.contains("index out of bounds"));
    assert!(
        !err.stack_trace.is_empty(),
        "expected non-empty stack trace, got: {:?}",
        err
    );
    let top = &err.stack_trace[0];
    assert_eq!(top.function, "<main>");
    assert!(
        top.line >= 4,
        "expected top-level frame to report line >= 4, got line {}",
        top.line
    );
}

#[test]
fn vm_error_stack_trace_includes_called_function() {
    // The error originates inside `inner`, called from top-level. The
    // trace should include both frames in caller order (innermost first).
    let chunk = compile_source(
        r#"
fn inner() {
let arr = [1, 2, 3]
return arr[100]
}
let _ = inner()
"#,
    );
    let mut vm = VM::new();
    let err = vm.execute(&chunk).expect_err("should fail");

    let function_names: Vec<&str> = err
        .stack_trace
        .iter()
        .map(|f| f.function.as_str())
        .collect();
    assert!(
        function_names.contains(&"inner"),
        "expected `inner` in trace, got: {:?}",
        function_names
    );
    assert!(
        function_names.contains(&"<main>"),
        "expected `<main>` in trace, got: {:?}",
        function_names
    );
}

#[test]
fn vm_error_display_includes_trace() {
    // The Display impl is what main.rs prints to the user — confirm it
    // serialises the trace, not just the message.
    let chunk = compile_source("let arr = [1, 2, 3]\nprintln(arr[100])\n");
    let mut vm = VM::new();
    let err = vm.execute(&chunk).expect_err("should fail");

    let rendered = err.to_string();
    assert!(rendered.contains("index out of bounds"));
    assert!(
        rendered.contains("at <main>"),
        "expected trace in Display output, got: {}",
        rendered
    );
    assert!(
        rendered.contains("(line "),
        "expected `(line N)` in Display output, got: {}",
        rendered
    );
}

// ----- String operations via JIT bridges (bytecode-level) -----

/// Build a JIT function from raw bytecode and execute it, returning the i64 result.
fn run_jit_chunk(chunk: &Chunk, vm: &mut VM) -> i64 {
    let type_info = type_analysis::analyze(chunk);
    let string_refs: Option<ir_builder::StringRefs> = if type_info.has_string_ops {
        Some(
            chunk
                .constants
                .iter()
                .map(|c| match c {
                    Constant::Str(s) => Some(vm.gc.alloc_string(s.clone()).0 as i64),
                    _ => None,
                })
                .collect(),
        )
    } else {
        None
    };
    let mut jit = JitCompiler::new().unwrap();
    let ptr = jit
        .compile_function(chunk, "test_fn", string_refs.as_ref())
        .expect("JIT compile failed");

    if type_info.has_string_ops {
        let vm_ptr = vm as *mut VM as i64;
        unsafe { jit_call_i64(ptr, &[vm_ptr]) }.unwrap()
    } else {
        unsafe { jit_call_i64(ptr, &[]) }.unwrap()
    }
}

/// Helper to call jit_call_i64
unsafe fn jit_call_i64(ptr: *const u8, args: &[i64]) -> Result<i64, crate::vm::machine::VMError> {
    super::machine::jit_call_i64(ptr, args)
}

#[test]
fn jit_bridge_string_concat() {
    // Build bytecode: load "hello ", load "world", concat, return
    let mut chunk = Chunk::new("concat_test");
    chunk.arity = 0;
    chunk.max_registers = 3;
    let s1 = chunk.add_constant(Constant::Str("hello ".to_string()));
    let s2 = chunk.add_constant(Constant::Str("world".to_string()));
    chunk.emit(encode_abx(OpCode::LoadConst, 0, s1), 1);
    chunk.emit(encode_abx(OpCode::LoadConst, 1, s2), 2);
    chunk.emit(encode_abc(OpCode::Concat, 2, 0, 1), 3);
    chunk.emit(encode_abc(OpCode::Return, 2, 0, 0), 4);

    let mut vm = VM::new();
    let result = run_jit_chunk(&chunk, &mut vm);
    // Result is a GcRef index — verify the string content
    let obj = vm
        .gc
        .get(GcRef(result as usize))
        .expect("GcRef should be valid");
    match &obj.kind {
        ObjKind::String(s) => assert_eq!(s, "hello world"),
        _ => panic!("expected String, got non-string ObjKind"),
    }
}

#[test]
fn jit_bridge_string_len() {
    // Build bytecode: load "hello", len, return
    let mut chunk = Chunk::new("len_test");
    chunk.arity = 0;
    chunk.max_registers = 2;
    let s = chunk.add_constant(Constant::Str("hello".to_string()));
    chunk.emit(encode_abx(OpCode::LoadConst, 0, s), 1);
    chunk.emit(encode_abc(OpCode::Len, 1, 0, 0), 2);
    chunk.emit(encode_abc(OpCode::Return, 1, 0, 0), 3);

    let mut vm = VM::new();
    let result = run_jit_chunk(&chunk, &mut vm);
    assert_eq!(result, 5);
}

#[test]
fn jit_bridge_string_eq() {
    // Build bytecode: load "hi", load "hi", eq, return
    let mut chunk = Chunk::new("eq_test");
    chunk.arity = 0;
    chunk.max_registers = 3;
    let s1 = chunk.add_constant(Constant::Str("hi".to_string()));
    let s2 = chunk.add_constant(Constant::Str("hi".to_string()));
    chunk.emit(encode_abx(OpCode::LoadConst, 0, s1), 1);
    chunk.emit(encode_abx(OpCode::LoadConst, 1, s2), 2);
    chunk.emit(encode_abc(OpCode::Eq, 2, 0, 1), 3);
    chunk.emit(encode_abc(OpCode::Return, 2, 0, 0), 4);

    let mut vm = VM::new();
    let result = run_jit_chunk(&chunk, &mut vm);
    assert_eq!(result, 1);
}

#[test]
fn jit_bridge_string_neq() {
    // Build bytecode: load "hi", load "bye", not-eq, return
    let mut chunk = Chunk::new("neq_test");
    chunk.arity = 0;
    chunk.max_registers = 3;
    let s1 = chunk.add_constant(Constant::Str("hi".to_string()));
    let s2 = chunk.add_constant(Constant::Str("bye".to_string()));
    chunk.emit(encode_abx(OpCode::LoadConst, 0, s1), 1);
    chunk.emit(encode_abx(OpCode::LoadConst, 1, s2), 2);
    chunk.emit(encode_abc(OpCode::NotEq, 2, 0, 1), 3);
    chunk.emit(encode_abc(OpCode::Return, 2, 0, 0), 4);

    let mut vm = VM::new();
    let result = run_jit_chunk(&chunk, &mut vm);
    assert_eq!(result, 1);
}

// High-level string eq/neq tests (compiler-generated bytecode)

#[test]
fn jit_string_eq() {
    let out = run_jit_function(
        "fn streq(a, b) { return a == b }\nprintln(streq(\"hi\", \"hi\"))\nprintln(streq(\"hi\", \"bye\"))",
    );
    assert_eq!(out, vec!["1", "0"]);
}

#[test]
fn jit_string_not_eq() {
    let out = run_jit_function(
        "fn strneq(a, b) { return a != b }\nprintln(strneq(\"hi\", \"hi\"))\nprintln(strneq(\"hi\", \"bye\"))",
    );
    assert_eq!(out, vec!["0", "1"]);
}
