use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::vm::compiler;
use crate::vm::jit::jit_module::JitCompiler;
use crate::vm::jit::tier::JitMode;
use crate::vm::jit::types::{JitType, TypeSig};
use crate::vm::jit::verifier::{self, RejectKind};
use crate::vm::machine::VM;

use crate::vm::bytecode::{encode_abc, encode_abx, Chunk, Constant, OpCode};
use crate::vm::value::{GcRef, ObjKind};

/// Run `source` with the given JIT mode. Returns the program output, the
/// error message (if execution failed) and the VM for introspection.
fn run_mode(source: &str, mode: JitMode) -> (Vec<String>, Option<String>, VM) {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let chunk = compiler::compile(&program).unwrap();
    let mut vm = VM::new();
    vm.set_jit_mode(mode);
    let err = vm.execute(&chunk).err().map(|e| e.message);
    let out = vm.output.clone();
    (out, err, vm)
}

/// Run `source` with the JIT off, auto-tiered and eager, assert all three
/// agree exactly (output and error), and return the output. The VM with the
/// JIT off is the reference.
fn run_jit_function(source: &str) -> Vec<String> {
    let (vm_out, vm_err, _) = run_mode(source, JitMode::Off);
    for mode in [JitMode::Auto, JitMode::Eager] {
        let (out, err, _) = run_mode(source, mode);
        assert_eq!(out, vm_out, "{:?} output differs from the VM", mode);
        assert_eq!(err, vm_err, "{:?} error differs from the VM", mode);
    }
    assert!(vm_err.is_none(), "unexpected error: {:?}", vm_err);
    vm_out
}

/// Like `run_jit_function`, and also assert that `name` really ran natively
/// under the eager tier (so the test exercises compiled code, not fallback).
fn run_jit_native(source: &str, name: &str) -> Vec<String> {
    let out = run_jit_function(source);
    let (_, _, vm) = run_mode(source, JitMode::Eager);
    assert!(
        vm.jit.native_runs(name) > 0,
        "expected `{}` to run natively; compiled: {:?}",
        name,
        vm.jit.compiled_function_names()
    );
    out
}

/// Parity including errors: returns (output, error) after asserting all
/// JIT modes match the VM.
fn run_parity(source: &str) -> (Vec<String>, Option<String>) {
    let (vm_out, vm_err, _) = run_mode(source, JitMode::Off);
    for mode in [JitMode::Auto, JitMode::Eager] {
        let (out, err, _) = run_mode(source, mode);
        assert_eq!(out, vm_out, "{:?} output differs from the VM", mode);
        assert_eq!(err, vm_err, "{:?} error differs from the VM", mode);
    }
    (vm_out, vm_err)
}

/// First prototype of a compiled snippet.
fn first_proto(source: &str) -> Chunk {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let chunk = compiler::compile(&program).unwrap();
    chunk.prototypes[0].clone()
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
    assert_eq!(out, vec!["true", "false"]);
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
    let proto = first_proto("fn greet() { return \"hello\" }");
    let reject = verifier::verify(&proto, &TypeSig(vec![])).expect_err("must reject");
    assert_eq!(reject.kind, RejectKind::UnsupportedConstant("string"));
    assert_eq!(
        run_jit_function("fn greet() { return \"hello\" }\nprintln(greet())"),
        vec!["hello"]
    );
}

#[test]
fn jit_rejects_array_function() {
    let proto = first_proto("fn make_arr() { return [1, 2, 3] }");
    let reject = verifier::verify(&proto, &TypeSig(vec![])).expect_err("must reject");
    assert_eq!(reject.kind, RejectKind::UnsupportedOpcode(OpCode::NewArray));
    assert_eq!(
        run_jit_function("fn make_arr() { return [1, 2, 3] }\nprintln(make_arr())"),
        vec!["[1, 2, 3]"]
    );
}

#[test]
fn jit_for_loop_over_array() {
    // IterGet is outside the current tier: the function must be rejected
    // and run in the VM (a silent miscompile would produce 0 instead of 60).
    let out = run_jit_function(
        "fn sum_arr() { let a = [10, 20, 30]\nlet mut t = 0\nfor x in a { t = t + x }\nreturn t }\nprintln(sum_arr())",
    );
    assert_eq!(out, vec!["60"]);
}

// ----- And/Or logical semantics -----

#[test]
fn jit_logical_and() {
    // 2 && 3 should produce 1 (true), not 2 (bitwise)
    let out = run_jit_function("fn test(a, b) { return a && b }\nprintln(test(2, 3))");
    assert_eq!(out, vec!["true"]);
}

#[test]
fn jit_logical_and_falsy() {
    let out = run_jit_function("fn test(a, b) { return a && b }\nprintln(test(0, 3))");
    assert_eq!(out, vec!["false"]);
}

#[test]
fn jit_logical_or() {
    // 2 || 0 should produce 1 (true), not 2 (bitwise)
    let out = run_jit_function("fn test(a, b) { return a || b }\nprintln(test(2, 0))");
    assert_eq!(out, vec!["true"]);
}

#[test]
fn jit_logical_or_both_false() {
    let out = run_jit_function("fn test(a, b) { return a || b }\nprintln(test(0, 0))");
    assert_eq!(out, vec!["false"]);
}

#[test]
fn jit_logical_not() {
    let out = run_jit_function("fn test(x) { return !x }\nprintln(test(0))\nprintln(test(42))");
    assert_eq!(out, vec!["true", "false"]);
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
    assert_eq!(out, vec!["true", "false"]);
}

#[test]
fn jit_float_and_or() {
    // Use 0.0 constant to force float mode
    let out = run_jit_function(
        "fn fand(a, b) { return (a + 0.0) && (b + 0.0) }\nfn foor(a, b) { return (a + 0.0) || (b + 0.0) }\nprintln(fand(1.5, 2.5))\nprintln(fand(0.0, 2.5))\nprintln(foor(0.0, 0.0))\nprintln(foor(0.0, 1.5))",
    );
    assert_eq!(out, vec!["true", "false", "false", "true"]);
}

