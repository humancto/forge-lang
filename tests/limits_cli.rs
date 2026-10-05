//! CLI integration tests for deterministic resource limits
//! (`--max-fuel`, `--max-memory`; see `src/runtime/limits.rs`).
//!
//! Every check runs on both engines: the bytecode VM (default) and the
//! tree-walking interpreter (`--interp`).

use std::path::PathBuf;
use std::process::{Command, Output};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");
const ENGINES: [&[&str]; 2] = [&[], &["--interp"]];

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "forge_limits_cli_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).expect("mkdir");
    d
}

fn run(tag: &str, engine: &[&str], flags: &[&str], source: &str) -> Output {
    let dir = tmpdir(tag);
    let file = dir.join("main.fg");
    std::fs::write(&file, source).expect("write script");
    let out = Command::new(FORGE)
        .args(engine)
        .args(flags)
        .arg("run")
        .arg(&file)
        .output()
        .expect("spawn forge");
    let _ = std::fs::remove_dir_all(&dir);
    out
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[track_caller]
fn assert_fails_with(o: &Output, needle: &str) {
    assert!(
        !o.status.success(),
        "expected failure, stdout: {}",
        stdout(o)
    );
    assert!(stderr(o).contains(needle), "stderr: {}", stderr(o));
}

const COUNTER: &str = "let mut i = 0\nwhile true {\n  i = i + 1\n  if i % 100 == 0 { say i }\n}\n";

#[test]
fn max_fuel_stops_runaway_loops_deterministically() {
    for engine in ENGINES {
        let a = run("fuel", engine, &["--max-fuel", "20000"], COUNTER);
        let b = run("fuel", engine, &["--max-fuel", "20000"], COUNTER);
        assert_fails_with(&a, "fuel exhausted");
        assert!(!stdout(&a).is_empty(), "{engine:?}");
        assert_eq!(stdout(&a), stdout(&b), "{engine:?}: not deterministic");
    }
}

#[test]
fn max_fuel_is_not_catchable() {
    let src = "try { while true { } } catch e { say \"caught\" }\nsay \"after\"\n";
    for engine in ENGINES {
        let o = run("fuelcatch", engine, &["--max-fuel", "5000"], src);
        assert_fails_with(&o, "fuel exhausted");
        assert_eq!(stdout(&o), "", "{engine:?}");
    }
}

#[test]
fn max_fuel_applies_with_the_jit() {
    let src = "fn spin(n) {\n  let mut i = 0\n  while i < n { i = i + 1 }\n  return i\n}\nsay spin(100000000000)\n";
    let o = run("fueljit", &["--jit"], &["--max-fuel", "100000"], src);
    assert_fails_with(&o, "fuel exhausted");
}

#[test]
fn max_memory_stops_growth_and_normal_programs_pass() {
    // Uses a function local so the VM appends in place (a top-level
    // binding would copy the array on every push).
    let grow = "fn grow() {\n  let mut kept = []\n  let mut i = 0\n  while true {\n    let item = \"some text that stays alive \" + str(i)\n    kept.push(item)\n    i = i + 1\n  }\n}\ngrow()\n";
    for engine in ENGINES {
        let o = run("mem", engine, &["--max-memory", "8MB"], grow);
        assert_fails_with(&o, "memory limit exceeded");
        let o = run(
            "memcap",
            engine,
            &["--max-memory", "8MB"],
            "let s = repeat_str(\"x\", 1000000000000)\n",
        );
        assert_fails_with(&o, "resource limit exceeded");
        let o = run(
            "memok",
            engine,
            &["--max-memory", "64MB", "--max-fuel", "10000000"],
            "let xs = map(range(0, 1000), fn(x) { return x * 2 })\nsay len(xs)\nsay xs[999]\n",
        );
        assert!(o.status.success(), "{engine:?}: {}", stderr(&o));
        assert_eq!(stdout(&o), "1000\n1998\n");
    }
}

#[test]
fn bad_limit_flags_are_usage_errors() {
    let o = run("bad", &[], &["--max-memory", "lots"], "say 1\n");
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert!(stderr(&o).contains("--max-memory"), "{}", stderr(&o));
    let o = run("bad0", &[], &["--max-fuel", "0"], "say 1\n");
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
}
