//! Embedding API: run untrusted Forge source under a host-defined policy.
//!
//! ```no_run
//! use forge_lang::{Capability, Sandbox};
//! use std::time::Duration;
//!
//! let out = Sandbox::new()                     // default-deny
//!     .allow(Capability::Ai)
//!     .allow_read(["./data"])                  // scoped grant
//!     .max_time(Duration::from_secs(2))        // wall-clock limit
//!     .run_source(r#"say "hello from the sandbox""#)
//!     .expect("script failed");
//! assert_eq!(out.stdout, "hello from the sandbox\n");
//! ```
//!
//! # Guarantees
//!
//! * **Default deny.** A fresh [`Sandbox`] grants nothing: no filesystem,
//!   network, environment, database, subprocess, AI or process-control
//!   (`exit`, `cd`) access. Each grant is explicit.
//! * **Isolation between sandboxes.** The policy is installed on the worker
//!   thread that runs the program (and inherited by every thread the engine
//!   forks from it), never process-wide, so concurrent sandboxes and the
//!   host's own use of Forge do not affect each other.
//! * **Wall-clock limit.** With [`Sandbox::max_time`], the host gets control
//!   back no later than the limit (plus a short grace period): the program
//!   is cancelled cooperatively at the next safe point, and if it does not
//!   stop in time (e.g. blocked inside a native call) its worker thread is
//!   detached and [`SandboxError::Timeout`] is returned anyway.
//! * **Captured output.** `say`/`println`/`print`/`io.print` output —
//!   including from `spawn`ed tasks, `timeout` blocks and imported modules —
//!   is captured into [`Output::stdout`] instead of the host's stdout.
//!   [`Sandbox::max_output`] bounds how much a program may print.
//! * **Cancellation.** [`Sandbox::run_source_cancellable`] takes a
//!   [`CancelHandle`] the host can trigger from another thread.
//!
//! # Current limits (future work)
//!
//! * Runs on the tree-walking interpreter. HTTP servers, `schedule` and
//!   `watch` blocks are host-runtime features and are not started.
//! * No memory limit yet; call depth is bounded by the engine's recursion
//!   limit.
//! * stderr output (`log`, `term`, warnings) is not captured.
//! * stdin is the host's: `input()`/`io.prompt` read from it. Hosts that
//!   use stdin for something else (like `forge mcp`) must redirect it.

use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::permissions::{self, Capabilities, Capability};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// Stack reserved for a sandbox worker thread. Only touched pages are
/// committed; the recursion guard turns deep recursion into an error.
const WORKER_STACK_SIZE: usize = 256 * 1024 * 1024;

/// How long to wait for a cancelled program to unwind before detaching it.
const CANCEL_GRACE: Duration = Duration::from_millis(250);

/// How often the host side checks the deadline, the output limit and the
/// cancel handle while a program runs.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// A configured, reusable sandbox.
#[derive(Debug, Clone)]
pub struct Sandbox {
    caps: Capabilities,
    max_time: Option<Duration>,
    max_output: Option<usize>,
    source_label: String,
}

/// Lets a host stop a running program from another thread. Cheap to clone;
/// all clones control the same run.
#[derive(Debug, Clone, Default)]
pub struct CancelHandle(Arc<AtomicBool>);

impl CancelHandle {
    pub fn new() -> Self {
        CancelHandle::default()
    }

    /// Ask the program to stop. It is cancelled cooperatively at the next
    /// safe point; the run returns [`SandboxError::Cancelled`] promptly
    /// either way.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// What a successful run produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Everything the program printed with `say`/`println`/`print`.
    pub stdout: String,
}

/// Why a run failed. Every variant carries the output printed before the
/// failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxError {
    /// The source did not lex or parse.
    Syntax { message: String },
    /// The program tried something its policy does not grant. `message` is
    /// the standard `permission denied: ...` text.
    PermissionDenied { message: String, stdout: String },
    /// Any other runtime error.
    Runtime {
        message: String,
        line: usize,
        stdout: String,
    },
    /// The program exceeded [`Sandbox::max_time`].
    Timeout { limit: Duration, stdout: String },
    /// The program printed more than [`Sandbox::max_output`] bytes and was
    /// stopped. `stdout` holds the first `limit` bytes.
    OutputLimit { limit: usize, stdout: String },
    /// The host cancelled the run through its [`CancelHandle`].
    Cancelled { stdout: String },
}

