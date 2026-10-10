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
//!     .max_fuel(10_000_000)                    // deterministic step budget
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
//! * **Resource limits** ([`Sandbox::limits`], `runtime::limits`):
//!   - [`Sandbox::max_fuel`] — a deterministic step budget (see
//!     [Engines](#engines) for what a step is). Running out is
//!     [`SandboxError::FuelExhausted`], at exactly the same step every time
//!     for a single-threaded program, whatever the machine's speed.
//!   - [`Sandbox::max_memory`] — bytes the run's threads may hold, polled at
//!     the engine's safe points; exceeding it is [`SandboxError::MemoryLimit`]. Needs
//!     [`crate::CountingAllocator`] as the host's global allocator (the run
//!     fails with a clear error otherwise).
//!   - caps on concurrently open files, sockets, subprocesses and tasks, on
//!     the size of one string or collection, and on imports — exceeding one
//!     is [`SandboxError::ResourceLimit`] unless the program catches it.
//!
//!   Fuel and memory exhaustion cannot be caught by the program. The host
//!   process is unaffected either way: the run's memory is released when
//!   its worker thread ends.
//!
//! # Engines
//!
//! [`Sandbox::engine`] picks the engine: the bytecode VM ([`Engine::Vm`],
//! the default, like `forge run`) or the tree-walking interpreter
//! ([`Engine::Interpreter`]). Every guarantee above holds on both: they are
//! applied by one wrapper ([`Sandbox::run_contained`]) around either
//! engine. A program the VM cannot run faithfully (an unknown decorator)
//! runs on the interpreter inside the same sandbox. Fuel counts the
//! engine's own steps (VM: instructions; interpreter: statements, calls and
//! loop iterations), so a program spends different amounts on each.
//!
//! # Current limits (future work)
//!
//! * HTTP servers, `schedule` and `watch` blocks are host-runtime features
//!   and are not started.
//! * Call depth is bounded by the engine's recursion limit.
//! * stderr output (`log`, `term`, warnings) is not captured.
//! * The host's stdin is only readable with the `process` capability
//!   (denied by default): without it `input()` / `io.prompt` see an empty
//!   stream, so a script can never consume a host's stdin (e.g. the
//!   `forge mcp` protocol stream) on any platform.

