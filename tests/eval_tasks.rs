//! The evaluation harness (`eval/`, RFC 0008) stays valid: every task's
//! Forge reference passes on the VM (edition 2026) and the interpreter
//! (edition 2027) and type-checks clean, every Python reference passes, and
//! the harness still classifies failures correctly. Skipped when `python3`
//! (3.9 or newer) is not installed.

use std::path::Path;
use std::process::Command;

const FORGE: &str = env!("CARGO_BIN_EXE_forge");

fn python() -> Option<&'static str> {
    let ok = Command::new("python3")
        .args([
            "-c",
            "import sys; sys.exit(0 if sys.version_info >= (3, 9) else 1)",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        Some("python3")
    } else {
        eprintln!("skipping: python3 >= 3.9 not found");
        None
    }
}

/// The two tests run the harness one at a time: `selftest` starts many
/// `forge` processes, and `validate` has tasks whose time limit proves
/// concurrency (`timed`), which CPU contention on a small runner breaks.
static HARNESS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn harness(args: &[&str]) {
    let _one_at_a_time = HARNESS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(py) = python() else { return };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new(py)
        .arg(root.join("eval/forge_eval.py"))
        .args(args)
        .args(["--forge", FORGE])
        .current_dir(root)
        .output()
        .expect("run eval/forge_eval.py");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "forge_eval.py {} failed\n--- stdout\n{}\n--- stderr\n{}",
        args.join(" "),
        stdout,
        stderr
    );
}

#[test]
fn eval_references_pass_on_both_engines() {
    harness(&["validate", "--quick"]);
}

#[test]
fn eval_harness_classifies_failures() {
    harness(&["selftest"]);
}
