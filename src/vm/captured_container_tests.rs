//! In-place updates of containers that are not plain uncaptured locals
//! (`vm::local_ops`): captured variables (a function updating a top-level
//! array through `BindUpvalue`, the declaring frame updating a captured
//! local), index reads whose key is an index or field read or a call
//! (`GetIndexLocal` / `PeekLocal` snapshots), and pushes through a chain of
//! two or more levels (`PushPlaceLocal`). Every fast path must be
//! unobservable: values taken before a write (aliases, arguments, closure
//! results, elements read out, spawned tasks) never change, under GC stress
//! too, and the loops must not copy the container per step.

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

/// The program's output on the interpreter, for parity.
fn interpreter_output(source: &str) -> Vec<String> {
    use crate::runtime::stdio::{capture, Stream};
    let guard = capture(1 << 20);
    crate::interpreter::Interpreter::new()
        .run(&parse_program(source))
        .expect("interpreter error");
    guard
        .finish()
        .text(Stream::Stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// Runs `source` with and without GC stress, and on the interpreter: all
/// three must print `expected`.
fn assert_output(source: &str, expected: &[&str]) {
    for stress in [false, true] {
        assert_eq!(
            run_output(source, stress),
            expected,
            "stress={stress}\n{source}"
        );
    }
    assert_eq!(
        interpreter_output(source),
        expected,
        "interpreter\n{source}"
    );
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

/// Objects allocated while running `source` (small programs: no
/// collection runs, so the count only grows).
fn allocations(source: &str) -> usize {
    let chunk = compiler::compile(&parse_program(source)).expect("compile error");
    let mut vm = VM::new();
    let before = vm.gc.alloc_count;
    vm.execute(&chunk).expect("vm error");
    vm.gc.alloc_count - before
}

/// Extra allocations of `program(n)` with `n = 1500` over `n = 100`: about
/// 1400 or more when every step copies the container.
fn extra_allocations(program: impl Fn(usize) -> String) -> usize {
    allocations(&program(1500)).saturating_sub(allocations(&program(100)))
}

#[test]
fn functions_bind_captured_containers() {
    let src = "let mut parent = [0, 1]\nfn put(x, r) {\n parent[x] = r\n}\nput(0, 1)";
    assert!(uses_op(src, OpCode::BindUpvalue));
    assert!(uses_op(src, OpCode::SetIndexLocal));
    // Immutable captures, globals (a forward reference) and parameters are
    // not bound.
    let fixed = "let xs = [1]\nfn f() {\n return xs[0]\n}\nf()";
    assert!(!uses_op(fixed, OpCode::BindUpvalue));
    let param = "let mut xs = [1]\nfn f(xs) {\n xs[0] = 2\n}\nf(xs)";
    assert!(!uses_op(param, OpCode::BindUpvalue));
    let scalar_only = "let mut n = 0\nfn f() {\n return n * 2\n}\nf()";
    assert!(!uses_op(scalar_only, OpCode::BindUpvalue));
}

#[test]
fn writes_through_a_captured_array_are_invisible_to_copies() {
    assert_output(
        r#"
        let mut parent = [0, 1, 2, 3]
        let before = parent
        fn put(x, r) {
            parent[x] = r
            return null
        }
        fn read() {
            return parent
        }
        fn change(p) {
            p[0] = 100
            return p
        }
        put(0, 9)
        let snap = parent
        put(1, 8)
        let got = read()
        put(2, 7)
        let changed = change(parent)
        put(3, 6)
        println(before)
        println(snap)
        println(got)
        println(changed)
        println(parent)
        "#,
        &[
            "[0, 1, 2, 3]",
            "[9, 1, 2, 3]",
            "[9, 8, 2, 3]",
            "[100, 8, 7, 3]",
            "[9, 8, 7, 6]",
        ],
    );
}

#[test]
fn the_declaring_frame_and_functions_share_one_owner() {
    // The top level writes the captured array in place too; a closure
    // result and an element read out stay as they were.
    assert_output(
        r#"
        let mut grid = [[1], [2]]
        let mut log = []
        fn add(i, v) {
            grid[i].push(v)
            log.push(v)
        }
        let row = grid[0]
        add(0, 5)
        grid[1].push(6)
        let get_grid = fn() { return grid }
        let g = get_grid()
        add(1, 7)
        grid[0] = [0]
        println(row)
        println(g)
        println(grid)
        println(log)
        "#,
        &["[1]", "[[1, 5], [2, 6]]", "[[0], [2, 6, 7]]", "[5, 7]"],
    );
}

#[test]
fn nested_closures_and_rebinding_see_the_same_variable() {
    assert_output(
        r#"
        let mut xs = [1]
        fn outer() {
            xs.push(2)
            let inner = fn() {
                xs.push(3)
            }
            inner()
            let n = len(xs)
            xs = xs + [n]
            return n
        }
        fn reset() {
            xs = [0]
        }
        let a = xs
        println(outer())
        let b = xs
        reset()
        xs.push(9)
        println(a)
        println(b)
        println(xs)
        "#,
        &["3", "[1]", "[1, 2, 3, 3]", "[0, 9]"],
    );
}

#[test]
fn a_local_after_the_alias_is_a_new_variable() {
    assert_output(
        r#"
        let mut items = [1]
        fn f() {
            items.push(2)
            let items = [7]
            return items
        }
        println(f())
        println(items)
        "#,
        &["[7]", "[1, 2]"],
    );
}

#[test]
fn spawned_tasks_get_copies() {
    // A task works on its own copy (VM); what the parent took before is
    // unchanged on both engines.
    assert_output(
        r#"
        let mut xs = [1, 2]
        let snap = xs
        fn bump() {
            xs[0] = xs[0] + 10
            return xs[0]
        }
        let t = spawn { bump() }
        println(await t)
        println(snap)
        "#,
        &["11", "[1, 2]"],
    );
}

#[test]
fn index_reads_with_computed_keys_keep_ownership_and_values() {
    assert_output(
        r#"
        let names = ["a", "b", "c"]
        let it = { sku: "b" }
        let mut o = { a: 0, b: 0, c: 0 }
        let w = o
        for i in range(0, 3) {
            o[names[i]] += 1
            o[it.sku] += 1
            o["k" + str(i)] = i
            o[str(i)] = has_key(o, str(i))
            o[str(i)] = o[str(i)] == false
        }
        println(w)
        println(o)
        "#,
        &[
            r#"{ "a": 0, "b": 0, "c": 0 }"#,
            r#"{ "a": 1, "b": 4, "c": 1, "k0": 0, "0": true, "k1": 1, "1": true, "k2": 2, "2": true }"#,
        ],
    );
}

#[test]
fn a_call_in_the_key_reads_the_snapshot() {
    // The key's call replaces the captured object: the read uses the value
    // the variable had before the key ran, like the generic read.
    assert_output(
        r#"
        let mut o = { k: 1 }
        fn key() {
            o = { k: 2 }
            return "k"
        }
        fn grow() {
            o["n"] = 5
            return "k"
        }
        let a = o[key()]
        let b = o[grow()]
        println(a)
        println(b)
        println(o)
        "#,
        &["1", "2", r#"{ "k": 2, "n": 5 }"#],
    );
}

#[test]
fn map_set_with_a_call_that_updates_the_captured_map() {
    assert_output(
        r#"
        let mut m = map()
        fn key() {
            m = m.set("a", 1)
            return "b"
        }
        m = m.set(key(), 2)
        println(m.len())
        println(m.get("b"))
        println(m.get("a"))
        "#,
        &["1", "2", "null"],
    );
}

#[test]
fn deep_push_in_place_is_invisible() {
    assert_output(
        r#"
        let mut r = {}
        let mut saved = []
        for i in range(0, 6) {
            let k = "k{i % 2}"
            if !has_key(r, k) {
                r[k] = { lat: [], n: 0 }
            }
            r[k].lat.push(i)
            if i == 2 {
                saved.push(r[k].lat)
                saved.push(r[k])
                saved.push(r)
            }
        }
        let mut deep = [[[1]]]
        let inner = deep[0][0]
        deep[0][0].push(2)
        deep[0][-1].push(3)
        println(saved)
        println(r)
        println(inner)
        println(deep)
        "#,
        &[
            r#"[[0, 2], { "lat": [0, 2], "n": 0 }, { "k0": { "lat": [0, 2], "n": 0 }, "k1": { "lat": [1], "n": 0 } }]"#,
            r#"{ "k0": { "lat": [0, 2, 4], "n": 0 }, "k1": { "lat": [1, 3, 5], "n": 0 } }"#,
            "[1]",
            "[[[1, 2, 3]]]",
        ],
    );
}

#[test]
fn deep_push_falls_back_to_the_generic_update() {
    // Missing keys, non-array leaves, frozen levels and immutable roots
    // behave exactly as without the fast path.
    assert_output(
        r#"
        let mut o = { a: { s: "x" }, f: freeze({ xs: [1] }) }
        try {
            o.a.missing.push(1)
        } catch e {
            println("missing")
        }
        try {
            o.f.xs.push(2)
        } catch e {
            println("frozen")
        }
        try {
            o.a.s.push(1)
        } catch e {
            println("string")
        }
        let fixed = { a: { b: [1] } }
        try {
            fixed.a.b.push(2)
        } catch e {
            println("immutable")
        }
        println(o.a)
        println(fixed)
        "#,
        &[
            "missing",
            "frozen",
            "string",
            "immutable",
            r#"{ "s": "x" }"#,
            r#"{ "a": { "b": [1] } }"#,
        ],
    );
}

#[test]
fn globals_written_by_functions_above_the_let_keep_value_semantics() {
    // `put` and friends are compiled before the `let`s, so `parent` and
    // `groups` are VM globals, updated through `BindGlobal` aliases.
    assert_output(
        r#"
        fn put(x, r) {
            parent[x] = r
            return null
        }
        fn add(k, v) {
            if !has_key(groups, k) {
                groups[k] = []
            }
            groups[k].push(v)
            groups.total.push(v)
            return len(groups[k])
        }
        fn root(x) {
            return parent[x]
        }
        let mut parent = [0, 1, 2]
        let mut groups = { total: [] }
        let before = parent
        put(0, 2)
        let mid = parent
        put(1, root(0))
        parent[2] = 9
        let g0 = groups
        add("a", 1)
        let inner = groups.a
        add("a", 2)
        try {
            parent[10] = 1
        } catch e {
            println("caught")
        }
        let after = [1]
        parent.push(3)
        println(before)
        println(mid)
        println(parent)
        println(after)
        println(g0)
        println(inner)
        println(groups)
        "#,
        &[
            "caught",
            "[0, 1, 2]",
            "[2, 1, 2]",
            "[2, 2, 9, 3]",
            "[1]",
            r#"{ "total": [] }"#,
            "[1]",
            r#"{ "total": [1, 2], "a": [1, 2] }"#,
        ],
    );
}

#[test]
fn a_global_used_before_its_let_is_undefined() {
    let src = "fn put(x) {\n parent[x] = 1\n}\nput(0)\nlet mut parent = [0]\n";
    let chunk = compiler::compile(&parse_program(src)).expect("compile error");
    let err = VM::new().execute(&chunk).expect_err("must fail");
    assert!(
        err.message.contains("undefined variable"),
        "{}",
        err.message
    );
    assert!(uses_op(src, OpCode::BindGlobal));
}

#[test]
fn captured_writes_do_not_copy_per_step() {
    // A function writing a top-level array (union-find style).
    let uf = |n: usize| {
        format!(
            "let mut parent = range(0, {n})\nfn put(x, r) {{\n parent[x] = r\n}}\nfor i in range(0, {n}) {{\n put(i, 0)\n}}\n"
        )
    };
    assert!(extra_allocations(uf) < 200, "{}", extra_allocations(uf));
    // The same with the function above the `let` (a global).
    let global = |n: usize| {
        format!(
            "fn put(x, r) {{\n parent[x] = r\n return parent[x]\n}}\nlet mut parent = range(0, {n})\nfor i in range(0, {n}) {{\n put(i, 0)\n if len(parent) < 0 {{\n parent.push(1)\n }}\n}}\n"
        )
    };
    assert!(
        extra_allocations(global) < 200,
        "{}",
        extra_allocations(global)
    );
    // The declaring frame writing an array a function captures.
    let main = |n: usize| {
        format!(
            "let mut xs = range(0, {n})\nfn first() {{\n return xs[0]\n}}\nfor i in range(0, {n}) {{\n xs[i] = first()\n}}\n"
        )
    };
    assert!(extra_allocations(main) < 200, "{}", extra_allocations(main));
    // A function pushing into groups of a top-level object.
    let groups = |n: usize| {
        format!(
            "let mut g = {{ a: [] }}\nfn add(v) {{\n g.a.push(v)\n return null\n}}\nfor i in range(0, {n}) {{\n add(i)\n}}\n"
        )
    };
    assert!(
        extra_allocations(groups) < 200,
        "{}",
        extra_allocations(groups)
    );
}

#[test]
fn computed_keys_and_deep_pushes_do_not_copy_per_step() {
    let keyed = |n: usize| {
        format!(
            "fn f(n) {{\n let names = map(range(0, 50), fn(i) {{ return \"k{{i}}\" }})\n let mut o = {{}}\n for k in names {{\n o[k] = 0\n }}\n for i in range(0, n) {{\n o[names[i % 50]] += 1\n }}\n return o\n}}\nf({n})\n"
        )
    };
    // No copy of the object per step.
    assert!(
        extra_allocations(keyed) < 200,
        "{}",
        extra_allocations(keyed)
    );
    let deep = |n: usize| {
        format!(
            "fn f(n) {{\n let mut r = {{ k: {{ lat: [] }} }}\n for i in range(0, n) {{\n r.k.lat.push(i)\n }}\n return r\n}}\nf({n})\n"
        )
    };
    assert!(extra_allocations(deep) < 200, "{}", extra_allocations(deep));
}
