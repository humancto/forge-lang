//! Python bindings for `forge_lang::Sandbox`.
//!
//! The extension module is `forge_lang._native`; `python/forge_lang/__init__.py`
//! re-exports everything as the public `forge_lang` package.
//!
//! Design notes:
//!
//! * **GIL.** A run never holds the GIL while Forge code executes. The run
//!   happens on a dedicated thread; the calling Python thread waits for it
//!   with the GIL released, waking every [`SIGNAL_POLL`] to let Python
//!   deliver signals (Ctrl-C cancels the run and re-raises
//!   `KeyboardInterrupt`) and to forward a user [`CancelToken`].
//! * **Stack.** Lexing, parsing and type-checking are recursive; Python's own
//!   threads may have small stacks (512 KiB on macOS). Every call into Forge
//!   therefore runs on a thread with [`FRONTEND_STACK_SIZE`] reserved (only
//!   touched pages are committed). The interpreter itself runs on the
//!   sandbox's own worker thread.
//! * **Thread safety.** [`Sandbox`] is an immutable (`frozen`) value. Each
//!   `run` builds its own interpreter, so one `Sandbox` can be used from many
//!   Python threads at once.

use forge_lang::mcp::{check_source, Diagnostic as ForgeDiagnostic};
use forge_lang::{CancelHandle, Capability, Sandbox as ForgeSandbox, SandboxError};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyType;
use std::path::PathBuf;
use std::sync::{mpsc, Mutex};
use std::time::Duration;

/// Stack reserved for the thread that lexes/parses/checks a program.
const FRONTEND_STACK_SIZE: usize = 256 * 1024 * 1024;

/// How often a waiting Python thread re-acquires the GIL to check signals
/// and the user's cancel token.
const SIGNAL_POLL: Duration = Duration::from_millis(50);

/// After cancelling on Ctrl-C, how long to wait for the run to unwind. The
/// sandbox itself returns within a few hundred milliseconds of a cancel.
const INTERRUPT_GRACE: Duration = Duration::from_secs(2);

create_exception!(
    forge_lang,
    ForgeError,
    PyException,
    "Base class for every error raised by a Forge run. Carries `kind` (stable string) and `stdout` (output printed before the failure)."
);
create_exception!(
    forge_lang,
    ForgeSyntaxError,
    ForgeError,
    "The source did not lex or parse. `line`/`column` locate the problem when known."
);
create_exception!(
    forge_lang,
    ForgePermissionError,
    ForgeError,
    "The program used a capability its sandbox does not grant (`permission denied: ...`)."
);
create_exception!(
    forge_lang,
    ForgeRuntimeError,
    ForgeError,
    "The program failed at runtime. `line` is the failing line when known."
);
create_exception!(
    forge_lang,
    ForgeTimeoutError,
    ForgeError,
    "The program exceeded `max_time`. `limit` is the limit in seconds."
);
create_exception!(
    forge_lang,
    ForgeOutputLimitError,
    ForgeError,
    "The program printed more than `max_output` bytes. `limit` is the limit in bytes; `stdout` holds the first `limit` bytes."
);
create_exception!(
    forge_lang,
    ForgeCancelledError,
    ForgeError,
    "The run was cancelled through a CancelToken."
);

/// Run `f` on a fresh thread with a large stack, waiting with the GIL
/// released. Signals are checked every [`SIGNAL_POLL`]; `on_tick` runs at
/// the same cadence (with the GIL held) and `on_interrupt` is called before
/// a pending Python exception (e.g. `KeyboardInterrupt`) is propagated.
fn run_off_thread<T, F>(
    py: Python<'_>,
    f: F,
    on_tick: impl Fn(),
    on_interrupt: impl Fn(),
) -> PyResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    // `detach` needs a `Sync` closure; only this thread ever receives.
    let rx = Mutex::new(rx);
    let recv = |timeout| match rx.lock() {
        Ok(rx) => rx.recv_timeout(timeout),
        Err(poisoned) => poisoned.into_inner().recv_timeout(timeout),
    };
    std::thread::Builder::new()
        .name("forge-py".to_string())
        .stack_size(FRONTEND_STACK_SIZE)
        .spawn(move || {
            let _ = tx.send(f());
        })
        .map_err(|e| PyRuntimeError::new_err(format!("failed to start Forge thread: {e}")))?;
    loop {
        match py.detach(|| recv(SIGNAL_POLL)) {
            Ok(v) => return Ok(v),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(PyRuntimeError::new_err("Forge worker thread panicked"))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        on_tick();
        if let Err(e) = py.check_signals() {
            on_interrupt();
            // Give the run a moment to unwind so its thread does not keep
            // burning CPU after the host has moved on.
            let _ = py.detach(|| recv(INTERRUPT_GRACE));
            return Err(e);
        }
    }
}