#[test]
fn jit_mixed_int_float_args() {
    // When function has float constants, all args are promoted to f64
    let out = run_jit_function("fn scale(x) { return x * 2.5 }\nprintln(scale(4))");
    assert_eq!(out, vec!["10"]);
}

// ----- Mixed type functions (float + string/collection) -----

#[test]
fn jit_float_with_int_conversion() {
    // Function with float ops that returns a whole number (should auto-convert to int display)
    let out = run_jit_function("fn double(x) { return x * 2.0 }\nprintln(double(5.0))");
    assert_eq!(out, vec!["10"]);
}

#[test]
fn jit_float_multi_op_chain() {
    // Chain of float operations: (a + b) * c - d
    let out = run_jit_function(
        "fn calc(a, b, c, d) { return (a + b) * c - d + 0.0 }\nprintln(calc(1.5, 2.5, 3.0, 1.0))",
    );
    assert_eq!(out, vec!["11"]);
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
    assert_eq!(out, vec!["true", "false"]);
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
    assert_eq!(out, vec!["true", "true", "false", "false", "true", "true"]);
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

fn compile_source_result(
    source: &str,
) -> Result<crate::vm::bytecode::Chunk, compiler::CompileError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lex");
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().expect("parse");
    compiler::compile(&program)
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
    assert!(top.col > 0, "expected top-level frame to report a column");
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
    let inner = err
        .stack_trace
        .iter()
        .find(|f| f.function == "inner")
        .expect("inner frame");
    assert!(inner.col > 0, "expected inner frame to report a column");
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
    assert!(
        rendered.contains(", col "),
        "expected `(line N, col M)` in Display output, got: {}",
        rendered
    );
}

#[test]
fn vm_compiler_treats_literal_server_decorator_as_metadata() {
    // The host runtime reads `@server` from the AST for both engines; with
    // literal arguments there is nothing for the VM to execute.
    assert!(compile_source_result("@server(port: 8080, cors: \"permissive\")\n").is_ok());
}

#[test]
fn vm_compiler_rejects_standalone_decorators_it_cannot_honor() {
    // The interpreter evaluates `@server` arguments; the VM must not drop
    // a call silently, so the program stays on the interpreter.
    for source in ["@server(port: pick_port())\n", "@cache(ttl: 60)\n"] {
        let err = compile_source_result(source)
            .expect_err("unsupported standalone decorators must not silently compile");
        assert!(err.is_unsupported(), "unexpected error: {}", err.message);
        assert!(
            err.message
                .contains("VM does not support decorator-driven runtime features"),
            "unexpected error: {}",
            err.message
        );
    }
}

#[test]
fn vm_compiler_accepts_metadata_function_decorator() {
    let result = compile_source_result("@test\nfn sample() { return 1 }\n");
    assert!(
        result.is_ok(),
        "metadata decorators attached to functions should still compile: {:?}",
        result.err().map(|e| e.message)
    );
}

// ----- String operations via JIT bridges (bytecode-level) -----

/// The string runtime bridges are the substrate for a future string tier.
/// The current tier must *reject* string bytecode (it cannot prove its
/// semantics), so these tests (1) assert the verifier rejects the chunk with
/// a structured reason and (2) drive the bridges directly over the chunk's
/// instructions to keep them tested.
fn run_jit_chunk(chunk: &Chunk, vm: &mut VM) -> i64 {
    use crate::vm::bytecode::{decode_a, decode_b, decode_bx, decode_c, decode_op};
    use crate::vm::jit::runtime::{rt_string_concat, rt_string_eq, rt_string_len};

    let reject =
        verifier::verify(chunk, &TypeSig(vec![])).expect_err("string code must be rejected");
    // String constants are allowed only as method names, so string code is
    // rejected where a string is used as a value or by a string opcode.
    assert!(
        matches!(
            reject.kind,
            RejectKind::UnsupportedConstant("string")
                | RejectKind::UnsupportedOpcode(OpCode::Concat | OpCode::Len)
        ),
        "{:?}",
        reject
    );

    let vm_ptr = vm as *mut VM;
    let mut regs = vec![0i64; chunk.max_registers as usize + 1];
    for &inst in &chunk.code {
        let op = OpCode::try_from(decode_op(inst)).unwrap();
        let (a, b, c) = (
            decode_a(inst) as usize,
            decode_b(inst) as usize,
            decode_c(inst) as usize,
        );
        match op {
            OpCode::LoadConst => match &chunk.constants[decode_bx(inst) as usize] {
                Constant::Str(s) => regs[a] = vm.gc.alloc_string(s.clone()).0 as i64,
                other => panic!("unexpected constant {:?}", other),
            },
            OpCode::Concat => regs[a] = rt_string_concat(vm_ptr, regs[b], regs[c]),
            OpCode::Len => regs[a] = rt_string_len(vm_ptr, regs[b]),
            OpCode::Eq => regs[a] = rt_string_eq(vm_ptr, regs[b], regs[c]),
            OpCode::NotEq => regs[a] = 1 - rt_string_eq(vm_ptr, regs[b], regs[c]),
            OpCode::Return => return regs[a],
            other => panic!("unexpected opcode {:?}", other),
        }
    }
    panic!("chunk did not return");
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
    assert_eq!(out, vec!["true", "false"]);
}

#[test]
fn jit_string_not_eq() {
    let out = run_jit_function(
        "fn strneq(a, b) { return a != b }\nprintln(strneq(\"hi\", \"hi\"))\nprintln(strneq(\"hi\", \"bye\"))",
    );
    assert_eq!(out, vec!["false", "true"]);
}

