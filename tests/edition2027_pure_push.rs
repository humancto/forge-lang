//! Edition 2027 makes `push(xs, v)` a pure function (docs/editions/2027.md):
//! a statement `push(xs, v)` throws the new array away, so the type checker
//! rejects it (T0019) on `run`, `check` and `-e` before anything runs, on
//! both engines. Edition 2026 keeps appending.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");
const ENGINES: [&[&str]; 2] = [&[], &["--interp"]];

const STATEMENT_PUSH: &str = "println(\"before\")\nlet mut xs = [1]\npush(xs, 2)\nprintln(xs)\n";
const ASSIGNED_PUSH: &str =
    "let mut xs = [1]\nxs = push(xs, 2)\nlet ys = push(xs, 3)\nprintln(xs)\nprintln(ys)\n";

fn script(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forge-pure-push-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write script");
    path
}

fn forge(args: &[&str]) -> Output {
    Command::new(FORGE)
        .args(args)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .expect("run forge")
}

fn args<'a>(engine: &[&'a str], rest: &[&'a str]) -> Vec<&'a str> {
    engine.iter().chain(rest).copied().collect()
}

#[test]
fn statement_push_is_a_type_error_in_2027() {
    let path = script("statement.fg", STATEMENT_PUSH);
    let file = path.to_str().expect("utf-8 path");
    for engine in ENGINES {
        let out = forge(&args(engine, &["--edition", "2027", "run", file]));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{engine:?}: {stderr}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "", "nothing runs");
        assert!(stderr.contains("[T0019] Error"), "{stderr}");
        assert!(stderr.contains("xs = push(xs, value)"), "{stderr}");
        assert!(stderr.contains("xs.push(value)"), "{stderr}");

        let eval = forge(&args(engine, &["--edition", "2027", "-e", STATEMENT_PUSH]));
        assert!(!eval.status.success(), "{engine:?}: -e must fail");
        assert!(String::from_utf8_lossy(&eval.stderr).contains("T0019"));
    }
    let check = forge(&["--edition", "2027", "check", file]);
    assert!(!check.status.success());
    assert!(String::from_utf8_lossy(&check.stderr).contains("T0019"));

    let explain = forge(&["explain", "T0019"]);
    assert!(explain.status.success());
    assert!(String::from_utf8_lossy(&explain.stdout).contains("xs = push(xs, 3)"));
}

#[test]
fn statement_push_appends_in_2026() {
    let path = script("statement_2026.fg", STATEMENT_PUSH);
    let file = path.to_str().expect("utf-8 path");
    for engine in ENGINES {
        let out = forge(&args(engine, &["run", file]));
        assert!(out.status.success(), "{engine:?}: {:?}", out);
        assert_eq!(String::from_utf8_lossy(&out.stdout), "before\n[1, 2]\n");
        assert!(!String::from_utf8_lossy(&out.stderr).contains("T0019"));
    }
}

#[test]
fn assigned_push_behaves_per_edition() {
    let path = script("assigned.fg", ASSIGNED_PUSH);
    let file = path.to_str().expect("utf-8 path");
    for engine in ENGINES {
        // 2026: `let ys = push(xs, 3)` also appends to the mutable `xs`.
        let old = forge(&args(engine, &["run", file]));
        assert_eq!(
            String::from_utf8_lossy(&old.stdout),
            "[1, 2, 3]\n[1, 2, 3]\n",
            "{engine:?}"
        );
        let new = forge(&args(engine, &["--edition", "2027", "run", file]));
        assert!(new.status.success(), "{engine:?}: {:?}", new);
        assert_eq!(
            String::from_utf8_lossy(&new.stdout),
            "[1, 2]\n[1, 2, 3]\n",
            "{engine:?}"
        );
    }
}