use crate::interpreter::{Interpreter, RuntimeError};
use crate::lexer::Lexer;
use crate::parser::ast::Program;
use crate::parser::Parser;
use crate::permissions::{self, Capabilities, Capability};
use crate::runtime::limits::{self, Budget, LimitKind, Limits, Trip};
use crate::runtime::stdio;
use crate::vm::embed;
use crate::vm::machine::{VMError, VM};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
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
    limits: Limits,
    source_label: String,
    /// See [`Sandbox::memory_baseline`].
    memory_baseline: usize,
    engine: Engine,
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
    /// The program used up its [`Sandbox::max_fuel`] budget.
    FuelExhausted { limit: u64, stdout: String },
    /// The program needed more than [`Sandbox::max_memory`] bytes.
    MemoryLimit { limit: usize, stdout: String },
    /// The program exceeded another resource limit (open handles, value
    /// size, imports) and did not catch the error. `message` starts with
    /// `resource limit exceeded:`.
    ResourceLimit { message: String, stdout: String },
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
            | SandboxError::Cancelled { stdout }
            | SandboxError::FuelExhausted { stdout, .. }
            | SandboxError::MemoryLimit { stdout, .. }
            | SandboxError::ResourceLimit { stdout, .. } => stdout,
        }
    }

    /// Stable machine-readable kind: `syntax`, `permission_denied`,
    /// `runtime`, `timeout`, `output_limit`, `cancelled`, `fuel_exhausted`,
    /// `memory_limit` or `resource_limit`.
    pub fn kind(&self) -> &'static str {
        match self {
            SandboxError::Syntax { .. } => "syntax",
            SandboxError::PermissionDenied { .. } => "permission_denied",
            SandboxError::Runtime { .. } => "runtime",
            SandboxError::Timeout { .. } => "timeout",
            SandboxError::OutputLimit { .. } => "output_limit",
            SandboxError::Cancelled { .. } => "cancelled",
            SandboxError::FuelExhausted { .. } => "fuel_exhausted",
            SandboxError::MemoryLimit { .. } => "memory_limit",
            SandboxError::ResourceLimit { .. } => "resource_limit",
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
            SandboxError::FuelExhausted { limit, .. } => write!(
                f,
                "{}: the program ran more than {} steps",
                limits::FUEL_EXHAUSTED,
                limit
            ),
            SandboxError::MemoryLimit { limit, .. } => write!(
                f,
                "{}: the program needed more than {}",
                limits::MEMORY_LIMIT_EXCEEDED,
                limits::format_bytes(*limit)
            ),
            SandboxError::ResourceLimit { message, .. } => f.write_str(message),
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
            limits: Limits::none(),
            source_label: "<sandbox>".to_string(),
            memory_baseline: 0,
            engine: Engine::default(),
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

    /// Grant `ffi` (native plugins) for libraries at or under these paths.
    ///
    /// A native library runs with the full privileges of the host process
    /// and bypasses every other capability: only grant libraries you trust
    /// as much as the host itself.
    pub fn allow_ffi<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.caps = self.caps.grant_ffi_paths(paths);
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

    /// Deterministic step budget for one run: one step per statement,
    /// function call and loop iteration. See [`SandboxError::FuelExhausted`].
    pub fn max_fuel(mut self, steps: u64) -> Self {
        self.limits.max_fuel = Some(steps);
        self
    }

    /// Bytes of memory one run may hold. Requires
    /// [`crate::CountingAllocator`] as the global allocator.
    pub fn max_memory(mut self, bytes: usize) -> Self {
        self.limits.max_memory = Some(bytes);
        self
    }

    /// Replace every resource limit at once (fuel, memory, handles, value
    /// sizes, imports).
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// The resource limits this sandbox runs under.
    pub fn resource_limits(&self) -> &Limits {
        &self.limits
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

    /// Run programs on `engine` (default: [`Engine::Vm`]). The containment
    /// is identical on both engines.
    pub fn engine(mut self, engine: Engine) -> Self {
        self.engine = engine;
        self
    }

    /// The engine this sandbox runs programs on.
    pub fn selected_engine(&self) -> Engine {
        self.engine
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
        let program = parse_source(source)?;
        let source = source.to_string();
        let label = self.source_label.clone();
        let engine = self.engine;
        self.run_contained(
            || (),
            cancel,
            move |(), scope| {
                let result = run_program(scope, engine, &program, source, label);
                // Release the program while still under the run's budget.
                drop(program);
                result
            },
        )
        .result
        .map(|((), stdout)| Output { stdout })
    }

    /// Run `job` on the interpreter `make_interp` returns, under this
    /// sandbox ([`Sandbox::run_contained`] with the interpreter contained
    /// before the job starts). The interpreter may carry state from earlier
    /// runs (it is handed back in [`InterpreterRun::interp`]); its
    /// cancellation token, output capture and host-runtime mode are replaced
    /// for this run.
    pub(crate) fn run_interpreter<T: Send + 'static>(
        &self,
        make_interp: impl FnOnce() -> Interpreter + Send + 'static,
        cancel: &CancelHandle,
        job: impl FnOnce(&mut Interpreter) -> Result<T, RuntimeError> + Send + 'static,
    ) -> InterpreterRun<T> {
        let run = self.run_contained(make_interp, cancel, move |interp, scope| {
            scope.contain_interpreter(interp);
            job(interp).map_err(JobError::from)
        });
        InterpreterRun {
            result: run.result,
            interp: run.state,
            memory_held: run.memory_held,
        }
    }

    /// Run `job` under this sandbox: on a fresh worker thread, under the
    /// policy and a fresh resource [`Budget`] (fuel, memory, handles,
    /// imports), with captured and budgeted output, the wall-clock limit,
    /// the host's `cancel` handle and no host runtime (`schedule`, `watch`,
    /// servers).
    ///
    /// This is the one place that applies a sandbox's containment, for
    /// both engines, so every way of running untrusted code (a whole
    /// program, a call into a loaded program, one step of a persistent
    /// session) gets the same guarantees. The containment that is not
    /// thread-scoped is the engine's: the job must hand every interpreter
    /// or VM it runs to [`RunScope::contain_interpreter`] /
    /// [`RunScope::contain_vm`] first, which installs the run's
    /// cancellation token and turns the host runtime off.
    ///
    /// `make_state` runs on the worker, under the budget, so building the
    /// engine (e.g. forking a template) is charged to the run; the state is
    /// handed back in [`ContainedRun::state`]. When the job ends, however it
    /// ends, the run's cancellation token is set, so tasks the code
    /// `spawn`ed and never awaited stop instead of outliving the run.
    pub(crate) fn run_contained<S: Send + 'static, T: Send + 'static>(
        &self,
        make_state: impl FnOnce() -> S + Send + 'static,
        cancel: &CancelHandle,
        job: impl FnOnce(&mut S, &RunScope) -> Result<T, JobError> + Send + 'static,
    ) -> ContainedRun<T, S> {
        let refuse = |error: SandboxError, state: Option<S>| ContainedRun {
            result: Err(error),
            state,
            memory_held: self.memory_baseline,
        };
        let host_cancel = cancel;
        if host_cancel.is_cancelled() {
            let stdout = String::new();
            return refuse(SandboxError::Cancelled { stdout }, Some(make_state()));
        }
        if self.limits.max_memory.is_some() && !limits::allocation_meter_installed() {
            return refuse(
                SandboxError::Runtime {
                    message: "Sandbox::max_memory needs forge_lang::CountingAllocator as the \
                              global allocator to measure the program's memory"
                        .to_string(),
                    line: 0,
                    stdout: String::new(),
                },
                Some(make_state()),
            );
        }
        // A fresh budget per run: fuel, memory and handles are never shared
        // between runs or sandboxes. Memory a caller says the state already
        // holds (a session's interpreter) counts from the start.
        let budget = Budget::new(self.limits.clone());
        budget.preload_memory(self.memory_baseline);
        // The flag the engines poll. Separate from the host's handle, so a
        // timeout or output-limit stop is not reported as a cancel.
        let run_cancel = Arc::new(AtomicBool::new(false));
        // The output capture, shared by every thread of the run (it is
        // inherited like the policy and the budget). Past `max_output` it
        // drops output and stops the run at its next safe point.
        let capture_limit = self.max_output.map_or(usize::MAX, |l| l.saturating_add(1));
        let sink = stdio::Sink::stdout_only(capture_limit, Some(run_cancel.clone()));
        let caps = Arc::new(self.caps.clone());
        let (tx, rx) = mpsc::channel::<(Result<T, JobError>, S)>();

        let scope = RunScope {
            cancel: run_cancel.clone(),
        };
        let worker_sink = sink.clone();
        let worker_budget = budget.clone();
        let spawned = std::thread::Builder::new()
            .name("forge-sandbox".to_string())
            .stack_size(WORKER_STACK_SIZE)
            .spawn(move || {
                crate::runtime::recursion::register_thread_stack(WORKER_STACK_SIZE);
                let _policy = permissions::scope(caps);
                let _limits = limits::scope(Some(worker_budget));
                let _output = stdio::scope(Some(worker_sink));
                let mut state = make_state();
                let result = job(&mut state, &scope);
                // The run is over: stop anything it started and left running.
                scope.cancel.store(true, Ordering::Release);
                let _ = tx.send((result, state));
            });
        if let Err(e) = spawned {
            let message = format!("failed to start sandbox thread: {}", e);
            let stdout = String::new();
            return refuse(
                SandboxError::Runtime {
                    message,
                    line: 0,
                    stdout,
                },
                None,
            );
        }

        let collect = || sink.snapshot().text(stdio::Stream::Stdout);
        // The sink keeps one byte more than the limit, so "printed more
        // than the limit" is visible in its byte count, or in its
        // truncation flag when the cut fell inside a multibyte character
        // (the byte count then backs down to the previous boundary).
        let over_output = || {
            self.max_output
                .is_some_and(|limit| sink.truncated() || sink.bytes() > limit)
        };
        // Stop the worker cooperatively and give it a moment to unwind;
        // either way the host gets control back now. The state comes back
        // only if the worker stopped within the grace period.
        let stop = || -> Option<S> {
            run_cancel.store(true, Ordering::Release);
            rx.recv_timeout(CANCEL_GRACE).ok().map(|(_, state)| state)
        };
        let finish = |result, state| ContainedRun {
            result,
            state,
            memory_held: budget.memory_used(),
        };
        let output_limit = |limit: usize| SandboxError::OutputLimit {
            limit,
            stdout: truncate_utf8(collect(), limit),
        };

        let deadline = self.max_time.map(|limit| (limit, Instant::now() + limit));
        let outcome = loop {
            match rx.recv_timeout(POLL_INTERVAL) {
                Ok(r) => break Some(r),
                Err(mpsc::RecvTimeoutError::Disconnected) => break None,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if host_cancel.is_cancelled() {
                let state = stop();
                let stdout = collect();
                return finish(Err(SandboxError::Cancelled { stdout }), state);
            }
            if let Some((limit, at)) = deadline {
                if Instant::now() >= at {
                    let state = stop();
                    let stdout = collect();
                    return finish(Err(SandboxError::Timeout { limit, stdout }), state);
                }
            }
            if let Some(limit) = self.max_output.filter(|_| over_output()) {
                let state = stop();
                return finish(Err(output_limit(limit)), state);
            }
        };
        let (outcome, state) = match outcome {
            Some((result, state)) => (Some(result), Some(state)),
            None => (None, None),
        };
        let result = if let Some(limit) = self.max_output.filter(|_| over_output()) {
            Err(output_limit(limit))
        } else if host_cancel.is_cancelled() && outcome.as_ref().is_some_and(|r| r.is_err()) {
            Err(SandboxError::Cancelled { stdout: collect() })
        } else if let Some(trip) = budget.tripped() {
            // A fatal limit is reported even if a builtin swallowed the
            // error (the budget remembers the trip).
            Err(self.limit_error(trip, collect()))
        } else {
            let stdout = collect();
            match outcome {
                Some(Ok(value)) => Ok((value, stdout)),
                Some(Err(e)) => Err(self.classify_error(e.message, e.line, stdout)),
                None => Err(SandboxError::Runtime {
                    message: "sandbox worker panicked".to_string(),
                    line: 0,
                    stdout,
                }),
            }
        };
        finish(result, state)
    }

    /// Bytes the interpreter handed to [`Sandbox::run_interpreter`] already
    /// holds (a persistent session's state). They count against
    /// [`Sandbox::max_memory`] from the start of the run, so state carried
    /// across runs stays bounded by the limit.
    pub(crate) fn memory_baseline(mut self, bytes: usize) -> Self {
        self.memory_baseline = bytes;
        self
    }

    /// Map an interpreter error to the sandbox error a host sees.
    fn classify_error(&self, message: String, line: usize, stdout: String) -> SandboxError {
        match limits::classify(&message) {
            Some(LimitKind::Fuel) => return self.limit_error(Trip::Fuel, stdout),
            Some(LimitKind::Memory) => return self.limit_error(Trip::Memory, stdout),
            Some(LimitKind::Resource) => return SandboxError::ResourceLimit { message, stdout },
            None => {}
        }
        if message.starts_with("permission denied:") {
            SandboxError::PermissionDenied { message, stdout }
        } else {
            SandboxError::Runtime {
                message,
                line,
                stdout,
            }
        }
    }
}

/// What [`Sandbox::run_interpreter`] produced.
pub(crate) struct InterpreterRun<T> {
    /// The job's value and everything the run printed, or why it failed.
    pub result: Result<(T, String), SandboxError>,
    /// The interpreter, with whatever state the run left in it. `None` when
    /// the worker had to be abandoned (it did not stop within the grace
    /// period after a timeout or cancel) or panicked.
    pub interp: Option<Interpreter>,
    /// Bytes the run's interpreter holds at the end according to the
    /// allocation meter, including the [`Sandbox::memory_baseline`] (0 when
    /// memory is not limited or the meter is not installed).
    pub memory_held: usize,
}

/// What [`Sandbox::run_contained`] produced.
pub(crate) struct ContainedRun<T, S> {
    /// The job's value and everything the run printed, or why it failed.
    pub result: Result<(T, String), SandboxError>,
    /// The job's state, as the run left it. `None` when the worker had to
    /// be abandoned (it did not stop within the grace period after a
    /// timeout or cancel) or panicked.
    pub state: Option<S>,
    /// Bytes the run holds at the end according to the allocation meter,
    /// including the [`Sandbox::memory_baseline`] (0 when memory is not
    /// limited or the meter is not installed).
    pub memory_held: usize,
}

/// The engine that runs sandboxed code.
///
/// Both engines run under exactly the same containment
/// ([`Sandbox::run_contained`]): policy, resource budget, output capture
/// and budget, deadline, cancellation, no host runtime. A program the VM
/// cannot run faithfully (e.g. an unknown decorator) runs on the
/// interpreter instead, inside the same sandbox, as `forge run` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Engine {
    /// The bytecode VM, the default engine of `forge run`.
    #[default]
    Vm,
    /// The tree-walking interpreter.
    Interpreter,
}

