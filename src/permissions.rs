//! Capability-based permission model shared by every execution engine.
//!
//! # Model
//!
//! A [`Capabilities`] value is a *policy*: the set of side effects a Forge
//! program may perform. Every privileged stdlib entry point (filesystem,
//! network, databases, subprocesses, environment, AI calls, process control)
//! asks the **active policy** before acting, through one central check,
//! [`require`] (plus the path/URL helpers built on it). A denial is a
//! [`PermissionError`] whose message always has the same shape:
//!
//! ```text
//! permission denied: fs.write (/etc/passwd) — run with --allow-write or grant it in the host policy
//! ```
//!
//! # Which policy is active?
//!
//! * A process-wide policy set by the host with [`set_global`] (the CLI does
//!   this from its flags). When nothing was set, the default is
//!   [`Capabilities::cli_default`] — everything except `run` — which is
//!   exactly Forge's historical behaviour.
//! * A per-thread override installed with [`scope`]. The embedding API
//!   ([`crate::sandbox::Sandbox`]) runs each program on its own thread under
//!   such a scope, so two sandboxes in one host process never see each
//!   other's grants.
//!
//! # Invariant: forks inherit the policy
//!
//! Engines execute concurrent Forge code (`spawn`, `squad`, `timeout`,
//! `schedule`, `watch`, server handlers) on other OS threads. Every such
//! thread must be started through [`spawn`] (or wrap its closure with
//! [`inherit`]) so it runs under the policy of the thread that forked it.
//! A thread started with a bare `std::thread::spawn` would silently fall back
//! to the process-wide policy — a sandbox escape for embedders. The same
//! wrapper carries the run's resource budget (`runtime::limits`), so a forked
//! thread also charges the fuel, memory and handle limits of its parent.
//!
//! Engine-agnostic by construction: the checks live in the shared stdlib (and
//! as one-line calls in the few builtins each engine implements itself), so
//! the interpreter, the VM and the JIT's fallback all consult the same policy.

use std::cell::RefCell;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

/// One kind of privileged operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Read files and directories (`fs.read`, `fs.list`, `csv.read`, `import`, ...).
    Read,
    /// Create, modify or delete files (`fs.write`, `fs.remove`, `http.download`, ...).
    Write,
    /// Network access: HTTP client, WebSocket client, HTTP server listen.
    Net,
    /// Read or modify environment variables (`env.*`).
    Env,
    /// Database drivers (`db.*`, `pg.*`, `mysql.*`).
    Db,
    /// Spawn subprocesses (`sh`, `shell`, `run_command`, `pipe_to`, ...).
    Run,
    /// LLM calls (`ask`).
    Ai,
    /// Host-process state: `exit()`, `cd()`, reading the host's stdin
    /// (`input()`, `io.prompt`, `term.confirm`, `term.menu`) and its command
    /// line (`io.args*`). Always granted by the CLI; denied by default for
    /// embedders so a script cannot kill the host, consume its stdin (which
    /// `forge mcp` uses for the protocol) or read its arguments. Without it,
    /// stdin reads behave like an empty stream, no prompt is printed and the
    /// argument list is empty.
    Process,
    /// Load native code: `import native "libfoo"`. A loaded library runs
    /// with the full privileges of the process, outside every other check,
    /// so this is equivalent to full trust. Scoped by library path.
    Ffi,
}

impl Capability {
    /// Every capability, in display order.
    pub const ALL: [Capability; 9] = [
        Capability::Read,
        Capability::Write,
        Capability::Net,
        Capability::Env,
        Capability::Db,
        Capability::Run,
        Capability::Ai,
        Capability::Process,
        Capability::Ffi,
    ];

