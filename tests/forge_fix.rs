//! `forge fix --edition 2027` end to end (RFC 0008 section 5).
//!
//! * Golden tests: every `tests/fix/<name>.fg` (edition 2026 input) is
//!   rewritten to exactly `tests/fix/<name>.expected` (edition 2027), and
//!   fixing the result again changes nothing.
//! * Property test over the repository corpus (`examples/*.fg`,
//!   `tests/parity/supported/*.fg` with the modules they import, and the
//!   `tests/*.fg` suites): a copy is fixed in one `forge fix` run; every
//!   file the fixer fully rewrote parses under edition 2027, is a fixed
//!   point, and — for programs that run deterministically (the
//!   `tests/engine_diff_known.txt` skip list is honoured) — prints exactly
//!   the same stdout under `--edition 2027` as the original under 2026.
//!   Test suites are compared through the same PASS/FAIL driver as
//!   `tests/engine_diff.rs`.
//! * CLI behaviour: `--check`, `--diff`, `--set-edition`, and the reports
//!   for forms that cannot be rewritten.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");
const TIMEOUT: Duration = Duration::from_secs(30);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forge-fix-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn forge(cwd: &Path, args: &[&str]) -> Output {
    Command::new(FORGE)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .env("NO_COLOR", "1")
        .output()
        .expect("run forge")
}

fn fg_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|ext| ext == "fg"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

#[test]
fn golden_files() {
    let dir = root().join("tests/fix");
    let inputs = fg_files(&dir);
    assert!(inputs.len() >= 3, "tests/fix/*.fg not found");
    let work = scratch("golden");
    for input in inputs {
        let name = input.file_name().unwrap().to_string_lossy().to_string();
        let expected = fs::read_to_string(input.with_extension("expected"))
            .unwrap_or_else(|_| panic!("{} has no .expected file", name));
        let copy = work.join(&name);
        fs::copy(&input, &copy).expect("copy");
        let out = forge(&work, &["fix", "--edition", "2027", &name]);
        assert!(
            out.status.success(),
            "{}: {}",
            name,
            String::from_utf8_lossy(&out.stderr)
        );
        let fixed = fs::read_to_string(&copy).expect("read fixed");
        assert_eq!(fixed, expected, "{}: fixed output differs", name);
        // Idempotent: nothing left to fix.
        let again = forge(&work, &["fix", "--check", &name]);
        assert!(again.status.success(), "{}: not a fixed point", name);
        // The result is edition 2027 code.
        let check = forge(&work, &["--edition", "2027", "check", &name]);
        assert!(
            check.status.success(),
            "{}: does not check under 2027: {}",
            name,
            String::from_utf8_lossy(&check.stderr)
        );
    }
    let _ = fs::remove_dir_all(&work);
}

#[test]
fn check_diff_and_set_edition() {
    let work = scratch("cli");
    fs::write(work.join("forge.toml"), "[project]\nname = \"demo\"\n").unwrap();
    fs::write(work.join("main.fg"), "set x to 1 // keep\nsay x\n").unwrap();

    let check = forge(&work, &["fix", "--check", "main.fg"]);
    assert_eq!(check.status.code(), Some(1), "--check must fail");
    assert!(String::from_utf8_lossy(&check.stdout).contains("would fix  main.fg"));

    let diff = forge(&work, &["fix", "--diff", "main.fg"]);
    let text = String::from_utf8_lossy(&diff.stdout);
    assert!(text.contains("-set x to 1 // keep\n"), "{}", text);
    assert!(text.contains("+let x = 1 // keep\n"), "{}", text);
    assert!(text.contains("+println(x)\n"), "{}", text);
    // Neither mode writes.
    assert!(fs::read_to_string(work.join("main.fg"))
        .unwrap()
        .starts_with("set x"));

    let out = forge(&work, &["fix", "--edition", "2027", "--set-edition", "."]);
    assert!(out.status.success(), "{:?}", out);
    assert_eq!(
        fs::read_to_string(work.join("main.fg")).unwrap(),
        "let x = 1 // keep\nprintln(x)\n"
    );
    assert_eq!(
        fs::read_to_string(work.join("forge.toml")).unwrap(),
        "[project]\nedition = \"2027\"\nname = \"demo\"\n"
    );
    // The project now runs under 2027 without a flag.
    let run = forge(&work, &["run", "main.fg"]);
    assert_eq!(String::from_utf8_lossy(&run.stdout), "1\n");

    // Only 2026 -> 2027 exists.
    let wrong = forge(&work, &["fix", "--edition", "2026", "main.fg"]);
    assert!(!wrong.status.success());
    let _ = fs::remove_dir_all(&work);
}

