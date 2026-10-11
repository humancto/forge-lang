//! Edition 2027 enforces annotations (docs/editions/2027.md): a
//! contradiction of a declared type stops `forge run`/`test`/`-e` before
//! anything runs, and annotated boundaries are checked at run time on both
//! engines. Edition 2026 keeps warning and running.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");
const ENGINES: [&[&str]; 2] = [&[], &["--interp"]];

/// A project directory with `forge.toml` naming `edition`.
fn project(name: &str, edition: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forge-ed2027-{}-{}-{}",
        name,
        edition,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("tests")).expect("mkdir");
    std::fs::write(
        dir.join("forge.toml"),
        format!(
            "[project]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"{}\"\n",
            name, edition
        ),
    )
    .expect("write forge.toml");
    for (path, source) in files {
        std::fs::write(dir.join(path), source).expect("write source");
    }
    dir
}

fn forge(dir: &Path, args: &[&str]) -> Output {
    Command::new(FORGE)
        .args(args)
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .expect("run forge")
}

fn with_engine<'a>(engine: &[&'a str], rest: &[&'a str]) -> Vec<&'a str> {
    engine.iter().chain(rest).copied().collect()
}

const DECLARED_ERROR: &str = "println(\"before\")\n\
fn twice(n: Int) -> Int { return n * 2 }\n\
println(twice(\"oops\"))\n";

#[test]
fn declared_type_error_stops_a_2027_run_before_it_starts() {
    let dir = project("declared", "2027", &[("main.fg", DECLARED_ERROR)]);
    for engine in ENGINES {
        let out = forge(&dir, &with_engine(engine, &["run", "main.fg"]));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{:?}: {}", engine, stderr);
        // Nothing ran: no partial output.
        assert_eq!(String::from_utf8_lossy(&out.stdout), "", "{:?}", engine);
        assert!(stderr.contains("[T0002] Error"), "{:?}: {}", engine, stderr);
        assert!(
            stderr.contains("argument 1 of 'twice': expected Int but got String"),
            "{}",
            stderr
        );
        assert!(
            stderr.contains("edition 2027: declared types are enforced"),
            "{}",
            stderr
        );
    }
    // JSON diagnostics carry severity "error".
    let out = forge(&dir, &["--error-format", "json", "run", "main.fg"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"T0002\"") && stderr.contains("\"severity\":\"error\""),
        "{}",
        stderr
    );
    // `forge check` agrees.
    let out = forge(&dir, &["check", "main.fg"]);
    assert_eq!(out.status.code(), Some(1));
    // The command-line flag selects the edition too, and `-e` honours it.
    let out = forge(&dir, &["--edition", "2026", "run", "main.fg"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "before\n",
        "{:?}",
        out
    );
    let out = forge(&dir, &["-e", DECLARED_ERROR]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edition_2026_warns_and_runs() {
    let dir = project("declared", "2026", &[("main.fg", DECLARED_ERROR)]);
    for engine in ENGINES {
        let out = forge(&dir, &with_engine(engine, &["run", "main.fg"]));
        let stderr = String::from_utf8_lossy(&out.stderr);
        // `"oops" * 2` is a run-time error, but only after "before".
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "before\n",
            "{:?}: {}",
            engine,
            stderr
        );
        assert!(
            stderr.contains("[T0002] Warning"),
            "{:?}: {}",
            engine,
            stderr
        );
        assert!(!stderr.contains("type error:"), "{}", stderr);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn annotated_boundaries_are_checked_at_run_time_in_2027() {
    // `json.parse` returns `Any`: only the run-time check can see it.
    let source = "fn double(n: Int) -> Int { return n * 2 }\n\
                  println(double(4))\n\
                  println(double(json.parse(\"\\\"seven\\\"\")))\n";
    for (edition, fails) in [("2027", true), ("2026", false)] {
        let dir = project("runtime", edition, &[("main.fg", source)]);
        for engine in ENGINES {
            let out = forge(&dir, &with_engine(engine, &["run", "main.fg"]));
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(
                String::from_utf8_lossy(&out.stdout).starts_with("8\n"),
                "{} {:?}: {}",
                edition,
                engine,
                stderr
            );
            assert_eq!(
                stderr.contains("type error: argument 'n' of 'double' must be Int, got String"),
                fails,
                "{} {:?}: {}",
                edition,
                engine,
                stderr
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn strict_in_2027_also_rejects_inferred_type_diagnostics() {
    // An unknown name on a path that never runs: a warning by default in
    // every edition, an error under --strict.
    let source = "if false { println(nope) }\nprintln(\"ran\")\n";
    let dir = project("strict", "2027", &[("main.fg", source)]);
    let out = forge(&dir, &["run", "main.fg"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "ran\n", "{:?}", out);
    let out = forge(&dir, &["--strict", "run", "main.fg"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{}", stderr);
    assert!(stderr.contains("[T0006] Error"), "{}", stderr);
    assert!(stderr.contains("(--strict)"), "{}", stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn forge_test_checks_test_files_in_2027() {
    let bad = "fn twice(n: Int) -> Int { return n * 2 }\n\
               @test\nfn uses_twice() { assert_eq(twice(\"x\"), 4) }\n";
    // `same("x")` does not fail by itself: only edition 2027's run-time
    // check makes it throw.
    let good = "fn twice(n: Int) -> Int { return n * 2 }\n\
                fn same(n: Int) -> Int { return n }\n\
                @test\nfn uses_twice() { assert_eq(twice(2), 4) }\n\
                @test\nfn checks_at_run_time() { assert_throws(fn() { same(json.parse(\"\\\"x\\\"\")) }) }\n";
    for (edition, passes) in [("2027", false), ("2026", true)] {
        let dir = project(
            "testcmd",
            edition,
            &[("tests/bad_test.fg", bad), ("tests/good_test.fg", good)],
        );
        let out = forge(&dir, &["test", "tests", "--engine", "both"]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if passes {
            // 2026 does not type-check tests; the bad test fails at run
            // time ("x" * 2), the run-time check is not there.
            assert!(!out.status.success());
            assert!(!stderr.contains("[T0002]"), "{}", stderr);
            assert!(stdout.contains("FAIL  checks_at_run_time"), "{}", stdout);
        } else {
            assert!(!out.status.success());
            assert!(stderr.contains("[T0002] Error"), "{}", stderr);
            assert!(
                stderr.contains("edition 2027: declared types are enforced"),
                "{}",
                stderr
            );
            assert!(stdout.contains("ok    checks_at_run_time"), "{}", stdout);
            // The bad file never ran: only the good file's test, once per
            // engine.
            assert_eq!(stdout.matches("uses_twice").count(), 2, "{}", stdout);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