#[test]
fn jit_global_function_call() {
    // A hot function calling another function: both are compiled and the
    // call is a direct native call (see `jit_calls_between_compiled_functions`).
    let out = run_jit_function(
        "fn double(n) { return n * 2 }\nfn apply(x) { return double(x) }\nprintln(apply(21))",
    );
    assert_eq!(out, vec!["42"]);
}

#[test]
fn jit_global_read() {
    // Reading a non-self global is rejected; the function runs in the VM.
    let out = run_jit_function("let x = 10\nfn get_x() { return x }\nprintln(get_x())");
    assert_eq!(out, vec!["10"]);
}

#[test]
fn jit_auto_tiered_compilation() {
    // Verify that VM::new() (profiler disabled) auto-JIT compiles hot functions.
    // A function called 101 times should be JIT-compiled and produce correct results.
    let source = r#"
fn add(a, b) { return a + b }
let mut total = 0
let mut i = 0
while i < 101 {
    total = total + add(i, 1)
    i = i + 1
}
println(total)
"#;
    let mut lexer = crate::lexer::Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = crate::parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let chunk = crate::vm::compiler::compile(&program).unwrap();

    let mut vm = VM::new();
    vm.execute(&chunk).unwrap();

    // After 100 calls, add(Int, Int) is compiled and runs natively.
    assert_eq!(vm.jit.compiled_function_names(), vec!["add".to_string()]);
    assert!(vm.jit.native_runs("add") > 0);
    // sum of (i+1) for i in 0..101 = sum of 1..102 = 101*102/2 = 5151
    assert_eq!(vm.output, vec!["5151"]);
}

// ----- Performance benchmark -----

/// Time a single VM execution (JIT off).
fn time_vm(source: &str) -> (Vec<String>, std::time::Duration) {
    time_mode(source, JitMode::Off)
}

/// Time a single eager-JIT execution.
fn time_jit(source: &str) -> (Vec<String>, std::time::Duration) {
    time_mode(source, JitMode::Eager)
}

fn time_mode(source: &str, mode: JitMode) -> (Vec<String>, std::time::Duration) {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let chunk = compiler::compile(&program).unwrap();

    let start = std::time::Instant::now();
    let mut vm = VM::new();
    vm.set_jit_mode(mode);
    vm.execute(&chunk).unwrap();
    (vm.output.clone(), start.elapsed())
}

#[test]
#[ignore] // Manual: cargo test --features jit --release bench_jit_performance -- --nocapture --ignored
fn bench_jit_performance() {
    let benchmarks: Vec<(&str, &str)> = vec![
        (
            "fib(30)",
            "fn fib(n) { if n <= 1 { return n } return fib(n - 1) + fib(n - 2) }\nprintln(fib(30))",
        ),
        (
            "sum_to(1000000)",
            "fn sum_to(n) { let mut s = 0\nlet mut i = 1\nwhile i <= n { s = s + i\ni = i + 1 }\nreturn s }\nprintln(sum_to(1000000))",
        ),
        (
            "fib(35)",
            "fn fib(n) { if n <= 1 { return n } return fib(n - 1) + fib(n - 2) }\nprintln(fib(35))",
        ),
    ];

    eprintln!("\n{:-<70}", "");
    eprintln!("JIT Performance Benchmark (--release)");
    eprintln!("{:-<70}", "");
    eprintln!(
        "{:<25} {:>10} {:>10} {:>10}",
        "Benchmark", "VM (ms)", "JIT (ms)", "Speedup"
    );
    eprintln!("{:-<70}", "");

    for (name, source) in &benchmarks {
        let (vm_output, vm_time) = time_vm(source);
        let (jit_output, jit_time) = time_jit(source);

        assert_eq!(
            vm_output, jit_output,
            "Output mismatch for {}: VM={:?} JIT={:?}",
            name, vm_output, jit_output
        );

        let vm_ms = vm_time.as_secs_f64() * 1000.0;
        let jit_ms = jit_time.as_secs_f64() * 1000.0;
        let speedup = vm_ms / jit_ms;

        eprintln!(
            "{:<25} {:>10.2} {:>10.2} {:>9.1}x",
            name, vm_ms, jit_ms, speedup
        );
    }
    eprintln!("{:-<70}\n", "");
}

// ----- JIT soundness regressions -----
//
// Each test drives a function across the auto-tier threshold (100 calls) or
// through the eager tier and asserts the output/error is identical to the VM
// with the JIT off (`run_jit_function` / `run_parity`).

#[test]
fn jit_mixed_types_after_int_warmup() {
    // Previously: add("foo","bar") -> 593, add(1.5, 2) -> raw f64 bits.
    let out = run_jit_native(
        "fn add(a, b) { return a + b }\n\
         let mut i = 0\nwhile i < 150 { add(i, 1)\ni = i + 1 }\n\
         println(add(\"foo\", \"bar\"))\nprintln(add(1.5, 2))\nprintln(add(2, 3))",
        "add",
    );
    assert_eq!(out, vec!["foobar", "3.5", "5"]);
}

#[test]
fn jit_string_returning_function_crossing_threshold() {
    // Previously the 100th+ call returned an Int GcRef index ("593 Int").
    let out = run_jit_function(
        "fn h(n) { return \"x\" + \"y\" }\n\
         let mut j = 0\nwhile j < 102 { let r = h(j)\nif j > 98 { println(\"{r} {type(r)}\") }\nj = j + 1 }",
    );
    assert_eq!(out, vec!["xy String", "xy String", "xy String"]);
}

#[test]
fn jit_float_args_after_int_warmup() {
    let out = run_jit_native(
        "fn half(x) { return x / 2 }\n\
         let mut k = 0\nwhile k < 150 { half(k)\nk = k + 1 }\n\
         println(half(5))\nprintln(half(5.0))",
        "half",
    );
    assert_eq!(out, vec!["2", "2.5"]);
}