/// Build the typed Python exception for a failed run. If building it fails
/// (it should not), the error from that failure is raised instead.
fn to_py_err(py: Python<'_>, err: SandboxError, source: &str) -> PyErr {
    build_exception(py, &err, source).unwrap_or_else(|e| e)
}

fn build_exception(py: Python<'_>, err: &SandboxError, source: &str) -> PyResult<PyErr> {
    let ty: Bound<'_, PyType> = match err {
        SandboxError::Syntax { .. } => py.get_type::<ForgeSyntaxError>(),
        SandboxError::PermissionDenied { .. } => py.get_type::<ForgePermissionError>(),
        SandboxError::Runtime { .. } => py.get_type::<ForgeRuntimeError>(),
        SandboxError::Timeout { .. } => py.get_type::<ForgeTimeoutError>(),
        SandboxError::OutputLimit { .. } => py.get_type::<ForgeOutputLimitError>(),
        SandboxError::Cancelled { .. } => py.get_type::<ForgeCancelledError>(),
        // Forward compatibility: if SandboxError grows a variant (e.g. a
        // memory or fuel limit) before this binding maps it, it surfaces as
        // the base class with its own `kind`, instead of breaking the build.
        #[allow(unreachable_patterns)]
        _ => py.get_type::<ForgeError>(),
    };
    let exc = ty.call1((err.to_string(),))?;
    exc.setattr("kind", err.kind())?;
    exc.setattr("stdout", err.stdout())?;
    let none = py.None();
    let (mut line, mut column, mut limit) = (none.clone_ref(py), none.clone_ref(py), none);
    match err {
        SandboxError::Syntax { .. } => {
            // The sandbox reports syntax errors as text; re-run the front
            // end (cheap, and on this already-failed path only) for a
            // structured location.
            if let Some(d) = check_source(source).into_iter().find(|d| d.is_error) {
                if d.line > 0 {
                    line = d.line.into_pyobject(py)?.into_any().unbind();
                }
                if d.column > 0 {
                    column = d.column.into_pyobject(py)?.into_any().unbind();
                }
            }
        }
        SandboxError::Runtime { line: l, .. } if *l > 0 => {
            line = l.into_pyobject(py)?.into_any().unbind();
        }
        SandboxError::Timeout { limit: l, .. } => {
            limit = l.as_secs_f64().into_pyobject(py)?.into_any().unbind();
        }
        SandboxError::OutputLimit { limit: l, .. } => {
            limit = l.into_pyobject(py)?.into_any().unbind();
        }
        _ => {}
    }
    exc.setattr("line", line)?;
    exc.setattr("column", column)?;
    exc.setattr("limit", limit)?;
    Ok(PyErr::from_value(exc))
}

/// What a successful run produced.
#[pyclass(name = "Result", module = "forge_lang", frozen, eq)]
#[derive(PartialEq, Eq)]
pub struct RunResult {
    /// Everything the program printed with `say` / `println` / `print`.
    #[pyo3(get)]
    stdout: String,
}

#[pymethods]
impl RunResult {
    fn __repr__(&self) -> String {
        format!("Result(stdout={:?})", self.stdout)
    }
}

/// One problem reported by `check`.
#[pyclass(name = "Diagnostic", module = "forge_lang", frozen, eq)]
#[derive(PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line, or 0 when unknown.
    #[pyo3(get)]
    line: usize,
    /// 1-based column, or 0 when unknown.
    #[pyo3(get)]
    column: usize,
    /// `True` for errors that stop the program from running; `False` for
    /// warnings from the type checker.
    #[pyo3(get)]
    is_error: bool,
    #[pyo3(get)]
    message: String,
}

