//! Cross-engine differential test: the interpreter and the bytecode VM must
//! produce identical stdout and exit status for every program in the corpus.
//!
//! Corpus:
//! * `examples/*.fg`
//! * `tests/parity/supported/*.fg`
//! * `tests/*.fg` — `forge test` suites. Each is turned into a driver
//!   program that calls every `@test` function (with its `@before` /
//!   `@after` hooks) inside `try`/`catch` and prints `PASS`/`FAIL` lines,
//!   so the comparison covers the test bodies, not just the definitions.
//!
//! Programs that cannot be compared (servers, network, databases,
//! non-deterministic output) and divergences that are known but not fixed
//! yet are listed in `tests/engine_diff_known.txt`. The test fails when
//!
//! * a program not listed there diverges (a new parity bug), or
//! * a program listed as `diverges` now matches (remove it from the list).
//!
//! Run it with:
//!
//! ```text
//! cargo test --test engine_diff -- --nocapture
//! ```
//!
//! Set `FORGE_ENGINE_DIFF_FILTER=<substring>` to run a subset, and
//! `FORGE_ENGINE_DIFF_VERBOSE=1` to print both outputs for each divergence.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expectation {
    /// Not compared (nondeterministic or needs external services).
    Skip,
    /// Known divergence; must still diverge.
    Diverges,
}

