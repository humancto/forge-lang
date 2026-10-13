//! `TypeCheck` (vm/type_check.rs) must be invisible: for every value and
//! annotation type, `__types.check` / `__types.widen` with a literal type
//! (compiled with the fast path) give exactly what the plain call gives
//! (a type held in a variable is never compiled with the fast path).

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::vm::bytecode::{decode_op, Chunk, OpCode};
use crate::vm::compiler;
use crate::vm::machine::VM;

fn compile(source: &str) -> Chunk {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer error");
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().expect("parse error");
    compiler::compile(&program).expect("compile error")
}

fn run_output(source: &str) -> Vec<String> {
    let chunk = compile(source);
    let mut vm = VM::new();
    vm.execute(&chunk).expect("vm error");
    vm.output.clone()
}

fn uses_type_check(chunk: &Chunk) -> bool {
    chunk
        .code
        .iter()
        .any(|&i| decode_op(i) == OpCode::TypeCheck as u8)
        || chunk.prototypes.iter().any(uses_type_check)
}

const VALUES: &[&str] = &[
    "1",
    "-7",
    "9007199254740993",
    "2.5",
    "1.0",
    "true",
    "\"s\"",
    "null",
    "[]",
    "[1, 2]",
    "[1, 2.5]",
    "[2.5, 3.5]",
    "[\"a\", \"b\"]",
    "[true]",
    "[null]",
    "[[1]]",
    "{ a: 1 }",
    "{}",
    "P { x: 1 }",
    "Q { y: 1 }",
    "Circle(2)",
    "Some(1)",
    "Some(2.5)",
    "None",
    "Ok(1)",
    "Err(\"e\")",
    "(1, 2)",
    "fn(x) { return x }",
    "freeze(1)",
    "freeze([1])",
];

const TYPES: &[&str] = &[
    "Int",
    "Float",
    "String",
    "Bool",
    "Null",
    "Object",
    "Named:P",
    "Named:Shape",
    "Named:Option",
    "?Int",
    "?Float",
    "?String",
    "?Bool",
    "?Object",
    "?Named:P",
    "[Int]",
    "[Float]",
    "[String]",
    "[Bool]",
    "[Any]",
    "?[Int]",
    "?[Float]",
    "[[Int]]",
    "Fn",
    "Union<Int,Float>",
];

/// A description that distinguishes Int from Float, also inside arrays.
const PRELUDE: &str = r#"
struct P { x: Int }
struct Q { y: Int }
type Shape = Circle(Int) | Dot
fn describe(x) {
    if typeof(x) == "Array" {
        return "[" + join(map(x, fn(e) { return describe(e) }), ",") + "]"
    }
    return typeof(x) + ":" + str(x)
}
"#;

#[test]
fn literal_type_checks_use_the_fast_path() {
    let chunk = compile("fn f(v) { return __types.check(v, \"Int\", \"c\") }");
    assert!(uses_type_check(&chunk));
    let chunk = compile("fn f(v) { return __types.widen(v, \"?Float\") }");
    assert!(uses_type_check(&chunk));
    // A type in a variable, or a type without a fast path, is a plain call.
    let chunk = compile("fn f(v, t) { return __types.check(v, t, \"c\") }");
    assert!(!uses_type_check(&chunk));
    let chunk = compile("fn f(v) { return __types.check(v, \"Map<String,Int>\", \"c\") }");
    assert!(!uses_type_check(&chunk));
    // A local named `__types` is not the module.
    let chunk = compile(
        "fn f(v) { let __types = { check: fn(a, b, c) { return a } }\n return __types.check(v, \"Int\", \"c\") }",
    );
    assert!(!uses_type_check(&chunk));
}

#[test]
fn fast_path_agrees_with_the_call() {
    for value in VALUES {
        let mut src = String::from(PRELUDE);
        src.push_str(&format!("let v = {}\n", value));
        for (i, ty) in TYPES.iter().enumerate() {
            for member in ["check", "widen"] {
                let extra = if member == "check" { ", \"ctx\"" } else { "" };
                // Fast path (literal type) and plain call (type in a
                // variable), each caught.
                src.push_str(&format!(
                    "let mut fast_{m}{i} = \"\"\ntry {{ fast_{m}{i} = describe(__types.{m}(v, \"{t}\"{x})) }} catch e {{ fast_{m}{i} = \"error \" + e.message }}\n\
                     let t_{m}{i} = \"{t}\"\nlet mut slow_{m}{i} = \"\"\ntry {{ slow_{m}{i} = describe(__types.{m}(v, t_{m}{i}{x})) }} catch e {{ slow_{m}{i} = \"error \" + e.message }}\n\
                     println(\"{m} {t}: \" + fast_{m}{i} + \" <=> \" + slow_{m}{i})\n",
                    m = member,
                    t = ty,
                    i = i,
                    x = extra
                ));
            }
        }
        for line in run_output(&src) {
            let (_, results) = line.split_once(": ").expect("BUG: test line");
            let (fast, slow) = results.split_once(" <=> ").expect("BUG: test line");
            assert_eq!(fast, slow, "value {}: {}", value, line);
        }
    }
}