#[test]
fn unrewritable_forms_are_reported_and_kept() {
    let work = scratch("manual");
    let src = "let v = safe { 1 }\nsus(2)\nset y to 3\n";
    fs::write(work.join("a.fg"), src).unwrap();
    let out = forge(&work, &["fix", "a.fg"]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("a.fg:1:9: cannot rewrite `safe { }`"),
        "{}",
        stderr
    );
    assert!(stderr.contains("a.fg:2:1: `sus` was removed"), "{}", stderr);
    assert_eq!(
        fs::read_to_string(work.join("a.fg")).unwrap(),
        "let v = safe { 1 }\nsus(2)\nlet y = 3\n"
    );
    let _ = fs::remove_dir_all(&work);
}

// ---------------------------------------------------------------------------
// Corpus property test

/// Programs whose output legitimately changes after `forge fix`, because a
/// replacement is not exactly equivalent (docs/editions/2027.md lists the
/// differences).
/// Files that use a 2027 *semantic* change `forge fix` does not rewrite
/// (it migrates syntax only): they are syntactically 2027 after fixing but
/// fail the 2027 type check on purpose.
const NOT_2027_CODE: &[(&str, &str)] = &[
    (
        "tests/parity/supported/string_ordering.fg",
        "`\"n\" + 1`: strict `+` (E0034) is a semantic change",
    ),
    (
        "tests/safety_test.fg",
        "`1 + \"x\"` inside `safe`: strict `+` (E0034) is a semantic change",
    ),
    (
        "tests/string_concat_2026_test.fg",
        "pins edition 2026 string + value conversion",
    ),
];

const KNOWN_DIFFERENCES: &[(&str, &str)] = &[
    (
        "examples/squad.fg",
        "`must null` and `unwrap(null)` fail with different messages",
    ),
    (
        "tests/parity/supported/must_passes_through.fg",
        "`must` passes a plain value through; `unwrap` requires a Result or Option",
    ),
];

fn skipped_programs() -> BTreeSet<String> {
    let text = fs::read_to_string(root().join("tests/engine_diff_known.txt")).expect("known list");
    text.lines()
        .filter_map(|l| {
            let line = l.split('#').next()?.trim();
            let (kind, path) = line.split_once(char::is_whitespace)?;
            (kind == "skip").then(|| path.trim().to_string())
        })
        .collect()
}

/// Names of functions carrying `decorator` (same rule as engine_diff.rs).
fn decorated_functions(source: &str, decorator: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut pending = false;
    let mut skipped = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('@') {
            let name = rest
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("");
            pending |= name == decorator;
            skipped |= name == "skip";
            continue;
        }
        if pending {
            for keyword in ["fn ", "define ", "async fn ", "forge "] {
                if let Some(rest) = trimmed.strip_prefix(keyword) {
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !name.is_empty() && !skipped {
                        names.push(name);
                    }
                    break;
                }
            }
        }
        if !trimmed.is_empty() && !trimmed.starts_with("//") {
            pending = false;
            skipped = false;
        }
    }
    names
}

fn test_driver(source: &str) -> String {
    let tests = decorated_functions(source, "test");
    let before = decorated_functions(source, "before");
    let after = decorated_functions(source, "after");
    let mut driver = String::from(source);
    driver.push_str("\n\n// ---- forge_fix driver ----\n");
    for test in tests {
        for hook in &before {
            driver.push_str(&format!("{}()\n", hook));
        }
        driver.push_str(&format!(
            "try {{\n    {test}()\n    println(\"PASS {test}\")\n}} catch err {{\n    println(\"FAIL {test}: {{err}}\")\n}}\n"
        ));
        for hook in &after {
            driver.push_str(&format!("{}()\n", hook));
        }
    }
    driver
}

struct Run {
    stdout: String,
    success: bool,
}

fn run_program(program: &Path, edition: &str) -> Run {
    let mut child = Command::new(FORGE)
        .args(["--edition", edition, "run"])
        .arg(program)
        .current_dir(root())
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn forge");
    let mut stdout = child.stdout.take().expect("stdout");
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait") {
            break Some(status);
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Run {
        stdout: reader.join().unwrap_or_default(),
        success: status.is_some_and(|s| s.success()),
    }
}