#[pymethods]
impl Diagnostic {
    /// `"error"` or `"warning"`.
    #[getter]
    fn severity(&self) -> &'static str {
        if self.is_error {
            "error"
        } else {
            "warning"
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Diagnostic(line={}, column={}, severity={:?}, message={:?})",
            self.line,
            self.column,
            self.severity(),
            self.message
        )
    }

    fn __str__(&self) -> String {
        format!(
            "line {}:{}: {}: {}",
            self.line,
            self.column,
            self.severity(),
            self.message
        )
    }
}

impl From<ForgeDiagnostic> for Diagnostic {
    fn from(d: ForgeDiagnostic) -> Self {
        Diagnostic {
            line: d.line,
            column: d.column,
            is_error: d.is_error,
            message: d.message,
        }
    }
}

/// Lets the host stop a running program from another thread. Once
/// cancelled a token stays cancelled; use a fresh token per run.
#[pyclass(module = "forge_lang", frozen)]
#[derive(Default)]
pub struct CancelToken(CancelHandle);

#[pymethods]
impl CancelToken {
    #[new]
    fn new() -> Self {
        CancelToken::default()
    }

    /// Ask the run to stop. It raises `ForgeCancelledError` promptly.
    fn cancel(&self) {
        self.0.cancel();
    }

    #[getter]
    fn cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn __repr__(&self) -> String {
        let state = if self.0.is_cancelled() {
            "True"
        } else {
            "False"
        };
        format!("CancelToken(cancelled={state})")
    }
}

/// Lex, parse and type-check Forge source without running it.
#[pyfunction]
fn check(py: Python<'_>, code: String) -> PyResult<Vec<Diagnostic>> {
    let diags = run_off_thread(py, move || check_source(&code), || {}, || {})?;
    Ok(diags.into_iter().map(Diagnostic::from).collect())
}

/// A deny-by-default Forge sandbox.
///
/// Immutable and thread-safe: configure it once, then call `run` from any
/// number of threads.
#[pyclass(module = "forge_lang", frozen)]
pub struct Sandbox {
    inner: ForgeSandbox,
    /// Kept for `__repr__` (the core policy type has no public accessors).
    allow: Vec<String>,
    allow_read: Vec<PathBuf>,
    allow_write: Vec<PathBuf>,
    allow_net: Vec<String>,
    max_time: Option<f64>,
    max_output: Option<usize>,
}