#[test]
fn jit_same_name_nested_functions_are_distinct() {
    // Previously both nested `f`s shared one cache entry keyed by name.
    let src = "fn outer_a(n) {\n fn f(x) { return x + 1 }\n let mut s = 0\n let mut i = 0\n\
               while i < n { s = s + f(i)\ni = i + 1 }\n return s\n}\n\
               fn outer_b(n) {\n fn f(x) { return x * 2 }\n let mut s = 0\n let mut i = 0\n\
               while i < n { s = s + f(i)\ni = i + 1 }\n return s\n}\n\
               println(outer_a(2000))\nprintln(outer_b(2000))\nprintln(outer_a(2000))";
    assert_eq!(
        run_jit_native(src, "f"),
        vec!["2001000", "3998000", "2001000"]
    );
    let (_, _, vm) = run_mode(src, JitMode::Auto);
    assert_eq!(
        vm.jit.compiled_function_names(),
        vec!["f".to_string(), "f".to_string()],
        "each nested `f` prototype gets its own specialization"
    );
}

#[test]
fn jit_wrong_arity_runs_in_vm() {
    let (out, err) = run_parity(
        "fn add(a, b) { return a + b }\n\
         let mut i = 0\nwhile i < 150 { add(i, 1)\ni = i + 1 }\n\
         println(add(1, 2))\n\
         try { add(1, 2, 3) } catch e { println(e.message) }\n\
         println(map([1], add))",
    );
    // Direct calls are arity-checked before the JIT is consulted
    // (crate::semantics::check_call_arity) ...
    assert_eq!(out, vec!["3", "fn add expects 2 arguments, got 3"]);
    // ... while a builtin calling back with fewer arguments fails the JIT
    // arity guard and runs in the VM: `1 + null`.
    assert!(err
        .unwrap_or_default()
        .contains("cannot perform arithmetic on null"));
}

#[test]
fn jit_bridge_errors_are_recorded_not_swallowed() {
    use crate::vm::jit::runtime::{encode_null, encode_value, rt_call_native, rt_get_global};
    let (_, err, mut vm) = run_mode("fn boom() {\n    emit 1\n}\n1", JitMode::Off);
    assert!(err.is_none());
    let boom = *vm.globals.get("boom").expect("boom is defined");
    let encoded = encode_value(&boom, &vm.gc);
    let vm_ptr: *mut VM = &mut vm;
    let result = rt_call_native(vm_ptr, encoded, std::ptr::null(), 0);
    assert_eq!(result, encode_null());
    let err = vm
        .take_jit_bridge_error()
        .expect("a failing bridge call must record its error");
    assert!(
        err.message.contains("yield/emit is not supported yet"),
        "{}",
        err.message
    );

    let name = vm.alloc_string("no_such_global");
    let name_ref = name.as_obj().expect("string is an object").0 as i64;
    let vm_ptr: *mut VM = &mut vm;
    assert_eq!(rt_get_global(vm_ptr, name_ref), encode_null());
    let err = vm
        .take_jit_bridge_error()
        .expect("missing global is an error");
    assert!(err.message.contains("undefined variable: 'no_such_global'"));
    assert!(vm.take_jit_bridge_error().is_none());
}

#[test]
fn jit_closures_and_upvalues_run_in_vm() {
    let out = run_jit_function(
        "fn make_adder(k) { return fn(x) { return x + k } }\n\
         let add5 = make_adder(5)\nlet mut t = 0\nlet mut z = 0\n\
         while z < 150 { t = t + add5(z)\nz = z + 1 }\nprintln(t)\n\
         fn make_counter() { let mut c = 0\nreturn fn() { c = c + 1\nreturn c } }\n\
         let next = make_counter()\nlet mut q = 0\nwhile q < 149 { next()\nq = q + 1 }\nprintln(next())",
    );
    assert_eq!(out, vec!["11925", "150"]);
    let proto = first_proto("fn make_adder(k) { return fn(x) { return x + k } }");
    let reject = verifier::verify(&proto, &TypeSig(vec![JitType::Int])).expect_err("reject");
    assert_eq!(reject.kind, RejectKind::UnsupportedOpcode(OpCode::Closure));
}

#[test]
fn jit_hot_function_calling_another_function() {
    // `apply` calls `double`: the call must target `double`'s own
    // specialization, never be turned into a self-call.
    let out = run_jit_native(
        "fn double(n) { return n * 2 }\nfn apply(x) { return double(x) + 1 }\n\
         let mut u = 0\nlet mut w = 0\nwhile w < 150 { u = u + apply(w)\nw = w + 1 }\nprintln(u)",
        "apply",
    );
    assert_eq!(out, vec!["22500"]);
    let (_, _, vm) = run_mode(
        "fn double(n) { return n * 2 }\nfn apply(x) { return double(x) + 1 }\nprintln(apply(4))",
        JitMode::Eager,
    );
    // `double` is compiled as `apply`'s callee and only ever called natively.
    assert_eq!(vm.jit.compiled_function_names(), vec!["apply", "double"]);
    assert_eq!(vm.jit.native_runs("double"), 0);
}

#[test]
fn jit_bool_results_and_params_keep_their_type() {
    let out = run_jit_native(
        "fn is_even(n) { return n % 2 == 0 }\nfn flip(b) { return !b }\n\
         let mut e = 0\nwhile e < 120 { is_even(e)\nflip(true)\ne = e + 1 }\n\
         println(is_even(4))\nprintln(is_even(7))\nprintln(flip(true))\nprintln(flip(0))\nprintln(type(is_even(2)))",
        "is_even",
    );
    assert_eq!(out, vec!["true", "false", "false", "true", "Bool"]);
}

#[test]
fn jit_int_overflow_deopts_to_vm_semantics() {
    // VM promotes overflowing Int arithmetic to Float; native code deopts.
    let out = run_jit_native(
        "fn fact(n) { if n <= 1 { return 1 } return n * fact(n - 1) }\n\
         let mut i = 0\nwhile i < 120 { fact(10)\ni = i + 1 }\n\
         println(fact(20))\nprintln(fact(25))\nprintln(fact(21))",
        "fact",
    );
    assert_eq!(
        out,
        vec![
            "2432902008176640000",
            "15511210043330986000000000",
            "51090942171709440000"
        ]
    );
}