    /// Stable name used in error messages and policy files.
    pub fn name(self) -> &'static str {
        match self {
            Capability::Read => "fs.read",
            Capability::Write => "fs.write",
            Capability::Net => "net",
            Capability::Env => "env",
            Capability::Db => "db",
            Capability::Run => "run",
            Capability::Ai => "ai",
            Capability::Process => "process",
            Capability::Ffi => "ffi",
        }
    }

    /// The CLI flag that grants this capability, if there is one.
    pub fn cli_flag(self) -> Option<&'static str> {
        match self {
            Capability::Read => Some("--allow-read"),
            Capability::Write => Some("--allow-write"),
            Capability::Net => Some("--allow-net"),
            Capability::Env => Some("--allow-env"),
            Capability::Db => Some("--allow-db"),
            Capability::Run => Some("--allow-run"),
            Capability::Ai => Some("--allow-ai"),
            Capability::Process => None,
            Capability::Ffi => Some("--allow-ffi"),
        }
    }

    /// Parse a capability name (`"fs.read"`, `"read"`, `"net"`, ...).
    pub fn parse(s: &str) -> Option<Capability> {
        match s.trim().to_ascii_lowercase().as_str() {
            "fs.read" | "read" => Some(Capability::Read),
            "fs.write" | "write" => Some(Capability::Write),
            "net" | "network" => Some(Capability::Net),
            "env" => Some(Capability::Env),
            "db" => Some(Capability::Db),
            "run" => Some(Capability::Run),
            "ai" => Some(Capability::Ai),
            "process" => Some(Capability::Process),
            "ffi" | "native" => Some(Capability::Ffi),
            _ => None,
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A denied capability check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionError {
    pub capability: Capability,
    /// What was being accessed (a path, a host, a function name); may be empty.
    pub detail: String,
}

impl fmt::Display for PermissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "permission denied: {}", self.capability)?;
        if !self.detail.is_empty() {
            write!(f, " ({})", self.detail)?;
        }
        match self.capability.cli_flag() {
            Some(flag) => write!(f, " — run with {} or grant it in the host policy", flag),
            None => write!(f, " — grant it in the host policy"),
        }
    }
}

impl std::error::Error for PermissionError {}

impl From<PermissionError> for String {
    fn from(e: PermissionError) -> String {
        e.to_string()
    }
}

/// How much of a scoped capability is granted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope<T> {
    Denied,
    All,
    Only(Vec<T>),
}

impl<T> Scope<T> {
    fn is_granted(&self) -> bool {
        !matches!(self, Scope::Denied)
    }

    fn add(&mut self, items: Vec<T>) {
        match self {
            Scope::All => {}
            Scope::Denied => *self = Scope::Only(items),
            Scope::Only(existing) => existing.extend(items),
        }
    }
}

/// One `--allow-net` entry: a host name or IP, optionally `*.`-prefixed for
/// subdomains, optionally with a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRule {
    host: String,
    port: Option<u16>,
}

impl HostRule {
    /// Parse `example.com`, `*.example.com`, `example.com:8080`,
    /// `[::1]:80`, or a full URL (`https://example.com`).
    pub fn parse(raw: &str) -> Option<HostRule> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        let raw = match raw.find("://") {
            Some(i) => &raw[i + 3..],
            None => raw,
        };
        let raw = raw.split('/').next().unwrap_or(raw);
        let (host, port) = split_host_port(raw);
        if host.is_empty() {
            return None;
        }
        Some(HostRule {
            host: host.to_ascii_lowercase(),
            port,
        })
    }

    fn matches(&self, host: &str, port: Option<u16>) -> bool {
        if let (Some(want), Some(got)) = (self.port, port) {
            if want != got {
                return false;
            }
        } else if self.port.is_some() && port.is_none() {
            return false;
        }
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let host = host.to_ascii_lowercase();
        match self.host.strip_prefix("*.") {
            Some(suffix) => host == suffix || host.ends_with(&format!(".{}", suffix)),
            None => host == self.host.trim_start_matches('[').trim_end_matches(']'),
        }
    }
}

impl fmt::Display for HostRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v6 = self.host.contains(':') && !self.host.starts_with('[');
        match (self.port, v6) {
            (Some(p), true) => write!(f, "[{}]:{}", self.host, p),
            (Some(p), false) => write!(f, "{}:{}", self.host, p),
            (None, _) => f.write_str(&self.host),
        }
    }
}

fn split_host_port(raw: &str) -> (&str, Option<u16>) {
    if let Some(rest) = raw.strip_prefix('[') {
        // [v6]:port
        if let Some(end) = rest.find(']') {
            let host = &rest[..end];
            let port = rest[end + 1..]
                .strip_prefix(':')
                .and_then(|p| p.parse().ok());
            return (host, port);
        }
        return (raw, None);
    }
    if raw.matches(':').count() == 1 {
        if let Some((h, p)) = raw.rsplit_once(':') {
            if let Ok(port) = p.parse() {
                return (h, Some(port));
            }
        }
    }
    (raw, None)
}

