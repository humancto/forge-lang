//! Indexed global slots (`vm::globals`): the observable behaviour of
//! globals must be exactly what the name-keyed table provided.

use super::*;
use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::parser::Parser;
use std::sync::Arc;

fn parse(source: &str) -> crate::parser::ast::Program {
    let tokens = Lexer::new(source).tokenize().expect("lexer error");
    Parser::new(tokens).parse_program().expect("parse error")
}

fn compile(source: &str) -> bytecode::Chunk {
    compiler::compile_repl(&parse(source)).expect("compile error")
}

fn vm_value(vm: &mut VM, chunk: &bytecode::Chunk) -> String {
    let v = vm.execute(chunk).expect("vm error");
    v.display(&vm.gc)
}

/// Value on the interpreter, the VM, and the VM running the chunk after a
/// serialize/deserialize round trip (fresh, empty id cache).
fn assert_value(source: &str, expected: &str) {
    let program = parse(source);
    let interp = Interpreter::new()
        .run_repl(&program)
        .expect("interpreter error")
        .to_string();
    assert_eq!(interp, expected, "interpreter");
    let chunk = compile(source);
    assert_eq!(vm_value(&mut VM::new(), &chunk), expected, "vm");
    let bytes = serialize::serialize_chunk(&chunk).expect("serialize");
    let restored = serialize::deserialize_chunk(&bytes).expect("deserialize");
    assert_eq!(vm_value(&mut VM::new(), &restored), expected, ".fgc");
    // The same chunk again: its id cache is now filled.
    assert_eq!(vm_value(&mut VM::new(), &chunk), expected, "vm, cached ids");
}

fn vm_error(source: &str) -> String {
    VM::new()
        .execute(&compile(source))
        .expect_err("vm should error")
        .message
}

fn interp_error(source: &str) -> String {
    Interpreter::new()
        .run_repl(&parse(source))
        .expect_err("interpreter should error")
        .to_string()
}

#[test]
fn undefined_global_error_text_is_unchanged() {
    // No suggestion.
    let src = "fn f() {\n    return zzqqxx\n}\nf()";
    assert_eq!(
        vm_error(src),
        crate::semantics::undefined_variable("zzqqxx", None)
    );
    // A global suggestion (a builtin).
    let src = "prinln(1)";
    let err = vm_error(src);
    assert_eq!(
        err,
        crate::semantics::undefined_variable("prinln", Some("println"))
    );
    assert!(interp_error(src).contains(&err), "{}", interp_error(src));
    // A user-defined global function.
    let src = "fn helper() {\n    return 1\n}\nfn f() {\n    return helpr()\n}\nf()";
    assert_eq!(
        vm_error(src),
        crate::semantics::undefined_variable("helpr", Some("helper"))
    );
    // A visible local beats globals (compiler-recorded hint).
    let src = "fn f() {\n    let counter = 1\n    return countr\n}\nf()";
    assert_eq!(
        vm_error(src),
        crate::semantics::undefined_variable("countr", Some("counter"))
    );
}

#[test]
fn undefined_global_after_bytecode_round_trip() {
    let chunk = compile("fn f() {\n    return zzqqyy\n}\nf()");
    let bytes = serialize::serialize_chunk(&chunk).expect("serialize");
    let restored = serialize::deserialize_chunk(&bytes).expect("deserialize");
    let err = VM::new().execute(&restored).expect_err("must fail").message;
    assert_eq!(err, crate::semantics::undefined_variable("zzqqyy", None));
}

#[test]
fn redefining_and_reading_later_definitions() {
    // A function reads a global that is defined after it and then redefined.
    assert_value(
        "fn g() {\n    return h()\n}\nfn h() {\n    return 1\n}\nlet a = g()\nfn h() {\n    return 2\n}\n[a, g()]",
        "[1, 2]",
    );
    // Recursion through the global binding.
    assert_value(
        "fn fact(n) {\n    if n <= 1 {\n        return 1\n    }\n    return n * fact(n - 1)\n}\nfact(10)",
        "3628800",
    );
}

#[test]
fn user_functions_shadow_builtins() {
    assert_value(
        "fn len(x) {\n    return 99\n}\nfn f() {\n    return len([1])\n}\n[len([1, 2]), f()]",
        "[99, 99]",
    );
}

#[test]
fn repl_steps_share_globals_and_chunks_are_reusable_across_vms() {
    let define = compile("fn answer() {\n    return 41\n}");
    let call = compile("answer() + 1");
    let redefine = compile("fn answer() {\n    return 1\n}");

    let mut a = VM::new();
    a.execute(&define).expect("define");
    assert_eq!(vm_value(&mut a, &call), "42");
    a.execute(&redefine).expect("redefine");
    assert_eq!(vm_value(&mut a, &call), "2");

    // `call` has its ids cached now; in a VM that never defined `answer`
    // the global is still undefined.
    let mut b = VM::new();
    let err = b.execute(&call).expect_err("undefined in b").message;
    assert!(err.starts_with("undefined variable: 'answer'"), "{}", err);
    b.execute(&define).expect("define in b");
    assert_eq!(vm_value(&mut b, &call), "42");
    // ...and `a` is unaffected by `b`.
    assert_eq!(vm_value(&mut a, &call), "2");
}

#[test]
fn spawn_gets_copies_of_globals() {
    assert_value(
        "let base = 10\nfn add(x) {\n    return x + base\n}\nlet h = spawn { add(5) }\nawait h",
        "15",
    );
    // A task's global writes stay in the task's VM.
    let chunk = compile(
        "fn set_it() {\n    return 1\n}\nlet h = spawn {\n    set_it()\n}\nawait h\nset_it()",
    );
    assert_eq!(vm_value(&mut VM::new(), &chunk), "1");
}

