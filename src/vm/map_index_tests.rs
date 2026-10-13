//! Maps on the VM: hash-indexed entries (`VmMap`), `m = m.set(k, v)` /
//! `m = m.remove(k)` updating an owned map in place (`UpdateMapLocal`) and
//! `m.get` / `m.has` / `m.len` reading a local without sharing it
//! (`PeekLocal` + `CallPure`). Every fast path must be unobservable.

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::vm::bytecode::{decode_op, encode_abc, Chunk, OpCode};
use crate::vm::compiler;
use crate::vm::machine::VM;

fn parse_program(source: &str) -> crate::parser::ast::Program {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer error");
    let mut parser = Parser::new(tokens);
    parser.parse_program().expect("parse error")
}

fn run_output(source: &str, stress: bool) -> Vec<String> {
    let chunk = compiler::compile(&parse_program(source)).expect("compile error");
    let mut vm = VM::new();
    vm.gc.set_stress(stress);
    vm.execute(&chunk).expect("vm error");
    vm.output.clone()
}

/// Runs `body` inside a function and at the top level, with and without GC
/// stress.
fn assert_output(body: &str, expected: &[&str]) {
    let in_fn = format!("fn main_test() {{\n{body}\n}}\nmain_test()\n");
    for source in [in_fn.as_str(), body] {
        for stress in [false, true] {
            assert_eq!(
                run_output(source, stress),
                expected,
                "stress={stress}\n{source}"
            );
        }
    }
}

fn uses_op(source: &str, op: OpCode) -> bool {
    fn walk(chunk: &Chunk, op: OpCode) -> bool {
        chunk.code.iter().any(|&i| decode_op(i) == op as u8)
            || chunk.prototypes.iter().any(|p| walk(p, op))
    }
    walk(
        &compiler::compile(&parse_program(source)).expect("compile error"),
        op,
    )
}

/// Objects allocated while running `source`.
fn allocations(source: &str) -> usize {
    let chunk = compiler::compile(&parse_program(source)).expect("compile error");
    let mut vm = VM::new();
    let before = vm.gc.alloc_count;
    vm.execute(&chunk).expect("vm error");
    vm.gc.alloc_count - before
}

/// Extra allocations of `body` (inside a function) with `n = 1500` over
/// `n = 100`: about 1400 or more when every step copies the map.
fn extra_allocations(body: &str) -> usize {
    let program = |n: usize| format!("fn f(n) {{\n{body}\n}}\nf({n})\n");
    allocations(&program(1500)).saturating_sub(allocations(&program(100)))
}

#[test]
fn map_updates_compile_to_update_map_local() {
    let body = "fn f() {\n let mut m = map()\n m = m.set(1, m.len())\n m = m.remove(1)\n return m.get(2)\n}";
    assert!(uses_op(body, OpCode::UpdateMapLocal));
    assert!(uses_op(body, OpCode::CallPure));
    assert!(uses_op(
        "let mut m = map()\nm = m.set(\"k\", m.get(\"j\") + 1)",
        OpCode::UpdateMapLocal
    ));
    // A different target, an immutable binding, or an argument that may
    // update a local in place keep the generic lowering.
    assert!(!uses_op(
        "let mut m = map()\nlet mut n = map()\nn = m.set(1, 2)",
        OpCode::UpdateMapLocal
    ));
    assert!(!uses_op(
        "let m = map()\nlet x = m.set(1, 2)",
        OpCode::UpdateMapLocal
    ));
    assert!(!uses_op(
        "let mut m = map()\nlet mut xs = [1]\nm = m.set(1, xs.pop())",
        OpCode::UpdateMapLocal
    ));
}

#[test]
fn map_updates_in_place_are_invisible_to_aliases() {
    assert_output(
        r#"
        let mut m = map()
        m = m.set("a", 1)
        m = m.set("b", 2)
        let a = m
        m = m.set("c", 3)
        m = m.remove("a")
        let holder = [m]
        m = m.set("b", 20)
        m = m.remove("c")
        let f = fn() { return m.len() }
        m = m.set("d", 4)
        let b = m.set("e", 5)
        m = m.set("a", 0)
        say a
        say holder[0]
        say b
        say m
        say f()
        "#,
        &[
            "Map(a => 1, b => 2)",
            "Map(b => 2, c => 3)",
            "Map(b => 20, d => 4, e => 5)",
            "Map(b => 20, d => 4, a => 0)",
            "3",
        ],
    );
}