/// A permission policy. Cheap to clone; immutable once installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    read: Scope<PathBuf>,
    write: Scope<PathBuf>,
    net: Scope<HostRule>,
    env: bool,
    db: bool,
    run: bool,
    ai: bool,
    process: bool,
    /// Native libraries that may be loaded (files or directory trees).
    ffi: Scope<PathBuf>,
    /// Directories from which `import` may load modules even when `fs.read`
    /// does not cover them (the CLI adds the entry script's directory).
    import_roots: Vec<PathBuf>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities::cli_default()
    }
}

impl Capabilities {
    /// Everything allowed, including subprocesses.
    pub fn allow_all() -> Self {
        Capabilities {
            read: Scope::All,
            write: Scope::All,
            net: Scope::All,
            env: true,
            db: true,
            run: true,
            ai: true,
            process: true,
            ffi: Scope::All,
            import_roots: Vec::new(),
        }
    }

    /// Nothing allowed. The starting point for sandboxes.
    pub fn deny_all() -> Self {
        Capabilities {
            read: Scope::Denied,
            write: Scope::Denied,
            net: Scope::Denied,
            env: false,
            db: false,
            run: false,
            ai: false,
            process: false,
            ffi: Scope::Denied,
            import_roots: Vec::new(),
        }
    }

    /// Forge's historical default for `forge run`: everything except
    /// subprocesses (`run` needs `--allow-run`) and native code (`ffi` needs
    /// `--allow-ffi`; it is strictly more powerful than `run`).
    pub fn cli_default() -> Self {
        Capabilities {
            run: false,
            ffi: Scope::Denied,
            ..Capabilities::allow_all()
        }
    }

    /// Grant a capability without restriction.
    pub fn grant(mut self, cap: Capability) -> Self {
        self.set(cap, true);
        self
    }

    /// Revoke a capability entirely.
    pub fn deny(mut self, cap: Capability) -> Self {
        self.set(cap, false);
        self
    }

    /// Grant or revoke a capability without restriction (in place).
    pub fn set(&mut self, cap: Capability, granted: bool) {
        match cap {
            Capability::Read => self.read = if granted { Scope::All } else { Scope::Denied },
            Capability::Write => self.write = if granted { Scope::All } else { Scope::Denied },
            Capability::Net => self.net = if granted { Scope::All } else { Scope::Denied },
            Capability::Env => self.env = granted,
            Capability::Db => self.db = granted,
            Capability::Run => self.run = granted,
            Capability::Ai => self.ai = granted,
            Capability::Process => self.process = granted,
            Capability::Ffi => self.ffi = if granted { Scope::All } else { Scope::Denied },
        }
    }

