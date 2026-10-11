//! Edition 2027 "one syntax" (RFC 0008 section 5, docs/editions/2027.md).
//!
//! Every natural-syntax form removed in 2027 is a syntax error `E0035`
//! whose hint names the replacement, on both engines, while the same
//! program still runs under edition 2026 (pinned here). Every removed
//! builtin is an unknown name in 2027 (`E0003` at run time, `T0006` in the
//! type checker) with a hint naming its replacement, and still works in
//! 2026.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forge-one-syntax-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn run(source: &str, args: &[&str]) -> Output {
    let dir = scratch();
    let file = dir.join(format!(
        "p{}.fg",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::write(&file, source).expect("write program");
    let mut cmd = Command::new(FORGE);
    cmd.args(["--error-format", "json"])
        .args(args)
        .arg(&file)
        .stdin(Stdio::null())
        .env("NO_COLOR", "1");
    let out = cmd.output().expect("run forge");
    let _ = std::fs::remove_file(&file);
    out
}

fn errors(out: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|j| j["severity"] == "error")
        .collect()
}

const ENGINES: &[&[&str]] = &[&[], &["--interp"]];

/// (2026 program, the replacement named by the hint). Each program is
/// valid edition 2026 code that prints `ok`.
const REMOVED_FORMS: &[(&str, &str)] = &[
    ("set x to 1\nprintln(\"ok\")", "let x = ..."),
    ("set mut x to 1\nprintln(\"ok\")", "let mut x = ..."),
    ("let mut x = 1\nchange x to 2\nprintln(\"ok\")", "x = ..."),
    ("define f() { return 1 }\nprintln(\"ok\")", "fn f(...)"),
    ("forge f() { return 1 }\nprintln(\"ok\")", "async fn f(...)"),
    ("if false { } otherwise { println(\"ok\") }", "else"),
    ("if false { } nah { println(\"ok\") }", "else"),
    (
        "fn f() { return 1 }\nlet v = hold f()\nprintln(\"ok\")",
        "await expr",
    ),
    ("fn g() { emit 1 }\nprintln(\"ok\")", "yield value"),
    (
        "fn g() { grab r from \"http://127.0.0.1:1\" }\nprintln(\"ok\")",
        "let r = fetch(url)",
    ),
    (
        "repeat 1 times { println(\"ok\") }",
        "for _ in range(n) { }",
    ),
    (
        "unpack {a} from {a: 1}\nprintln(\"ok\")",
        "let {...} = value",
    ),
    (
        "unpack [a, ...r] from [1]\nprintln(\"ok\")",
        "let [...] = value",
    ),
    ("safe { println(\"ok\") }", "try { ... } catch e { }"),
    (
        "let v = safe { 1 }\nprintln(\"ok\")",
        "try { ... } catch e { }",
    ),
    ("let v = must Ok(1)\nprintln(\"ok\")", "unwrap(expr)"),
    ("check 1 > 0\nprintln(\"ok\")", "assert(condition, message)"),
    ("say \"ok\"", "println(value)"),
    ("yell \"ok\"", "println(upper(str(value)))"),
    ("whisper \"OK\"", "println(lower(str(value)))"),
    (
        "let v = when 1 { < 0 -> 0, otherwise -> 1 }\nprintln(\"ok\")",
        "else ->",
    ),
];