#[test]
fn jit_large_int_results_are_boxed() {
    // Results beyond the NaN-box inline range must be re-boxed, not truncated.
    let out = run_jit_native(
        "fn big(n) { return n * 1000000000000 }\n\
         let mut k = 0\nwhile k < 120 { big(k)\nk = k + 1 }\n\
         println(big(1000))\nprintln(big(-1000))\nprintln(big(9223372))\nprintln(big(big(1)) / 1000000000000)",
        "big",
    );
    assert_eq!(
        out,
        vec![
            "1000000000000000",
            "-1000000000000000",
            "9223372000000000000",
            "1000000000000"
        ]
    );
}

#[test]
fn jit_division_by_zero_matches_vm_error() {
    let (out, err) = run_parity(
        "fn d(a, b) { return a / b }\nfn m(a, b) { return a % b }\n\
         let mut i = 0\nwhile i < 120 { d(10, 3)\nm(10, 3)\ni = i + 1 }\n\
         println(d(-7, 2))\nprintln(m(-7, 2))\nprintln(d(1, 0))",
    );
    assert_eq!(out, vec!["-3", "-1"]);
    assert!(err.unwrap_or_default().contains("division by zero"));
}

#[test]
fn jit_recursion_depth_matches_vm_stack_limit() {
    let (out, err) = run_parity(
        "fn count(n) { if n == 0 { return 0 } return 1 + count(n - 1) }\n\
         let mut c = 0\nwhile c < 120 { count(5)\nc = c + 1 }\n\
         println(count(250))\nprintln(count(300))\nprintln(count(20000))",
    );
    // Native JIT recursion deopts back to the VM before the shared
    // recursion limit (runtime/recursion.rs, default 10000), so every mode
    // reports the same catchable error past it.
    assert_eq!(out, vec!["250", "300"]);
    assert!(err
        .unwrap_or_default()
        .contains("maximum recursion depth exceeded"));
}

#[test]
fn jit_repeated_deopts_disable_the_specialization() {
    use crate::vm::jit::tier::SpecState;
    let src = "fn sq(n) { return n * n }\n\
               let mut i = 0\nwhile i < 120 { sq(i)\ni = i + 1 }\n\
               let mut j = 0\nwhile j < 20 { println(sq(4000000000))\nj = j + 1 }";
    let (out, err) = run_parity(src);
    assert!(err.is_none());
    assert_eq!(out.len(), 20);
    let (_, _, vm) = run_mode(src, JitMode::Auto);
    let states = vm.jit.debug_states();
    let (_, specs, _) = states.iter().find(|s| s.0 == "sq").expect("sq tracked");
    assert_eq!(specs.len(), 1);
    assert!(
        matches!(specs[0], SpecState::Disabled),
        "sq(Int) must be disabled after repeated deopts"
    );
}

#[test]
fn jit_guard_failures_disable_function() {
    // String args fail the type guard before any compile attempt; after
    // enough guard failures the function is no longer considered at all.
    let src = "fn add(a, b) { return a + b }\n\
               let mut i = 0\nwhile i < 400 { add(\"x\", \"y\")\ni = i + 1 }";
    let (_, err, vm) = run_mode(src, JitMode::Auto);
    assert!(err.is_none());
    let states = vm.jit.debug_states();
    let (_, specs, disabled) = states.iter().find(|s| s.0 == "add").expect("add tracked");
    assert!(specs.is_empty());
    assert!(*disabled);
}

#[test]
fn jit_verifier_rejects_non_self_calls_and_accepts_fib() {
    // A call through another global must never be compiled as a self-call.
    let proto = first_proto("fn apply(x) { return other(x) }");
    let reject = verifier::verify(&proto, &TypeSig(vec![JitType::Int])).expect_err("reject");
    assert!(
        matches!(
            reject.kind,
            RejectKind::NonSelfGlobal(_) | RejectKind::UnsupportedOpcode(OpCode::GetUpvalue)
        ),
        "{:?}",
        reject
    );
    let fib = first_proto("fn fib(n) { if n <= 1 { return n } return fib(n - 1) + fib(n - 2) }");
    let vf = verifier::verify(&fib, &TypeSig(vec![JitType::Int])).expect("fib verifies");
    assert!(vf.has_self_calls);
    assert_eq!(vf.ret, JitType::Int);
    let mut jit = JitCompiler::new().unwrap();
    assert!(jit
        .compile(&fib, &vf, &crate::vm::jit::ir_builder::CalleeBodies::new())
        .is_ok());
}

#[test]
fn jit_eager_mode_runs_fib_natively() {
    let out = run_jit_native(
        "fn fib(n) { if n <= 1 { return n } return fib(n - 1) + fib(n - 2) }\nprintln(fib(25))",
        "fib",
    );
    assert_eq!(out, vec!["75025"]);
}

#[test]
fn vm_error_display_collapses_repeated_frames() {
    use crate::vm::machine::{StackFrame, VMError};
    let frame = |name: &str, line| StackFrame {
        function: name.to_string(),
        line,
        col: 5,
    };
    let mut err = VMError::new("maximum recursion depth exceeded");
    err.stack_trace = std::iter::repeat_with(|| frame("d", 1))
        .take(10_000)
        .chain(std::iter::once(frame("<main>", 3)))
        .collect();
    let rendered = err.to_string();
    assert_eq!(rendered.matches("at d (line 1, col 5)").count(), 3);
    assert!(rendered.contains("previous frame repeated 9997 more times"));
    assert!(rendered.ends_with("at <main> (line 3, col 5)"));
    assert!(rendered.lines().count() < 10);
}

// ---------------------------------------------------------------------------
// Loop tier-up: a hot loop in a function called once restarts the call in
// native code (`VM::try_jit_loop_restart`).
// ---------------------------------------------------------------------------

