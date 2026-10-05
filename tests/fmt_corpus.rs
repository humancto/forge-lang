//! `forge fmt` must never change what a program does.
//!
//! Every `examples/*.fg` and `tests/*.fg` file is copied to a temporary
//! directory and formatted there with the real `forge fmt` binary. Then:
//!
//! * formatting the copy again is a no-op (`forge fmt --check` passes);
//! * each runnable example prints the same stdout and exits with the same
//!   status as the original, on both engines (`--interp` and the VM);
//! * the formatted `tests/*.fg` suite passes exactly the same tests as the
//!   original under `forge test --engine both`.
//!
//! (The formatter's unit tests additionally check that every corpus file
//! keeps its token stream and AST.)

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");
const TIMEOUT: Duration = Duration::from_secs(60);

/// Examples that cannot run unattended (same list as tools/run_examples.sh).
const SKIP: &[&str] = &[
    "api.fg",
    "bench_server.fg",
    "bench_server_closure.fg",
    "bench_server_concurrent.fg",
    "bench_client.fg",
    "fetch_demo.fg",
    "mysql_demo.fg",
];
const ALLOW_RUN: &[&str] = &["devops.fg", "showcase.fg"];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn fg_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "fg"))
        .collect();
    files.sort();
    files
}

/// Run `forge` from the repository root with stdin closed and a time limit.
fn forge(args: &[&str]) -> Output {
    let mut child = Command::new(FORGE)
        .args(args)
        .current_dir(root())
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn forge");
    let mut stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).ok();
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        stderr.read_to_end(&mut buf).ok();
        buf
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait") {
            break status;
        }
        if start.elapsed() > TIMEOUT {
            child.kill().ok();
            panic!("forge {:?} timed out", args);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: out_reader.join().expect("stdout reader"),
        stderr: err_reader.join().expect("stderr reader"),
    }
}

/// Copy `files` into `dir` and format the copies with `forge fmt`.
fn formatted_copies(files: &[PathBuf], dir: &Path) -> Vec<PathBuf> {
    fs::create_dir_all(dir).unwrap();
    let copies: Vec<PathBuf> = files
        .iter()
        .map(|f| {
            let copy = dir.join(f.file_name().unwrap());
            fs::copy(f, &copy).unwrap();
            copy
        })
        .collect();
    let mut args = vec!["fmt".to_string()];
    args.extend(copies.iter().map(|c| c.to_string_lossy().into_owned()));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = forge(&args);
    assert!(out.status.success(), "forge fmt failed: {:?}", out);

    let mut check = vec!["fmt", "--check"];
    check.extend(args[1..].iter().copied());
    let out = forge(&check);
    assert!(
        out.status.success(),
        "forge fmt is not idempotent:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
    copies
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("forge-fmt-corpus-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[test]
fn formatted_examples_behave_identically_on_both_engines() {
    let originals: Vec<PathBuf> = fg_files(&root().join("examples"))
        .into_iter()
        .filter(|p| !SKIP.contains(&p.file_name().unwrap().to_str().unwrap()))
        .collect();
    assert!(originals.len() >= 10, "examples not found");
    let dir = scratch_dir("examples");
    let copies = formatted_copies(&originals, &dir);

    let mut failures = Vec::new();
    for (original, copy) in originals.iter().zip(&copies) {
        let name = original.file_name().unwrap().to_str().unwrap();
        for engine in [Some("--interp"), None] {
            let mut flags: Vec<&str> = Vec::new();
            if ALLOW_RUN.contains(&name) {
                flags.push("--allow-run");
            }
            flags.extend(engine);
            let run = |path: &Path| {
                let mut args = flags.clone();
                let path = path.to_string_lossy().into_owned();
                args.push("run");
                args.push(&path);
                forge(&args)
            };
            let first = run(original);
            let second = run(original);
            let formatted = run(copy);
            let engine = engine.unwrap_or("--vm");
            if formatted.status.code() != first.status.code() {
                failures.push(format!(
                    "{} [{}]: exit {:?} after formatting, {:?} before\n{}",
                    name,
                    engine,
                    formatted.status.code(),
                    first.status.code(),
                    String::from_utf8_lossy(&formatted.stderr)
                ));
                continue;
            }
            if first.stdout == second.stdout {
                if formatted.stdout != first.stdout {
                    failures.push(format!(
                        "{} [{}]: stdout changed after formatting",
                        name, engine
                    ));
                }
            } else {
                // Nondeterministic output (random data, timings): compare shape.
                let lines = |o: &Output| String::from_utf8_lossy(&o.stdout).lines().count();
                if lines(&formatted) != lines(&first) {
                    failures.push(format!(
                        "{} [{}]: output line count changed after formatting",
                        name, engine
                    ));
                }
            }
        }
    }
    let _ = fs::remove_dir_all(&dir);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Per-test result lines of `forge test`, without timings and directories.
fn test_results(out: &Output, dir: &Path) -> Vec<String> {
    let dir = dir.to_string_lossy().into_owned();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|line| {
            let line = line.replace(&dir, "<dir>");
            match line.rfind(" (") {
                Some(i) if line.ends_with("ms)") => line[..i].to_string(),
                _ => line,
            }
        })
        .filter(|line| !line.trim().is_empty())
        .collect()
}

#[test]
fn formatted_test_suite_passes_the_same_tests() {
    let originals = fg_files(&root().join("tests"));
    assert!(originals.len() >= 20, "tests/*.fg not found");
    let dir = scratch_dir("tests");
    formatted_copies(&originals, &dir);

    let tests_dir = root().join("tests");
    let run = |d: &Path| {
        forge(&[
            "--allow-run",
            "test",
            "--engine",
            "both",
            &d.to_string_lossy(),
        ])
    };
    let before = run(&tests_dir);
    let after = run(&dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(before.status.code(), after.status.code());
    // File headers name files by their base name; the directories only
    // appear in error paths, which are mapped to `<dir>`.
    assert_eq!(
        test_results(&before, &tests_dir),
        test_results(&after, &dir)
    );
}