#[test]
fn removed_forms_are_syntax_errors_in_2027_and_run_in_2026() {
    let mut failures = Vec::new();
    for (source, replacement) in REMOVED_FORMS {
        for engine in ENGINES {
            let mut old: Vec<&str> = engine.to_vec();
            old.push("run");
            let out = run(source, &old);
            let stdout = String::from_utf8_lossy(&out.stdout).to_lowercase();
            if !out.status.success() || stdout.trim() != "ok" {
                failures.push(format!(
                    "2026 {:?} {:?}: {:?} {}",
                    engine,
                    source,
                    stdout,
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
            let mut new: Vec<&str> = engine.to_vec();
            new.extend(["--edition", "2027", "run"]);
            let out = run(source, &new);
            match errors(&out).as_slice() {
                [e] if e["code"] == "E0035"
                    && e["hint"].as_str().is_some_and(|h| h.contains(replacement)) => {}
                other => failures.push(format!(
                    "2027 {:?} {:?}: expected E0035 naming `{}`, got {:?}",
                    engine, source, replacement, other
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn removed_words_are_identifiers_in_2027() {
    let source = "let set = 1\nlet check = 2\nfn hold(x) { return x }\nlet o = { safe: 3 }\nprintln(set + check + hold(4) + o.safe)";
    for engine in ENGINES {
        let mut args: Vec<&str> = engine.to_vec();
        args.extend(["--edition", "2027", "run"]);
        let out = run(source, &args);
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "10\n",
            "{:?}: {}",
            engine,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

const REMOVED_BUILTINS: &[(&str, &str)] = &[
    ("say(\"x\")", "println(x)"),
    ("yell(\"x\")", "println(upper(str(x)))"),
    ("whisper(\"x\")", "println(lower(str(x)))"),
    ("sus(1)", "println(x)"),
    ("bruh(\"x\")", "assert(false, message)"),
    ("bet(true)", "assert(condition, message)"),
    ("no_cap(1, 1)", "assert_eq(a, b)"),
    ("ick(false)", "assert(!condition, message)"),
    ("cook(fn() { 1 })", "time.now()"),
    ("yolo(fn() { 1 })", "try { f() } catch e { }"),
    ("ghost(fn() { 1 })", "f()"),
    ("slay(fn() { 1 }, 1)", "time.now()"),
];

#[test]
fn removed_builtins_are_unknown_in_2027() {
    let mut failures = Vec::new();
    for (call, replacement) in REMOVED_BUILTINS {
        let name = &call[..call.find('(').unwrap_or(0)];
        let expected = format!("undefined variable: '{}'", name);
        for engine in ENGINES {
            let mut args: Vec<&str> = engine.to_vec();
            args.extend(["--edition", "2027", "run"]);
            let out = run(call, &args);
            match errors(&out).as_slice() {
                [e] if e["code"] == "E0003"
                    && e["message"] == expected.as_str()
                    && e["hint"].as_str().is_some_and(|h| {
                        h.contains("was removed in edition 2027") && h.contains(replacement)
                    }) => {}
                other => failures.push(format!("2027 {:?} {}: got {:?}", engine, call, other)),
            }
            // Edition 2026 keeps them (bruh fails on purpose).
            let mut args: Vec<&str> = engine.to_vec();
            args.push("run");
            let out = run(call, &args);
            if out.status.success() == name.starts_with("bruh") {
                failures.push(format!("2026 {:?} {}: {:?}", engine, call, out));
            }
        }
        // The type checker reports the name as unknown, with the same hint.
        let out = run(call, &["--edition", "2027", "check"]);
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let warned = text.lines().any(|l| {
            serde_json::from_str::<serde_json::Value>(l).is_ok_and(|j| {
                j["code"] == "T0006"
                    && j["hint"]
                        .as_str()
                        .is_some_and(|h| h.contains("was removed in edition 2027"))
            })
        });
        if !warned {
            failures.push(format!("check 2027 {}: no T0006 with hint: {}", call, text));
        }
        let out = run(call, &["check"]);
        if String::from_utf8_lossy(&out.stdout).contains("T0006")
            || String::from_utf8_lossy(&out.stderr).contains("T0006")
        {
            failures.push(format!("check 2026 {}: unexpected T0006", call));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Natural and query words are soft keywords in every edition: names
/// outside their constructs (parser/soft.rs).
#[test]
fn soft_keywords_are_names_in_every_edition() {
    let source = "let order = 1\nlet from = 2\nlet emit = 3\nlet to = 4\nfn safe(x) { return x * 10 }\nlet limit = { where: 5, select: 6 }\nlet mut times = 0\ntimes += 1\nprintln(order + from + emit + to, safe(2), limit.where + limit.select, times)";
    for edition in ["2026", "2027"] {
        for engine in ENGINES {
            let mut args: Vec<&str> = engine.to_vec();
            args.extend(["--edition", edition, "run"]);
            let out = run(source, &args);
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                "10 20 11 1\n",
                "{} {:?}: {}",
                edition,
                engine,
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

/// The constructs of those words still parse and run (edition 2026).
#[test]
fn soft_keyword_constructs_still_work() {
    let source = "let xs = [{ n: 1 }, { n: 5 }]\nlet big = xs where n > 2\nprintln(len(big))\nretry 2 times { println(\"r\") }\nsafe {\n    println(\"s\")\n}\nlet v = must Ok(7)\nlet label = when v { < 5 -> \"low\", otherwise -> \"high\" }\nif v > 9 { println(\"no\") } otherwise { println(label) }\nset w to v\ncheck w > 0\nrepeat 1 times { say w }\nunpack {n} from xs[1]\nsay n";
    for engine in ENGINES {
        let mut args: Vec<&str> = engine.to_vec();
        args.push("run");
        let out = run(source, &args);
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "1\nr\ns\nhigh\n7\n5\n",
            "{:?}: {}",
            engine,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