#[test]
fn globals_are_gc_roots_under_stress() {
    let src = "fn make() {\n    return [\"a\" + str(1), {k: \"v\" + str(2)}]\n}\nfn retain_one() {\n    return make()\n}\nlet mut out = []\nfor i in range(50) {\n    out = push(out, retain_one())\n}\nfn last() {\n    return out[49]\n}\nstr(last())";
    let chunk = compile(src);
    let mut vm = VM::new();
    vm.gc.set_stress(true);
    let got = vm_value(&mut vm, &chunk);
    let expected = Interpreter::new()
        .run_repl(&parse(src))
        .expect("interpreter")
        .to_string();
    assert_eq!(got, expected);
}

#[test]
fn defining_a_global_registers_its_name() {
    let mut vm = VM::new();
    vm.execute(&compile("fn __slot_probe_fn() {\n    return 7\n}"))
        .expect("define");
    assert!(vm.globals.contains_key("__slot_probe_fn"));
    assert!(vm.globals.keys().any(|k| k == "__slot_probe_fn"));
    let id = vm.globals.names().intern("__slot_probe_fn");
    assert!(vm.globals.get_id(id).is_some());
    // Every defined global is reachable by name and by id, and vice versa.
    for (name, value) in vm.globals.iter() {
        let by_id = vm
            .globals
            .get_id(vm.globals.names().intern(name))
            .expect("slot");
        assert_eq!(by_id.0.to_bits(), value.0.to_bits(), "{}", name);
    }
    assert_eq!(vm.globals.iter().count(), vm.globals.len());
    assert_eq!(vm.globals.values().count(), vm.globals.len());
}

/// One compiled chunk run by VMs of different name domains, interleaved:
/// each VM must see its own globals, even when the other domain numbered
/// the same names differently (a cached id is tagged with its domain).
#[test]
fn a_chunk_shared_by_vms_of_different_domains_uses_each_domains_ids() {
    // Functions are globals (top-level `let`s are registers).
    let define = compile(
        "fn alpha() {\n    return 1\n}\nfn beta() {\n    return 2\n}\nfn pick() {\n    return alpha() * 10 + beta()\n}",
    );
    let call = compile("pick() + alpha()");

    let mut a = VM::new();
    let mut b = VM::new();
    assert!(!Arc::ptr_eq(a.globals.names(), b.globals.names()));
    // `b` interns extra names, in another order, before running the chunk:
    // its ids for `beta`, `alpha` and `pick` differ from `a`'s.
    b.execute(&compile(
        "fn zz_pad_1() {}\nfn beta() {}\nfn zz_pad_2() {}\nfn alpha() {}",
    ))
    .expect("pad b");
    a.execute(&define).expect("define in a");
    assert_eq!(vm_value(&mut a, &call), "13");
    let a_alpha = a.globals.names().intern("alpha");
    let b_alpha = b.globals.names().intern("alpha");
    assert_ne!(
        a_alpha, b_alpha,
        "the domains must number names differently"
    );

    b.execute(&define).expect("define in b");
    for _ in 0..3 {
        assert_eq!(vm_value(&mut b, &call), "13");
        assert_eq!(vm_value(&mut a, &call), "13");
    }
    // Writes stay in their own VM.
    let use_gamma = compile("gamma() + alpha()");
    b.execute(&compile("fn gamma() {\n    return 5\n}"))
        .expect("write b");
    assert_eq!(vm_value(&mut b, &use_gamma), "6");
    let err = a.execute(&use_gamma).expect_err("gamma is b's").message;
    assert!(err.starts_with("undefined variable: 'gamma'"), "{}", err);
    assert_eq!(vm_value(&mut b, &use_gamma), "6");
    assert_eq!(vm_value(&mut a, &call), "13");
    // The same chunk on threads, one VM (and domain) per thread.
    let call = Arc::new(call);
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let (define, call) = (define.clone(), Arc::clone(&call));
            std::thread::spawn(move || {
                let mut vm = VM::new();
                vm.execute(&compile(&format!("fn pad_{}() {{}}", i)))
                    .expect("pad");
                vm.execute(&define).expect("define");
                (0..50)
                    .map(|_| vm_value(&mut vm, &call))
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    for h in handles {
        assert!(h.join().expect("thread").iter().all(|v| v == "13"));
    }
}

#[test]
fn spawn_and_server_forks_share_the_domain() {
    let mut vm = VM::new();
    vm.execute(&compile("fn handler() {\n    return 1\n}"))
        .expect("define");
    assert!(Arc::ptr_eq(&vm.spawn_fork_domain(), vm.globals.names()));
    let template = serve::VmTemplate::new(&vm, std::collections::HashMap::new()).expect("template");
    let fork = template.fork(Arc::new(std::sync::atomic::AtomicBool::new(false)));
    assert!(Arc::ptr_eq(fork.globals.names(), vm.globals.names()));
}

/// Fresh VMs do not inherit names other VMs interned: the slot table of a
/// new VM has the same size however many unrelated globals were defined
/// before (the old process-wide interner grew it without bound).
#[test]
fn fresh_vms_do_not_see_other_vms_names() {
    let baseline = VM::new().globals.slot_count();
    for i in 0..100 {
        let mut vm = VM::new();
        vm.execute(&compile(&format!(
            "fn generated_{}() {{\n    return {}\n}}\nlet value_{} = generated_{}()",
            i, i, i, i
        )))
        .expect("run");
        assert_eq!(vm.globals.slot_count(), baseline + 1);
    }
    assert_eq!(VM::new().globals.slot_count(), baseline);
}