impl SandboxError {
    /// Output printed before the failure (empty for syntax errors).
    pub fn stdout(&self) -> &str {
        match self {
            SandboxError::Syntax { .. } => "",
            SandboxError::PermissionDenied { stdout, .. }
            | SandboxError::Runtime { stdout, .. }
            | SandboxError::Timeout { stdout, .. }
            | SandboxError::OutputLimit { stdout, .. }
            | SandboxError::Cancelled { stdout } => stdout,
        }
    }

    /// Stable machine-readable kind: `syntax`, `permission_denied`,
    /// `runtime`, `timeout`, `output_limit` or `cancelled`.
    pub fn kind(&self) -> &'static str {
        match self {
            SandboxError::Syntax { .. } => "syntax",
            SandboxError::PermissionDenied { .. } => "permission_denied",
            SandboxError::Runtime { .. } => "runtime",
            SandboxError::Timeout { .. } => "timeout",
            SandboxError::OutputLimit { .. } => "output_limit",
            SandboxError::Cancelled { .. } => "cancelled",
        }
    }
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxError::Syntax { message } => write!(f, "syntax error: {}", message),
            SandboxError::PermissionDenied { message, .. } => f.write_str(message),
            SandboxError::Runtime { message, line, .. } if *line > 0 => {
                write!(f, "runtime error on line {}: {}", line, message)
            }
            SandboxError::Runtime { message, .. } => write!(f, "runtime error: {}", message),
            SandboxError::Timeout { limit, .. } => {
                write!(f, "execution exceeded max time of {:?}", limit)
            }
            SandboxError::OutputLimit { limit, .. } => {
                write!(f, "output exceeded the limit of {} bytes", limit)
            }
            SandboxError::Cancelled { .. } => f.write_str("execution cancelled by the host"),
        }
    }
}

impl std::error::Error for SandboxError {}

impl Default for Sandbox {
    fn default() -> Self {
        Sandbox::new()
    }
}

impl Sandbox {
    /// A sandbox that grants nothing.
    pub fn new() -> Self {
        Sandbox {
            caps: Capabilities::deny_all(),
            max_time: None,
            max_output: None,
            source_label: "<sandbox>".to_string(),
        }
    }

    /// A sandbox running under an explicit policy.
    pub fn with_capabilities(caps: Capabilities) -> Self {
        Sandbox {
            caps,
            ..Sandbox::new()
        }
    }

    /// Grant a capability without restriction.
    pub fn allow(mut self, cap: Capability) -> Self {
        self.caps = self.caps.grant(cap);
        self
    }

