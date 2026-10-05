//! CLI integration tests for the capability/permission model
//! (`--sandbox`, `--allow-*`, `--max-time`, forge.toml `[permissions]`).
//!
//! Every behavioural check runs on both engines: the bytecode VM (default)
//! and the tree-walking interpreter (`--interp`), because the policy must be
//! engine-agnostic.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");
const ENGINES: [&[&str]; 2] = [&[], &["--interp"]];

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "forge_perm_cli_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).expect("mkdir");
    std::fs::canonicalize(&d).expect("canonicalize")
}

/// Run `forge <engine> <flags> run <file>` in `cwd` with `source` as the file.
fn run_file(cwd: &Path, engine: &[&str], flags: &[&str], source: &str) -> Output {
    let file = cwd.join("main.fg");
    std::fs::write(&file, source).expect("write script");
    Command::new(FORGE)
        .current_dir(cwd)
        .args(engine)
        .args(flags)
        .arg("run")
        .arg(&file)
        .env_remove("FORGE_FS_BASE")
        .output()
        .expect("spawn forge")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[track_caller]
fn assert_denied(o: &Output, cap: &str) {
    let err = stderr(o);
    assert!(
        !o.status.success(),
        "expected failure, stdout: {}",
        stdout(o)
    );
    assert!(
        err.contains(&format!("permission denied: {}", cap)),
        "expected `{cap}` denial, stderr: {err}"
    );
}

#[track_caller]
fn assert_ok(o: &Output) {
    assert!(
        o.status.success(),
        "expected success\nstdout: {}\nstderr: {}",
        stdout(o),
        stderr(o)
    );
}

#[test]
fn sandbox_denies_every_capability_on_both_engines() {
    let dir = tmpdir("deny");
    std::fs::write(dir.join("data.txt"), "secret").expect("write");
    let cases: &[(&str, &str)] = &[
        (r#"say fs.read("data.txt")"#, "fs.read"),
        (r#"say fs.list(".")"#, "fs.read"),
        (r#"fs.write("out.txt", "x")"#, "fs.write"),
        (r#"fs.remove("data.txt")"#, "fs.write"),
        (r#"say csv.read("data.txt")"#, "fs.read"),
        (r#"say http.get("https://example.com")"#, "net"),
        (r#"say fetch("https://example.com")"#, "net"),
        (r#"say env.get("HOME")"#, "env"),
        (r#"db.open(":memory:")"#, "db"),
        (r#"sh("echo pwned")"#, "run"),
        (r#"say shell("echo pwned")"#, "run"),
        (r#"say run_command("echo pwned")"#, "run"),
        (r#"let r = ask "hello""#, "ai"),
    ];
    for engine in ENGINES {
        for (src, cap) in cases {
            let o = run_file(&dir, engine, &["--sandbox"], src);
            assert!(!stdout(&o).contains("pwned"), "{src}: shelled out");
            assert_denied(&o, cap);
        }
    }
    // Nothing was written or removed.
    assert!(dir.join("data.txt").exists());
    assert!(!dir.join("out.txt").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn grants_enable_each_capability() {
    let dir = tmpdir("grant");
    std::fs::write(dir.join("data.txt"), "hello").expect("write");
    for engine in ENGINES {
        assert_ok(&run_file(
            &dir,
            engine,
            &["--sandbox", "--allow-env"],
            r#"say len(env.get("PATH")) > 0"#,
        ));
        assert_ok(&run_file(
            &dir,
            engine,
            &["--sandbox", "--allow-db"],
            r#"db.open(":memory:")"#,
        ));
        assert_ok(&run_file(
            &dir,
            engine,
            &["--sandbox", "--allow-read", "--allow-write"],
            r#"fs.write("copy.txt", fs.read("data.txt"))"#,
        ));
        let o = run_file(
            &dir,
            engine,
            &["--sandbox", "--allow-run"],
            r#"say sh("echo allowed")"#,
        );
        assert_ok(&o);
        assert!(stdout(&o).contains("allowed"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn default_policy_is_unchanged() {
    let dir = tmpdir("default");
    for engine in ENGINES {
        // fs, env, db stay allowed without flags...
        let o = run_file(
            &dir,
            engine,
            &[],
            "fs.write(\"a.txt\", \"1\")\nsay fs.read(\"a.txt\")\nsay len(env.get(\"PATH\")) > 0\ndb.open(\":memory:\")",
        );
        assert_ok(&o);
        assert_eq!(stdout(&o), "1\ntrue\n");
        // ...and `run` still needs --allow-run, which works in both positions.
        assert_denied(&run_file(&dir, engine, &[], r#"sh("echo x")"#), "run");
        assert_ok(&run_file(&dir, engine, &["--allow-run"], r#"sh("echo x")"#));
    }
    // -e keeps shell access (interactive context) unless sandboxed.
    let o = Command::new(FORGE)
        .args(["-e", r#"say sh("echo eval-ok")"#])
        .output()
        .expect("spawn");
    assert_ok(&o);
    let o = Command::new(FORGE)
        .args(["--sandbox", "-e", r#"say sh("echo nope")"#])
        .output()
        .expect("spawn");
    assert_denied(&o, "run");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_subcommand_accepts_permission_flags() {
    let dir = tmpdir("subflags");
    let file = dir.join("main.fg");
    std::fs::write(&file, "say len(env.get(\"PATH\")) > 0").expect("write");
    let o = Command::new(FORGE)
        .current_dir(&dir)
        .args(["run", "--sandbox"])
        .arg(&file)
        .output()
        .expect("spawn");
    assert_denied(&o, "env");
    let o = Command::new(FORGE)
        .current_dir(&dir)
        .args(["run", "--sandbox", "--allow-env"])
        .arg(&file)
        .output()
        .expect("spawn");
    assert_ok(&o);
    std::fs::write(&file, r#"sh("true")"#).expect("write");
    let o = Command::new(FORGE)
        .current_dir(&dir)
        .args(["run", "--allow-run"])
        .arg(&file)
        .output()
        .expect("spawn");
    assert_ok(&o);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scoped_paths_block_dotdot_and_symlink_escapes() {
    let root = tmpdir("scoped");
    let work = root.join("work");
    let allowed = work.join("allowed");
    std::fs::create_dir_all(&allowed).expect("mkdir");
    std::fs::write(allowed.join("in.txt"), "inside").expect("write");
    std::fs::write(root.join("secret.txt"), "TOPSECRET").expect("write");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("secret.txt"), allowed.join("link.txt")).expect("symlink");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&root, allowed.join("rootlink")).expect("symlink");

    let read_flag = format!("--allow-read={}", allowed.display());
    let write_flag = format!("--allow-write={}", allowed.display());
    for engine in ENGINES {
        let flags = ["--sandbox", read_flag.as_str(), write_flag.as_str()];
        let o = run_file(
            &work,
            engine,
            &flags,
            "fs.write(\"allowed/out.txt\", fs.read(\"allowed/in.txt\"))\nsay fs.read(\"allowed/out.txt\")",
        );
        assert_ok(&o);
        assert_eq!(stdout(&o), "inside\n");

        let escapes = [
            "say fs.read(\"allowed/../../secret.txt\")".to_string(),
            format!("say fs.read(\"{}\")", root.join("secret.txt").display()),
            "say fs.read(\"allowed/nope/../../../secret.txt\")".to_string(),
            #[cfg(unix)]
            "say fs.read(\"allowed/link.txt\")".to_string(),
            #[cfg(unix)]
            "say fs.read(\"allowed/rootlink/secret.txt\")".to_string(),
        ];
        for src in &escapes {
            let o = run_file(&work, engine, &flags, src);
            assert!(!stdout(&o).contains("TOPSECRET"), "{src} leaked");
            assert_denied(&o, "fs.read");
        }
        #[cfg(unix)]
        {
            let o = run_file(
                &work,
                engine,
                &flags,
                "fs.write(\"allowed/rootlink/pwned.txt\", \"x\")",
            );
            assert_denied(&o, "fs.write");
            assert!(!root.join("pwned.txt").exists());
        }
        // exists() answers false instead of probing outside the grant.
        let o = run_file(
            &work,
            engine,
            &flags,
            &format!("say fs.exists(\"{}\")", root.join("secret.txt").display()),
        );
        assert_ok(&o);
        assert_eq!(stdout(&o), "false\n");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn imports_next_to_the_entry_script_still_work_in_a_sandbox() {
    let dir = tmpdir("import");
    std::fs::write(
        dir.join("helper.fg"),
        "fn greet() { return \"hi from helper\" }",
    )
    .expect("write");
    for engine in ENGINES {
        let o = run_file(
            &dir,
            engine,
            &["--sandbox"],
            "import \"helper\"\nsay greet()",
        );
        assert_ok(&o);
        assert_eq!(stdout(&o), "hi from helper\n");
        // ...but reading the same file as data is still denied.
        assert_denied(
            &run_file(&dir, engine, &["--sandbox"], r#"say fs.read("helper.fg")"#),
            "fs.read",
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sandbox_cannot_open_sockets() {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.set_nonblocking(true).expect("nonblocking");
    let port = listener.local_addr().expect("addr").port();
    let dir = tmpdir("sockets");
    for engine in ENGINES {
        for src in [
            format!("say http.get(\"http://127.0.0.1:{port}/\")"),
            format!("say fetch(\"http://127.0.0.1:{port}/\")"),
        ] {
            let o = run_file(&dir, engine, &["--sandbox"], &src);
            assert_denied(&o, "net");
        }
        // A host allowlist that doesn't include the target is also a denial.
        let o = run_file(
            &dir,
            engine,
            &["--allow-net=example.com"],
            &format!("say http.get(\"http://127.0.0.1:{port}/\")"),
        );
        assert_denied(&o, "net");
    }
    // The `ws` module is only registered on the interpreter today.
    let o = run_file(
        &dir,
        &["--interp"],
        &["--sandbox"],
        &format!("say ws.connect(\"ws://127.0.0.1:{port}/\")"),
    );
    assert_denied(&o, "net");
    // No connection ever reached the listener.
    assert!(listener.accept().is_err(), "sandboxed program connected");

    // Listening (HTTP server) needs `net` too.
    let o = run_file(
        &dir,
        &[],
        &["--sandbox"],
        "@server(port: 0)\n@get(\"/\")\nfn root() -> Json { return { ok: true } }",
    );
    assert_denied(&o, "net");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn max_time_stops_an_infinite_loop() {
    let dir = tmpdir("maxtime");
    for engine in ENGINES {
        let start = Instant::now();
        let o = run_file(
            &dir,
            engine,
            &["--max-time", "1"],
            "say \"started\"\nlet mut i = 0\nwhile true { i = i + 1 }",
        );
        assert_eq!(o.status.code(), Some(124), "stderr: {}", stderr(&o));
        assert!(stdout(&o).contains("started"));
        assert!(stderr(&o).contains("exceeded --max-time"));
        assert!(start.elapsed() < Duration::from_secs(20));
    }
    // A program that finishes in time is unaffected.
    assert_ok(&run_file(&dir, &[], &["--max-time", "30"], "say 1"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn forge_toml_permissions_section() {
    let dir = tmpdir("toml");
    std::fs::create_dir_all(dir.join("data")).expect("mkdir");
    std::fs::write(dir.join("data/x.txt"), "x").expect("write");
    std::fs::write(dir.join("secret.txt"), "s").expect("write");
    std::fs::write(
        dir.join("forge.toml"),
        "[project]\nname = \"p\"\n\n[permissions]\nsandbox = true\nallow-read = [\"data\"]\n",
    )
    .expect("write");
    for engine in ENGINES {
        assert_ok(&run_file(&dir, engine, &[], r#"say fs.read("data/x.txt")"#));
        assert_denied(
            &run_file(&dir, engine, &[], r#"say fs.read("secret.txt")"#),
            "fs.read",
        );
        assert_denied(
            &run_file(&dir, engine, &[], r#"say env.get("HOME")"#),
            "env",
        );
        // CLI flags add on top of the project policy.
        assert_ok(&run_file(
            &dir,
            engine,
            &["--allow-env"],
            r#"say env.get("HOME")"#,
        ));
    }
    // A typo in [permissions] fails closed instead of being ignored.
    std::fs::write(
        dir.join("forge.toml"),
        "[permissions]\nsandbox = true\nallow-raed = true\n",
    )
    .expect("write");
    let o = run_file(&dir, &[], &[], "say 1");
    assert!(!o.status.success());
    assert!(stderr(&o).contains("[permissions]"), "{}", stderr(&o));
    let _ = std::fs::remove_dir_all(&dir);
}