impl Engine {
    /// `"vm"` or `"interp"`.
    pub fn name(self) -> &'static str {
        match self {
            Engine::Vm => "vm",
            Engine::Interpreter => "interp",
        }
    }

    /// Parse `vm` or `interp` / `interpreter`.
    pub fn parse(s: &str) -> Option<Engine> {
        match s.trim().to_ascii_lowercase().as_str() {
            "vm" => Some(Engine::Vm),
            "interp" | "interpreter" => Some(Engine::Interpreter),
            _ => None,
        }
    }
}

impl std::fmt::Display for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The containment a [`Sandbox::run_contained`] job applies to every
/// engine instance it runs. Policy, budget and output capture are scoped to
/// the worker thread (and inherited by every thread the engines start);
/// these are the parts that live in the engine.
pub(crate) struct RunScope {
    /// The run's cancellation token: set by a deadline, a host cancel, the
    /// output budget, and when the job ends.
    cancel: Arc<AtomicBool>,
}

impl RunScope {
    /// The run's cancellation token.
    pub(crate) fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    /// Contain an interpreter for this run: the run's cancellation token,
    /// output through the run's capture, no host runtime (`schedule`,
    /// `watch`). Children it creates (`child_context`) inherit all three.
    pub(crate) fn contain_interpreter(&self, interp: &mut Interpreter) {
        interp.cancelled = self.cancel.clone();
        // Output goes through `runtime::stdio`, i.e. the run's capture.
        interp.output_sink = None;
        interp.output_budget = None;
        interp.set_defer_host_runtime(true);
    }

