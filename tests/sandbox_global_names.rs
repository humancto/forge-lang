//! SEC-26: a long-lived host running untrusted code on the VM must not keep
//! the global names each run defines. Every run interns into its own name
//! domain (`vm::globals::GlobalNames`), freed when the run ends; before,
//! one process-wide interner leaked every name forever and grew every later
//! VM's slot table.
//!
//! One test per binary on purpose: the counters are process-wide, so no
//! other test may create VMs while this one measures.

use forge_lang::vm::globals::GlobalNames;
use forge_lang::{Engine, Sandbox};

fn run(i: usize) {
    let source = format!(
        "fn generated_fn_{i}() {{\n    return {i}\n}}\nlet generated_value_{i} = generated_fn_{i}()\nsay generated_value_{i}"
    );
    let out = Sandbox::new()
        .engine(Engine::Vm)
        .run_source(&source)
        .expect("sandbox run");
    assert_eq!(out.stdout.trim(), i.to_string());
}

#[test]
fn sandbox_runs_do_not_keep_their_global_names() {
    // Warm up any lazily initialized state first.
    run(0);
    let baseline = GlobalNames::live_counts();
    for i in 1..=200 {
        run(i);
        assert_eq!(
            GlobalNames::live_counts(),
            baseline,
            "run {} left interned global names behind",
            i
        );
    }
    // Nothing else holds a domain once the runs are over.
    assert_eq!(baseline, (0, 0));
}