    /// Allow reading under these paths (files or directory trees). Paths
    /// are resolved now (symlinks followed, made absolute).
    pub fn grant_read_paths<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.read.add(resolve_roots(paths));
        self
    }

    /// Allow writing under these paths (files or directory trees).
    pub fn grant_write_paths<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.write.add(resolve_roots(paths));
        self
    }

    /// Allow network access to these hosts (`example.com`,
    /// `*.example.com`, `example.com:443`). Invalid entries are ignored.
    pub fn grant_net_hosts<I, S>(mut self, hosts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rules = hosts
            .into_iter()
            .filter_map(|h| HostRule::parse(h.as_ref()))
            .collect();
        self.net.add(rules);
        self
    }

    /// Allow loading native libraries at or under these paths (files or
    /// directory trees).
    pub fn grant_ffi_paths<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.ffi.add(resolve_roots(paths));
        self
    }

    /// Allow `import` of modules under `dir` regardless of `fs.read`.
    pub fn grant_import_root(mut self, dir: impl AsRef<Path>) -> Self {
        if let Some(p) = resolve_for_check(dir.as_ref()) {
            self.import_roots.push(p);
        }
        self
    }

    /// Whether `cap` is granted at all (possibly only for some paths/hosts).
    pub fn is_granted(&self, cap: Capability) -> bool {
        match cap {
            Capability::Read => self.read.is_granted(),
            Capability::Write => self.write.is_granted(),
            Capability::Net => self.net.is_granted(),
            Capability::Env => self.env,
            Capability::Db => self.db,
            Capability::Run => self.run,
            Capability::Ai => self.ai,
            Capability::Process => self.process,
            Capability::Ffi => self.ffi.is_granted(),
        }
    }

    /// Whether network access is restricted to an allowlist of hosts.
    pub fn net_is_scoped(&self) -> bool {
        matches!(self.net, Scope::Only(_))
    }

    /// Human-readable summary of what is granted, one entry per granted
    /// capability (`fs.read (/srv/data)`, `net (api.example.com)`, `env`).
    /// Empty when nothing is granted. Used to tell agents and operators
    /// what a sandbox allows.
    pub fn describe(&self) -> Vec<String> {
        fn scoped<T>(name: &str, scope: &Scope<T>, show: impl Fn(&T) -> String) -> Option<String> {
            match scope {
                Scope::Denied => None,
                Scope::All => Some(name.to_string()),
                Scope::Only(items) => Some(format!(
                    "{} ({})",
                    name,
                    items.iter().map(show).collect::<Vec<_>>().join(", ")
                )),
            }
        }
        let mut out = Vec::new();
        out.extend(scoped("fs.read", &self.read, |p| p.display().to_string()));
        out.extend(scoped("fs.write", &self.write, |p| p.display().to_string()));
        out.extend(scoped("net", &self.net, |h| h.to_string()));
        out.extend(scoped("ffi", &self.ffi, |p| p.display().to_string()));
        for cap in [
            Capability::Env,
            Capability::Db,
            Capability::Run,
            Capability::Ai,
            Capability::Process,
        ] {
            if self.is_granted(cap) {
                out.push(cap.name().to_string());
            }
        }
        out
    }

    /// The central check. `detail` is what is being accessed: a path for
    /// `fs.read`/`fs.write`, a host (or URL) for `net`, free text otherwise.
    /// For scoped grants an empty `detail` only passes an `All` grant.
    pub fn check(&self, cap: Capability, detail: &str) -> Result<(), PermissionError> {
        let ok = match cap {
            Capability::Read | Capability::Write | Capability::Ffi => {
                if detail.is_empty() {
                    matches!(self.path_scope(cap), Scope::All)
                } else {
                    return self.check_path(cap, Path::new(detail));
                }
            }
            Capability::Net => {
                if detail.is_empty() {
                    matches!(self.net, Scope::All)
                } else {
                    return self.check_net(detail);
                }
            }
            other => self.is_granted(other),
        };
        if ok {
            Ok(())
        } else {
            Err(denied(cap, detail))
        }
    }

    fn path_scope(&self, cap: Capability) -> &Scope<PathBuf> {
        match cap {
            Capability::Write => &self.write,
            Capability::Ffi => &self.ffi,
            _ => &self.read,
        }
    }

    /// Check a filesystem path against the `fs.read`, `fs.write` or `ffi`
    /// scope.
    /// The path is resolved the way the OS would (symlinks followed, `..`
    /// applied) before comparing, so neither `..` nor a symlink can reach
    /// outside a granted directory.
    pub fn check_path(&self, cap: Capability, path: &Path) -> Result<(), PermissionError> {
        let shown = path.display().to_string();
        match self.path_scope(cap) {
            Scope::All => Ok(()),
            Scope::Denied => Err(denied(cap, &shown)),
            Scope::Only(roots) => match resolve_for_check(path) {
                Some(resolved) if roots.iter().any(|r| resolved.starts_with(r)) => Ok(()),
                _ => Err(denied(cap, &shown)),
            },
        }
    }

    /// Like [`Capabilities::check_path`], but returns the path the caller
    /// must open. Under a scoped grant that is the *resolved absolute* path
    /// that was checked, so a later change of the working directory (another
    /// thread calling `cd`) or of a relative component cannot make the
    /// operation land somewhere other than what was approved. Under an
    /// unrestricted grant the path is returned unchanged (no behaviour
    /// change for trusted scripts).
    pub fn checked_path(&self, cap: Capability, path: &Path) -> Result<PathBuf, PermissionError> {
        let shown = path.display().to_string();
        match self.path_scope(cap) {
            Scope::All => Ok(path.to_path_buf()),
            Scope::Denied => Err(denied(cap, &shown)),
            Scope::Only(roots) => match resolve_for_check(path) {
                Some(resolved) if roots.iter().any(|r| resolved.starts_with(r)) => Ok(resolved),
                _ => Err(denied(cap, &shown)),
            },
        }
    }

    /// Whether `cap` (`fs.read`, `fs.write` or `net`) is granted without any
    /// path or host restriction. Embedded engines (SQLite) that can reach
    /// files or hosts on their own only get those features when this holds.
    pub fn is_unrestricted(&self, cap: Capability) -> bool {
        match cap {
            Capability::Read => matches!(self.read, Scope::All),
            Capability::Write => matches!(self.write, Scope::All),
            Capability::Net => matches!(self.net, Scope::All),
            Capability::Ffi => matches!(self.ffi, Scope::All),
            other => self.is_granted(other),
        }
    }

    /// Check a module import: allowed under an import root or by `fs.read`.
    /// Returns the path to read (see [`Capabilities::checked_path`]).
    pub fn check_import(&self, path: &Path) -> Result<PathBuf, PermissionError> {
        if !self.import_roots.is_empty() {
            if let Some(resolved) = resolve_for_check(path) {
                if self.import_roots.iter().any(|r| resolved.starts_with(r)) {
                    return Ok(resolved);
                }
            }
        }
        self.checked_path(Capability::Read, path)
            .map_err(|e| denied(Capability::Read, &format!("import {}", e.detail)))
    }

    /// Check a network target given as a URL, `host`, or `host:port`.
    pub fn check_net(&self, target: &str) -> Result<(), PermissionError> {
        match &self.net {
            Scope::All => Ok(()),
            Scope::Denied => Err(denied(Capability::Net, target)),
            Scope::Only(rules) => {
                let (host, port) = match url::Url::parse(target) {
                    Ok(u) if u.host_str().is_some() => (
                        u.host_str().unwrap_or_default().to_string(),
                        u.port_or_known_default(),
                    ),
                    _ => {
                        let (h, p) = split_host_port(target);
                        (h.to_string(), p)
                    }
                };
                if rules.iter().any(|r| r.matches(&host, port)) {
                    Ok(())
                } else {
                    Err(denied(Capability::Net, target))
                }
            }
        }
    }
}