    /// Contain a VM for this run: the run's cancellation token (forked
    /// tasks share it), no host runtime, and the run's memory limit
    /// enforced across all its threads.
    pub(crate) fn contain_vm(&self, vm: &mut VM) {
        vm.set_cancel_flag(self.cancel.clone());
        vm.defer_host_runtime();
        vm.poll_allocation_meter();
    }
}

/// How a [`Sandbox::run_contained`] job failed, on either engine.
pub(crate) struct JobError {
    pub message: String,
    /// 1-based source line, 0 when unknown.
    pub line: usize,
}

impl JobError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        JobError {
            message: message.into(),
            line: 0,
        }
    }
}

impl From<RuntimeError> for JobError {
    fn from(e: RuntimeError) -> Self {
        JobError {
            message: e.message,
            line: e.line,
        }
    }
}

impl From<VMError> for JobError {
    fn from(e: VMError) -> Self {
        JobError {
            line: embed::error_line(&e),
            message: e.message,
        }
    }
}

/// Run a whole program on `engine` (falling back to the interpreter for
/// programs the VM cannot run faithfully), contained by `scope`. Compiling
/// happens here, on the worker, because it reads imported modules under
/// the run's policy.
fn run_program(
    scope: &RunScope,
    engine: Engine,
    program: &Program,
    source: String,
    label: String,
) -> Result<(), JobError> {
    if engine == Engine::Vm {
        match embed::compile_program(program, None) {
            Ok(chunk) => {
                let mut vm = VM::new();
                scope.contain_vm(&mut vm);
                let result = vm.execute(&chunk).map(|_| ()).map_err(JobError::from);
                // Release the heap while still under the run's budget.
                drop(vm);
                return result;
            }
            Err(embed::CompileFailure::Error(message)) => return Err(JobError::new(message)),
            Err(embed::CompileFailure::Unsupported) => {}
        }
    }
    let mut interp = Interpreter::new();
    interp.source = Some(source);
    interp.source_file = Some(label.into());
    scope.contain_interpreter(&mut interp);
    interp.run(program).map(|_| ()).map_err(JobError::from)
}

/// Lex and parse `source`, reporting failures as [`SandboxError::Syntax`].
pub(crate) fn parse_source(source: &str) -> Result<crate::parser::ast::Program, SandboxError> {
    let tokens = Lexer::new(source)
        .tokenize()
        .map_err(|e| SandboxError::Syntax {
            message: e.to_string(),
        })?;
    Parser::new(tokens)
        .parse_program()
        .map_err(|e| SandboxError::Syntax {
            message: e.to_string(),
        })
}

impl Sandbox {
    fn limit_error(&self, trip: Trip, stdout: String) -> SandboxError {
        match trip {
            Trip::Fuel => SandboxError::FuelExhausted {
                limit: self.limits.max_fuel.unwrap_or(0),
                stdout,
            },
            Trip::Memory => SandboxError::MemoryLimit {
                limit: self.limits.max_memory.unwrap_or(0),
                stdout,
            },
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

    const ENGINES: [Engine; 2] = [Engine::Vm, Engine::Interpreter];

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
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            captures_stdout_on(engine);
        }
    }

    fn captures_stdout_on(engine: Engine) {
        let out = Sandbox::new()
            .engine(engine)
            .run_source("say \"hi\"\nprint(\"a\")\nprintln(1 + 2)")
            .expect("runs");
        assert_eq!(out.stdout, "hi\na3\n");
    }

