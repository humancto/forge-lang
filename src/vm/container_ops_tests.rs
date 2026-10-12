//! The container opcodes on locals (`GetIndexLocal` .. `CallPure`, see
//! `vm::local_ops`): in-place updates must be unobservable through any other
//! reference (aliases, arguments, closures, other containers, values read
//! out), and loops that build objects, arrays of records or groups must not
//! copy the container on every step.

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::vm::bytecode::{decode_op, Chunk, OpCode};
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

/// Runs `body` inside a function (locals in a frame) and at the top level
/// (locals of the main chunk), with and without GC stress.
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

/// Objects allocated while running `source` (small programs: no
/// collection runs, so the count only grows).
fn allocations(source: &str) -> usize {
    let chunk = compiler::compile(&parse_program(source)).expect("compile error");
    let mut vm = VM::new();
    let before = vm.gc.alloc_count;
    vm.execute(&chunk).expect("vm error");
    vm.gc.alloc_count - before
}

/// Extra allocations of `body` (inside a function) with `n = 1500` over
/// `n = 100`: about 1400 or more when every step copies the container.
fn extra_allocations(body: &str) -> usize {
    let program = |n: usize| format!("fn f(n) {{\n{body}\n}}\nf({n})\n");
    allocations(&program(1500)).saturating_sub(allocations(&program(100)))
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

#[test]
fn index_and_field_assignment_compile_to_local_ops() {
    let src = "fn f() {\n let mut o = {}\n o[\"a\"] = 1\n o.b = 2\n return o.a + o[\"b\"]\n}";
    assert!(uses_op(src, OpCode::SetIndexLocal));
    assert!(uses_op(src, OpCode::SetFieldLocal));
    assert!(uses_op(src, OpCode::GetIndexLocal));
    assert!(uses_op(src, OpCode::GetFieldLocal));
    let push = "fn f() {\n let mut g = { a: [] }\n g[\"a\"].push(1)\n g.a.push(2)\n return g\n}";
    assert!(uses_op(push, OpCode::PushIndexLocal));
    assert!(uses_op(push, OpCode::PushFieldLocal));
    let pure = "fn f() {\n let mut c = {}\n c[\"w\"] = get(c, \"w\", 0) + 1\n return len(c)\n}";
    assert!(uses_op(pure, OpCode::PeekLocal));
    assert!(uses_op(pure, OpCode::CallPure));
    // An immutable local is never updated in place.
    let fixed = "fn f() {\n let o = { a: [1] }\n o[\"b\"] = 1\n}";
    assert!(!uses_op(fixed, OpCode::SetIndexLocal));
}

#[test]
fn set_index_in_place_is_invisible_to_aliases() {
    assert_output(
        r#"
        let mut z = {}
        z["a"] = 1
        z["b"] = 2
        let w = z
        z["c"] = 3
        z.a = 10
        say w
        say z
        let mut xs = [1, 2, 3]
        xs[0] = 9
        let ys = xs
        xs[1] = 8
        xs[-1] = 7
        say ys
        say xs
        "#,
        &[
            r#"{ "a": 1, "b": 2 }"#,
            r#"{ "a": 10, "b": 2, "c": 3 }"#,
            "[9, 2, 3]",
            "[9, 8, 7]",
        ],
    );
}

#[test]
fn set_index_in_place_is_invisible_to_containers_callees_and_closures() {
    assert_output(
        r#"
        let mut o = {}
        o.n = 1
        let held = [o]
        let boxed = { inner: o }
        fn keep(v) {
            return v
        }
        let kept = keep(o)
        let peek = fn() { return o.n }
        o.n = 2
        say held[0].n
        say boxed.inner.n
        say kept.n
        say peek()
        say o.n
        "#,
        &["1", "1", "1", "2", "2"],
    );
}

#[test]
fn values_read_out_of_an_owned_container_are_copies() {
    assert_output(
        r#"
        let mut g = {}
        g["a"] = []
        g["a"].push(1)
        let first = g["a"]
        let second = get(g, "a", [])
        let third = g.a
        g["a"].push(2)
        g.a.push(3)
        say first
        say second
        say third
        say g
        "#,
        &["[1]", "[1]", "[1]", r#"{ "a": [1, 2, 3] }"#],
    );
}

#[test]
fn nested_push_is_invisible_to_snapshots_of_the_container() {
    assert_output(
        r#"
        let mut g = { a: [0] }
        g.a.push(1)
        let snap = g
        g.a.push(2)
        g["a"].push(3)
        let snap2 = g
        g.a.push(4)
        say snap
        say snap2
        say g
        let mut grid = [[0], [0]]
        grid[1].push(1)
        let row = grid[1]
        grid[1].push(2)
        say row
        say grid
        "#,
        &[
            r#"{ "a": [0, 1] }"#,
            r#"{ "a": [0, 1, 2, 3] }"#,
            r#"{ "a": [0, 1, 2, 3, 4] }"#,
            "[0, 1]",
            "[[0], [0, 1, 2]]",
        ],
    );
}

#[test]
fn values_stored_into_an_owned_container_stay_independent() {
    assert_output(
        r#"
        let mut g = { a: [] }
        g.a.push(1)
        let mut mine = [5]
        g["b"] = mine
        g.b.push(6)
        mine.push(7)
        say g
        say mine
        let mut other = { a: [] }
        other.a.push(1)
        g["c"] = other.a
        g.c.push(2)
        other.a.push(3)
        say g.c
        say other.a
        "#,
        &[r#"{ "a": [1], "b": [5, 6] }"#, "[5, 7]", "[1, 2]", "[1, 3]"],
    );
}

#[test]
fn copied_containers_do_not_share_owned_elements() {
    // `g2` starts as a copy of `g`: their elements are shared until one of
    // them is updated, and an update through one must not reach the other.
    assert_output(
        r#"
        let mut g = { a: [] }
        g.a.push(1)
        let mut g2 = g
        g2.a.push(2)
        g.a.push(3)
        g2["b"] = 0
        g2.a.push(4)
        g.a.push(5)
        say g
        say g2
        "#,
        &[r#"{ "a": [1, 3, 5] }"#, r#"{ "a": [1, 2, 4], "b": 0 }"#],
    );
}

#[test]
fn pure_calls_borrow_only_builtins() {
    // A user function named `get` receives a shared copy.
    assert_output(
        r#"
        let mut kept = []
        let get = fn(o, k, d) {
            kept.push(o)
            return d
        }
        let mut counts = {}
        counts["a"] = 1
        counts["b"] = get(counts, "b", 2)
        counts["c"] = 3
        say kept
        say counts
        "#,
        &[r#"[{ "a": 1 }]"#, r#"{ "a": 1, "b": 2, "c": 3 }"#],
    );
}

#[test]
fn captured_locals_are_updated_through_their_cell() {
    assert_output(
        r#"
        let mut state = { log: [] }
        let note = fn(x) { state.log.push(x) }
        state["n"] = 0
        note("a")
        state.log.push("b")
        state["n"] = len(state.log)
        note("c")
        say state
        "#,
        &[r#"{ "log": ["a", "b", "c"], "n": 2 }"#],
    );
}

#[test]
fn building_containers_does_not_copy_them() {
    // Each loop allocates its keys/strings; a copying update would add an
    // allocation per step on top of that.
    let object = r#"
        let mut ob = {}
        for i in range(0, n) {
            ob["k" + str(i)] = i
        }
        return ob
    "#;
    let records = r#"
        let mut recs = []
        for i in range(0, n) {
            recs.push({ id: i })
        }
        return recs
    "#;
    let groups = r#"
        let mut g = { a: [], b: [] }
        for i in range(0, n) {
            g.a.push(i)
            g["b"].push(i)
        }
        return g
    "#;
    let counts = r#"
        let mut counts = { a: 0 }
        for i in range(0, n) {
            counts["a"] = get(counts, "a", 0) + 1
            counts["a"] = counts["a"] + 1
            counts.a = counts.a + 1
        }
        return counts
    "#;
    // Expected extra allocations per step: object 2 (key string, one
    // `str`), records 1 (the literal), groups and counts 0.
    let limits = [
        (object, 2 * 1400 + 200),
        (records, 1400 + 200),
        (groups, 200),
        (counts, 200),
    ];
    for (body, limit) in limits {
        let extra = extra_allocations(body);
        assert!(
            extra <= limit,
            "{extra} extra allocations (limit {limit}) for:\n{body}"
        );
    }
}
