//! Security regression tests (see `docs/SECURITY_AUDIT.md`).
//!
//! Each test replays a concrete exploit found in the audit and asserts that
//! it now *fails to exploit*: the escape is denied with the standard
//! `permission denied: ...` error, the host keeps control, and nothing lands
//! outside the grant. The attacker controls the script; the grants are the
//! ones an operator would plausibly give (`--allow-db`, a scoped
//! `--allow-read`, ...).
//!
//! * CLI exploits (`exploits/*.fg`) run under `forge --sandbox` on both
//!   engines (VM and `--interp`).
//! * Embedding exploits run through `forge_lang::Sandbox` (the API behind
//!   `forge mcp` and the Python binding).

use forge_lang::{Capability, Sandbox, SandboxError};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");
const ENGINES: [&[&str]; 2] = [&[], &["--interp"]];

/// A scratch layout: `<root>/data` is granted, `<root>/secret` is not.
struct Layout {
    root: PathBuf,
    data: PathBuf,
    secret: PathBuf,
}

impl Layout {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "forge_sec_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let data = root.join("data");
        let secret = root.join("secret");
        std::fs::create_dir_all(&data).expect("mkdir data");
        std::fs::create_dir_all(&secret).expect("mkdir secret");
        let root = std::fs::canonicalize(&root).expect("canonicalize");
        std::fs::write(root.join("secret").join("key.txt"), "TOPSECRET").expect("write");
        Layout {
            data: root.join("data"),
            secret: root.join("secret"),
            root,
        }
    }

    /// Fill an exploit template's placeholders.
    fn script(&self, template: &str) -> String {
        template
            .replace("@DATA@", &lit(&self.data))
            .replace("@SECRET@", &lit(&self.secret))
    }

    fn read_flag(&self) -> String {
        format!("--allow-read={}", self.data.display())
    }

    fn write_flag(&self) -> String {
        format!("--allow-write={}", self.data.display())
    }
}

impl Drop for Layout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A path as the body of a Forge string literal.
fn lit(p: &Path) -> String {
    p.display().to_string().replace('\\', "\\\\")
}

fn run_cli(cwd: &Path, engine: &[&str], flags: &[&str], source: &str) -> Output {
    let file = cwd.join("main.fg");
    std::fs::write(&file, source).expect("write script");
    Command::new(FORGE)
        .current_dir(cwd)
        .args(engine)
        .args(flags)
        .arg("run")
        .arg(&file)
        .env("NO_COLOR", "1")
        .env_remove("FORGE_FS_BASE")
        .env_remove("FORGE_HTTP_ALLOW_PRIVATE")
        .output()
        .expect("spawn forge")
}