#[pymethods]
impl Sandbox {
    #[new]
    #[pyo3(signature = (
        *,
        allow = Vec::new(),
        allow_read = Vec::new(),
        allow_write = Vec::new(),
        allow_net = Vec::new(),
        max_time = None,
        max_output = None,
        label = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        allow: Vec<String>,
        allow_read: Vec<PathBuf>,
        allow_write: Vec<PathBuf>,
        allow_net: Vec<String>,
        max_time: Option<f64>,
        max_output: Option<usize>,
        label: Option<String>,
    ) -> PyResult<Self> {
        let mut inner = ForgeSandbox::new();
        for name in &allow {
            let cap = Capability::parse(name).ok_or_else(|| {
                let known: Vec<&str> = Capability::ALL.iter().map(|c| c.name()).collect();
                PyValueError::new_err(format!(
                    "unknown capability {name:?}; expected one of: {}",
                    known.join(", ")
                ))
            })?;
            inner = inner.allow(cap);
        }
        if !allow_read.is_empty() {
            inner = inner.allow_read(&allow_read);
        }
        if !allow_write.is_empty() {
            inner = inner.allow_write(&allow_write);
        }
        if !allow_net.is_empty() {
            inner = inner.allow_net(&allow_net);
        }
        if let Some(secs) = max_time {
            let limit = Duration::try_from_secs_f64(secs)
                .ok()
                .filter(|d| !d.is_zero())
                .ok_or_else(|| {
                    PyValueError::new_err(format!(
                        "max_time must be a positive number of seconds, got {secs}"
                    ))
                })?;
            inner = inner.max_time(limit);
        }
        if let Some(bytes) = max_output {
            inner = inner.max_output(bytes);
        }
        if let Some(label) = label {
            inner = inner.source_label(label);
        }
        // EXTENSION POINT (resource limits): when the core Sandbox gains
        // instruction fuel / a memory cap, add `max_fuel=` / `max_memory=`
        // keyword arguments here, map the new SandboxError variants to
        // dedicated exception classes in `build_exception` (they currently
        // fall back to ForgeError), and update `_native.pyi`.
        Ok(Sandbox {
            inner,
            allow,
            allow_read,
            allow_write,
            allow_net,
            max_time,
            max_output,
        })
    }

    /// Run Forge source to completion. Returns a `Result`; raises a
    /// `ForgeError` subclass on failure.
    #[pyo3(signature = (code, *, cancel = None))]
    fn run(
        &self,
        py: Python<'_>,
        code: String,
        cancel: Option<Py<CancelToken>>,
    ) -> PyResult<RunResult> {
        // The handle the sandbox watches is private to this run, so a
        // Ctrl-C never marks the caller's token as cancelled.
        let handle = CancelHandle::new();
        let user = cancel.map(|t| t.get().0.clone());
        let forward = || {
            if user.as_ref().is_some_and(CancelHandle::is_cancelled) {
                handle.cancel();
            }
        };
        forward();
        let sandbox = self.inner.clone();
        let worker_handle = handle.clone();
        let source = code.clone();
        let result = run_off_thread(
            py,
            move || sandbox.run_source_cancellable(&source, &worker_handle),
            forward,
            || handle.cancel(),
        )?;
        match result {
            Ok(out) => Ok(RunResult { stdout: out.stdout }),
            Err(e) => Err(to_py_err(py, e, &code)),
        }
    }

    /// Lex, parse and type-check without running. Same as `forge_lang.check`.
    fn check(&self, py: Python<'_>, code: String) -> PyResult<Vec<Diagnostic>> {
        check(py, code)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        // Python's own repr for the lists, so the output reads like the
        // constructor call that would rebuild this sandbox.
        let list = |items: Vec<String>| -> PyResult<String> {
            Ok(items.into_pyobject(py)?.repr()?.to_string())
        };
        let paths = |ps: &[PathBuf]| ps.iter().map(|p| p.display().to_string()).collect();
        let mut parts = vec![format!("allow={}", list(self.allow.clone())?)];
        if !self.allow_read.is_empty() {
            parts.push(format!("allow_read={}", list(paths(&self.allow_read))?));
        }
        if !self.allow_write.is_empty() {
            parts.push(format!("allow_write={}", list(paths(&self.allow_write))?));
        }
        if !self.allow_net.is_empty() {
            parts.push(format!("allow_net={}", list(self.allow_net.clone())?));
        }
        if let Some(t) = self.max_time {
            parts.push(format!("max_time={t}"));
        }
        if let Some(b) = self.max_output {
            parts.push(format!("max_output={b}"));
        }
        Ok(format!("Sandbox({})", parts.join(", ")))
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add_class::<Sandbox>()?;
    m.add_class::<RunResult>()?;
    m.add_class::<Diagnostic>()?;
    m.add_class::<CancelToken>()?;
    m.add_function(wrap_pyfunction!(check, m)?)?;
    m.add("ForgeError", py.get_type::<ForgeError>())?;
    m.add("ForgeSyntaxError", py.get_type::<ForgeSyntaxError>())?;
    m.add(
        "ForgePermissionError",
        py.get_type::<ForgePermissionError>(),
    )?;
    m.add("ForgeRuntimeError", py.get_type::<ForgeRuntimeError>())?;
    m.add("ForgeTimeoutError", py.get_type::<ForgeTimeoutError>())?;
    m.add(
        "ForgeOutputLimitError",
        py.get_type::<ForgeOutputLimitError>(),
    )?;
    m.add("ForgeCancelledError", py.get_type::<ForgeCancelledError>())?;
    m.add(
        "CAPABILITIES",
        Capability::ALL
            .iter()
            .map(|c| c.name())
            .collect::<Vec<&str>>(),
    )?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