fn denied(cap: Capability, detail: &str) -> PermissionError {
    PermissionError {
        capability: cap,
        detail: detail.to_string(),
    }
}

fn resolve_roots<I, P>(paths: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    paths
        .into_iter()
        .filter_map(|p| resolve_for_check(p.as_ref()))
        .collect()
}

/// Resolve `path` the way the OS will when it is opened: made absolute
/// against the current directory, every existing component canonicalised
/// (so symlinks are followed and `..` is applied to the *real* parent), and
/// any trailing not-yet-existing components appended lexically.
///
/// Returns `None` (deny) for a dangling symlink, whose target a write would
/// create somewhere we cannot vouch for.
pub fn resolve_for_check(path: &Path) -> Option<PathBuf> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    let mut resolved = PathBuf::new();
    let mut exists = true;
    for comp in abs.components() {
        match comp {
            Component::Prefix(_) | Component::RootDir => resolved.push(comp.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                let candidate = resolved.join(name);
                if exists {
                    match std::fs::symlink_metadata(&candidate) {
                        Ok(_) => match std::fs::canonicalize(&candidate) {
                            Ok(real) => resolved = real,
                            Err(_) => return None, // dangling symlink or unreadable
                        },
                        Err(_) => {
                            exists = false;
                            resolved = candidate;
                        }
                    }
                } else {
                    resolved = candidate;
                }
            }
        }
    }
    Some(resolved)
}

// ---------------------------------------------------------------------------
// Active policy
// ---------------------------------------------------------------------------

static GLOBAL: RwLock<Option<Arc<Capabilities>>> = RwLock::new(None);

thread_local! {
    static CURRENT: RefCell<Option<Arc<Capabilities>>> = const { RefCell::new(None) };
}

fn default_policy() -> Arc<Capabilities> {
    static DEFAULT: OnceLock<Arc<Capabilities>> = OnceLock::new();
    DEFAULT
        .get_or_init(|| Arc::new(Capabilities::cli_default()))
        .clone()
}

fn global_policy() -> Arc<Capabilities> {
    let guard = GLOBAL.read().unwrap_or_else(|e| e.into_inner());
    guard.clone().unwrap_or_else(default_policy)
}

/// The policy in force on this thread.
pub fn current() -> Arc<Capabilities> {
    CURRENT
        .with(|c| c.borrow().clone())
        .unwrap_or_else(global_policy)
}