#[test]
fn hot_loop_in_function_called_once_runs_natively_under_auto() {
    let source = r#"
fn count(n) {
    let mut i = 0
    let mut total = 0
    while i < n {
        total = total + i
        i = i + 1
    }
    return total
}
say count(50000)
"#;
    assert_eq!(run_jit_function(source), vec!["1249975000"]);
    let (_, _, vm) = run_mode(source, JitMode::Auto);
    assert_eq!(
        vm.jit.native_runs("count"),
        1,
        "a single call with a hot loop must tier up; compiled: {:?}",
        vm.jit.compiled_function_names()
    );
}

#[test]
fn loop_restart_uses_entry_arguments_after_parameters_change() {
    // The body overwrites its parameter before the loop gets hot; the native
    // restart must start from the arguments the call was entered with.
    let source = r#"
fn countdown(n) {
    let mut steps = 0
    while n > 0 {
        n = n - 1
        steps = steps + 1
    }
    return steps
}
say countdown(20000)
"#;
    assert_eq!(run_jit_function(source), vec!["20000"]);
    let (_, _, vm) = run_mode(source, JitMode::Auto);
    assert_eq!(vm.jit.native_runs("countdown"), 1);
}

#[test]
fn loop_restart_deopt_continues_in_the_vm() {
    // Overflow promotes to Float in the VM; native code deopts, and the
    // frame must carry on in the VM with its state intact.
    let source = r#"
fn grow(n) {
    let mut x = 1
    let mut i = 0
    while i < n {
        x = x * 3
        i = i + 1
    }
    return x
}
say grow(3000)
"#;
    let out = run_jit_function(source);
    assert_eq!(out.len(), 1);
}

#[test]
fn short_loops_do_not_tier_up() {
    let source = r#"
fn small(n) {
    let mut i = 0
    while i < n {
        i = i + 1
    }
    return i
}
say small(10)
"#;
    assert_eq!(run_jit_function(source), vec!["10"]);
    let (_, _, vm) = run_mode(source, JitMode::Auto);
    assert_eq!(vm.jit.native_runs("small"), 0);
}

#[test]
fn hot_loop_with_side_effects_is_not_restarted() {
    // `say` makes the function impure: the verifier rejects it, so the loop
    // keeps running in the VM and every line is printed exactly once.
    let source = r#"
fn noisy(n) {
    let mut i = 0
    while i < n {
        if i % 500 == 0 {
            say i
        }
        i = i + 1
    }
    return i
}
say noisy(2000)
"#;
    assert_eq!(
        run_jit_function(source),
        vec!["0", "500", "1000", "1500", "2000"]
    );
}

#[test]
fn hot_loop_inside_timeout_is_not_restarted() {
    let source = r#"
fn count(n) {
    let mut i = 0
    while i < n {
        i = i + 1
    }
    return i
}
let mut r = 0
timeout 5 seconds {
    r = count(5000)
}
say r
"#;
    assert_eq!(run_jit_function(source), vec!["5000"]);
    let (_, _, vm) = run_mode(source, JitMode::Auto);
    assert_eq!(vm.jit.native_runs("count"), 0);
}

// ---------------------------------------------------------------------------
// Float tier: IEEE semantics, Int/Float promotion and pure math builtins
// must match the VM bit for bit (output and errors in every mode).
// ---------------------------------------------------------------------------

#[test]
fn jit_float_arithmetic_matches_vm_including_nan_inf_and_signed_zero() {
    let source = r#"
fn ops(x, y) {
    let s = x + y
    let d = x - y
    let p = x * y
    let q = x / y
    let r = x % y
    return s + d * 2.0 - p + q * r
}
fn show(x, y) {
    say "{x + y} {x - y} {x * y} {x / y} {x % y} {-x}"
    say "{x < y} {x <= y} {x > y} {x >= y} {x == y} {x != y} {!x} {x && y} {x || y}"
}
let nan = 0.0 / 0.0
let inf = 1.0 / 0.0
let vals = [1.5, -2.25, 0.0, -0.0, nan, inf, -inf, 7.0, 1000000000000000000000000000000.0]
for x in vals {
    for y in vals {
        show(x, y)
        say ops(x, y)
    }
}
"#;
    let out = run_jit_native(source, "ops");
    assert_eq!(out.len(), 9 * 9 * 3);
    let (_, _, vm) = run_mode(source, JitMode::Eager);
    assert_eq!(vm.jit.native_runs("show"), 0, "`say` is not pure");
}

#[test]
fn jit_float_predicates_match_vm() {
    let source = r#"
fn lt(x, y) { return x < y }
fn eq(x, y) { return x == y }
fn ne(x, y) { return x != y }
fn truthy(x) { if x { return 1 } return 0 }
fn neg(x) { return !x }
let nan = 0.0 / 0.0
let vals = [1.5, 0.0, -0.0, nan, 2, 0, true, false]
for x in vals {
    for y in vals {
        say "{eq(x, y)} {ne(x, y)}"
    }
    say "{truthy(x)} {neg(x)}"
}
for x in [1.5, 0.0, -0.0, nan, 2, -3] {
    for y in [1.5, 0.0, nan, 2] { say lt(x, y) }
}
"#;
    for f in ["lt", "eq", "ne", "truthy", "neg"] {
        run_jit_native(source, f);
    }
}

#[test]
fn jit_mixed_int_float_promotion_matches_vm() {
    let source = r#"
fn mix(i, x) { return i * x + i / 2 - x / i + i % 3 + x % 2 }
fn cmp(i, x) { return i < x && i != x || i == x }
let xs = [0.5, -3.0, 2.0, 1000000000000000000.0, 0.0]
for i in [1, -7, 3, 1000000007, 9007199254740993] {
    for x in xs {
        say "{mix(i, x)} {cmp(i, x)} {mix(i, i)} {cmp(i, i)}"
    }
}
"#;
    run_jit_native(source, "mix");
    run_jit_native(source, "cmp");
}