#[test]
fn map_values_stored_in_place_keep_value_semantics() {
    assert_output(
        r#"
        let mut xs = [1]
        let mut m = map()
        m = m.set("xs", xs)
        xs.push(2)
        let mut k = [9]
        m = m.set(k, "list key")
        k.push(10)
        let got = m.get("xs")
        say got
        say m.get([9])
        say m.has([9, 10])
        say m
        "#,
        &[
            "[1]",
            "list key",
            "false",
            "Map(xs => [1], [9] => list key)",
        ],
    );
}

#[test]
fn map_set_reads_its_receiver_before_the_arguments() {
    // `set` reads the map, then evaluates its arguments; `remove` (a
    // mutating method name, lowered through `__forge_method_mut`) reads it
    // last. A value that reassigns the map must see the same order as the
    // generic lowering.
    assert_output(
        r#"
        let mut m = map([("x", 0)])
        let f = fn() {
            m = map([("z", 9)])
            return 1
        }
        m = m.set("a", f())
        say m
        let g = fn() {
            m = map([("q", 1), ("r", 2)])
            return "q"
        }
        m = m.remove(g())
        say m
        "#,
        &["Map(x => 0, a => 1)", "Map(r => 2)"],
    );
}

#[test]
fn map_keys_follow_set_equality_in_place() {
    assert_output(
        r#"
        let big = 9007199254740992
        let mut m = map()
        m = m.set(big + 1, "b1")
        m = m.set(big, "b0")
        say m.get(9007199254740992.0)
        m = m.set(9007199254740992.0, "f")
        say m
        m = m.remove(9007199254740992.0)
        say m
        let nan = math.sqrt(-1.0)
        m = m.set(nan, 1)
        m = m.set(nan, 2)
        m = m.set(1, "int")
        m = m.set(1.0, "float")
        m = m.set(-0.0, "zero")
        say m.get(0)
        say m
        "#,
        &[
            "b1",
            "Map(9007199254740993 => f, 9007199254740992 => b0)",
            "Map()",
            "zero",
            "Map(NaN => 2, 1 => float, -0 => zero)",
        ],
    );
}

#[test]
fn map_removal_then_reinsert_appends_and_iteration_skips_removed() {
    assert_output(
        r#"
        let mut m = map()
        for i in range(30) { m = m.set(i, i * i) }
        for i in range(30) {
            if i % 3 != 0 { m = m.remove(i) }
        }
        m = m.set(1, "again")
        let mut keys = []
        for k, v in m { keys.push(k) }
        say keys
        say m.len()
        say m.get(27)
        say m.to_array()[10]
        "#,
        &[
            "[0, 3, 6, 9, 12, 15, 18, 21, 24, 27, 1]",
            "11",
            "729",
            "(1, again)",
        ],
    );
}

#[test]
fn map_updates_on_other_receivers_keep_their_errors() {
    for (source, message) in [
        (
            "let mut m = freeze(map([(1, 2)]))\nm = m.set(1, 3)",
            "cannot mutate a frozen map",
        ),
        (
            "let mut m = freeze(map([(1, 2)]))\nm = m.remove(1)",
            "cannot remove a frozen value",
        ),
        (
            "let mut m = [1, 2]\nm = m.set(1, 3)",
            "no method 'set' on Array",
        ),
    ] {
        let chunk = compiler::compile(&parse_program(source)).expect("compile error");
        let mut vm = VM::new();
        let err = vm.execute(&chunk).expect_err("must fail");
        assert!(err.message.contains(message), "{}: {}", source, err.message);
    }
    assert_output(
        "let mut s = set([1, 2])\ns = s.remove(1)\nsay s\nlet mut o = { get: fn(k) { return k * 2 } }\nsay o.get(21)",
        &["set(2)", "42"],
    );
}