fn text(o: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

/// The run failed cleanly (exit code 1, not a signal/abort), printed no
/// `ESCAPED` marker, and its error mentions `needle`.
#[track_caller]
fn assert_blocked(o: &Output, needle: &str, what: &str) {
    let (out, err) = text(o);
    assert!(
        !out.contains("ESCAPED"),
        "{what}: exploit succeeded\n{out}{err}"
    );
    assert_eq!(
        o.status.code(),
        Some(1),
        "{what}: expected a clean runtime error (exit 1), got {:?}\nstdout: {out}\nstderr: {err}",
        o.status
    );
    assert!(
        err.contains(needle),
        "{what}: expected `{needle}` in the error\nstderr: {err}"
    );
}

// ---------------------------------------------------------------------------
// CLI (`forge --sandbox`), both engines
// ---------------------------------------------------------------------------

#[test]
fn sqlite_cannot_open_files_outside_the_grant() {
    let l = Layout::new("sqlite");
    let (r, w) = (l.read_flag(), l.write_flag());
    let flags = ["--sandbox", "--allow-db", r.as_str(), w.as_str()];
    let cases = [
        (include_str!("exploits/sqlite_attach.fg"), "attached.db"),
        (include_str!("exploits/sqlite_vacuum_into.fg"), "vacuum.db"),
        (include_str!("exploits/sqlite_uri.fg"), "uri.db"),
    ];
    for engine in ENGINES {
        for (template, target) in cases {
            let o = run_cli(&l.data, engine, &flags, &l.script(template));
            let (out, err) = text(&o);
            assert!(!out.contains("ESCAPED"), "{target}: {out}{err}");
            assert!(!o.status.success(), "{target}: {out}{err}");
            assert!(
                !l.secret.join(target).exists(),
                "{target} was created outside the grant"
            );
        }
    }
    // Inside the grant, file databases still work.
    for engine in ENGINES {
        let o = run_cli(
            &l.data,
            engine,
            &flags,
            "db.open(\"app.db\")\ndb.execute(\"CREATE TABLE IF NOT EXISTS t(a)\")\nsay \"ok\"",
        );
        assert_eq!(text(&o).0, "ok\n", "{:?}", text(&o));
    }
    // An unrestricted filesystem keeps full SQLite (ATTACH is legitimate there).
    let o = run_cli(
        &l.data,
        &[],
        &[],
        &format!(
            "db.open(\":memory:\")\ndb.execute(\"ATTACH DATABASE '{}' AS x\")\nsay \"ok\"",
            lit(&l.data.join("other.db"))
        ),
    );
    assert_eq!(text(&o).0, "ok\n", "{:?}", text(&o));
}

#[test]
fn env_load_does_not_search_parent_directories_outside_the_grant() {
    let l = Layout::new("dotenv");
    std::fs::write(l.root.join(".env"), "FORGE_SEC_STOLEN=TOPSECRET\n").expect("write");
    let r = l.read_flag();
    for engine in ENGINES {
        let o = run_cli(
            &l.data,
            engine,
            &["--sandbox", "--allow-env", r.as_str()],
            &l.script(include_str!("exploits/dotenv_parent_search.fg")),
        );
        let (out, err) = text(&o);
        assert!(!out.contains("TOPSECRET"), "{out}{err}");
    }
}

#[test]
fn sandboxed_scripts_cannot_rewrite_forge_host_configuration() {
    let l = Layout::new("envcfg");
    for engine in ENGINES {
        for template in [
            include_str!("exploits/env_reserved_proxy.fg"),
            include_str!("exploits/env_reserved_ssrf.fg"),
        ] {
            let o = run_cli(
                &l.data,
                engine,
                &["--sandbox", "--allow-env", "--allow-net=api.example.com"],
                template,
            );
            assert_blocked(&o, "permission denied: env", template);
        }
        // Invalid names are an error, not a panic.
        let o = run_cli(
            &l.data,
            engine,
            &["--sandbox", "--allow-env"],
            include_str!("exploits/env_invalid_key.fg"),
        );
        assert_blocked(&o, "invalid variable name", "env.set(\"A=B\")");
        // Ordinary variables are still settable.
        let o = run_cli(
            &l.data,
            engine,
            &["--sandbox", "--allow-env"],
            "env.set(\"MY_APP_MODE\", \"x\")\nsay env.get(\"MY_APP_MODE\")",
        );
        assert_eq!(text(&o).0, "x\n", "{:?}", text(&o));
    }
}

#[test]
fn which_needs_allow_run() {
    let l = Layout::new("which");
    for engine in ENGINES {
        let o = run_cli(
            &l.data,
            engine,
            &["--sandbox"],
            include_str!("exploits/which_spawns_process.fg"),
        );
        assert_blocked(&o, "permission denied: run", "which");
    }
}

#[test]
fn filesystem_oracles_respect_fs_read() {
    let l = Layout::new("oracle");
    let r = l.read_flag();
    for engine in ENGINES {
        for template in [
            include_str!("exploits/watch_outside_grant.fg"),
            include_str!("exploits/path_resolve_oracle.fg"),
        ] {
            let o = run_cli(
                &l.data,
                engine,
                &["--sandbox", r.as_str()],
                &l.script(template),
            );
            assert_blocked(&o, "permission denied: fs.read", template);
            assert!(!text(&o).0.contains("key.txt"), "{template}");
        }
    }
}

#[test]
fn database_drivers_respect_the_net_allowlist() {
    let l = Layout::new("dbnet");
    for engine in ENGINES {
        for template in [
            include_str!("exploits/pg_connect_any_host.fg"),
            include_str!("exploits/mysql_connect_any_host.fg"),
        ] {
            let o = run_cli(
                &l.data,
                engine,
                &["--sandbox", "--allow-db", "--allow-net=api.example.com"],
                template,
            );
            assert_blocked(&o, "permission denied: net", template);
        }
    }
}

#[test]
fn websocket_client_applies_the_private_address_guard() {
    let l = Layout::new("ws");
    for engine in ENGINES {
        let o = run_cli(
            &l.data,
            engine,
            &["--sandbox", "--allow-net"],
            include_str!("exploits/ws_private_address.fg"),
        );
        assert_blocked(&o, "private/loopback", "ws.connect");
    }
}

#[test]
fn oversized_allocations_are_errors_not_aborts() {
    let l = Layout::new("alloc");
    let cases = [
        "let s = repeat_str(\"x\", 100000000000000)",
        "let r = range(0, 100000000000000)",
        "let r = range(-9223372036854775807, 9223372036854775807)",
        "let s = pad_start(\"x\", 100000000000000, \"y\")",
        "let s = pad_end(\"x\", 100000000000000)",
        "let s = sample([1, 2, 3], -1)",
        "let s = slay(fn() { return 1 }, -1)",
        "let b = crypto.random_bytes(-1)",
    ];
    for engine in ENGINES {
        for src in cases {
            let o = run_cli(
                &l.data,
                engine,
                &["--sandbox"],
                &format!("{}\nsay \"ESCAPED\"", src),
            );
            let (out, err) = text(&o);
            assert!(!out.contains("ESCAPED"), "{src}: {out}");
            assert_eq!(o.status.code(), Some(1), "{src}: aborted?\n{err}");
        }
        // A negative pad length pads nothing (it used to abort).
        let o = run_cli(&l.data, engine, &["--sandbox"], "say pad_start(\"ab\", -5)");
        assert_eq!(text(&o).0, "ab\n", "{:?}", text(&o));
    }
}

/// Run `forge` with a hard wall-clock cap; `None` if it had to be killed.
fn run_cli_capped(cwd: &Path, engine: &[&str], source: &str, cap: Duration) -> Option<Output> {
    let file = cwd.join("main.fg");
    std::fs::write(&file, source).expect("write script");
    let mut child = Command::new(FORGE)
        .current_dir(cwd)
        .args(engine)
        .arg("run")
        .arg(&file)
        .env("NO_COLOR", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn forge");
    let start = Instant::now();
    loop {
        if child.try_wait().expect("wait").is_some() {
            return Some(child.wait_with_output().expect("output"));
        }
        if start.elapsed() > cap {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn timeout_blocks_stop_everything_inside_them() {
    // SEC-02, observable from the CLI: a `timeout` block must stop whatever
    // runs inside it. Before the fix every case hung on the interpreter.
    let l = Layout::new("nested");
    std::fs::write(
        l.data.join("spin.fg"),
        "let mut i = 0\nwhile true { i = i + 1 }\n",
    )
    .expect("write module");
    let spin = |body: &str| format!("timeout 1 seconds {{\n{body}\n}}\nsay \"after\"");
    let squad_task =
        spin("squad {\n  spawn {\n    let mut i = 0\n    while true { i = i + 1 }\n  }\n}");
    let receive = spin("let ch = channel()\nreceive(ch)");
    let channel_loop = spin("let ch = channel()\nfor v in ch { say v }");
    let await_task =
        spin("let h = spawn {\n  let mut i = 0\n  while true { i = i + 1 }\n}\nawait h");
    let both = [
        squad_task,
        receive,
        channel_loop,
        await_task,
        spin("squad {\n  let mut i = 0\n  while true { i = i + 1 }\n}"),
        spin(&format!("import \"{}\"", lit(&l.data.join("spin.fg")))),
        spin("while true { }"),
        spin("timeout 100000 seconds {\n  let mut i = 0\n  while true { i = i + 1 }\n}"),
    ];
    let mut cases: Vec<(&[&str], &str)> = Vec::new();
    for src in &both {
        for engine in ENGINES {
            cases.push((engine, src.as_str()));
        }
    }
    for (engine, src) in cases {
        let o = run_cli_capped(&l.data, engine, src, Duration::from_secs(20))
            .unwrap_or_else(|| panic!("{engine:?} hung on:\n{src}"));
        let (out, err) = text(&o);
        assert!(
            !o.status.success() && !out.contains("after"),
            "{engine:?}:\n{src}\nstdout: {out}\nstderr: {err}"
        );
        assert!(err.contains("timeout"), "{engine:?}: {src}\nstderr: {err}");
        assert!(
            !err.contains("internal control transfer"),
            "{engine:?}: {src}\nstderr: {err}"
        );
    }
}

#[test]
fn timeout_cancels_the_tasks_it_started() {
    // SEC-02 on the VM: the tasks a `timeout` block started must stop when
    // its deadline fires, not keep running after the block (here they would
    // keep sending; the receiver after the block sees the channel go quiet).
    let l = Layout::new("tasks_stop");
    let src = "let ch = channel()\n\
               try {\n\
                 timeout 1 seconds {\n\
                   squad {\n\
                     spawn { while true { send(ch, 1)\nwait(0.01) } }\n\
                   }\n\
                 }\n\
               } catch e { say \"timed out\" }\n\
               wait(0.3)\n\
               while !is_none(try_receive(ch)) { }\n\
               wait(0.5)\n\
               say is_none(try_receive(ch))";
    for engine in ENGINES {
        let o = run_cli_capped(&l.data, engine, src, Duration::from_secs(20))
            .unwrap_or_else(|| panic!("{engine:?} hung"));
        let (out, err) = text(&o);
        assert_eq!(out, "timed out\ntrue\n", "{engine:?}\nstderr: {err}");
    }
}

#[test]
fn deeply_nested_values_are_errors_not_stack_overflows() {
    // SEC-16: a value millions of levels deep (built in a second on the VM)
    // used to abort the process in the recursive VM->interpreter conversion
    // behind json.stringify. Every recursive value walker now stops at a
    // fixed depth (runtime::recursion::MAX_VALUE_DEPTH).
    let l = Layout::new("deep");
    let src = "let mut a = []\n\
               let mut i = 0\n\
               while i < 3000000 {\n  a = [a]\n  i = i + 1\n}\n\
               try { json.stringify(a) } catch e { say e.message }\n\
               try { json.pretty(a) } catch e { say e.message }\n\
               let mut b = []\n\
               let mut j = 0\n\
               while j < 3000000 {\n  b = [b]\n  j = j + 1\n}\n\
               say a == b\n\
               say len(str(a)) > 0\n\
               say \"alive\"";
    // The interpreter copies on every `a = [a]` (quadratic), so it never
    // gets this deep within a test's time; the VM builds it in a second.
    let o = run_cli_capped(&l.data, &[], src, Duration::from_secs(120))
        .unwrap_or_else(|| panic!("VM hung"));
    let (out, err) = text(&o);
    assert!(o.status.success(), "stdout: {out}\nstderr: {err}");
    assert_eq!(
        out,
        "value nested too deeply (more than 10000 levels)\n\
         value nested too deeply (more than 10000 levels)\n\
         false\ntrue\nalive\n",
        "stderr: {err}"
    );
}

// ---------------------------------------------------------------------------
// Embedding API (`Sandbox`): what `forge mcp` and the Python binding run
// ---------------------------------------------------------------------------

/// Run `src` under `sandbox` and assert the host got control back by the
/// deadline plus a small grace, whatever the script did.
fn run_bounded(sandbox: Sandbox, src: &str) -> Result<forge_lang::Output, SandboxError> {
    let start = Instant::now();
    let r = sandbox.max_time(Duration::from_millis(400)).run_source(src);
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "host did not get control back: {:?}",
        start.elapsed()
    );
    r
}

/// Contents of a counter file the escaped code keeps rewriting.
fn counter(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// After a timed-out run, code the script started must stop: a file it
/// keeps updating must stop changing.
#[track_caller]
fn assert_stopped(path: &Path, what: &str) {
    std::thread::sleep(Duration::from_millis(600));
    let before = counter(path);
    std::thread::sleep(Duration::from_millis(900));
    let after = counter(path);
    assert_eq!(
        before, after,
        "{what}: code kept running after the deadline"
    );
}

#[test]
fn deadline_reaches_every_task_the_script_started() {
    let l = Layout::new("deadline");
    let mark = l.data.join("counter.txt");
    let tick = format!(
        "let mut i = 0\n    while true {{ i = i + 1\n      fs.write(\"{}\", str(i))\n      wait(0.02) }}",
        lit(&mark)
    );
    let cases = [
        (
            "squad task",
            format!("squad {{\n  spawn {{\n    {tick}\n  }}\n}}"),
        ),
        ("squad body", format!("squad {{\n    {tick}\n}}")),
        (
            "timeout body",
            format!("timeout 100000 seconds {{\n    {tick}\n}}"),
        ),
        (
            "task in timeout",
            format!(
                "timeout 100000 seconds {{\n  let h = spawn {{\n    {tick}\n  }}\n  await h\n}}"
            ),
        ),
    ];
    for (what, src) in &cases {
        let _ = std::fs::remove_file(&mark);
        let sb = Sandbox::new().allow_write([&l.data]).allow_read([&l.data]);
        let r = run_bounded(sb, src);
        assert!(
            matches!(r, Err(SandboxError::Timeout { .. })),
            "{what}: {r:?}"
        );
        assert!(mark.exists(), "{what}: the task never ran");
        assert_stopped(&mark, what);
    }
}

#[test]
fn imported_modules_are_contained() {
    let l = Layout::new("import");
    let mark = l.data.join("counter.txt");
    // An imported module's top level runs under the same deadline...
    std::fs::write(
        l.data.join("spin.fg"),
        format!(
            "let mut i = 0\nwhile true {{ i = i + 1\n  fs.write(\"{}\", str(i))\n  wait(0.02) }}\n",
            lit(&mark)
        ),
    )
    .expect("write module");
    let sb = Sandbox::new().allow_write([&l.data]).allow_read([&l.data]);
    let r = run_bounded(sb, &format!("import \"{}\"", lit(&l.data.join("spin.fg"))));
    assert!(matches!(r, Err(SandboxError::Timeout { .. })), "{r:?}");
    assert_stopped(&mark, "import");

    // ...and cannot start host-runtime threads (`schedule`) that outlive it.
    let sched = l.data.join("sched.txt");
    std::fs::write(
        l.data.join("sched.fg"),
        format!(
            "schedule every 1 seconds {{\n  fs.append(\"{}\", \"x\")\n}}\n",
            lit(&sched)
        ),
    )
    .expect("write module");
    let sb = Sandbox::new().allow_write([&l.data]).allow_read([&l.data]);
    let r = sb.run_source(&format!(
        "import \"{}\"\nlet h = spawn {{\n  schedule every 1 seconds {{\n    fs.append(\"{}\", \"y\")\n  }}\n}}\nawait h",
        lit(&l.data.join("sched.fg")),
        lit(&sched)
    ));
    assert!(r.is_ok(), "{r:?}");
    std::thread::sleep(Duration::from_millis(2500));
    assert!(
        !sched.exists(),
        "a schedule block ran outside the sandboxed run"
    );
}

#[test]
fn blocking_waits_honour_the_deadline() {
    // Each of these blocks forever; the run must time out promptly and the
    // worker must not stay parked (checked by the run returning at all for
    // `squad`, which joins its tasks).
    let cases = [
        "let ch = channel()\nreceive(ch)",
        "let ch = channel()\nfor x in ch { say x }",
        "let ch = channel()\nlet h = spawn { return receive(ch) }\nawait h",
        "let ch = channel()\nsquad { spawn { receive(ch) } }",
        "let ch = channel()\nlet h = spawn { receive(ch) }\nawait_all([h])",
        "let ch = channel()\nselect([ch])",
        "time.sleep(100000)",
        "wait(100000000000000)",
    ];
    for src in cases {
        let r = run_bounded(Sandbox::new(), src);
        assert!(
            matches!(r, Err(SandboxError::Timeout { .. })),
            "{src}: {r:?}"
        );
    }
}

#[test]
fn output_cap_bounds_memory_between_polls() {
    let start = Instant::now();
    let r = Sandbox::new()
        .max_output(1000)
        .max_time(Duration::from_secs(20))
        .run_source("let big = repeat_str(\"x\", 1000000)\nwhile true { say big }");
    match r {
        Err(SandboxError::OutputLimit { stdout, .. }) => assert!(stdout.len() <= 1000),
        other => panic!("{other:?}"),
    }
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
fn host_stdin_and_argv_are_not_readable_without_process() {
    let out = Sandbox::new()
        .run_source(
            "say term.confirm(\"ok?\")\nsay term.menu([\"a\", \"b\"])\nsay io.args()\nsay io.args_has(\"--x\")",
        )
        .expect("not errors");
    assert_eq!(out.stdout, "false\nnull\n[]\nfalse\n");
    // With `process`, the arguments are the host's.
    let out = Sandbox::new()
        .allow(Capability::Process)
        .run_source("say len(io.args()) > 0")
        .expect("runs");
    assert_eq!(out.stdout, "true\n");
}

#[test]
fn oversized_allocations_do_not_abort_the_host() {
    for src in [
        "repeat_str(\"x\", 100000000000000)",
        "range(0, 100000000000000)",
        "pad_start(\"x\", 100000000000000)",
    ] {
        match Sandbox::new().run_source(src) {
            Err(SandboxError::Runtime { message, .. }) => {
                assert!(message.contains("too large"), "{src}: {message}")
            }
            other => panic!("{src}: {other:?}"),
        }
    }
}