#[test]
fn jit_int_overflow_in_mixed_function_still_deopts() {
    let source = r#"
fn f(n, x) { return n * n + x }
let mut k = 0
while k < 150 { f(k, 0.5)
k = k + 1 }
say f(3, 0.5)
say f(4000000000, 0.5)
say f(-4000000000, 1.0)
"#;
    let out = run_jit_native(source, "f");
    assert_eq!(out[0], "9.5");
}

#[test]
fn jit_float_return_and_loops_run_natively() {
    let source = r#"
fn harmonic(n) {
    let mut s = 0.0
    let mut i = 1
    while i <= n {
        s = s + 1.0 / i
        i = i + 1
    }
    return s
}
say harmonic(10)
say harmonic(100000)
say harmonic(0)
"#;
    run_jit_native(source, "harmonic");
    let (_, _, vm) = run_mode(source, JitMode::Auto);
    assert!(
        vm.jit.native_runs("harmonic") >= 1,
        "loop tier-up with a Float result"
    );
}

#[test]
fn jit_math_builtins_match_vm() {
    let source = r#"
fn m1(x) {
    return math.sqrt(x) + math.sin(x) + math.cos(x) + math.tan(x) + math.abs(x) + math.log(x)
}
fn r(x) { return math.floor(x) + math.ceil(x) + math.round(x) + math.abs(x) }
fn p(x, y) { return math.pow(x, y) }
fn mm(x, y) { return math.min(x, y) * 10 + math.max(x, y) }
fn cl(v, lo, hi) { return math.clamp(v, lo, hi) }
fn conv(x) { return float(x) + int(x) }
fn consts(x) { return x * math.pi + math.e - math.inf }
let nan = 0.0 / 0.0
let inf = 1.0 / 0.0
for x in [2.5, -2.5, 0.5, -0.5, 1.5, 0.0, -0.0, 10000000000000000000.0, -10000000000000000000.0, 9300000000000000000.0, nan, inf, -inf, 4, -9] {
    say "{m1(x)} {r(x)} {conv(x)} {consts(x)}"
    for y in [2, -1, 0.5, 63, 64, nan, 3] {
        say "{p(x, y)} {mm(x, y)} {cl(x, y, 3)} {cl(x, -1.5, y)}"
    }
}
for i in [3, -3, 9223372036854775807, -9223372036854775807] {
    say "{r(i)} {p(i, 2)} {p(2, i)} {mm(i, 1)} {cl(i, 0, 10)} {conv(i)}"
}
"#;
    for f in ["m1", "r", "p", "mm", "cl", "conv", "consts"] {
        run_jit_native(source, f);
    }
}

#[test]
fn jit_math_int_edge_cases_deopt_to_vm_results() {
    // |i64::MIN|, floor of a huge float and Int pow overflow are Floats in
    // the VM; native code deopts and the VM's answer is used.
    let (out, err) = run_parity(
        r#"
fn a(x) { return math.abs(x) }
fn f(x) { return math.floor(x) }
fn p(x, y) { return math.pow(x, y) }
let mut k = 0
while k < 150 { a(k)
f(1.5)
p(2, 3)
k = k + 1 }
let min = -9223372036854775807 - 1
say a(min)
say f(1000000000000000000000000000000.0)
say f(0.0 / 0.0)
say p(10, 30)
say p(2, -2)
say p(3, 4)
"#,
    );
    assert!(err.is_none(), "{:?}", err);
    assert_eq!(out[5], "81");
}

#[test]
fn jit_builtin_errors_match_vm() {
    // Arguments the JIT does not accept (strings, bools) run in the VM and
    // raise the VM's errors.
    let (_, err) = run_parity(
        r#"
fn s(x) { return math.sqrt(x) }
fn fl(x) { return float(x) }
let mut k = 0
while k < 150 { s(k)
fl(k)
k = k + 1 }
say fl("2.5")
try { fl(true) } catch e { say e.message }
say s("x")
"#,
    );
    assert!(err
        .unwrap_or_default()
        .contains("math.sqrt() requires a number"));
}

#[test]
fn jit_shadowed_math_runs_in_the_vm() {
    // Inside `s`, `math` is a parameter, not the module: never compiled as
    // the builtin.
    let (out, err) = run_parity(
        r#"
fn s(math, x) { return math.sqrt(x) }
let fake = { sqrt: fn(x) { return x + 1 } }
let mut k = 0
while k < 150 { s(fake, 1.0)
k = k + 1 }
say s(fake, 16.0)
"#,
    );
    assert!(err.is_none(), "{:?}", err);
    assert_eq!(out, vec!["17"]);
}

// ---------------------------------------------------------------------------
// Calls between compiled functions.
// ---------------------------------------------------------------------------

#[test]
fn jit_calls_between_compiled_functions() {
    let source = r#"
fn sq(x) { return x * x }
fn norm(x, y) { return math.sqrt(sq(x) + sq(y)) }
fn dist_sum(n) {
    let mut s = 0.0
    let mut i = 0
    while i < n {
        s = s + norm(i * 0.5, 3.0)
        i = i + 1
    }
    return s
}
say norm(3.0, 4.0)
say norm(3, 4)
say dist_sum(5000)
"#;
    let out = run_jit_native(source, "norm");
    assert_eq!(out[0], "5");
    let (_, _, vm) = run_mode(source, JitMode::Auto);
    let compiled = vm.jit.compiled_function_names();
    for f in ["dist_sum", "norm", "sq"] {
        assert!(
            compiled.contains(&f.to_string()),
            "{} not compiled: {:?}",
            f,
            compiled
        );
    }
    // `dist_sum` tiers up once through its loop; its calls of `norm` (and
    // `norm`'s of `sq`) are then native calls, not VM entries.
    assert_eq!(vm.jit.native_runs("dist_sum"), 1);
}