#[test]
fn fixed_corpus_parses_under_2027_and_behaves_the_same() {
    let root = root();
    let work = scratch("corpus");
    // Drivers of the original (2026) suites live outside the fixed tree.
    let drivers = scratch("corpus-drivers");
    // (corpus path, original program, copy that gets fixed)
    let mut programs: Vec<(String, PathBuf, PathBuf)> = Vec::new();
    for dir in [
        "examples",
        "tests/parity/supported",
        "tests/parity/modules",
        "tests",
    ] {
        fs::create_dir_all(work.join(dir)).expect("mkdir");
        for file in fg_files(&root.join(dir)) {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let rel = format!("{}/{}", dir, name);
            let source = fs::read_to_string(&file).expect("read");
            let (original, copy) = if dir == "tests" {
                // Test suites run through a PASS/FAIL driver.
                let original = drivers.join(&name);
                fs::write(&original, test_driver(&source)).expect("write driver");
                (original, work.join(&rel))
            } else {
                (file.clone(), work.join(&rel))
            };
            fs::write(&copy, &source).expect("copy");
            if dir != "tests/parity/modules" {
                programs.push((rel, original, copy));
            }
        }
    }

    // One fixer run over the whole copy.
    let out = forge(&work, &["fix", "--edition", "2027", "."]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let mut reported: BTreeMap<String, usize> = BTreeMap::new();
    for line in stderr.lines() {
        if let Some(path) = line.split(':').next() {
            let path = path.trim_start_matches("./");
            *reported.entry(path.to_string()).or_default() += 1;
        }
    }

    let skip = skipped_programs();
    let mut problems = Vec::new();
    let (mut compared, mut fully_fixed) = (0, 0);
    for (rel, original, copy) in &programs {
        if reported.contains_key(rel.as_str()) {
            continue;
        }
        fully_fixed += 1;
        let copy_str = copy.to_string_lossy().to_string();
        if NOT_2027_CODE.iter().any(|(p, _)| p == rel) {
            continue;
        }
        let parses = forge(&work, &["--edition", "2027", "check", &copy_str]);
        if !parses.status.success() {
            problems.push(format!(
                "{}: fixed file does not parse under 2027: {}",
                rel,
                String::from_utf8_lossy(&parses.stderr)
            ));
            continue;
        }
        let again = forge(&work, &["fix", "--check", &copy_str]);
        if !again.status.success() {
            problems.push(format!("{}: fixing the output changes it again", rel));
        }
        if skip.contains(rel) || KNOWN_DIFFERENCES.iter().any(|(p, _)| p == rel) {
            continue;
        }
        let fixed_program = if rel.starts_with("tests/") && !rel.starts_with("tests/parity/") {
            let driver = copy.with_extension("driver.fg");
            let fixed = fs::read_to_string(copy).expect("read fixed");
            fs::write(&driver, test_driver(&fixed)).expect("write driver");
            driver
        } else {
            copy.clone()
        };
        let before = run_program(original, "2026");
        let after = run_program(&fixed_program, "2027");
        compared += 1;
        if before.stdout != after.stdout || before.success != after.success {
            let diff = before
                .stdout
                .lines()
                .zip(after.stdout.lines())
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| format!("line {}: {:?} vs {:?}", i + 1, a, b))
                .unwrap_or_else(|| {
                    format!(
                        "success {} vs {}, {} vs {} lines",
                        before.success,
                        after.success,
                        before.stdout.lines().count(),
                        after.stdout.lines().count()
                    )
                });
            problems.push(format!("{}: output differs after fixing ({})", rel, diff));
        }
    }
    eprintln!(
        "forge fix corpus: {} programs, {} fully rewritten, {} with forms left to rewrite by hand {:?}, {} compared",
        programs.len(),
        fully_fixed,
        reported.len(),
        reported.keys().collect::<Vec<_>>(),
        compared
    );
    let _ = fs::remove_dir_all(&work);
    let _ = fs::remove_dir_all(&drivers);
    // Most of the corpus must be rewritable mechanically.
    assert!(
        fully_fixed * 10 >= programs.len() * 9,
        "only {} of {} files fully rewritten",
        fully_fixed,
        programs.len()
    );
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
