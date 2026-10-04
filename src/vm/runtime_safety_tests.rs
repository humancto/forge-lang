//! Regression tests for VM memory/runtime safety:
//! GC rooting across native callbacks (with GC stress mode), big-integer
//! construction, recursion limits, import cycles and Result matching.

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::vm::compiler;
use crate::vm::machine::{VMError, VM};

fn parse_program(source: &str) -> crate::parser::ast::Program {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer error");
    let mut parser = Parser::new(tokens);
    parser.parse_program().expect("parse error")
}

fn run_vm(source: &str, stress: bool) -> Result<Vec<String>, VMError> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.gc.set_stress(stress);
    vm.execute(&chunk)?;
    Ok(vm.output.clone())
}

fn stress_output(source: &str) -> Vec<String> {
    run_vm(source, true).expect("vm error under GC stress")
}

/// Value of the program's last expression, rendered (REPL mode).
fn run_vm_value(source: &str, stress: bool) -> Result<String, VMError> {
    let program = parse_program(source);
    let chunk = compiler::compile_repl(&program).expect("compile error");
    let mut vm = VM::new();
    vm.gc.set_stress(stress);
    let value = vm.execute(&chunk)?;
    Ok(value.display(&vm.gc))
}

// ---------------------------------------------------------------------------
// GC rooting across native callbacks
// ---------------------------------------------------------------------------

#[test]
fn gc_map_results_survive_collection_in_callbacks() {
    // Every callback allocates; under stress mode a GC runs after each
    // allocation, so unrooted results of earlier callbacks would be freed.
    let out = stress_output(
        r#"
        let r = map(range(0, 200), fn(x) { return "item-" + str(x) + "-padding" })
        say r[0]
        say r[199]
        let mut ok = true
        for i in range(0, 200) { if r[i] != "item-" + str(i) + "-padding" { ok = false } }
        say ok
        "#,
    );
    assert_eq!(out, vec!["item-0-padding", "item-199-padding", "true"]);
}

#[test]
fn gc_map_objects_survive_collection_in_callbacks() {
    let out = stress_output(
        r#"
        let objs = map(range(0, 100), fn(i) { return {id: i, tags: [i, i + 1]} })
        say objs[5]
        say objs[99].tags[1]
        "#,
    );
    assert_eq!(out, vec![r#"{ "id": 5, "tags": [5, 6] }"#, "100"]);
}

#[test]
fn gc_callback_builtins_under_stress() {
    let out = stress_output(
        r#"
        let xs = map(range(0, 30), fn(x) { return "s" + str(x) })
        say len(filter(xs, fn(s) { let t = s + "!"
 return len(t) > 3 }))
        say reduce(xs, "", fn(acc, s) { return acc + s })
        let sorted = sort(range(0, 40), fn(a, b) { let t = "x" + str(a)
 return b - a })
        say sorted[0]
        say find(xs, fn(s) { let t = s + "?"
 return s == "s17" })
        say len(flat_map(range(0, 10), fn(i) { return ["a" + str(i), "b" + str(i)] }))
        let parts = partition(xs, fn(s) { let t = s + "?"
 return len(s) == 2 })
        say len(parts[0])
        let groups = group_by(xs, fn(s) { return "k" + str(len(s)) })
        say len(groups.k3)
        let sorted_keys = sort_by(xs, fn(s) { return "key-" + s })
        say sorted_keys[0]
        let mut n = 0
        for_each(xs, fn(s) { let t = s + "."
 n = n + 1 })
        say any(xs, fn(s) { return s + "" == "s29" })
        say all(xs, fn(s) { return len(s + "") >= 2 })
        "#,
    );
    assert_eq!(
        out,
        vec![
            "20",
            "s0s1s2s3s4s5s6s7s8s9s10s11s12s13s14s15s16s17s18s19s20s21s22s23s24s25s26s27s28s29",
            "39",
            "s17",
            "20",
            "10",
            "20",
            "s0",
            "true",
            "true",
        ]
    );
}

#[test]
fn gc_snapshot_survives_callback_mutating_source() {
    // The callback drops the source array's last references to its items;
    // the builtin's snapshot must keep them alive.
    let out = stress_output(
        r#"
        let mut src = map(range(0, 20), fn(i) { return "v" + str(i) })
        let out = map(src, fn(s) {
            src = []
            let junk = map(range(0, 5), fn(j) { return "junk" + str(j) })
            return s + "!"
        })
        say out[0]
        say out[19]
        "#,
    );
    assert_eq!(out, vec!["v0!", "v19!"]);
}

#[test]
fn gc_stream_pipeline_under_stress() {
    let out = stress_output(
        r#"
        let r = range(0, 50).stream().map(fn(x) { return "n" + str(x) }).filter(fn(s) { return len(s + "") == 3 }).collect()
        say len(r)
        say r[0]
        say r[39]
        "#,
    );
    assert_eq!(out, vec!["40", "n10", "n49"]);
}

/// Run every supported parity fixture with the GC collecting at every safe
/// point and require the same result as a normal run. Any value a builtin
/// holds without rooting shows up here as a mismatch or a `<freed>` value.
#[test]
fn gc_stress_matches_normal_run_on_parity_fixtures() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/supported");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("read parity fixtures")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "fg"))
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "no parity fixtures found");
    let mut checked = 0;
    for path in entries {
        let source = std::fs::read_to_string(&path).expect("read fixture");
        let normal = run_vm_value(&source, false);
        let stressed = run_vm_value(&source, true);
        match (normal, stressed) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a, b, "GC stress changed the result of {}", path.display());
                assert!(!b.contains("<freed>"), "{}: {}", path.display(), b);
                checked += 1;
            }
            (Err(a), Err(b)) => assert_eq!(a.message, b.message, "{}", path.display()),
            (a, b) => panic!(
                "{}: normal={:?} stressed={:?}",
                path.display(),
                a.map_err(|e| e.message),
                b.map_err(|e| e.message)
            ),
        }
    }
    assert!(checked > 10, "only {} fixtures checked", checked);
}