    /// Grant `fs.read` under these paths only.
    pub fn allow_read<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.caps = self.caps.grant_read_paths(paths);
        self
    }

    /// Grant `fs.write` under these paths only.
    pub fn allow_write<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.caps = self.caps.grant_write_paths(paths);
        self
    }

    /// Grant `net` for these hosts only (`example.com`, `*.example.com`,
    /// `host:port`).
    pub fn allow_net<I, S>(mut self, hosts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.caps = self.caps.grant_net_hosts(hosts);
        self
    }

    /// Wall-clock limit for one run.
    pub fn max_time(mut self, limit: Duration) -> Self {
        self.max_time = Some(limit);
        self
    }

    /// Stop the program once it has printed more than `bytes` bytes, so a
    /// runaway print loop cannot exhaust host memory.
    pub fn max_output(mut self, bytes: usize) -> Self {
        self.max_output = Some(bytes);
        self
    }

    /// Label used in error messages (e.g. the agent tool name).
    pub fn source_label(mut self, label: impl Into<String>) -> Self {
        self.source_label = label.into();
        self
    }

    /// The policy this sandbox runs under.
    pub fn capabilities(&self) -> &Capabilities {
        &self.caps
    }

    /// Run Forge source to completion under this sandbox's policy.
    pub fn run_source(&self, source: &str) -> Result<Output, SandboxError> {
        self.run_source_cancellable(source, &CancelHandle::new())
    }

    /// Like [`Sandbox::run_source`], but the host can stop the run early
    /// with `cancel` (e.g. when the agent that asked for it goes away).
    pub fn run_source_cancellable(
        &self,
        source: &str,
        cancel: &CancelHandle,
    ) -> Result<Output, SandboxError> {
        let tokens = Lexer::new(source)
            .tokenize()
            .map_err(|e| SandboxError::Syntax {
                message: e.to_string(),
            })?;
        let program = Parser::new(tokens)
            .parse_program()
            .map_err(|e| SandboxError::Syntax {
                message: e.to_string(),
            })?;

        let sink: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let host_cancel = cancel;
        if host_cancel.is_cancelled() {
            return Err(SandboxError::Cancelled {
                stdout: String::new(),
            });
        }
        // The flag the interpreter polls. Separate from the host's handle,
        // so a timeout or output-limit stop is not reported as a cancel.
        let cancel = Arc::new(AtomicBool::new(false));
        let caps = Arc::new(self.caps.clone());
        let (tx, rx) = mpsc::channel();

        let worker_sink = sink.clone();
        let worker_cancel = cancel.clone();
        let label = self.source_label.clone();
        let source = source.to_string();
        let spawned = std::thread::Builder::new()
            .name("forge-sandbox".to_string())
            .stack_size(WORKER_STACK_SIZE)
            .spawn(move || {
                crate::runtime::recursion::register_thread_stack(WORKER_STACK_SIZE);
                let _policy = permissions::scope(caps);
                let mut interp = Interpreter::new();
                interp.source = Some(source);
                interp.source_file = Some(label.into());
                interp.output_sink = Some(worker_sink);
                interp.cancelled = worker_cancel;
                interp.set_defer_host_runtime(true);
                let result = interp.run(&program).map(|_| ());
                let _ = tx.send(result.map_err(|e| (e.message, e.line)));
            });
        if let Err(e) = spawned {
            return Err(SandboxError::Runtime {
                message: format!("failed to start sandbox thread: {}", e),
                line: 0,
                stdout: String::new(),
            });
        }

        let collect = |sink: &Arc<Mutex<Vec<String>>>| -> String {
            sink.lock()
                .map(|b| b.concat())
                .unwrap_or_else(|e| e.into_inner().concat())
        };
        let printed = |sink: &Arc<Mutex<Vec<String>>>| -> usize {
            let count = |b: &Vec<String>| b.iter().map(String::len).sum();
            sink.lock()
                .map(|b| count(&b))
                .unwrap_or_else(|e| count(&e.into_inner()))
        };
        // Stop the worker cooperatively and give it a moment to unwind;
        // either way the host gets control back now.
        let stop = |cancel: &AtomicBool| {
            cancel.store(true, Ordering::Release);
            let _ = rx.recv_timeout(CANCEL_GRACE);
        };

        let deadline = self.max_time.map(|limit| (limit, Instant::now() + limit));
        let outcome = loop {
            match rx.recv_timeout(POLL_INTERVAL) {
                Ok(r) => break Some(r),
                Err(mpsc::RecvTimeoutError::Disconnected) => break None,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if host_cancel.is_cancelled() {
                stop(&cancel);
                return Err(SandboxError::Cancelled {
                    stdout: collect(&sink),
                });
            }
            if let Some((limit, at)) = deadline {
                if Instant::now() >= at {
                    stop(&cancel);
                    return Err(SandboxError::Timeout {
                        limit,
                        stdout: collect(&sink),
                    });
                }
            }
            if let Some(limit) = self.max_output {
                if printed(&sink) > limit {
                    stop(&cancel);
                    return Err(SandboxError::OutputLimit {
                        limit,
                        stdout: truncate_utf8(collect(&sink), limit),
                    });
                }
            }
        };
        let stdout = collect(&sink);
        if let Some(limit) = self.max_output {
            if stdout.len() > limit {
                return Err(SandboxError::OutputLimit {
                    limit,
                    stdout: truncate_utf8(stdout, limit),
                });
            }
        }
        if host_cancel.is_cancelled() && outcome.as_ref().is_some_and(|r| r.is_err()) {
            return Err(SandboxError::Cancelled { stdout });
        }
        match outcome {
            Some(Ok(())) => Ok(Output { stdout }),
            Some(Err((message, line))) => {
                if message.starts_with("permission denied:") {
                    Err(SandboxError::PermissionDenied { message, stdout })
                } else {
                    Err(SandboxError::Runtime {
                        message,
                        line,
                        stdout,
                    })
                }
            }
            None => Err(SandboxError::Runtime {
                message: "sandbox worker panicked".to_string(),
                line: 0,
                stdout,
            }),
        }
    }
}

