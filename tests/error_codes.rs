//! Stable error codes, end to end.
//!
//! * Every runtime/syntax code (`E....`, listed by `forge explain`) has a
//!   fixture `tests/errors/<CODE>_<name>.fg` that makes a program fail with
//!   exactly that code — on the VM (the default engine) *and* on the
//!   interpreter, with the same message. Codes no script can trigger
//!   deterministically are listed in `UNREACHABLE_FROM_FIXTURES` with the
//!   reason.
//! * `forge explain <CODE>` documents every code (E and T) with an example
//!   and a fix.
//! * `--error-format json` and `forge check --format json` emit one JSON
//!   object per diagnostic with the documented fields.
//!
//! A fixture's leading comment lines may carry extra CLI flags
//! (`// forge-args: --strict`) and environment variables
//! (`// forge-env: FORGE_HTTP_ALLOW_PRIVATE=1`). Every fixture also gets
//! `FORGE_TEST_SILENT_ADDR`: a loopback address that accepts connections
//! and never answers (for the HTTP client timeout, E0036).

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");

/// Codes without a fixture, and why.
const UNREACHABLE_FROM_FIXTURES: &[(&str, &str)] = &[(
    "E0033",
    "internal stream re-entrancy guard; not reachable from a single-threaded script",
)];

/// Fixtures whose two engines agree on the code but word the message
/// differently (the interpreter knows more context).
const MESSAGE_MAY_DIFFER: &[(&str, &str)] = &[(
    "E0029",
    "the interpreter names the frozen variable and field; the VM's SetField does not know them",
)];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn forge(cwd: &Path, args: &[&str]) -> Output {
    forge_with_env(cwd, args, &[])
}

fn forge_with_env(cwd: &Path, args: &[&str], envs: &[(String, String)]) -> Output {
    let mut cmd = Command::new(FORGE);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .env("NO_COLOR", "1")
        .env_remove("FORGE_MAX_DEPTH");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run forge")
}

/// The values of a fixture's leading `// <key>: ...` comment lines.
fn header_values<'a>(source: &'a str, key: &str) -> Vec<&'a str> {
    source
        .lines()
        .take_while(|l| l.starts_with("//"))
        .filter_map(|l| l.split(key).nth(1))
        .flat_map(|v| v.split_whitespace())
        .collect()
}

/// `(code, title)` for every code `forge explain` lists, in order.
fn listed_codes() -> Vec<(String, String)> {
    let out = forge(&root(), &["explain"]);
    assert!(out.status.success(), "{:?}", out);
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut parts = l.trim().splitn(2, "  ");
            let code = parts.next()?.trim();
            let title = parts.next()?.trim();
            let looks_like_code = code.len() == 5
                && (code.starts_with('E') || code.starts_with('T'))
                && code[1..].chars().all(|c| c.is_ascii_digit());
            looks_like_code.then(|| (code.to_string(), title.to_string()))
        })
        .collect()
}

fn fixtures() -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(root().join("tests/errors"))
        .expect("tests/errors exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "fg"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            (name[..5].to_string(), p)
        })
        .collect();
    out.sort();
    out
}

/// The error diagnostics (severity "error") a JSON-format run printed.
fn json_errors(stderr: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|j| j["severity"] == "error")
        .collect()
}