    #[test]
    fn syntax_and_runtime_errors() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            syntax_and_runtime_errors_on(engine);
        }
    }

    fn syntax_and_runtime_errors_on(engine: Engine) {
        assert!(matches!(
            Sandbox::new().engine(engine).run_source("let = ="),
            Err(SandboxError::Syntax { .. })
        ));
        match Sandbox::new()
            .engine(engine)
            .run_source("say \"before\"\nlet x = 1 / 0")
        {
            Err(SandboxError::Runtime { stdout, .. }) => assert_eq!(stdout, "before\n"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn default_deny_every_capability() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            default_deny_every_capability_on(engine);
        }
    }

    fn default_deny_every_capability_on(engine: Engine) {
        let sb = Sandbox::new().engine(engine);
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
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            grants_are_honoured_on(engine);
        }
    }

    fn grants_are_honoured_on(engine: Engine) {
        let out = Sandbox::new().engine(engine)
            .allow(Capability::Env)
            .allow(Capability::Db)
            .run_source(
                "env.set(\"FORGE_SANDBOX_T\", \"1\")\nsay env.get(\"FORGE_SANDBOX_T\")\ndb.open(\":memory:\")\nsay \"db ok\"",
            )
            .expect("granted");
        assert_eq!(out.stdout, "1\ndb ok\n");
    }

    #[test]
    fn host_stdin_is_not_readable_without_process() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            host_stdin_is_not_readable_without_process_on(engine);
        }
    }

    fn host_stdin_is_not_readable_without_process_on(engine: Engine) {
        // Must return immediately (empty stream), never block on or consume
        // the host's stdin; the prompt is not printed either.
        let out = Sandbox::new()
            .engine(engine)
            .run_source("let a = io.prompt(\"name? \")\nlet b = input()\nsay \"[\" + a + b + \"]\"")
            .expect("stdin reads are not errors");
        assert_eq!(out.stdout, "[]\n");
    }

    #[test]
    fn scoped_filesystem() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            scoped_filesystem_on(engine);
        }
    }

    fn scoped_filesystem_on(engine: Engine) {
        let root = tmpdir("fs");
        let data = root.join("data");
        std::fs::create_dir_all(&data).expect("mkdir");
        std::fs::write(data.join("in.txt"), "hello").expect("write");
        std::fs::write(root.join("secret.txt"), "s3cret").expect("write");
        let sb = Sandbox::new()
            .engine(engine)
            .allow_read([&data])
            .allow_write([&data]);
        // Forge string literals treat `\` as an escape (Windows paths).
        let d = data.display().to_string().replace('\\', "\\\\");
        let root_lit = root.display().to_string().replace('\\', "\\\\");
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
            sb.run_source(&format!("fs.write(\"{root_lit}/x.txt\", \"x\")")),
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
            .run_source(&format!("say fs.exists(\"{root_lit}/secret.txt\")"))
            .expect("exists is not an error");
        assert_eq!(out.stdout, "false\n");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unawaited_tasks_stop_when_the_run_ends() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            unawaited_tasks_stop_when_the_run_ends_on(engine);
        }
    }

    fn unawaited_tasks_stop_when_the_run_ends_on(engine: Engine) {
        // A task the program never awaits must not keep running (and
        // burning CPU or writing files) after the run returns.
        let dir = tmpdir("leak");
        let file = dir.join("ticks.txt");
        let lit = file.display().to_string().replace('\\', "\\\\");
        let out = Sandbox::new().engine(engine)
            .allow_write([&dir])
            .allow_read([&dir])
            .run_source(&format!(
                "spawn {{ while true {{ fs.append(\"{lit}\", \"x\")\nwait(0.01) }} }}\nwait(0.1)\nsay \"done\""
            ))
            .expect("runs");
        assert_eq!(out.stdout, "done\n");
        // Let a straggler notice the cancellation, then check it stopped.
        std::thread::sleep(Duration::from_millis(200));
        let size = || std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        let before = size();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(size(), before, "the task kept running after the run");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn memory_baseline_counts_against_the_limit() {
        // A session's carried-over state counts from the first statement.
        let run = |baseline: usize| {
            let program =
                parse_source("let mut i = 0\nwhile i < 100 { i = i + 1 }\nsay i").expect("parses");
            Sandbox::new()
                .max_memory(32 << 20)
                .memory_baseline(baseline)
                .run_interpreter(Interpreter::new, &CancelHandle::new(), move |i| {
                    i.run(&program).map(|_| ())
                })
        };
        let small = run(1 << 20);
        assert_eq!(small.result.expect("fits").1, "100\n");
        assert!(small.memory_held >= 1 << 20);
        let full = run(64 << 20);
        assert!(
            matches!(full.result, Err(SandboxError::MemoryLimit { .. })),
            "{:?}",
            full.result.err()
        );
    }

    #[test]
    fn run_interpreter_hands_back_state() {
        let sb = Sandbox::new().max_time(Duration::from_secs(5));
        let program = parse_source("let x = 41").expect("parses");
        let run = sb.run_interpreter(Interpreter::new, &CancelHandle::new(), move |i| {
            i.run(&program).map(|_| ())
        });
        assert!(run.result.is_ok());
        let interp = run.interp.expect("interpreter comes back");
        let program = parse_source("say x + 1").expect("parses");
        let run = sb.run_interpreter(
            move || interp,
            &CancelHandle::new(),
            move |i| i.run(&program).map(|_| ()),
        );
        assert_eq!(run.result.expect("runs").1, "42\n");
        // After a timeout the interpreter still comes back when the worker
        // stops within the grace period.
        let sb = Sandbox::new().max_time(Duration::from_millis(200));
        let program = parse_source("while true { }").expect("parses");
        let interp = run.interp.expect("interp");
        let run = sb.run_interpreter(
            move || interp,
            &CancelHandle::new(),
            move |i| i.run(&program).map(|_| ()),
        );
        assert!(matches!(run.result, Err(SandboxError::Timeout { .. })));
        assert!(run.interp.is_some());
    }

    #[test]
    fn spawned_tasks_inherit_the_sandbox() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            spawned_tasks_inherit_the_sandbox_on(engine);
        }
    }

    fn spawned_tasks_inherit_the_sandbox_on(engine: Engine) {
        let r = Sandbox::new()
            .engine(engine)
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
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            max_time_stops_infinite_loop_on(engine);
        }
    }

    fn max_time_stops_infinite_loop_on(engine: Engine) {
        let start = std::time::Instant::now();
        let r = Sandbox::new()
            .engine(engine)
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
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            max_time_does_not_wait_for_blocking_sleep_on(engine);
        }
    }

    fn max_time_does_not_wait_for_blocking_sleep_on(engine: Engine) {
        let start = std::time::Instant::now();
        let r = Sandbox::new()
            .engine(engine)
            .max_time(Duration::from_millis(200))
            .run_source("wait(30)");
        assert!(matches!(r, Err(SandboxError::Timeout { .. })), "{r:?}");
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn output_from_tasks_timeouts_and_io_print_is_captured() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            output_from_tasks_timeouts_and_io_print_is_captured_on(engine);
        }
    }

    fn output_from_tasks_timeouts_and_io_print_is_captured_on(engine: Engine) {
        let out = Sandbox::new().engine(engine)
            .run_source(
                "io.print(\"a\")\nlet h = spawn { say \"from task\" }\nawait h\ntimeout 5 seconds { say \"in timeout\" }",
            )
            .expect("runs");
        assert_eq!(out.stdout, "afrom task\nin timeout\n");
    }

    #[test]
    fn max_output_stops_runaway_printing() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            max_output_stops_runaway_printing_on(engine);
        }
    }

    fn max_output_stops_runaway_printing_on(engine: Engine) {
        let start = std::time::Instant::now();
        let r = Sandbox::new()
            .engine(engine)
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
            .engine(engine)
            .max_output(10)
            .run_source("say \"ok\"")
            .is_ok());
    }

    #[test]
    fn output_limit_cut_inside_a_multibyte_char_is_reported() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            // The capture keeps limit + 1 = 2 bytes, inside the 4-byte
            // emoji: nothing is stored, but output still overflowed.
            match Sandbox::new()
                .engine(engine)
                .max_output(1)
                .run_source("say \"\u{1F600}\"")
            {
                Err(SandboxError::OutputLimit { limit, stdout }) => {
                    assert_eq!(limit, 1);
                    assert_eq!(stdout, "");
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn cancel_handle_stops_a_run() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            cancel_handle_stops_a_run_on(engine);
        }
    }

    fn cancel_handle_stops_a_run_on(engine: Engine) {
        let handle = CancelHandle::new();
        let remote = handle.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            remote.cancel();
        });
        let start = std::time::Instant::now();
        let r = Sandbox::new()
            .engine(engine)
            .run_source_cancellable("say \"started\"\nwhile true { }", &handle);
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
            Sandbox::new()
                .engine(engine)
                .run_source_cancellable("say 1", &handle),
            Err(SandboxError::Cancelled { .. })
        ));
    }

    /// Run `src` on its own interpreter thread, cancel it after a moment,
    /// and require the thread itself to finish (not just the host to stop
    /// waiting): a blocked or spinning worker would outlive the sandbox.
    #[track_caller]
    fn assert_cancel_unblocks(src: &str) {
        let program = Parser::new(Lexer::new(src).tokenize().expect("lex"))
            .parse_program()
            .expect("parse");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (tx, rx) = mpsc::channel();
        crate::runtime::recursion::spawn_worker(move || {
            let mut interp = Interpreter::new();
            interp.cancelled = flag;
            interp.set_defer_host_runtime(true);
            let _ = tx.send(interp.run(&program).is_err());
        })
        .expect("spawn");
        std::thread::sleep(Duration::from_millis(150));
        cancel.store(true, Ordering::Release);
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(errored) => assert!(errored, "{src}: finished without error"),
            Err(_) => panic!("{src}: worker still running 5s after cancel"),
        }
    }

    #[test]
    fn cancellation_reaches_blocked_and_nested_work() {
        for src in [
            "let ch = channel()\nreceive(ch)",
            "let ch = channel()\nfor x in ch { say x }",
            "let ch = channel()\nselect([ch])",
            "let ch = channel()\nlet h = spawn { return receive(ch) }\nawait h",
            "let ch = channel()\nlet h = spawn { receive(ch) }\nawait_all([h])",
            "squad { spawn { while true { } } }",
            "squad { while true { } }",
            "timeout 100000 seconds { while true { } }",
            "timeout 100000 seconds { let h = spawn { while true { } }\nawait h }",
            "time.sleep(100000)",
            "wait(100000)",
            "while true { }",
        ] {
            assert_cancel_unblocks(src);
        }
    }

    /// The same check through the sandbox, on either engine: cancel the
    /// run after a moment and require its worker to finish (the job sends
    /// once the engine has returned), not just the host to stop waiting.
    #[track_caller]
    fn assert_sandbox_cancel_unblocks(engine: Engine, src: &str) {
        let program = parse_source(src).expect("parse");
        let (done_tx, done_rx) = mpsc::channel();
        let handle = CancelHandle::new();
        let remote = handle.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            remote.cancel();
        });
        let run = Sandbox::new().engine(engine).run_contained(
            || (),
            &handle,
            move |(), scope| {
                let r = run_program(scope, engine, &program, String::new(), String::new());
                let _ = done_tx.send(());
                r
            },
        );
        canceller.join().expect("canceller");
        assert!(
            matches!(run.result, Err(SandboxError::Cancelled { .. })),
            "{engine} {src}: {:?}",
            run.result.err()
        );
        assert!(
            done_rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "{engine} {src}: worker still running 5s after cancel"
        );
    }

    #[test]
    fn sandbox_cancellation_reaches_blocked_and_nested_work() {
        for engine in ENGINES {
            for src in [
                "let ch = channel()\nreceive(ch)",
                "let ch = channel()\nfor x in ch { say x }",
                "let ch = channel()\nselect([ch])",
                "let ch = channel()\nlet h = spawn { return receive(ch) }\nawait h",
                "let ch = channel()\nlet h = spawn { receive(ch) }\nawait_all([h])",
                "squad { spawn { while true { } } }",
                "squad { while true { } }",
                "timeout 100000 seconds { while true { } }",
                "timeout 100000 seconds { let h = spawn { while true { } }\nawait h }",
                "time.sleep(100000)",
                "wait(100000)",
                "while true { }",
            ] {
                assert_sandbox_cancel_unblocks(engine, src);
            }
        }
    }

    #[test]
    fn vm_imports_are_read_under_the_policy() {
        // Compiling an import reads the module (for its exported names):
        // that read needs `fs.read` like running it, so a sandboxed program
        // cannot probe files outside its grant or leak them through lex and
        // parse errors.
        let dir = tmpdir("vm_import");
        let secret = dir.join("secret.fg");
        std::fs::write(&secret, "root:x:0:0:secret line\n").expect("write");
        let lit = secret.display().to_string().replace('\\', "/");
        for engine in ENGINES {
            let r = Sandbox::new()
                .engine(engine)
                .run_source(&format!("import \"{lit}\""));
            match &r {
                Err(SandboxError::PermissionDenied { message, .. }) => {
                    assert!(message.contains("fs.read"), "{engine}: {message}");
                    assert!(!message.contains("secret line"), "{engine}: {message}");
                }
                other => panic!("{engine}: {other:?}"),
            }
        }
        // With the grant, the import works on both engines.
        std::fs::write(dir.join("m.fg"), "fn helper() { return 41 + 1 }").expect("write");
        let dir_lit = dir.display().to_string().replace('\\', "/");
        for engine in ENGINES {
            let out = Sandbox::new()
                .engine(engine)
                .allow_read([&dir])
                .run_source(&format!("import \"{dir_lit}/m.fg\"\nsay helper()"))
                .unwrap_or_else(|e| panic!("{engine}: {e}"));
            assert_eq!(out.stdout, "42\n", "{engine}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_programs_fall_back_to_the_contained_interpreter() {
        // An unknown decorator: the VM cannot honor it, so the program runs
        // on the interpreter, still inside the sandbox.
        let r = Sandbox::new()
            .engine(Engine::Vm)
            .run_source("@cache\nfn f() { return 1 }\nsay f()\nsh(\"echo escaped\")");
        match r {
            Err(SandboxError::PermissionDenied { message, stdout }) => {
                assert_eq!(stdout, "1\n");
                assert!(message.starts_with("permission denied: run"), "{message}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn memory_limit_covers_every_task_of_the_run() {
        // Four tasks holding ~12 MB each: none alone passes 32 MB, together
        // they do. On the VM every task has its own GC heap (which only
        // bounds itself); the run-wide allocation meter catches the sum.
        let src = "let mut hs = []\n\
                   for t in range(0, 4) {\n\
                     hs.push(spawn {\n\
                       let mut k = []\n\
                       let mut j = 0\n\
                       while j < 12000 { k.push(repeat_str(\"x\", 1000) + str(j))\n j = j + 1 }\n\
                       wait(2)\n\
                       return len(k)\n\
                     })\n\
                   }\n\
                   for h in hs { say await h }";
        for engine in ENGINES {
            let r = Sandbox::new()
                .engine(engine)
                .max_memory(32 << 20)
                .max_time(Duration::from_secs(60))
                .run_source(src);
            assert!(
                matches!(r, Err(SandboxError::MemoryLimit { .. })),
                "{engine}: {r:?}"
            );
        }
    }

    #[test]
    fn engine_names() {
        assert_eq!(Engine::default(), Engine::Vm);
        assert_eq!(Engine::parse("vm"), Some(Engine::Vm));
        assert_eq!(Engine::parse("interp"), Some(Engine::Interpreter));
        assert_eq!(Engine::parse("Interpreter"), Some(Engine::Interpreter));
        assert_eq!(Engine::parse("jit"), None);
        assert_eq!(Engine::Interpreter.to_string(), "interp");
        assert_eq!(Sandbox::new().selected_engine(), Engine::Vm);
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

    // -- resource limits -------------------------------------------------

    const COUNTER: &str =
        "let mut i = 0\nwhile true {\n  i = i + 1\n  if i % 100 == 0 { say i }\n}";

    #[test]
    fn fuel_exhaustion_is_typed_and_deterministic() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            fuel_exhaustion_is_typed_and_deterministic_on(engine);
        }
    }

    fn fuel_exhaustion_is_typed_and_deterministic_on(engine: Engine) {
        let sb = Sandbox::new().engine(engine).max_fuel(20_000);
        let first = sb.run_source(COUNTER);
        let second = sb.run_source(COUNTER);
        match (&first, &second) {
            (
                Err(SandboxError::FuelExhausted { limit, stdout }),
                Err(SandboxError::FuelExhausted { stdout: again, .. }),
            ) => {
                assert_eq!(*limit, 20_000);
                assert!(!stdout.is_empty());
                // Exhaustion happens at exactly the same step every run.
                assert_eq!(stdout, again);
            }
            other => panic!("{other:?}"),
        }
        let e = first.expect_err("fuel");
        assert_eq!(e.kind(), "fuel_exhausted");
        assert!(e.to_string().starts_with("fuel exhausted"), "{e}");
    }

    #[test]
    fn fuel_cannot_be_caught_or_swallowed() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            fuel_cannot_be_caught_or_swallowed_on(engine);
        }
    }

    fn fuel_cannot_be_caught_or_swallowed_on(engine: Engine) {
        let sb = Sandbox::new().engine(engine).max_fuel(5_000);
        for src in [
            "try { while true { } } catch e { say \"caught\" }\nsay \"after\"",
            "safe { while true { } }\nsay \"after\"",
            "let t = assert_throws(fn() { while true { } })\nsay \"after\"",
            // A swallowed trip as the very last statement still fails the run.
            "let t = assert_throws(fn() { while true { } })",
        ] {
            match sb.run_source(src) {
                Err(SandboxError::FuelExhausted { stdout, .. }) => {
                    assert!(
                        !stdout.contains("caught") && !stdout.contains("after"),
                        "{src}: {stdout}"
                    )
                }
                other => panic!("{src}: {other:?}"),
            }
        }
    }

    #[test]
    fn fuel_counts_recursion_and_empty_loops() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            fuel_counts_recursion_and_empty_loops_on(engine);
        }
    }

    fn fuel_counts_recursion_and_empty_loops_on(engine: Engine) {
        let sb = Sandbox::new().engine(engine).max_fuel(10_000);
        for src in [
            "while true { }",
            "loop { }",
            "fn f(n) { return f(n + 1) }\nf(0)",
            "for x in range(0, 100000) { }",
        ] {
            assert!(
                matches!(sb.run_source(src), Err(SandboxError::FuelExhausted { .. })),
                "{src}"
            );
        }
    }

    #[test]
    fn memory_limit_is_typed_and_the_host_survives() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            memory_limit_is_typed_and_the_host_survives_on(engine);
        }
    }

    fn memory_limit_is_typed_and_the_host_survives_on(engine: Engine) {
        let start = std::time::Instant::now();
        let sb = Sandbox::new()
            .engine(engine)
            .max_memory(16 << 20)
            .max_time(Duration::from_secs(60));
        let r = sb.run_source(
            "say \"start\"\nlet mut kept = []\nwhile true { kept.push(\"some text that stays alive \" + str(len(kept))) }",
        );
        match r {
            Err(e @ SandboxError::MemoryLimit { .. }) => {
                assert_eq!(e.kind(), "memory_limit");
                assert_eq!(e.stdout(), "start\n");
                assert!(e.to_string().starts_with("memory limit exceeded"), "{e}");
            }
            other => panic!("{other:?}"),
        }
        assert!(start.elapsed() < Duration::from_secs(30));
        // The host is fine and the next run starts from a clean budget.
        let out = sb
            .run_source("let xs = range(0, 1000)\nsay len(xs)")
            .expect("runs");
        assert_eq!(out.stdout, "1000\n");
        // Uncatchable, like fuel.
        let r = sb.run_source(
            // Distinct strings: the VM shares one constant string between
            // elements, so pushing a literal would hit the (catchable)
            // collection-length cap long before the memory limit.
            "try { let mut k = []\nwhile true { k.push(\"xxxxxxxxxxxxxxxxxxxxxxxx\" + str(len(k))) } } catch e { say \"caught\" }",
        );
        assert!(matches!(r, Err(SandboxError::MemoryLimit { .. })), "{r:?}");
    }

    #[test]
    fn size_caps_reject_single_huge_allocations() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            size_caps_reject_single_huge_allocations_on(engine);
        }
    }

    fn size_caps_reject_single_huge_allocations_on(engine: Engine) {
        // With a memory limit the caps default to what it could hold, so
        // these fail fast instead of trying to allocate terabytes.
        let sb = Sandbox::new().engine(engine).max_memory(32 << 20);
        for src in [
            "let s = repeat_str(\"x\", 1000000000000)",
            "let r = range(1000000000000)",
            "let p = pad_end(\"x\", 1000000000000)",
        ] {
            match sb.run_source(src) {
                Err(e @ SandboxError::ResourceLimit { .. }) => {
                    assert_eq!(e.kind(), "resource_limit");
                    assert!(e.to_string().starts_with("resource limit exceeded"), "{e}");
                }
                other => panic!("{src}: {other:?}"),
            }
        }
        // Catchable.
        let out = sb
            .run_source("try { let r = range(1000000000000) } catch e { say \"too big\" }")
            .expect("caught");
        assert_eq!(out.stdout, "too big\n");
        // An explicit cap stops doubling concatenation at the cap.
        let capped = Sandbox::new().engine(engine).limits(Limits {
            max_string_bytes: Some(1 << 20),
            ..Limits::none()
        });
        match capped.run_source("let mut s = \"x\"\nwhile true { s = s + s }") {
            Err(SandboxError::ResourceLimit { message, .. }) => {
                assert!(message.contains("limit of 1048576 bytes"), "{message}")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn handle_and_import_limits() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            handle_and_import_limits_on(engine);
        }
    }

    fn handle_and_import_limits_on(engine: Engine) {
        let tasks = Sandbox::new().engine(engine).limits(Limits {
            max_tasks: Some(1),
            ..Limits::none()
        });
        match tasks.run_source("let a = spawn { wait(0.3) }\nlet b = spawn { return 1 }") {
            Err(SandboxError::ResourceLimit { message, .. }) => {
                assert!(message.contains("too many tasks (limit 1)"), "{message}")
            }
            other => panic!("{other:?}"),
        }
        assert!(tasks
            .run_source("let a = spawn { return 1 }\nawait a\nlet b = spawn { return 2 }\nawait b")
            .is_ok());

        let procs = Sandbox::new()
            .engine(engine)
            .allow(Capability::Run)
            .limits(Limits {
                max_processes: Some(0),
                ..Limits::none()
            });
        assert!(matches!(
            procs.run_source("sh(\"echo hi\")"),
            Err(SandboxError::ResourceLimit { .. })
        ));

        let dir = tmpdir("imports");
        std::fs::write(dir.join("m.fg"), "fn helper() { return 1 }").expect("write");
        let imports = Sandbox::new()
            .engine(engine)
            .allow_read([&dir])
            .limits(Limits {
                max_imports: Some(0),
                ..Limits::none()
            });
        // Forward slashes: a Windows path's backslashes would be string escapes.
        let dir_lit = dir.display().to_string().replace('\\', "/");
        let src = format!("import \"{dir_lit}/m.fg\"\nsay helper()");
        assert!(
            matches!(
                imports.run_source(&src),
                Err(SandboxError::ResourceLimit { .. })
            ),
            "{:?}",
            imports.run_source(&src)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normal_programs_run_unchanged_under_limits() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            normal_programs_run_unchanged_under_limits_on(engine);
        }
    }

    fn normal_programs_run_unchanged_under_limits_on(engine: Engine) {
        let src = "fn fib(n) { if n < 2 { return n }\nreturn fib(n - 1) + fib(n - 2) }\nlet xs = map(range(0, 50), fn(x) { return x * x })\nlet mut s = \"\"\nfor x in xs { s = s + str(x) }\nsay fib(15)\nsay len(s)\nlet h = spawn { return 7 }\nsay await h";
        let free = Sandbox::new()
            .engine(engine)
            .run_source(src)
            .expect("unlimited");
        let limited = Sandbox::new()
            .engine(engine)
            .limits(Limits {
                max_fuel: Some(10_000_000),
                max_memory: Some(64 << 20),
                max_tasks: Some(4),
                max_imports: Some(4),
                ..Limits::none()
            })
            .run_source(src)
            .expect("limited");
        assert_eq!(free, limited);
        assert!(limited.stdout.starts_with("610\n"), "{}", limited.stdout);
    }
}