#[test]
fn building_and_reading_a_map_does_not_copy_it() {
    let build = r#"
        let mut m = map()
        for i in range(0, n) {
            m = m.set(i % 50, i)
            if m.has(i) { m = m.set(i, m.get(i) + 1) } else { m = m.set(i, 1) }
            m = m.set(i, m.get(i) + m.len())
        }
        for i in range(0, n) {
            if i % 2 == 0 { m = m.remove(i) }
        }
        return m.len()
    "#;
    let extra = extra_allocations(build);
    assert!(extra <= 200, "{extra} extra allocations for:\n{build}");
}

#[test]
fn update_map_local_with_an_unknown_operation_is_rejected() {
    let mut chunk = Chunk::new("bad");
    chunk.max_registers = 4;
    chunk.emit_at(encode_abc(OpCode::UpdateMapLocal, 0, 1, 7), 1, 0);
    chunk.emit_at(encode_abc(OpCode::ReturnNull, 0, 0, 0), 1, 0);
    let err = crate::vm::verify::verify_chunk(&chunk).expect_err("must be rejected");
    assert!(err.to_string().contains("unknown operation"), "{}", err);
    // A set reads three operand registers.
    let mut chunk = Chunk::new("short");
    chunk.max_registers = 3;
    chunk.emit_at(
        encode_abc(
            OpCode::UpdateMapLocal,
            0,
            1,
            crate::vm::bytecode::MAP_UPDATE_SET,
        ),
        1,
        0,
    );
    chunk.emit_at(encode_abc(OpCode::ReturnNull, 0, 0, 0), 1, 0);
    assert!(crate::vm::verify::verify_chunk(&chunk).is_err());
}

/// Maps built from values that compare equal agree with the linear
/// definition: `get` finds the oldest equal key, `set` replaces its value.
#[test]
fn map_lookups_agree_with_a_linear_scan() {
    use crate::vm::value::{ObjKind, Value, VmMap};
    let corpus = "[1, 1.0, 0, -0.0, math.sqrt(-1.0), \"a\", [1, 2], [1.0, 2], {a: 1, b: 2}, {b: 2.0, a: 1}, set([1, 2]), set([2.0, 1]), Some(1), Some(1.0), None, null, true, 9007199254740992, 9007199254740993, 9007199254740992.0, map([[1, 2]]), map([[1.0, 2]])]";
    let program = parse_program(&format!("let corpus = {}\ncorpus", corpus));
    let chunk = compiler::compile_repl(&program).expect("compile error");
    let mut vm = VM::new();
    let result = vm.execute(&chunk).expect("execution error");
    let values: Vec<Value> = match result.as_obj().and_then(|r| vm.gc.get(r)).map(|o| &o.kind) {
        Some(ObjKind::Array(items)) => items.clone(),
        _ => panic!("corpus is an array"),
    };
    let gc = &vm.gc;
    let mut map = VmMap::new();
    let mut linear: Vec<(Value, Value)> = Vec::new();
    for (i, k) in values.iter().enumerate() {
        let v = Value::try_inline_int(i as i64).expect("small int");
        map.insert_in(*k, v, gc);
        match linear.iter_mut().find(|(x, _)| x.set_eq(k, gc)) {
            Some(entry) => entry.1 = v,
            None => linear.push((*k, v)),
        }
    }
    assert_eq!(map.len(), linear.len());
    for ((mk, mv), (lk, lv)) in map.iter().zip(&linear) {
        assert!(mk.set_eq(lk, gc) && mv.set_eq(lv, gc));
    }
    for k in &values {
        let expected = linear.iter().find(|(x, _)| x.set_eq(k, gc)).map(|e| e.1);
        let got = map.get_in(k, gc);
        assert_eq!(
            got.map(|v| v.display(gc)),
            expected.map(|v| v.display(gc)),
            "{}",
            k.display(gc)
        );
        assert!(map.has_in(k, gc));
    }
}