#[test]
fn jit_callee_deopt_reruns_the_whole_call_in_the_vm() {
    let (out, err) = run_parity(
        r#"
fn sq(x) { return x * x }
fn sum_sq(a, b) { return sq(a) + sq(b) }
let mut k = 0
while k < 150 { sum_sq(k, 1)
k = k + 1 }
say sum_sq(3, 4)
say sum_sq(4000000000, 5000000000)
say sum_sq(3037000499, 1)
"#,
    );
    assert!(err.is_none(), "{:?}", err);
    assert_eq!(out[0], "25");
}

#[test]
fn jit_callee_rebinding_fails_the_guard() {
    let (out, err) = run_parity(
        r#"
let mut g = fn(x) { return x * 2 }
fn h(x) { return g(x) + 1 }
let mut k = 0
while k < 150 { h(k)
k = k + 1 }
say h(10)
g = fn(x) { return x * 3 }
say h(10)
"#,
    );
    assert!(err.is_none(), "{:?}", err);
    assert_eq!(out, vec!["21", "31"]);
}

#[test]
fn jit_mutual_recursion_and_impure_callees_run_in_the_vm() {
    let (out, err) = run_parity(
        r#"
fn is_even(n) { if n == 0 { return true } return is_odd(n - 1) }
fn is_odd(n) { if n == 0 { return false } return is_even(n - 1) }
fn loud(x) { say x
return x }
fn calls_loud(x) { return loud(x) + 1 }
let mut k = 0
while k < 120 { is_even(k)
calls_loud(k)
k = k + 1 }
say is_even(101)
say calls_loud(5)
"#,
    );
    assert!(err.is_none(), "{:?}", err);
    assert_eq!(out[out.len() - 3..], ["false", "5", "6"]);
}

#[test]
fn jit_callee_recursion_depth_matches_vm() {
    let (out, err) = run_parity(
        r#"
fn down(n) { if n == 0 { return 0 } return 1 + down(n - 1) }
fn outer(n) { return down(n) + 1 }
let mut k = 0
while k < 120 { outer(5)
k = k + 1 }
say outer(250)
say outer(20000)
"#,
    );
    assert_eq!(out, vec!["251"]);
    assert!(err
        .unwrap_or_default()
        .contains("maximum recursion depth exceeded"));
}

// ---------------------------------------------------------------------------
// Counting `for` loops (`for i in range(..)`, `repeat n times`).
// ---------------------------------------------------------------------------

#[test]
fn range_loops_match_vm_semantics_and_run_natively() {
    let source = r#"
fn sum_range(n) {
    let mut s = 0
    for i in range(n) { s = s + i }
    return s
}
fn sum_between(a, b) {
    let mut s = 0
    for i in range(a, b) {
        if i % 7 == 3 { continue }
        if i > 100000 { break }
        s = s + i * 2
    }
    return s
}
fn reps(n) {
    let mut c = 0.0
    repeat n times { c = c + 0.5 }
    return c
}
say sum_range(10)
say sum_range(0)
say sum_range(-5)
say sum_range(100000)
say sum_between(-50, 50)
say sum_between(10, 3)
say sum_between(0, 1000000)
say reps(3)
say reps(20000)
"#;
    let out = run_jit_native(source, "sum_range");
    assert_eq!(out[..3], ["45", "0", "0"]);
    run_jit_native(source, "sum_between");
    run_jit_native(source, "reps");
}

#[test]
fn range_loop_with_user_defined_range_uses_it() {
    // A user `range` replaces the builtin: the generic path calls it.
    let out = run_jit_function(
        r#"
fn range(n) { return [n, n * 10] }
fn total(n) {
    let mut s = 0
    for i in range(n) { s = s + i }
    return s
}
say total(4)
let mut k = 0
while k < 150 { total(k)
k = k + 1 }
say total(5)
repeat 2 times { say "r" }
"#,
    );
    assert_eq!(out, vec!["44", "55", "r", "r"]);
}

#[test]
fn range_loop_errors_and_odd_arguments_match_vm() {
    let (out, err) = run_parity(
        r#"
for i in range(2, 5, 9) { say i }
let n = 3
let mut fs = []
for i in range(n) {
    fs.push(fn() { return i * 10 })
}
for f in fs { say f() }
for i in range(9223372036854775806, 9223372036854775807) { say i }
let big = 9223372036854775807
let mut c = 0
for i in range(big - 3, big) { c = c + 1 }
say c
for i in range(1.5) { say i }
"#,
    );
    assert_eq!(
        out,
        vec!["2", "3", "4", "0", "10", "20", "9223372036854775806", "3"]
    );
    assert!(err
        .unwrap_or_default()
        .contains("range() requires integer arguments"));
}

#[test]
fn range_loops_do_not_materialize_the_range() {
    // On the old code path each loop allocated the whole range as an array.
    let out = run_jit_function(
        r#"
let mut s = 0
for i in range(3000000) { s = s + 1 }
repeat 3000000 times { s = s + 1 }
say s
"#,
    );
    assert_eq!(out, vec!["6000000"]);
}

#[test]
fn jit_captured_callees_are_guarded_per_closure() {
    // Both closures share one prototype (and specialization); each call is
    // guarded on the function *that* closure captured.
    let source = r#"
fn twice(x) { return x * 2 }
fn thrice(x) { return x * 3 }
fn make(g) { return fn(x) { return g(x) + 1 } }
let a = make(twice)
let b = make(thrice)
let mut k = 0
while k < 150 { a(k)
k = k + 1 }
say a(10)
say b(10)
let mut j = 0
while j < 150 { b(j)
j = j + 1 }
say a(10)
say b(10)
"#;
    assert_eq!(run_jit_function(source), vec!["21", "31", "21", "31"]);
    let (_, _, vm) = run_mode(source, JitMode::Auto);
    assert!(vm.jit.native_runs("<lambda>") > 0);
}