/// Cut `s` to at most `limit` bytes on a character boundary.
pub(crate) fn truncate_utf8(mut s: String, limit: usize) -> String {
    if s.len() > limit {
        let mut end = limit;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "forge_sandbox_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).expect("mkdir");
        d
    }

    fn denied(r: Result<Output, SandboxError>, cap: &str) {
        match r {
            Err(SandboxError::PermissionDenied { message, .. }) => assert!(
                message.starts_with(&format!("permission denied: {}", cap)),
                "{message}"
            ),
            other => panic!("expected {cap} denial, got {other:?}"),
        }
    }

    #[test]
    fn captures_stdout() {
        let out = Sandbox::new()
            .run_source("say \"hi\"\nprint(\"a\")\nprintln(1 + 2)")
            .expect("runs");
        assert_eq!(out.stdout, "hi\na3\n");
    }

    #[test]
    fn syntax_and_runtime_errors() {
        assert!(matches!(
            Sandbox::new().run_source("let = ="),
            Err(SandboxError::Syntax { .. })
        ));
        match Sandbox::new().run_source("say \"before\"\nlet x = 1 / 0") {
            Err(SandboxError::Runtime { stdout, .. }) => assert_eq!(stdout, "before\n"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn default_deny_every_capability() {
        let sb = Sandbox::new();
        denied(sb.run_source(r#"fs.read("/etc/hostname")"#), "fs.read");
        denied(sb.run_source(r#"fs.write("x.txt", "y")"#), "fs.write");
        denied(sb.run_source(r#"http.get("https://example.com")"#), "net");
        denied(sb.run_source(r#"fetch("https://example.com")"#), "net");
        denied(sb.run_source(r#"env.get("HOME")"#), "env");
        denied(sb.run_source(r#"db.open(":memory:")"#), "db");
        denied(sb.run_source(r#"sh("echo pwned")"#), "run");
        denied(sb.run_source(r#"run_command("echo pwned")"#), "run");
        denied(sb.run_source(r#"exit(3)"#), "process");
        denied(sb.run_source(r#"cd("/")"#), "process");
        denied(sb.run_source(r#"let r = ask "hi""#), "ai");
    }

    #[test]
    fn grants_are_honoured() {
        let out = Sandbox::new()
            .allow(Capability::Env)
            .allow(Capability::Db)
            .run_source(
                "env.set(\"FORGE_SANDBOX_T\", \"1\")\nsay env.get(\"FORGE_SANDBOX_T\")\ndb.open(\":memory:\")\nsay \"db ok\"",
            )
            .expect("granted");
        assert_eq!(out.stdout, "1\ndb ok\n");
    }

    #[test]
    fn scoped_filesystem() {
        let root = tmpdir("fs");
        let data = root.join("data");
        std::fs::create_dir_all(&data).expect("mkdir");
        std::fs::write(data.join("in.txt"), "hello").expect("write");
        std::fs::write(root.join("secret.txt"), "s3cret").expect("write");
        let sb = Sandbox::new().allow_read([&data]).allow_write([&data]);
        let d = data.display();
        let out = sb
            .run_source(&format!(
                "fs.write(\"{d}/out.txt\", fs.read(\"{d}/in.txt\") + \"!\")\nsay fs.read(\"{d}/out.txt\")"
            ))
            .expect("inside the grant");
        assert_eq!(out.stdout, "hello!\n");
        denied(
            sb.run_source(&format!("fs.read(\"{d}/../secret.txt\")")),
            "fs.read",
        );
        denied(
            sb.run_source(&format!("fs.write(\"{}/x.txt\", \"x\")", root.display())),
            "fs.write",
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("secret.txt"), data.join("link.txt"))
                .expect("symlink");
            denied(
                sb.run_source(&format!("fs.read(\"{d}/link.txt\")")),
                "fs.read",
            );
        }
        // exists() never reveals what is outside the grant.
        let out = sb
            .run_source(&format!("say fs.exists(\"{}/secret.txt\")", root.display()))
            .expect("exists is not an error");
        assert_eq!(out.stdout, "false\n");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn spawned_tasks_inherit_the_sandbox() {
        let r = Sandbox::new()
            .run_source("let h = spawn { return sh(\"echo escaped\") }\nlet v = await h\nsay v");
        // The task's own thread must still be under the sandbox: the shell
        // call fails with the standard denial, whichever way the engine
        // surfaces a failed task.
        let text = match r {
            Ok(out) => out.stdout,
            Err(e) => e.to_string(),
        };
        assert!(!text.contains("escaped"), "{text}");
        assert!(text.contains("permission denied: run"), "{text}");
    }

    #[test]
    fn max_time_stops_infinite_loop() {
        let start = std::time::Instant::now();
        let r = Sandbox::new()
            .max_time(Duration::from_millis(300))
            .run_source("say \"start\"\nlet mut i = 0\nwhile true { i = i + 1 }");
        match r {
            Err(SandboxError::Timeout { stdout, .. }) => assert_eq!(stdout, "start\n"),
            other => panic!("{other:?}"),
        }
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn max_time_does_not_wait_for_blocking_sleep() {
        let start = std::time::Instant::now();
        let r = Sandbox::new()
            .max_time(Duration::from_millis(200))
            .run_source("wait(30)");
        assert!(matches!(r, Err(SandboxError::Timeout { .. })), "{r:?}");
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn output_from_tasks_timeouts_and_io_print_is_captured() {
        let out = Sandbox::new()
            .run_source(
                "io.print(\"a\")\nlet h = spawn { say \"from task\" }\nawait h\ntimeout 5 seconds { say \"in timeout\" }",
            )
            .expect("runs");
        assert_eq!(out.stdout, "afrom task\nin timeout\n");
    }

    #[test]
    fn max_output_stops_runaway_printing() {
        let start = std::time::Instant::now();
        let r = Sandbox::new()
            .max_output(1000)
            .max_time(Duration::from_secs(20))
            .run_source("while true { say \"spam spam spam\" }");
        match r {
            Err(SandboxError::OutputLimit { limit, stdout }) => {
                assert_eq!(limit, 1000);
                assert!(
                    stdout.len() <= 1000 && stdout.starts_with("spam"),
                    "{stdout}"
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        // Under the limit is fine.
        assert!(Sandbox::new()
            .max_output(10)
            .run_source("say \"ok\"")
            .is_ok());
    }

    #[test]
    fn cancel_handle_stops_a_run() {
        let handle = CancelHandle::new();
        let remote = handle.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            remote.cancel();
        });
        let start = std::time::Instant::now();
        let r = Sandbox::new().run_source_cancellable("say \"started\"\nwhile true { }", &handle);
        t.join().expect("canceller");
        match r {
            Err(e @ SandboxError::Cancelled { .. }) => {
                assert_eq!(e.kind(), "cancelled");
                assert_eq!(e.stdout(), "started\n");
            }
            other => panic!("{other:?}"),
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        // An already-cancelled handle never starts the program.
        assert!(matches!(
            Sandbox::new().run_source_cancellable("say 1", &handle),
            Err(SandboxError::Cancelled { .. })
        ));
    }

    #[test]
    fn truncate_utf8_respects_char_boundaries() {
        assert_eq!(truncate_utf8("héllo".to_string(), 2), "h");
        assert_eq!(truncate_utf8("abc".to_string(), 10), "abc");
    }

    #[test]
    fn sandboxes_do_not_leak_into_the_host() {
        let _ = Sandbox::new().run_source("say 1");
        // The calling thread is still under the process-wide policy.
        assert!(permissions::require(Capability::Env, "").is_ok());
    }
}