/// Replace the process-wide policy (used by threads with no [`scope`]).
pub fn set_global(caps: Capabilities) {
    let mut guard = GLOBAL.write().unwrap_or_else(|e| e.into_inner());
    *guard = Some(Arc::new(caps));
}

/// Restores the previous thread policy when dropped.
#[must_use = "the policy is only in force while the guard lives"]
pub struct PolicyGuard {
    previous: Option<Arc<Capabilities>>,
}

impl Drop for PolicyGuard {
    fn drop(&mut self) {
        let prev = self.previous.take();
        CURRENT.with(|c| *c.borrow_mut() = prev);
    }
}

/// Run this thread under `caps` until the guard is dropped.
pub fn scope(caps: Arc<Capabilities>) -> PolicyGuard {
    let previous = CURRENT.with(|c| c.borrow_mut().replace(caps));
    PolicyGuard { previous }
}

/// Wrap `f` so that, wherever it runs, it runs under the policy that is
/// current *here* — and charges the resource budget that is current here
/// ([`crate::runtime::limits`]). Use for `tokio::task::spawn_blocking` and
/// friends.
pub fn inherit<F, T>(f: F) -> impl FnOnce() -> T + Send + 'static
where
    F: FnOnce() -> T + Send + 'static,
    T: 'static,
{
    let caps = current();
    let budget = crate::runtime::limits::current();
    move || {
        let _guard = scope(caps);
        let _limits = crate::runtime::limits::scope(budget);
        f()
    }
}

/// `std::thread::spawn` that carries the current policy into the new thread.
/// Every engine thread that runs Forge code must be started with this.
pub fn spawn<F, T>(f: F) -> std::thread::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::spawn(inherit(f))
}

// ---------------------------------------------------------------------------
// Check helpers used by stdlib and builtins
// ---------------------------------------------------------------------------

/// The one central check against the active policy. See
/// [`Capabilities::check`] for how `detail` is interpreted.
/// Whether the running script may read the host's stdin (see
/// [`Capability::Process`]). Denied reads behave like an empty stream.
pub fn host_stdin_allowed() -> bool {
    current().check(Capability::Process, "stdin").is_ok()
}

pub fn require(cap: Capability, detail: &str) -> Result<(), PermissionError> {
    current().check(cap, detail)
}

/// [`require`] for a filesystem path.
pub fn require_path(cap: Capability, path: impl AsRef<Path>) -> Result<(), PermissionError> {
    current().check_path(cap, path.as_ref())
}

/// [`require_path`] that returns the path to open (see
/// [`Capabilities::checked_path`]). Every stdlib filesystem entry point
/// opens what this returns, never the caller's original string.
pub fn checked_path(cap: Capability, path: impl AsRef<Path>) -> Result<PathBuf, PermissionError> {
    current().checked_path(cap, path.as_ref())
}

/// [`require`] for a network target (URL, `host` or `host:port`).
pub fn require_net(target: &str) -> Result<(), PermissionError> {
    current().check_net(target)
}

/// [`require_net`] for a raw `host` + `port` pair (database drivers and
/// other non-URL clients). IPv6 literals are bracketed for the check.
pub fn require_net_host(host: &str, port: u16) -> Result<(), PermissionError> {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let target = if host.contains(':') {
        format!("[{}]:{}", host, port)
    } else {
        format!("{}:{}", host, port)
    };
    current().check_net(&target)
}

/// Whether the active policy grants `net` with no host restriction. Clients
/// that can reach endpoints the allowlist cannot describe (Unix sockets)
/// only get them in that case.
pub fn net_unrestricted() -> bool {
    current().is_unrestricted(Capability::Net)
}

/// Check that a native library may be loaded (`import native`).
pub fn require_ffi(path: impl AsRef<Path>) -> Result<(), PermissionError> {
    current().check_path(Capability::Ffi, path.as_ref())
}

/// Check that a module file may be imported; returns the path to read.
pub fn require_import(path: impl AsRef<Path>) -> Result<PathBuf, PermissionError> {
    current().check_import(path.as_ref())
}