#[test]
fn every_code_is_explained() {
    let codes = listed_codes();
    assert!(codes.iter().any(|(c, _)| c == "E0000"));
    assert!(codes.iter().any(|(c, _)| c == "T0001"));
    for (code, title) in &codes {
        assert!(!title.is_empty(), "{} has no title", code);
        let out = forge(&root(), &["explain", &code.to_lowercase()]);
        assert!(out.status.success(), "forge explain {} failed", code);
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.starts_with(&format!("{}: {}", code, title)),
            "{}",
            text
        );
        assert!(
            text.contains("Example:"),
            "{} has no example:\n{}",
            code,
            text
        );
        assert!(text.contains("Fix"), "{} has no fix:\n{}", code, text);
    }
    let out = forge(&root(), &["explain", "E9999"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown error code"));
}

#[test]
fn every_runtime_code_has_a_fixture() {
    let have: Vec<String> = fixtures().into_iter().map(|(c, _)| c).collect();
    for (code, _) in listed_codes().iter().filter(|(c, _)| c.starts_with('E')) {
        let exempt = UNREACHABLE_FROM_FIXTURES.iter().any(|(c, _)| c == code);
        assert!(
            have.contains(code) || exempt,
            "{} has no fixture in tests/errors/ (add one, or list it in UNREACHABLE_FROM_FIXTURES with a reason)",
            code
        );
        assert!(
            !(have.contains(code) && exempt),
            "{} has a fixture; remove it from UNREACHABLE_FROM_FIXTURES",
            code
        );
    }
}

/// Each fixture fails with its code on both engines, with the same message.
#[test]
fn fixtures_fail_with_their_code_on_both_engines() {
    let dir = root().join("tests/errors");
    // Accepts connections (the kernel completes the handshake for the
    // backlog) but never reads or answers: a request to it times out.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").expect("bind silent listener");
    let silent_addr = silent.local_addr().expect("silent listener address");
    let mut failures = Vec::new();
    for (code, path) in fixtures() {
        let source = std::fs::read_to_string(&path).unwrap();
        let extra = header_values(&source, "forge-args:");
        let mut envs: Vec<(String, String)> = header_values(&source, "forge-env:")
            .into_iter()
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        envs.push((
            "FORGE_TEST_SILENT_ADDR".to_string(),
            silent_addr.to_string(),
        ));
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        let mut seen = Vec::new();
        for engine in [None, Some("--interp")] {
            let mut args: Vec<&str> = engine.into_iter().collect();
            args.extend(["--error-format", "json"]);
            args.extend(extra.iter().copied());
            args.extend(["run", file.as_str()]);
            let out = forge_with_env(&dir, &args, &envs);
            let errors = json_errors(&out.stderr);
            let label = engine.unwrap_or("vm");
            if out.status.success() {
                failures.push(format!("{} ({}): exited 0", file, label));
                continue;
            }
            match errors.as_slice() {
                [only] => {
                    if only["code"] != code.as_str() {
                        failures.push(format!(
                            "{} ({}): expected {}, got {} — {}",
                            file, label, code, only["code"], only["message"]
                        ));
                    }
                    for field in ["message", "file", "hint", "phase"] {
                        if !only[field].is_string() {
                            failures.push(format!("{} ({}): no {}", file, label, field));
                        }
                    }
                    seen.push(only["message"].as_str().unwrap_or("").to_string());
                }
                other => failures.push(format!(
                    "{} ({}): expected one error diagnostic, got {:?}\nstderr: {}",
                    file,
                    label,
                    other,
                    String::from_utf8_lossy(&out.stderr)
                )),
            }
        }
        let may_differ = MESSAGE_MAY_DIFFER.iter().any(|(c, _)| *c == code);
        if seen.len() == 2 && seen[0] != seen[1] && !may_differ {
            failures.push(format!(
                "{}: engines word the error differently:\n  vm:     {}\n  interp: {}",
                file, seen[0], seen[1]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn human_errors_show_code_hint_and_explain_pointer() {
    let dir = root().join("tests/errors");
    let out = forge(&dir, &["run", "E0009_index_out_of_bounds.fg"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("[E0009]"), "{}", stderr);
    assert!(
        stderr.contains("E0009_index_out_of_bounds.fg:2:1"),
        "{}",
        stderr
    );
    assert!(
        stderr.contains("valid indices are 0 to 2 (or -3 to -1 from the end)"),
        "{}",
        stderr
    );
    assert!(stderr.contains("forge explain E0009"), "{}", stderr);
}

#[test]
fn forge_check_reports_without_running() {
    let dir = std::env::temp_dir().join(format!("forge-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Would fail at run time (and print) if `check` ran it.
    std::fs::write(
        dir.join("warn.fg"),
        "say \"ran\"\nlet count = 1\nsay coutn\n",
    )
    .unwrap();
    std::fs::write(dir.join("bad.fg"), "fn f( {\n").unwrap();

    let out = forge(&dir, &["check", "--format", "json", "warn.fg"]);
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("ran"), "check must not run the program");
    let diags: Vec<serde_json::Value> = stdout
        .lines()
        .map(|l| serde_json::from_str(l).expect("one JSON object per line"))
        .collect();
    assert_eq!(diags.len(), 1, "{}", stdout);
    let d = &diags[0];
    assert_eq!(d["code"], "T0006");
    assert_eq!(d["severity"], "warning");
    assert_eq!(d["file"], "warn.fg");
    assert_eq!(d["line"], 3);
    assert_eq!(d["col"], 5);
    assert_eq!(d["hint"], "did you mean 'count'?");
    assert_eq!(d["phase"], "type");

    // --strict turns the warning into an error and a failing exit code.
    let out = forge(&dir, &["--strict", "check", "--format", "json", "warn.fg"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"severity\":\"error\""));

    let out = forge(&dir, &["check", "--format", "json", "bad.fg"]);
    assert!(!out.status.success());
    let d: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    assert_eq!(d["code"], "E0002");
    assert_eq!(d["phase"], "syntax");

    // Human format: the snippet and a summary on stderr.
    let out = forge(&dir, &["check", "warn.fg"]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("[T0006]"), "{}", stderr);
    assert!(
        stderr.contains("warn.fg: 0 errors, 1 warning"),
        "{}",
        stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn caught_errors_expose_the_code() {
    let dir = std::env::temp_dir().join(format!("forge-catch-code-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("c.fg"),
        "try {\n  let a = [1]\n  say a[5]\n} catch e {\n  say e.code\n  say e.type\n}\n",
    )
    .unwrap();
    for engine in [None, Some("--interp")] {
        let mut args: Vec<&str> = engine.into_iter().collect();
        args.extend(["run", "c.fg"]);
        let out = forge(&dir, &args);
        assert!(out.status.success(), "{:?}", out);
        assert_eq!(String::from_utf8_lossy(&out.stdout), "E0009\nIndexError\n");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unknown_edition_is_rejected() {
    let dir = std::env::temp_dir().join(format!("forge-edition-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Valid in every edition (`say` is removed in 2027).
    std::fs::write(dir.join("main.fg"), "println(\"hi\")\n").unwrap();
    std::fs::write(
        dir.join("forge.toml"),
        "[project]\nname = \"e\"\nedition = \"2026\"\nentry = \"main.fg\"\n",
    )
    .unwrap();
    let out = forge(&dir, &["run", "main.fg"]);
    assert!(out.status.success(), "{:?}", out);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");

    std::fs::write(
        dir.join("forge.toml"),
        "[project]\nname = \"e\"\nedition = \"2099\"\n",
    )
    .unwrap();
    for args in [
        &["run", "main.fg"][..],
        &["check", "main.fg"][..],
        &["build", "main.fg"][..],
    ] {
        let out = forge(&dir, args);
        assert!(!out.status.success(), "{:?}", args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("unknown edition \"2099\""), "{}", stderr);
    }
    // `--edition` overrides forge.toml, and is validated the same way.
    let out = forge(&dir, &["--edition", "2026", "run", "main.fg"]);
    assert!(out.status.success(), "{:?}", out);
    let out = forge(&dir, &["--edition", "2027", "run", "main.fg"]);
    assert!(out.status.success(), "{:?}", out);
    let out = forge(&dir, &["--edition", "1999", "run", "main.fg"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown edition \"1999\""));
    // Commands that do not run code still work with a bad manifest.
    let out = forge(&dir, &["version"]);
    assert!(out.status.success(), "{:?}", out);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Edition 2027 strict `+`: a statically known `String + Int` is a type
/// error (T0005) without `--strict`; an unknown one fails at run time with
/// E0034; a compiled `.fgc` keeps the edition it was compiled under.
#[test]
fn edition_2027_strict_string_concat() {
    let dir = std::env::temp_dir().join(format!("forge-strict-concat-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("static.fg"), "println(\"n=\" + 1)\n").unwrap();
    std::fs::write(
        dir.join("dynamic.fg"),
        "fn plus(a, b) { return a + b }\nprintln(plus(\"a\", 1))\n",
    )
    .unwrap();
    let stdout = |out: &Output| String::from_utf8_lossy(&out.stdout).to_string();

    // Edition 2026 (default): implicit conversion, at most a warning.
    for file in ["static.fg", "dynamic.fg"] {
        for engine in [None, Some("--interp")] {
            let mut args: Vec<&str> = engine.into_iter().collect();
            args.extend(["run", file]);
            let out = forge(&dir, &args);
            assert!(out.status.success(), "{:?}", out);
        }
    }
    assert_eq!(stdout(&forge(&dir, &["run", "static.fg"])), "n=1\n");

    // Edition 2027, statically known: T0005 error before running.
    let out = forge(
        &dir,
        &[
            "--edition",
            "2027",
            "--error-format",
            "json",
            "run",
            "static.fg",
        ],
    );
    assert!(!out.status.success());
    assert_eq!(stdout(&out), "", "must not run");
    let errors = json_errors(&out.stderr);
    assert_eq!(errors.len(), 1, "{:?}", errors);
    assert_eq!(errors[0]["code"], "T0005");
    let out = forge(
        &dir,
        &[
            "--edition",
            "2027",
            "check",
            "--format",
            "json",
            "static.fg",
        ],
    );
    let diags: Vec<serde_json::Value> = stdout(&out)
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    assert!(
        diags
            .iter()
            .any(|d| d["code"] == "T0005" && d["severity"] == "error"),
        "{:?}",
        diags
    );

    // Edition 2027, known only at run time: E0034 on both engines.
    for engine in [None, Some("--interp")] {
        let mut args: Vec<&str> = engine.into_iter().collect();
        args.extend([
            "--edition",
            "2027",
            "--error-format",
            "json",
            "run",
            "dynamic.fg",
        ]);
        let out = forge(&dir, &args);
        assert!(!out.status.success(), "{:?}", out);
        let errors = json_errors(&out.stderr);
        assert_eq!(errors.len(), 1, "{:?}", errors);
        assert_eq!(errors[0]["code"], "E0034", "{:?}", errors);
    }

    // `.fgc` files record their edition: the compiled program's meaning
    // does not depend on the edition it is later run under.
    let out = forge(&dir, &["--edition", "2027", "build", "dynamic.fg"]);
    assert!(out.status.success(), "{:?}", out);
    let out = forge(&dir, &["--error-format", "json", "run", "dynamic.fgc"]);
    assert!(!out.status.success(), "{:?}", out);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("cannot add String and Int"),
        "{:?}",
        out
    );
    let out = forge(&dir, &["--edition", "2026", "build", "dynamic.fg"]);
    assert!(out.status.success(), "{:?}", out);
    let out = forge(&dir, &["--edition", "2027", "run", "dynamic.fgc"]);
    assert!(out.status.success(), "{:?}", out);
    assert_eq!(stdout(&out), "a1\n");
    let _ = std::fs::remove_dir_all(&dir);
}
