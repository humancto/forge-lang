//! `--strict`: static type errors stop the run, and annotations are
//! enforced at run time with identical behavior on both engines.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");

fn write(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forge-strict-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write");
    path
}

fn run(engine: &[&str], strict: bool, path: &Path) -> Output {
    let mut args: Vec<&str> = engine.to_vec();
    if strict {
        args.push("--strict");
    }
    let p = path.to_string_lossy().into_owned();
    args.push("run");
    args.push(&p);
    Command::new(FORGE)
        .args(&args)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .expect("run forge")
}

const ENGINES: [&[&str]; 2] = [&[], &["--interp"]];

#[test]
fn wrong_argument_from_dynamic_value_fails_at_run_time_on_both_engines() {
    // `json.parse` returns `Any`, so only the run-time check can catch it.
    let path = write(
        "arg.fg",
        "fn double(n: Int) -> Int { return n * 2 }\nlet v = json.parse(\"\\\"seven\\\"\")\nsay double(v)\n",
    );
    for engine in ENGINES {
        let out = run(engine, true, &path);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{:?}: {}", engine, stderr);
        assert!(
            stderr.contains("type error: argument 'n' of 'double' must be Int, got String"),
            "{:?}: {}",
            engine,
            stderr
        );
        // Without --strict the program runs (and fails differently or not
        // at all): annotations are only enforced on request.
        let lax = run(engine, false, &path);
        assert!(
            !String::from_utf8_lossy(&lax.stderr).contains("type error:"),
            "{:?}",
            engine
        );
    }
}

#[test]
fn wrong_return_value_fails_at_run_time() {
    let path = write(
        "ret.fg",
        "fn name_of(o) -> String { return o.name }\nsay name_of(json.parse(\"\"\"{\"name\": 3}\"\"\"))\n",
    );
    for engine in ENGINES {
        let out = run(engine, true, &path);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{:?}: {}", engine, stderr);
        assert!(
            stderr.contains("type error: return value of 'name_of' must be String, got Int"),
            "{:?}: {}",
            engine,
            stderr
        );
    }
}

#[test]
fn correct_programs_behave_identically_under_strict() {
    let path = write(
        "ok.fg",
        "struct P { x: Int }\n\
         fn mk(x: Int) -> P { P { x: x } }\n\
         fn total(xs: [Int], scale: Float = 1.0) -> Float { return sum(xs) * scale }\n\
         fn maybe(n: Int) -> ?String { if n > 0 { \"pos\" } }\n\
         fn nothing(a: Int) { }\n\
         let f = fn(a: Int, b: Int) { a + b }\n\
         say mk(3).x\n\
         say total([1, 2, 3])\n\
         say maybe(1)\n\
         say maybe(-1)\n\
         say nothing(1)\n\
         say f(2, 3)\n",
    );
    let expected = "3\n6\npos\nnull\nnull\n5\n";
    for engine in ENGINES {
        for strict in [false, true] {
            let out = run(engine, strict, &path);
            assert!(
                out.status.success(),
                "{:?} strict={}: {}",
                engine,
                strict,
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                expected,
                "{:?} strict={}",
                engine,
                strict
            );
        }
    }
}

#[test]
fn static_errors_stop_strict_runs_but_only_warn_by_default() {
    let path = write("static.fg", "let n: Int = \"hello\"\nsay \"ran\"\n");
    for engine in ENGINES {
        let lax = run(engine, false, &path);
        assert!(lax.status.success());
        assert_eq!(String::from_utf8_lossy(&lax.stdout), "ran\n");
        let stderr = String::from_utf8_lossy(&lax.stderr);
        assert!(stderr.contains("T0001"), "{}", stderr);
        assert!(stderr.contains("Warning"), "{}", stderr);

        let strict = run(engine, true, &path);
        assert!(!strict.status.success());
        assert!(String::from_utf8_lossy(&strict.stdout).is_empty());
        let stderr = String::from_utf8_lossy(&strict.stderr);
        assert!(stderr.contains("[T0001] Error"), "{}", stderr);
        assert!(
            stderr.contains("type checking failed with 1 error"),
            "{}",
            stderr
        );
    }
}