struct Outcome {
    stdout: String,
    success: bool,
    fell_back: bool,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn load_expectations() -> BTreeMap<String, Expectation> {
    let path = repo_root().join("tests/engine_diff_known.txt");
    let text = fs::read_to_string(&path).expect("tests/engine_diff_known.txt must exist");
    let mut map = BTreeMap::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (kind, path) = line.split_once(char::is_whitespace).unwrap_or_else(|| {
            panic!(
                "engine_diff_known.txt:{}: expected '<kind> <path>'",
                lineno + 1
            )
        });
        let expectation = match kind {
            "skip" => Expectation::Skip,
            "diverges" => Expectation::Diverges,
            other => panic!(
                "engine_diff_known.txt:{}: unknown kind '{}' (use skip/diverges)",
                lineno + 1,
                other
            ),
        };
        map.insert(path.trim().to_string(), expectation);
    }
    map
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

/// Names of functions carrying `decorator` (`@test`, `@before`, ...).
fn decorated_functions(source: &str, decorator: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut pending = false;
    let mut skipped = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('@') {
            let name = trimmed[1..]
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("");
            if name == decorator {
                pending = true;
            }
            if name == "skip" {
                skipped = true;
            }
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

/// Turn a `forge test` suite into a program that runs every test.
fn test_driver(source: &str) -> String {
    let tests = decorated_functions(source, "test");
    let before = decorated_functions(source, "before");
    let after = decorated_functions(source, "after");
    let mut driver = String::from(source);
    driver.push_str("\n\n// ---- engine_diff driver ----\n");
    for test in tests {
        for hook in &before {
            driver.push_str(&format!("{}()\n", hook));
        }
        driver.push_str(&format!(
            "try {{\n    {test}()\n    println(\"PASS {test}\")\n}} catch err {{\n    println(\"FAIL {test}: {{err}}\")\n}}\n",
            test = test
        ));
        for hook in &after {
            driver.push_str(&format!("{}()\n", hook));
        }
    }
    driver
}

fn run_engine(program: &Path, interp: bool) -> Outcome {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forge"));
    if interp {
        cmd.arg("--interp");
    }
    cmd.arg("run")
        .arg(program)
        .current_dir(repo_root())
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn forge");
    let mut stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let out_reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf);
        buf
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait forge") {
            break Some(status);
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    Outcome {
        stdout: if status.is_none() {
            format!("{}\n<timed out>", stdout)
        } else {
            stdout
        },
        success: status.is_some_and(|s| s.success()),
        fell_back: stderr.contains("falling back to interpreter"),
    }
}

#[test]
fn interpreter_and_vm_agree_on_corpus() {
    let root = repo_root();
    let expectations = load_expectations();
    let filter = std::env::var("FORGE_ENGINE_DIFF_FILTER").ok();
    let verbose = std::env::var("FORGE_ENGINE_DIFF_VERBOSE").is_ok();

    let scratch = std::env::temp_dir().join(format!("forge-engine-diff-{}", std::process::id()));
    fs::create_dir_all(&scratch).expect("create scratch dir");

    // (display path, program to run)
    let mut corpus: Vec<(String, PathBuf)> = Vec::new();
    for dir in ["examples", "tests/parity/supported"] {
        for file in fg_files(&root.join(dir)) {
            let rel = format!("{}/{}", dir, file.file_name().unwrap().to_string_lossy());
            corpus.push((rel, file));
        }
    }
    for file in fg_files(&root.join("tests")) {
        let rel = format!("tests/{}", file.file_name().unwrap().to_string_lossy());
        let source = fs::read_to_string(&file).expect("read test suite");
        let driver = scratch.join(file.file_name().unwrap());
        fs::write(&driver, test_driver(&source)).expect("write driver");
        corpus.push((rel, driver));
    }

    let mut new_divergences = Vec::new();
    let mut fixed = Vec::new();
    let mut compared = 0;
    let mut fallbacks = Vec::new();
    for (rel, program) in &corpus {
        if filter.as_ref().is_some_and(|f| !rel.contains(f.as_str())) {
            continue;
        }
        let expectation = expectations.get(rel).copied();
        if expectation == Some(Expectation::Skip) {
            continue;
        }
        let interp = run_engine(program, true);
        let vm = run_engine(program, false);
        compared += 1;
        if vm.fell_back {
            fallbacks.push(rel.clone());
        }
        let same = interp.stdout == vm.stdout && interp.success == vm.success;
        match (same, expectation) {
            (false, None) => {
                if verbose {
                    eprintln!(
                        "--- {} (interp, success={})\n{}\n--- {} (vm, success={})\n{}",
                        rel, interp.success, interp.stdout, rel, vm.success, vm.stdout
                    );
                }
                new_divergences.push(first_difference(rel, &interp, &vm));
            }
            (true, Some(Expectation::Diverges)) => fixed.push(rel.clone()),
            _ => {}
        }
    }
    let _ = fs::remove_dir_all(&scratch);

    eprintln!(
        "engine_diff: compared {} programs ({} ran on the interpreter via VM fallback: {:?})",
        compared,
        fallbacks.len(),
        fallbacks
    );
    let mut problems = String::new();
    if !new_divergences.is_empty() {
        problems.push_str(&format!(
            "{} program(s) behave differently on the interpreter and the VM:\n{}\n",
            new_divergences.len(),
            new_divergences.join("\n")
        ));
    }
    if !fixed.is_empty() {
        problems.push_str(&format!(
            "now identical on both engines; remove from tests/engine_diff_known.txt: {:?}\n",
            fixed
        ));
    }
    assert!(problems.is_empty(), "{}", problems);
}

/// Imports resolve relative to the importing file (not the working
/// directory) on both engines, including nested imports, and an imported
/// function can still call its module's private helpers.
#[test]
fn imports_resolve_relative_to_importing_file() {
    let dir = std::env::temp_dir().join(format!("forge-import-rel-{}", std::process::id()));
    let app = dir.join("app");
    fs::create_dir_all(app.join("lib")).expect("create dirs");
    fs::write(
        app.join("main.fg"),
        "import { greet } from \"lib/util.fg\"\nprintln(greet())\n",
    )
    .expect("write main");
    fs::write(
        app.join("lib/util.fg"),
        "import { suffix } from \"inner.fg\"\nfn helper() { return \"hi\" }\nfn greet() { return helper() + suffix() }\n",
    )
    .expect("write util");
    fs::write(app.join("lib/inner.fg"), "fn suffix() { return \"!\" }\n").expect("write inner");

    for interp in [true, false] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_forge"));
        if interp {
            cmd.arg("--interp");
        }
        // Run from the temp root, so CWD-relative resolution would fail.
        let output = cmd
            .arg("run")
            .arg("app/main.fg")
            .current_dir(&dir)
            .output()
            .expect("run forge");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "hi!",
            "interp={} stderr={}",
            interp,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

fn first_difference(rel: &str, interp: &Outcome, vm: &Outcome) -> String {
    if interp.success != vm.success {
        let tail = |s: &str| s.lines().last().unwrap_or("").to_string();
        return format!(
            "  {}: exit success interp={} vm={} (last stdout line: interp={:?} vm={:?})",
            rel,
            interp.success,
            vm.success,
            tail(&interp.stdout),
            tail(&vm.stdout)
        );
    }
    let mut il = interp.stdout.lines();
    let mut vl = vm.stdout.lines();
    let mut line = 1;
    loop {
        match (il.next(), vl.next()) {
            (Some(a), Some(b)) if a == b => line += 1,
            (a, b) => return format!("  {}: stdout line {}: interp={:?} vm={:?}", rel, line, a, b),
        }
    }
}