/// Grant or revoke `run` in the process-wide policy (standalone binaries
/// built with `forge build --native --allow-run`, tests). The CLI builds a
/// full policy with [`set_global`] instead.
#[allow(dead_code)] // unused in the `forge` binary's copy of this module
pub fn set_allow_run(allowed: bool) {
    let mut guard = GLOBAL.write().unwrap_or_else(|e| e.into_inner());
    let mut caps = guard
        .as_deref()
        .cloned()
        .unwrap_or_else(Capabilities::cli_default);
    caps.set(Capability::Run, allowed);
    *guard = Some(Arc::new(caps));
}

/// Check that subprocess execution is allowed.
pub fn check_run_permission() -> Result<(), String> {
    require(Capability::Run, "shell execution").map_err(String::from)
}

/// Check that subprocess execution is allowed and take a subprocess slot
/// from the run's resource budget (`runtime::limits`). Keep the slot alive
/// until the subprocess has exited. Every builtin that starts a process
/// goes through this.
pub fn begin_subprocess() -> Result<crate::runtime::limits::Slot, String> {
    check_run_permission()?;
    crate::runtime::limits::acquire(crate::runtime::limits::Resource::Processes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "forge_perm_{}_{}_{}",
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

    #[test]
    fn error_message_shape() {
        let e = Capabilities::deny_all()
            .check(Capability::Write, "/etc/passwd")
            .expect_err("denied");
        assert_eq!(
            e.to_string(),
            "permission denied: fs.write (/etc/passwd) — run with --allow-write or grant it in the host policy"
        );
        let e = Capabilities::deny_all()
            .check(Capability::Process, "exit")
            .expect_err("denied");
        assert_eq!(
            e.to_string(),
            "permission denied: process (exit) — grant it in the host policy"
        );
    }

    #[test]
    fn cli_default_matches_historical_behaviour() {
        let c = Capabilities::cli_default();
        for cap in Capability::ALL {
            let opt_in = matches!(cap, Capability::Run | Capability::Ffi);
            assert_eq!(c.is_granted(cap), !opt_in, "{cap}");
        }
    }

    #[test]
    fn deny_all_denies_everything() {
        let c = Capabilities::deny_all();
        for cap in Capability::ALL {
            assert!(c.check(cap, "").is_err(), "{cap}");
        }
        assert!(c.check_path(Capability::Read, Path::new("/tmp")).is_err());
        assert!(c.check_net("https://example.com").is_err());
    }

    #[test]
    fn scoped_paths_block_dotdot_and_symlinks() {
        let root = tmpdir("scope");
        let allowed = root.join("allowed");
        let secret = root.join("secret");
        std::fs::create_dir_all(&allowed).expect("mkdir");
        std::fs::create_dir_all(&secret).expect("mkdir");
        std::fs::write(allowed.join("ok.txt"), "ok").expect("write");
        std::fs::write(secret.join("key.txt"), "key").expect("write");

        let c = Capabilities::deny_all().grant_read_paths([&allowed]);
        assert!(c
            .check_path(Capability::Read, &allowed.join("ok.txt"))
            .is_ok());
        // Not-yet-existing files under the root are fine (write targets).
        assert!(c
            .check_path(Capability::Read, &allowed.join("new/x.txt"))
            .is_ok());
        assert!(c
            .check_path(Capability::Read, &secret.join("key.txt"))
            .is_err());
        assert!(c
            .check_path(Capability::Read, &allowed.join("../secret/key.txt"))
            .is_err());
        assert!(c
            .check_path(Capability::Read, &allowed.join("nope/../../secret/key.txt"))
            .is_err());
        // Read scope does not imply write.
        assert!(c
            .check_path(Capability::Write, &allowed.join("ok.txt"))
            .is_err());

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&secret, allowed.join("link")).expect("symlink");
            assert!(c
                .check_path(Capability::Read, &allowed.join("link/key.txt"))
                .is_err());
            std::os::unix::fs::symlink(secret.join("missing"), allowed.join("dangling"))
                .expect("symlink");
            assert!(c
                .check_path(Capability::Read, &allowed.join("dangling"))
                .is_err());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn checked_path_returns_what_was_approved() {
        // Under a scoped grant the caller gets the resolved absolute path,
        // so a later `cd` (or a swapped relative component) cannot change
        // which file is opened.
        let root = tmpdir("checked");
        let allowed = root.join("allowed");
        std::fs::create_dir_all(&allowed).expect("mkdir");
        let allowed = std::fs::canonicalize(&allowed).expect("canonicalize");
        std::fs::write(allowed.join("a.txt"), "a").expect("write");
        let c = Capabilities::deny_all().grant_read_paths([&allowed]);
        let got = c
            .checked_path(Capability::Read, &allowed.join("x/../a.txt"))
            .expect("inside");
        assert_eq!(got, allowed.join("a.txt"));
        assert!(got.is_absolute());
        assert!(c
            .checked_path(Capability::Read, &allowed.join("../outside.txt"))
            .is_err());
        // Unrestricted grants hand the path back untouched.
        let all = Capabilities::allow_all();
        assert_eq!(
            all.checked_path(Capability::Read, Path::new("rel/x.txt"))
                .expect("all"),
            PathBuf::from("rel/x.txt")
        );
        assert!(all.is_unrestricted(Capability::Read));
        assert!(!c.is_unrestricted(Capability::Read));
        assert!(!c.is_unrestricted(Capability::Net));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn host_allowlist() {
        let c = Capabilities::deny_all().grant_net_hosts([
            "api.example.com",
            "*.test.dev",
            "localhost:8080",
        ]);
        assert!(c.check_net("https://api.example.com/v1").is_ok());
        assert!(c.check_net("https://API.EXAMPLE.COM").is_ok());
        assert!(c.check_net("https://evil.com").is_err());
        assert!(c.check_net("https://api.example.com.evil.com").is_err());
        assert!(c.check_net("https://a.b.test.dev").is_ok());
        assert!(c.check_net("http://localhost:8080/x").is_ok());
        assert!(c.check_net("http://localhost:9090/x").is_err());
        assert!(c.check_net("localhost:8080").is_ok());
        assert!(c.check_net("").is_err());
    }

    #[test]
    fn thread_scope_and_inheritance() {
        let caps = Arc::new(Capabilities::deny_all());
        {
            let _g = scope(caps);
            assert!(require(Capability::Env, "").is_err());
            // A thread started with `spawn` inherits the sandbox...
            let inherited = spawn(|| require(Capability::Env, "").is_err())
                .join()
                .expect("join");
            assert!(inherited);
        }
        // ...and the guard restores the previous (process) policy.
        assert!(require(Capability::Env, "").is_ok());
    }

    #[test]
    fn describe_lists_grants_and_scopes() {
        assert!(Capabilities::deny_all().describe().is_empty());
        let caps = Capabilities::deny_all()
            .grant_net_hosts(["API.example.com", "*.cdn.example.com:8443"])
            .grant(Capability::Env);
        assert_eq!(
            caps.describe(),
            vec![
                "net (api.example.com, *.cdn.example.com:8443)".to_string(),
                "env".to_string()
            ]
        );
        let all = Capabilities::allow_all().describe();
        assert_eq!(all.len(), Capability::ALL.len());
        assert_eq!(all[0], "fs.read");
    }

    #[test]
    fn ffi_is_scoped_by_library_path() {
        let root = tmpdir("ffi");
        let plugins = root.join("plugins");
        std::fs::create_dir_all(&plugins).expect("mkdir");
        std::fs::write(plugins.join("libok.so"), "").expect("write");
        std::fs::write(root.join("libevil.so"), "").expect("write");

        let none = Capabilities::deny_all();
        let e = none
            .check_path(Capability::Ffi, &plugins.join("libok.so"))
            .expect_err("denied");
        assert!(e.to_string().contains("run with --allow-ffi"), "{e}");

        let c = Capabilities::deny_all().grant_ffi_paths([&plugins]);
        assert!(c.is_granted(Capability::Ffi));
        assert!(c
            .check_path(Capability::Ffi, &plugins.join("libok.so"))
            .is_ok());
        assert!(c
            .check_path(Capability::Ffi, &root.join("libevil.so"))
            .is_err());
        assert!(c
            .check_path(Capability::Ffi, &plugins.join("../libevil.so"))
            .is_err());
        // An ffi grant is not a read grant, and vice versa.
        assert!(c
            .check_path(Capability::Read, &plugins.join("libok.so"))
            .is_err());
        let reader = Capabilities::deny_all().grant(Capability::Read);
        assert!(reader
            .check_path(Capability::Ffi, &plugins.join("libok.so"))
            .is_err());
        assert_eq!(
            c.describe(),
            vec![format!(
                "ffi ({})",
                resolve_for_check(&plugins).expect("resolve").display()
            )]
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
