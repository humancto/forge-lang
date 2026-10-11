mod builtins_registry;
mod chat;
mod clock;
mod color;
mod dap;
mod doc;
mod errors;
mod formatter;
mod interpreter;
/// Forge — Internet-Native Programming Language
/// Go's simplicity. Rust's safety. The internet built in.
mod learn;
mod lexer;
mod lsp;
mod manifest;
mod mcp;
mod native;
mod package;
mod parser;
mod permissions;
mod plugins;
mod publish;
mod publish_index;
mod registry;
mod repl;
mod runtime;
// The binary only uses the sandbox through `forge mcp`.
#[allow(dead_code)]
mod sandbox;
mod scaffold;
mod semantics;
mod stdlib;
mod testing;
mod typechecker;
mod vm;
mod watch;

use runtime::metadata::vm_incompatibilities;
use std::fs;
use std::path::PathBuf;
use std::process;

#[cfg(test)]
use clap::CommandFactory;
use clap::{Parser, Subcommand};

use interpreter::Interpreter;
use parser::ast::Program;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Charges each thread's allocations to the run's resource budget when it
/// has a memory limit (`--max-memory`, `forge mcp`); see
/// `runtime::limits::CountingAllocator`.
#[global_allocator]
static ALLOCATOR: runtime::limits::CountingAllocator = runtime::limits::CountingAllocator;

#[derive(Debug)]
enum FrontendError {
    Lex {
        line: usize,
        col: usize,
        message: String,
    },
    Parse {
        line: usize,
        col: usize,
        message: String,
    },
    Type(Vec<typechecker::Diagnostic>),
}

#[derive(Parser)]
#[command(
    name = "forge",
    version = VERSION,
    about = "Forge — Internet-Native Programming Language",
    long_about = "Go's simplicity. Rust's safety. The internet built in."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Evaluate a Forge expression inline
    #[arg(short = 'e', long = "eval")]
    eval_code: Option<String>,

    /// Use the bytecode VM (this is now the default). Kept for backwards
    /// compatibility — has no effect since VM is already the default engine.
    #[arg(long = "vm")]
    use_vm: bool,

    /// Use the tree-walking interpreter instead of the VM (also for
    /// `@server` programs, which the VM serves by default). The VM
    /// auto-falls back to the interpreter for constructs it cannot run,
    /// such as unknown decorators.
    #[arg(long = "interp")]
    use_interp: bool,

    /// JIT-compile numeric leaf functions via Cranelift on top of --vm.
    /// Only Int/Float arithmetic and comparisons are supported: any function
    /// that touches strings, arrays, objects, closures, or builtins falls
    /// back to the bytecode interpreter automatically. Best for tight math
    /// loops; for everything else --vm alone is usually enough.
    #[arg(long = "jit")]
    use_jit: bool,

    /// Profile function calls (uses VM, prints report after execution)
    #[arg(long = "profile")]
    profile: bool,

    /// Strict typing: type-checker diagnostics become errors that stop the
    /// run, and annotated function arguments and results are checked at
    /// run time on both engines
    #[arg(long = "strict")]
    strict: bool,

    /// Language edition to run under (overrides `edition` in forge.toml).
    /// "2027" is in development: docs/editions/2027.md
    #[arg(long = "edition", value_name = "YEAR", global = true)]
    edition: Option<String>,

    /// Allow shell execution (sh, shell, run_command, sh_lines, sh_json, sh_ok, pipe_to).
    /// Without this flag, these builtins return a permission error.
    #[arg(long = "allow-run")]
    allow_run: bool,

    /// Maximum Forge call depth before "maximum recursion depth exceeded"
    /// (default 10000; also settable with FORGE_MAX_DEPTH).
    #[arg(long = "max-depth", value_name = "N")]
    max_depth: Option<usize>,

    /// How syntax, type and runtime errors are written to stderr: `human`
    /// (source snippets) or `json` (one object per line with code,
    /// severity, message, file, line, col, hint — for editors and agents)
    #[arg(
        long = "error-format",
        value_enum,
        value_name = "FORMAT",
        global = true
    )]
    error_format: Option<DiagnosticFormat>,

    #[command(flatten)]
    perms: PermissionFlags,
}

/// `--error-format` / `forge check --format`.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum DiagnosticFormat {
    Human,
    Json,
}

impl From<DiagnosticFormat> for errors::ErrorFormat {
    fn from(f: DiagnosticFormat) -> Self {
        match f {
            DiagnosticFormat::Human => errors::ErrorFormat::Human,
            DiagnosticFormat::Json => errors::ErrorFormat::Json,
        }
    }
}

/// Deno-style permission flags. Without `--sandbox`, Forge keeps its
/// historical defaults (everything except `run`); a flag only changes the
/// capability it names. With `--sandbox`, everything not granted is denied.
#[derive(clap::Args, Debug, Default, Clone)]
struct PermissionFlags {
    /// Deny every capability (fs, net, env, db, run, ai) not granted with an
    /// --allow-* flag. Also settable with `sandbox = true` under
    /// [permissions] in forge.toml.
    #[arg(long = "sandbox", global = true)]
    sandbox: bool,

    /// Allow file reads; with =PATHS, only under those comma-separated paths
    #[arg(long = "allow-read", value_name = "PATHS", num_args = 0..=1,
          require_equals = true, value_delimiter = ',', default_missing_value = "",
          global = true)]
    allow_read: Option<Vec<String>>,

    /// Allow file writes; with =PATHS, only under those comma-separated paths
    #[arg(long = "allow-write", value_name = "PATHS", num_args = 0..=1,
          require_equals = true, value_delimiter = ',', default_missing_value = "",
          global = true)]
    allow_write: Option<Vec<String>>,

    /// Allow network access; with =HOSTS, only to those hosts (host, *.domain, host:port)
    #[arg(long = "allow-net", value_name = "HOSTS", num_args = 0..=1,
          require_equals = true, value_delimiter = ',', default_missing_value = "",
          global = true)]
    allow_net: Option<Vec<String>>,

    /// Allow reading and setting environment variables (env.*)
    #[arg(long = "allow-env", global = true)]
    allow_env: bool,

    /// Allow database drivers (db.*, pg.*, mysql.*)
    #[arg(long = "allow-db", global = true)]
    allow_db: bool,

    /// Allow AI/LLM calls (ask)
    #[arg(long = "allow-ai", global = true)]
    allow_ai: bool,

    /// Allow loading native plugins (`import native`); with =PATHS, only
    /// libraries at or under those paths. Native code runs with full trust.
    #[arg(long = "allow-ffi", value_name = "PATHS", num_args = 0..=1,
          require_equals = true, value_delimiter = ',', default_missing_value = "",
          global = true)]
    allow_ffi: Option<Vec<String>>,

    /// Stop the program after SECS seconds of wall-clock time (exit code 124)
    #[arg(long = "max-time", value_name = "SECS", global = true)]
    max_time: Option<f64>,

    /// Deterministic step budget: fail with "fuel exhausted" after N steps
    /// (VM: instructions; --interp: statements, calls and loop iterations).
    /// Disables JIT tier-up.
    #[arg(long = "max-fuel", value_name = "N", global = true)]
    max_fuel: Option<u64>,

    /// Fail with "memory limit exceeded" when the program holds more than
    /// SIZE (bytes, or with a K/M/G suffix, e.g. 256MB)
    #[arg(long = "max-memory", value_name = "SIZE", global = true)]
    max_memory: Option<String>,
}

/// Resource limits from `--max-fuel` / `--max-memory`, or an error message.
fn build_limits(flags: &PermissionFlags) -> Result<runtime::limits::Limits, String> {
    let mut limits = runtime::limits::Limits::none();
    if let Some(n) = flags.max_fuel {
        if n == 0 {
            return Err("--max-fuel must be a positive number of steps".to_string());
        }
        limits.max_fuel = Some(n);
    }
    if let Some(raw) = &flags.max_memory {
        limits.max_memory =
            Some(runtime::limits::parse_bytes(raw).map_err(|e| format!("--max-memory: {}", e))?);
    }
    Ok(limits)
}

/// A program can swallow a fatal limit error inside a builtin (e.g.
/// `assert_throws`); the budget remembers the trip, so report it anyway.
fn exit_if_limit_tripped() {
    if let Some(message) = runtime::limits::current().and_then(|b| b.trip_message()) {
        eprintln!("{}", errors::format_simple_error(&message));
        process::exit(1);
    }
}

/// Exit code used when `--max-time` expires (same as coreutils `timeout`).
const MAX_TIME_EXIT_CODE: i32 = 124;

/// Turn `--allow-x[=a,b]` / a forge.toml grant into a scope update.
/// `None` = leave as is; empty list = unrestricted; list = only those.
fn apply_scoped_grant(
    caps: permissions::Capabilities,
    cap: permissions::Capability,
    grant: Option<manifest::GrantSpec>,
) -> permissions::Capabilities {
    use permissions::Capability;
    let items: Vec<String> = match grant {
        None => return caps,
        Some(manifest::GrantSpec::Flag(b)) => {
            return if b { caps.grant(cap) } else { caps.deny(cap) }
        }
        Some(manifest::GrantSpec::List(items)) => items
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    };
    if items.is_empty() {
        return caps.grant(cap);
    }
    let caps = caps.deny(cap);
    match cap {
        Capability::Read => caps.grant_read_paths(items),
        Capability::Write => caps.grant_write_paths(items),
        Capability::Net => caps.grant_net_hosts(items),
        Capability::Ffi => caps.grant_ffi_paths(items),
        _ => caps.grant(cap),
    }
}

/// Build the process-wide policy from forge.toml `[permissions]` and CLI
/// flags (flags win per capability).
fn build_policy(
    flags: &PermissionFlags,
    toml: Option<manifest::PermissionsConfig>,
    allow_run: bool,
    is_interactive: bool,
    import_root: Option<PathBuf>,
) -> permissions::Capabilities {
    use permissions::{Capabilities, Capability};
    let toml = toml.unwrap_or_default();
    let sandboxed = flags.sandbox || toml.sandbox;
    let caps = if sandboxed {
        // The CLI owns its process: exit()/cd() stay available.
        Capabilities::deny_all().grant(Capability::Process)
    } else {
        Capabilities::cli_default()
    };
    let mut caps = apply_grants(caps, flags, &toml);
    // REPL and -e are user-invoked contexts — shell execution stays allowed
    // there unless the user explicitly asked for a sandbox.
    let run = allow_run || toml.allow_run.unwrap_or(false) || (is_interactive && !sandboxed);
    caps.set(Capability::Run, run);
    // Native plugins follow `run`: opt-in for scripts, available when a
    // person is typing (REPL, -e) unless sandboxed. An explicit grant
    // (`--allow-ffi[=PATHS]`, `allow-ffi`) was applied above and wins.
    if is_interactive && !sandboxed && flags.allow_ffi.is_none() && toml.allow_ffi.is_none() {
        caps.set(Capability::Ffi, true);
    }
    // Modules next to the entry script (and installed packages) stay
    // importable even when fs.read is scoped elsewhere.
    if let Some(root) = import_root {
        caps = caps.grant_import_root(root);
    }
    caps.grant_import_root("forge_modules")
}

/// The policy for scripts run by `forge mcp`: always default-deny
/// (including `process`, so a script cannot end the server), plus exactly
/// the grants from the flags and forge.toml. `run` needs an explicit
/// `--allow-run` / `allow-run = true`; `sandbox = false` is ignored.
fn build_mcp_policy(
    flags: &PermissionFlags,
    toml: Option<manifest::PermissionsConfig>,
    allow_run: bool,
) -> permissions::Capabilities {
    use permissions::{Capabilities, Capability};
    let toml = toml.unwrap_or_default();
    let mut caps = apply_grants(Capabilities::deny_all(), flags, &toml);
    caps.set(Capability::Run, allow_run || toml.allow_run == Some(true));
    caps
}

/// Apply the fs/net/ffi/env/db/ai grants from CLI flags and forge.toml (flags
/// win per capability). `run` and `process` are left to the caller.
fn apply_grants(
    mut caps: permissions::Capabilities,
    flags: &PermissionFlags,
    toml: &manifest::PermissionsConfig,
) -> permissions::Capabilities {
    use permissions::Capability;
    let toml = toml.clone();
    let cli_list = |v: &Option<Vec<String>>| v.clone().map(manifest::GrantSpec::List);
    caps = apply_scoped_grant(
        caps,
        Capability::Read,
        cli_list(&flags.allow_read).or(toml.allow_read),
    );
    caps = apply_scoped_grant(
        caps,
        Capability::Write,
        cli_list(&flags.allow_write).or(toml.allow_write),
    );
    caps = apply_scoped_grant(
        caps,
        Capability::Net,
        cli_list(&flags.allow_net).or(toml.allow_net),
    );
    caps = apply_scoped_grant(
        caps,
        Capability::Ffi,
        cli_list(&flags.allow_ffi).or(toml.allow_ffi),
    );
    for (cap, flag, from_toml) in [
        (Capability::Env, flags.allow_env, toml.allow_env),
        (Capability::Db, flags.allow_db, toml.allow_db),
        (Capability::Ai, flags.allow_ai, toml.allow_ai),
    ] {
        if flag {
            caps = caps.grant(cap);
        } else if let Some(b) = from_toml {
            caps.set(cap, b);
        }
    }
    caps
}

/// `forge mcp` settings beyond the policy.
struct McpOptions {
    max_time: Option<f64>,
    max_sessions: Option<usize>,
    session_idle: Option<u64>,
    /// `--engine` (or the global `--interp`).
    engine: sandbox::Engine,
    /// `forge mcp serve FILE [--with-code-tools]`.
    serve: Option<(PathBuf, bool)>,
}

/// Load the tools of `forge mcp serve FILE` under the server's policy, plus
/// the file's directory (and `forge_modules/`) as import roots.
fn load_mcp_tools(
    file: &std::path::Path,
    caps: &permissions::Capabilities,
    max_time: std::time::Duration,
    limits: &runtime::limits::Limits,
    engine: sandbox::Engine,
) -> Result<mcp::ToolSet, String> {
    let source =
        fs::read_to_string(file).map_err(|e| format!("cannot read {}: {}", file.display(), e))?;
    let dir = match file.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let caps = caps
        .clone()
        .grant_import_root(dir)
        .grant_import_root("forge_modules");
    let (tools, report) =
        mcp::ToolSet::load(file, &source, caps, max_time, limits.clone(), engine)?;
    if !report.stdout.is_empty() {
        eprint!("{}", report.stdout);
    }
    Ok(tools)
}

/// `forge mcp`: serve until the client closes stdin, then exit.
fn run_mcp(
    caps: permissions::Capabilities,
    options: McpOptions,
    limits: runtime::limits::Limits,
) -> ! {
    let fail = |message: &str| -> ! {
        eprintln!("{}", errors::format_simple_error(message));
        process::exit(2);
    };
    let mut config = mcp::ServerConfig::new(caps).with_engine(options.engine);
    if let Some(n) = options.max_sessions {
        config.max_sessions = n;
    }
    if let Some(secs) = options.session_idle {
        config.session_idle = std::time::Duration::from_secs(secs);
    }
    // Flags override the server's default per-call limits.
    if let Some(n) = limits.max_fuel {
        config.limits.max_fuel = Some(n);
    }
    if let Some(n) = limits.max_memory {
        config.limits.max_memory = Some(n);
    }
    if let Some(secs) = options.max_time {
        match std::time::Duration::try_from_secs_f64(secs) {
            Ok(limit) if secs > 0.0 => config.max_time = limit,
            _ => {
                eprintln!(
                    "{}",
                    errors::format_simple_error("--max-time must be a positive number of seconds")
                );
                process::exit(2);
            }
        }
    }
    // Nothing on this side of the protocol runs user code; scripts get the
    // configured policy on their own sandbox threads.
    permissions::set_global(permissions::Capabilities::deny_all());
    if let Some((file, with_code_tools)) = options.serve {
        config.code_tools = with_code_tools;
        let tools = load_mcp_tools(
            &file,
            &config.capabilities,
            config.max_time,
            &config.limits,
            config.engine,
        )
        .unwrap_or_else(|e| fail(&format!("forge mcp serve: {}", e)));
        config = config
            .with_tools(tools)
            .unwrap_or_else(|e| fail(&format!("forge mcp serve: {}", e)));
    }
    eprintln!(
        "forge mcp {} ({} engine): {}",
        env!("CARGO_PKG_VERSION"),
        config.engine,
        config.policy_summary()
    );
    match mcp::serve_stdio(config) {
        Ok(()) => process::exit(0),
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_simple_error(&format!("forge mcp: {}", e))
            );
            process::exit(1);
        }
    }
}

/// Enforce `--max-time` on every engine: after `secs`, flush output, report
/// and exit with [`MAX_TIME_EXIT_CODE`].
fn start_max_time_watchdog(secs: f64) {
    if !(secs.is_finite() && secs > 0.0) {
        eprintln!(
            "{}",
            errors::format_simple_error("--max-time must be a positive number of seconds")
        );
        process::exit(2);
    }
    let limit = std::time::Duration::from_secs_f64(secs);
    std::thread::spawn(move || {
        std::thread::sleep(limit);
        use std::io::Write;
        let _ = std::io::stdout().flush();
        eprintln!(
            "{}",
            errors::format_simple_error(&format!("execution exceeded --max-time of {}s", secs))
        );
        process::exit(MAX_TIME_EXIT_CODE);
    });
}

#[derive(Subcommand)]
enum McpCommand {
    /// Serve the `@tool` and `@resource` functions of a Forge file as MCP
    /// tools and resources. Every call runs in a fresh sandboxed fork of
    /// the file's top level, under the same grants and limits as run_forge.
    Serve {
        /// The Forge file that defines the tools
        file: PathBuf,
        /// Let tools run subprocesses (sh, run_command, ...)
        #[arg(long = "allow-run")]
        allow_run: bool,
        /// Also serve run_forge, check_forge, forge_reference and
        /// reset_session (arbitrary sandboxed code)
        #[arg(long = "with-code-tools")]
        with_code_tools: bool,
    },
}

#[derive(Subcommand)]
enum Command {
    /// Run a Forge source file (.fg) or compiled bytecode (.fgc)
    Run {
        /// Path to a .fg or .fgc file (reads entry from forge.toml if omitted)
        file: Option<PathBuf>,
        /// Allow shell execution (same as the top-level --allow-run)
        #[arg(long = "allow-run")]
        allow_run: bool,
    },
    /// Parse and type-check a file without running it. Exits 1 when it has
    /// errors (syntax errors, or any type diagnostic under --strict).
    Check {
        /// Path to a .fg file (reads entry from forge.toml if omitted)
        file: Option<PathBuf>,
        /// Output format: human (default) or json (one diagnostic per line
        /// on stdout)
        #[arg(long, value_enum)]
        format: Option<DiagnosticFormat>,
    },
    /// Explain an error code (E0009, T0006, ...) with an example and the
    /// fix; without a code, list every code
    Explain {
        /// The code to explain
        code: Option<String>,
    },
    /// Start the interactive REPL
    Repl,
    /// Show version information
    Version,
    /// Format Forge source files
    Fmt {
        /// Files to format (defaults to all .fg files in current directory)
        files: Vec<PathBuf>,
        /// Check formatting without writing (exit code 1 if unformatted)
        #[arg(long)]
        check: bool,
    },
    /// Run tests in the tests/ directory
    Test {
        /// Test directory (defaults to "tests")
        #[arg(default_value = "tests")]
        dir: String,
        /// Filter tests by name pattern
        #[arg(long)]
        filter: Option<String>,
        /// Show line coverage report after tests (interpreter only)
        #[arg(long)]
        coverage: bool,
        /// Engine to run tests on: vm, interp, or both. Defaults to the same
        /// engine as `forge run` (VM, or interpreter with the global --interp
        /// flag); files the VM cannot run yet fall back to the interpreter.
        #[arg(long, value_enum)]
        engine: Option<testing::Engine>,
        /// Per-test time limit in seconds; a test that exceeds it aborts the
        /// run with a failure (0 disables the limit)
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
    /// Create a new Forge project
    New {
        /// Project name
        name: String,
    },
    /// Compile Forge source to bytecode
    Build {
        /// Emit a native launcher that embeds source and shells into the Forge runtime
        #[arg(long, conflicts_with = "aot")]
        native: bool,
        /// Compile to bytecode and embed in a native binary (no source exposure)
        #[arg(long, conflicts_with = "native")]
        aot: bool,
        /// Bake shell execution permission into a --native standalone source-runtime binary
        #[arg(long = "allow-run", requires = "native", conflicts_with = "aot")]
        allow_run: bool,
        /// Source file to compile
        file: PathBuf,
    },
    /// Install a Forge package from git URL or local path
    Install {
        /// Git URL or local path
        source: String,
    },
    /// Add a dependency to forge.toml and install it
    Add {
        /// Package name or name@version (e.g., "router" or "router@^1.0")
        package: String,
    },
    /// Update all dependencies to latest compatible versions
    Update,
    /// Publish the current project to a local registry, or to a clone of a
    /// sparse-index repository (a directory with config.json; rfcs/0007)
    Publish {
        /// Show what would be packaged without publishing
        #[arg(long)]
        dry_run: bool,
        /// Local registry directory (default ~/.forge/registry/), or a local
        /// clone of a sparse-index repository
        #[arg(long)]
        registry: Option<String>,
        /// Sign the index entry with your ed25519 publisher key
        /// (~/.forge/keys/publish.key or $FORGE_SIGNING_KEY; created on first use)
        #[arg(long)]
        sign: bool,
        /// Archive URL template for index publishing ({name}, {vers});
        /// default: a GitHub release asset of project.repository
        #[arg(long, value_name = "URL")]
        download_url: Option<String>,
        /// Where to write the archive for index publishing (default ./dist)
        #[arg(long, value_name = "DIR")]
        out_dir: Option<PathBuf>,
        /// Do not create a branch and commit in the index clone
        #[arg(long)]
        no_commit: bool,
    },
    /// Mark a published version as yanked (or un-yank it) in a local clone
    /// of a sparse-index repository
    Yank {
        /// name@version to yank
        package: String,
        /// Local clone of the sparse-index repository
        #[arg(long)]
        registry: PathBuf,
        /// Un-yank instead
        #[arg(long)]
        undo: bool,
        /// Do not create a branch and commit in the index clone
        #[arg(long)]
        no_commit: bool,
    },
    /// Search the package registry
    Search {
        /// Search query (matches name and description)
        query: Option<String>,
    },
    /// Start the Language Server Protocol server
    Lsp,
    /// Start the Debug Adapter Protocol server
    Dap,
    /// Serve the Model Context Protocol over stdio so AI agents can run
    /// Forge code in a sandbox (tools: run_forge, check_forge,
    /// forge_reference, reset_session), or serve tools written in Forge
    /// (`forge mcp serve tools.fg`). Scripts are denied everything (files,
    /// network, env, db, subprocesses, AI) unless granted with --allow-*
    /// flags or [permissions] in forge.toml; --max-time caps each call
    /// (default 30s).
    #[command(
        after_help = "Example (Claude Desktop / Claude Code config):\n  {\"command\": \"forge\", \"args\": [\"mcp\", \"--allow-net=api.example.com\"]}\n  {\"command\": \"forge\", \"args\": [\"mcp\", \"serve\", \"/path/to/tools.fg\"]}"
    )]
    Mcp {
        /// Let scripts run subprocesses (sh, run_command, ...). Never granted
        /// by default.
        #[arg(long = "allow-run")]
        allow_run: bool,
        /// Keep at most N persistent run_forge sessions (default 16; 0
        /// disables sessions)
        #[arg(long = "max-sessions", value_name = "N")]
        max_sessions: Option<usize>,
        /// Drop a run_forge session after SECS seconds without calls
        /// (default 900)
        #[arg(long = "session-idle", value_name = "SECS")]
        session_idle: Option<u64>,
        /// Engine that runs scripts and tools: `vm` (default) or `interp`.
        /// Both run under the same sandbox (`--interp` also selects interp)
        #[arg(long = "engine", value_name = "ENGINE", global = true,
              value_parser = ["vm", "interp"])]
        engine: Option<String>,
        #[command(subcommand)]
        action: Option<McpCommand>,
    },
    /// Interactive tutorials to learn Forge
    Learn {
        /// Lesson number (optional)
        lesson: Option<usize>,
    },
    /// Start an AI chat session
    Chat,
    /// Watch a file and re-run on changes
    Watch {
        /// Path to a .fg file
        file: PathBuf,
    },
    /// Generate documentation from source files
    Doc {
        /// Files or directories (defaults to current directory)
        paths: Vec<PathBuf>,
    },
}

/// Entry point: run the real CLI on a thread with a large stack so deep (but
/// bounded) Forge recursion works; the recursion guard in
/// `runtime/recursion.rs` turns anything deeper into a catchable error
/// instead of a native stack overflow.
fn main() {
    let stack = runtime::recursion::MAIN_STACK_SIZE;
    let worker = std::thread::Builder::new()
        .name("forge-main".to_string())
        .stack_size(stack)
        .spawn(move || {
            runtime::recursion::register_thread_stack(stack);
            runtime::recursion::configure_runtime(&mut tokio::runtime::Builder::new_multi_thread())
                .enable_all()
                .build()
                .expect("BUG: failed to build the tokio runtime")
                .block_on(async_main());
        })
        .expect("BUG: failed to spawn the forge main thread");
    if let Err(panic) = worker.join() {
        std::panic::resume_unwind(panic);
    }
}

async fn async_main() {
    // OTel must initialize on the main tokio runtime (not from a
    // nested runtime created by a stdlib helper). Calling here ensures
    // CLI scripts that emit `tracing` events (via the `log` stdlib)
    // export them to the configured collector. No-op when the otel
    // feature is off or OTEL_EXPORTER_OTLP_ENDPOINT is unset.
    forge_lang::runtime::tracing_init::init_otel();

    let cli = Cli::parse();
    if let Some(format) = cli.error_format {
        errors::set_error_format(format.into());
    }
    // The edition is fixed before any source is parsed: `--edition` wins
    // over forge.toml, which wins over the default.
    // Only commands that run or check code fail on a bad manifest edition.
    let edition = match &cli.edition {
        Some(text) => semantics::edition::Edition::parse(text),
        None if matches!(
            cli.command,
            None | Some(
                Command::Run { .. }
                    | Command::Test { .. }
                    | Command::Mcp { .. }
                    | Command::Check { .. }
                    | Command::Repl
            )
        ) =>
        {
            manifest::load_edition()
        }
        None => Ok(manifest::load_edition().unwrap_or(semantics::edition::Edition::DEFAULT)),
    };
    match edition {
        Ok(e) => semantics::edition::set_default(e),
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e));
            process::exit(1);
        }
    }
    if let Some(n) = cli.max_depth {
        runtime::recursion::set_max_depth(n);
    }
    let use_jit = cli.use_jit;
    #[cfg(not(feature = "jit"))]
    if use_jit {
        eprintln!("error: --jit requires the 'jit' feature (install with: cargo install forge-lang --features jit)");
        std::process::exit(1);
    }
    let use_vm = !cli.use_interp || cli.use_jit || cli.profile;
    let profile = cli.profile;
    let strict = cli.strict;
    // REPL and -e are user-invoked contexts — always allow shell execution.
    // For file execution (forge run), require explicit --allow-run.
    let is_interactive =
        cli.eval_code.is_some() || matches!(cli.command, Some(Command::Repl) | None);
    let run_allow_run = matches!(
        cli.command,
        Some(Command::Run {
            allow_run: true,
            ..
        })
    );
    // forge.toml [permissions] applies to commands that execute the project.
    let toml_perms = if matches!(
        cli.command,
        Some(Command::Run { .. } | Command::Test { .. } | Command::Mcp { .. })
    ) {
        match manifest::load_permissions() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{}", errors::format_simple_error(&e));
                process::exit(1);
            }
        }
    } else {
        None
    };
    let import_root = match &cli.command {
        Some(Command::Run { file: Some(f), .. }) => {
            Some(f.parent().map(|p| p.to_path_buf()).unwrap_or_default())
        }
        Some(Command::Test { dir, .. }) => Some(PathBuf::from(dir)),
        _ => Some(PathBuf::from(".")),
    }
    .map(|p| {
        if p.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            p
        }
    });
    let max_time = cli
        .perms
        .max_time
        .or(toml_perms.as_ref().and_then(|p| p.max_time));
    let limits = match build_limits(&cli.perms) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e));
            process::exit(2);
        }
    };
    if let Some(Command::Mcp {
        allow_run,
        max_sessions,
        session_idle,
        engine,
        action,
    }) = cli.command
    {
        // No process watchdog for the server: the limit applies to each
        // script instead.
        let serve_run = matches!(
            action,
            Some(McpCommand::Serve {
                allow_run: true,
                ..
            })
        );
        let caps = build_mcp_policy(
            &cli.perms,
            toml_perms,
            cli.allow_run || allow_run || serve_run,
        );
        let engine = match engine.as_deref() {
            Some(name) => sandbox::Engine::parse(name).unwrap_or_default(),
            None if cli.use_interp => sandbox::Engine::Interpreter,
            None => sandbox::Engine::default(),
        };
        let options = McpOptions {
            max_time,
            max_sessions,
            session_idle,
            engine,
            serve: action.map(
                |McpCommand::Serve {
                     file,
                     with_code_tools,
                     ..
                 }| (file, with_code_tools),
            ),
        };
        run_mcp(caps, options, limits);
    }
    // One budget for the whole process: threads without a scope (runtime
    // pools) see it through the global, and the scope on this thread (and
    // every thread forked from it) also meters allocations.
    let _limits = (!limits.is_unlimited()).then(|| {
        let budget = runtime::limits::Budget::new(limits);
        runtime::limits::set_global(Some(budget.clone()));
        runtime::limits::scope(Some(budget))
    });
    permissions::set_global(build_policy(
        &cli.perms,
        toml_perms,
        cli.allow_run || run_allow_run,
        is_interactive,
        import_root,
    ));
    if let Some(secs) = max_time {
        start_max_time_watchdog(secs);
    }

    if let Some(code) = cli.eval_code {
        let code = code.replace(';', "\n");
        #[cfg(feature = "jit")]
        if use_jit && !profile {
            run_jit(&code, "<eval>", strict);
            return;
        }
        run_source(&code, "<eval>", use_vm, profile, strict).await;
        return;
    }

    match cli.command {
        Some(Command::Run { file, .. }) => {
            let file = match file {
                Some(f) => f,
                None => {
                    if let Some(m) = manifest::load_manifest() {
                        if m.project.entry.is_empty() {
                            eprintln!(
                                "{}",
                                errors::format_simple_error(
                                    "forge.toml found but no 'entry' field set. Add entry = \"src/main.fg\" to [project] or specify a file: forge run <file>"
                                )
                            );
                            process::exit(1);
                        }
                        PathBuf::from(&m.project.entry)
                    } else {
                        eprintln!(
                            "{}",
                            errors::format_simple_error(
                                "no file specified and no forge.toml found. Usage: forge run <file>"
                            )
                        );
                        process::exit(1);
                    }
                }
            };
            if file.extension().map(|e| e == "fgc").unwrap_or(false) {
                run_bytecode_file(&file, profile);
                return;
            }
            let path_str = file.display().to_string();
            let source = match fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "{}",
                        errors::format_simple_error(&format!(
                            "could not read '{}': {}",
                            path_str, e
                        ))
                    );
                    process::exit(1);
                }
            };
            #[cfg(feature = "jit")]
            if use_jit && !profile {
                run_jit(&source, &path_str, strict);
                return;
            }
            run_source(&source, &path_str, use_vm, profile, strict).await;
        }
        Some(Command::Check { file, format }) => {
            check_file(file, format, strict);
        }
        Some(Command::Explain { code }) => explain(code),
        Some(Command::Repl) => {
            repl::run_repl();
        }
        Some(Command::Version) => {
            println!("Forge v{}", VERSION);
            println!("Internet-native programming language");
            println!("Bytecode VM with mark-sweep GC");
        }
        Some(Command::Fmt { files, check }) => {
            formatter::format_files(&files, check);
        }
        Some(Command::Test {
            dir,
            filter,
            coverage,
            engine,
            timeout,
        }) => {
            let test_dir = if dir == "tests" {
                if let Some(m) = manifest::load_manifest() {
                    m.test.directory
                } else {
                    dir
                }
            } else {
                dir
            };
            let engine = engine.unwrap_or(if cli.use_interp {
                testing::Engine::Interp
            } else {
                testing::Engine::Vm
            });
            let vm_compat =
                |program: &Program| ensure_vm_compatible(program, "VM", Serving::Supported);
            testing::run_tests(
                &test_dir,
                &testing::TestOptions {
                    filter: filter.as_deref(),
                    coverage,
                    engine,
                    vm_compat: &vm_compat,
                    timeout: (timeout > 0).then(|| std::time::Duration::from_secs(timeout)),
                },
            );
        }
        Some(Command::New { name }) => {
            scaffold::create_project(&name);
        }
        Some(Command::Build {
            file,
            native,
            aot,
            allow_run: build_allow_run,
        }) => {
            let path_str = file.display().to_string();
            let source = match fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "{}",
                        errors::format_simple_error(&format!(
                            "could not read '{}': {}",
                            path_str, e
                        ))
                    );
                    process::exit(1);
                }
            };
            if aot {
                compile_to_native_aot(&source, &path_str, &file, strict);
            } else if native {
                compile_to_native_launcher(
                    &source,
                    &path_str,
                    &file,
                    strict,
                    cli.allow_run || build_allow_run,
                );
            } else {
                compile_to_bytecode(&source, &path_str, &file, strict);
            }
        }
        Some(Command::Install { source }) => {
            run_off_runtime(|| package::install(&source));
        }
        Some(Command::Add { package: pkg }) => match manifest::parse_package_spec(&pkg) {
            Ok((name, version)) => run_off_runtime(|| package::add(&name, &version)),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
        Some(Command::Update) => {
            run_off_runtime(package::update);
        }
        Some(Command::Publish {
            dry_run,
            registry,
            sign,
            download_url,
            out_dir,
            no_commit,
        }) => {
            let index_dir = registry
                .as_deref()
                .map(PathBuf::from)
                .filter(|p| publish_index::is_index_repo(p));
            match index_dir {
                Some(index_dir) => {
                    let opts = publish_index::IndexPublishOptions {
                        project_dir: std::path::Path::new("."),
                        index_dir: &index_dir,
                        dry_run,
                        sign,
                        key_path: None,
                        download_url,
                        out_dir,
                        commit: !no_commit,
                    };
                    match publish_index::publish_to_index(&opts) {
                        Ok(published) => publish_index::print_report(&index_dir, &published),
                        Err(e) => {
                            eprintln!("Error: {}", e);
                            process::exit(1);
                        }
                    }
                }
                None => {
                    if sign || download_url.is_some() || out_dir.is_some() || no_commit {
                        eprintln!(
                            "Error: --sign, --download-url, --out-dir and --no-commit apply to \
                             publishing into a sparse-index clone (a --registry directory with config.json)"
                        );
                        process::exit(2);
                    }
                    publish::publish(dry_run, registry.as_deref());
                }
            }
        }
        Some(Command::Yank {
            package: pkg,
            registry,
            undo,
            no_commit,
        }) => {
            let Some((name, vers)) = pkg.split_once('@') else {
                eprintln!("Error: expected name@version, got '{}'", pkg);
                process::exit(2);
            };
            match publish_index::yank(&registry, name, vers, undo, !no_commit) {
                Ok(branch) => {
                    let verb = if undo { "Un-yanked" } else { "Yanked" };
                    println!("  {} {}@{} in {}", verb, name, vers, registry.display());
                    match branch {
                        Some(b) => println!("  Push branch '{}' and open a pull request.", b),
                        None => println!("  Commit the change and open a pull request."),
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    process::exit(1);
                }
            }
        }
        Some(Command::Search { query }) => {
            let q = query.as_deref().unwrap_or("");
            let roots = package::default_registry_roots();
            let remote = run_off_runtime(registry::fetch_index);
            let report = registry::search_all(q, &roots, remote);
            let code = registry::print_search_report(q, &report, &roots);
            if code != 0 {
                std::process::exit(code);
            }
        }
        Some(Command::Lsp) => {
            lsp::run_lsp();
        }
        Some(Command::Dap) => {
            dap::run_dap();
        }
        Some(Command::Mcp { .. }) => {
            unreachable!("BUG: forge mcp is dispatched before the CLI policy is installed")
        }
        Some(Command::Learn { lesson }) => {
            learn::run_learn(lesson);
        }
        Some(Command::Chat) => {
            chat::run_chat();
        }
        Some(Command::Watch { file }) => {
            watch::run_watch(&file).await;
        }
        Some(Command::Doc { paths }) => {
            doc::generate_docs(&paths);
        }
        None => {
            repl::run_repl();
        }
    }

    // Flush pending OpenTelemetry spans on normal exit so CLI scripts
    // that emit `tracing` events (e.g. via the `log` stdlib module)
    // don't drop their last batch. No-op when the otel feature is off
    // or init_otel was never called. spawn_blocking because
    // provider.shutdown() is synchronous.
    tokio::task::spawn_blocking(forge_lang::runtime::tracing_init::flush_otel)
        .await
        .ok();
}

/// Lex, parse and type-check `source` (from `filename`, which locates
/// imports). With `strict`, type diagnostics are errors and stop the run,
/// and annotations are additionally enforced at run time (see
/// `typechecker::enforce`).
fn prepare_program(
    source: &str,
    filename: &str,
    strict: bool,
) -> Result<(Program, Vec<typechecker::Diagnostic>), FrontendError> {
    let path = std::path::Path::new(filename);
    let options = typechecker::CheckOptions {
        strict,
        file: path.exists().then(|| path.to_path_buf()),
    };
    let analysis = typechecker::analyze(source, &options).map_err(|e| match e {
        typechecker::FrontendError::Lex { line, col, message } => {
            FrontendError::Lex { line, col, message }
        }
        typechecker::FrontendError::Parse { line, col, message } => {
            FrontendError::Parse { line, col, message }
        }
    })?;
    let diagnostics = analysis.diagnostics;
    if diagnostics.iter().any(|d| d.is_error()) {
        return Err(FrontendError::Type(diagnostics));
    }
    let mut program = analysis.program;
    if strict {
        typechecker::enforce::instrument(&mut program);
    }
    Ok((program, diagnostics))
}

/// A type-checker diagnostic as a program diagnostic (`T0006` ...).
fn type_diagnostic(filename: &str, d: &typechecker::Diagnostic) -> errors::ProgramDiagnostic {
    let len = if d.span.end.line == d.span.start.line {
        d.span.end.col.saturating_sub(d.span.start.col).max(1)
    } else {
        1
    };
    errors::ProgramDiagnostic {
        code: d.code.as_str().to_string(),
        is_error: d.is_error(),
        message: d.message.clone(),
        file: errors::display_path(filename),
        line: d.line(),
        col: d.col().max(1),
        len,
        hint: d.help.clone(),
        phase: errors::Phase::Type,
    }
}

/// Syntax errors (E0001 lexer, E0002 parser) or the type diagnostics of a
/// program that could not be prepared, in checking order.
fn frontend_diagnostics(filename: &str, err: &FrontendError) -> Vec<errors::ProgramDiagnostic> {
    let file = errors::display_path(filename);
    match err {
        FrontendError::Lex { line, col, message } => vec![errors::ProgramDiagnostic::syntax(
            true, message, &file, *line, *col,
        )],
        FrontendError::Parse { line, col, message } => vec![errors::ProgramDiagnostic::syntax(
            false, message, &file, *line, *col,
        )],
        FrontendError::Type(diagnostics) => diagnostics
            .iter()
            .map(|d| type_diagnostic(filename, d))
            .collect(),
    }
}

fn print_frontend_error(source: &str, filename: &str, err: FrontendError) -> ! {
    for d in frontend_diagnostics(filename, &err) {
        eprintln!("{}", d.render(source));
    }
    if let FrontendError::Type(diagnostics) = &err {
        if errors::error_format() == errors::ErrorFormat::Human {
            let errors = diagnostics.iter().filter(|d| d.is_error()).count();
            eprintln!(
                "{}",
                errors::format_simple_error(&format!(
                    "type checking failed with {} error{} (--strict)",
                    errors,
                    if errors == 1 { "" } else { "s" }
                ))
            );
        }
    }
    process::exit(1);
}

fn emit_type_warnings(source: &str, filename: &str, warnings: &[typechecker::Diagnostic]) {
    for d in warnings.iter().filter(|d| !d.is_error()) {
        eprintln!("{}", type_diagnostic(filename, d).render(source));
    }
}

/// The file named by `forge.toml`'s `entry`, or exit with an error.
fn entry_file_or_exit() -> PathBuf {
    match manifest::load_manifest() {
        Some(m) if !m.project.entry.is_empty() => PathBuf::from(&m.project.entry),
        Some(_) => {
            eprintln!(
                "{}",
                errors::format_simple_error(
                    "forge.toml found but no 'entry' field set. Add entry = \"src/main.fg\" to [project] or specify a file"
                )
            );
            process::exit(1);
        }
        None => {
            eprintln!(
                "{}",
                errors::format_simple_error("no file specified and no forge.toml found")
            );
            process::exit(1);
        }
    }
}

/// `forge check`: parse and type-check without running. Human output goes
/// to stderr like `forge run`'s; JSON goes to stdout, one diagnostic per
/// line. Exits 1 when there is an error (any diagnostic under --strict).
fn check_file(file: Option<PathBuf>, format: Option<DiagnosticFormat>, strict: bool) -> ! {
    if let Some(format) = format {
        errors::set_error_format(format.into());
    }
    let file = file.unwrap_or_else(entry_file_or_exit);
    let filename = file.display().to_string();
    let source = match fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_simple_error(&format!("could not read '{}': {}", filename, e))
            );
            process::exit(1);
        }
    };
    let (diagnostics, failed) = match prepare_program(&source, &filename, strict) {
        Ok((_, warnings)) => (
            warnings
                .iter()
                .map(|d| type_diagnostic(&filename, d))
                .collect::<Vec<_>>(),
            false,
        ),
        Err(err) => (frontend_diagnostics(&filename, &err), true),
    };
    let json = errors::error_format() == errors::ErrorFormat::Json;
    for d in &diagnostics {
        if json {
            println!("{}", d.to_json());
        } else {
            eprintln!("{}", d.to_human(&source));
        }
    }
    if !json {
        let error_count = diagnostics.iter().filter(|d| d.is_error).count();
        let warning_count = diagnostics.len() - error_count;
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        let summary = format!(
            "{}: {} error{}, {} warning{}",
            errors::display_path(&filename),
            error_count,
            plural(error_count),
            warning_count,
            plural(warning_count)
        );
        if failed {
            eprintln!("{}", errors::format_simple_error(&summary));
        } else {
            eprintln!("{}", summary);
        }
    }
    process::exit(i32::from(failed));
}

/// `forge explain [CODE]`.
fn explain(code: Option<String>) -> ! {
    use semantics::errors::RUNTIME_ERRORS;
    use typechecker::diagnostics::Code;
    let Some(code) = code else {
        println!("Runtime and syntax errors:");
        for e in RUNTIME_ERRORS {
            println!("  {}  {}", e.code, e.title);
        }
        println!("\nType checker diagnostics:");
        for c in Code::ALL {
            println!("  {}  {}", c.as_str(), c.title());
        }
        println!("\nRun `forge explain <CODE>` for an explanation with an example and the fix.");
        process::exit(0);
    };
    if let Some(e) = semantics::errors::lookup(&code) {
        println!("{}: {}\n\n{}", e.code, e.title, e.explanation);
        process::exit(0);
    }
    if let Some(c) = Code::parse(&code) {
        println!("{}: {}\n\n{}", c.as_str(), c.title(), c.explanation());
        process::exit(0);
    }
    eprintln!(
        "{}",
        errors::format_simple_error(&format!(
            "unknown error code '{}' (codes look like E0009 or T0006; run `forge explain` to list them)",
            code
        ))
    );
    process::exit(1);
}

/// Whether a VM entry point can host a program's HTTP server.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Serving {
    /// `forge run` / `forge test`: an `@server` program is served by the VM
    /// (`runtime::host::launch_vm`).
    Supported,
    /// `--jit`, `forge build` and the parity corpus only execute the
    /// bytecode; a program that would start a server is rejected.
    Rejected,
}

/// Whether the bytecode VM can run `program` faithfully. Combines the AST
/// scan above with a trial compile: any construct the VM compiler reports as
/// `Unsupported` (instead of silently dropping it) is rejected here, so
/// `forge run` falls back to the interpreter rather than misbehaving.
fn ensure_vm_compatible(program: &Program, mode: &str, serving: Serving) -> Result<(), String> {
    let mut issues = vm_incompatibilities(program);
    if serving == Serving::Rejected
        && runtime::metadata::extract_runtime_plan(program)
            .server
            .is_some()
    {
        issues.push("decorator-driven runtime features (@server)".to_string());
    }
    if issues.is_empty() {
        if let Err(e) = vm::compiler::compile(program) {
            if e.is_unsupported() {
                issues.push(e.message);
            }
        }
    }
    if issues.is_empty() {
        return Ok(());
    }

    Err(format!(
        "{} mode does not support this program yet. Unsupported constructs: {}.\n  hint: run without {} for full language support",
        mode,
        issues.join(", "),
        mode
    ))
}

/// Print a VM runtime error with the same source snippet the interpreter
/// shows (anchored at the failing statement of the main program), followed
/// by the VM stack trace.
fn report_vm_error(source: &str, filename: &str, error: &vm::machine::VMError) {
    let file = errors::display_path(filename);
    let main_frame = error
        .stack_trace
        .iter()
        .rev()
        .find(|frame| frame.function == "<main>" && frame.line > 0);
    let (line, col) = match main_frame {
        Some(frame) if source.lines().count() >= frame.line => (frame.line, frame.col.max(1)),
        _ => (0, 0),
    };
    let diagnostic = errors::ProgramDiagnostic::runtime(&error.message, &file, line, col);
    eprintln!("{}", diagnostic.render(source));
    if errors::error_format() == errors::ErrorFormat::Human
        && (line == 0 || error.stack_trace.len() > 1)
    {
        for frame in &error.stack_trace {
            eprintln!("  at {} (line {})", frame.function, frame.line);
        }
    }
}

/// Print an interpreter runtime error (rendered like the VM's).
fn report_interpreter_error(source: &str, filename: &str, e: &interpreter::RuntimeError) {
    let file = errors::display_path(filename);
    let (line, col) = if e.line > 0 {
        (e.line, e.col.max(1))
    } else {
        (0, 0)
    };
    eprintln!(
        "{}",
        errors::ProgramDiagnostic::runtime(&e.message, &file, line, col).render(source)
    );
}

/// Run a package-manager operation on a plain OS thread.
///
/// Tell the user a program is running on the interpreter instead of the
/// VM. Under `FORGE_LOG_FORMAT=json` stderr carries only JSON events, so
/// the note becomes a structured `forge.runtime` event instead of text.
fn note_interpreter_fallback(reason: &str) {
    if std::env::var("FORGE_LOG_FORMAT").as_deref() == Ok("json") {
        forge_lang::runtime::tracing_init::init_subscriber();
        tracing::info!(target: "forge.runtime", reason = %reason, "falling back to interpreter");
    } else {
        eprintln!("  Info: falling back to interpreter ({})", reason);
    }
}

/// The registry client uses `reqwest::blocking`, which panics ("Cannot drop a
/// runtime in a context where blocking is not allowed") when called from the
/// `#[tokio::main]` async context. A scoped thread has no runtime context.
fn run_off_runtime<T: Send, F: FnOnce() -> T + Send>(f: F) -> T {
    std::thread::scope(|scope| match scope.spawn(f).join() {
        Ok(value) => value,
        // The panic message has already been printed by the panic hook.
        Err(_) => process::exit(101),
    })
}

async fn run_source(source: &str, filename: &str, use_vm: bool, profile: bool, strict: bool) {
    let (program, warnings) = match prepare_program(source, filename, strict) {
        Ok(prepared) => prepared,
        Err(err) => print_frontend_error(source, filename, err),
    };
    emit_type_warnings(source, filename, &warnings);

    // Auto-fallback: if VM is requested but the program uses constructs the
    // VM does not support (decorators, or anything the compiler rejects as
    // `Unsupported`), run it on the interpreter instead.
    let mut chunk = None;
    if use_vm {
        match ensure_vm_compatible(&program, "VM", Serving::Supported) {
            Ok(()) => {
                let path = std::path::Path::new(filename);
                let options = vm::compiler::CompileOptions {
                    base_dir: path
                        .exists()
                        .then(|| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
                        .and_then(|p| p.parent().map(|d| d.to_path_buf())),
                };
                match vm::compiler::compile_with(&program, &options) {
                    Ok(compiled) => chunk = Some(compiled),
                    Err(e) if e.is_unsupported() => {
                        note_interpreter_fallback(&e.message);
                    }
                    Err(e) => {
                        // Compile-time failures (an import that cannot be
                        // resolved, ...) use the runtime codes the
                        // interpreter reports for the same problem.
                        let file = errors::display_path(filename);
                        eprintln!(
                            "{}",
                            errors::ProgramDiagnostic::runtime(&e.message, &file, 0, 0)
                                .render(source)
                        );
                        process::exit(1);
                    }
                }
            }
            Err(message) => {
                note_interpreter_fallback(&message);
            }
        }
    }

    if let Some(chunk) = chunk {
        let runtime_plan = runtime::metadata::extract_runtime_plan(&program);
        if runtime_plan.server.is_some() {
            // Serve on the VM: run the top level with schedule/watch start-up
            // deferred (as the interpreter path does), then fork the final
            // state per request.
            let mut machine = if profile {
                vm::machine::VM::with_profiling()
            } else {
                vm::machine::VM::new()
            };
            machine.defer_host_runtime();
            if let Err(e) = machine.execute(&chunk) {
                report_vm_error(source, filename, &e);
                process::exit(1);
            }
            if profile {
                machine.profiler.print_report();
            }
            let fn_params = runtime::metadata::top_level_fn_params(&program);
            if let Err(e) = runtime::host::launch_vm(machine, &runtime_plan, fn_params).await {
                eprintln!("{}", errors::format_simple_error(&e.message));
                process::exit(1);
            }
        } else if let Err(e) = vm::run_chunk(&chunk, profile) {
            report_vm_error(source, filename, &e);
            process::exit(1);
        }
        exit_if_limit_tripped();
    } else {
        let mut interpreter = Interpreter::new();
        interpreter.source = Some(source.to_string());
        let path = std::path::Path::new(filename);
        if path.exists() {
            interpreter.source_file =
                Some(std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
        }
        interpreter.set_defer_host_runtime(true);
        match interpreter.run(&program) {
            Ok(_) => {}
            Err(e) => {
                report_interpreter_error(source, filename, &e);
                process::exit(1);
            }
        }
        exit_if_limit_tripped();

        let runtime_plan = runtime::metadata::extract_runtime_plan(&program);
        if let Err(e) = runtime::host::launch(interpreter, &runtime_plan).await {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    }
}

#[cfg(feature = "jit")]
fn run_jit(source: &str, filename: &str, strict: bool) {
    let (program, warnings) = match prepare_program(source, filename, strict) {
        Ok(prepared) => prepared,
        Err(err) => print_frontend_error(source, filename, err),
    };
    emit_type_warnings(source, filename, &warnings);
    if let Err(message) = ensure_vm_compatible(&program, "--jit", Serving::Rejected) {
        eprintln!("{}", errors::format_simple_error(&message));
        process::exit(1);
    }

    let chunk = match vm::compiler::compile(&program) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    };

    // Eager tier: every function is offered to the JIT on its first call.
    // Each (function, argument-kind signature) pair is verified and compiled
    // once; anything the verifier cannot prove is reported and runs in the
    // VM. Native code never changes program semantics.
    let mut vm = vm::machine::VM::new();
    vm.set_jit_mode(vm::jit::tier::JitMode::Eager);
    vm.jit.verbose = true;

    match vm.execute(&chunk) {
        Ok(_) => exit_if_limit_tripped(),
        Err(e) => {
            report_vm_error(source, filename, &e);
            process::exit(1);
        }
    }
}

fn compile_to_bytecode(source: &str, filename: &str, file_path: &PathBuf, strict: bool) {
    let (program, warnings) = match prepare_program(source, filename, strict) {
        Ok(prepared) => prepared,
        Err(err) => print_frontend_error(source, filename, err),
    };
    emit_type_warnings(source, filename, &warnings);
    if let Err(message) = ensure_vm_compatible(&program, "bytecode build", Serving::Rejected) {
        eprintln!("{}", errors::format_simple_error(&message));
        process::exit(1);
    }

    match vm::compiler::compile(&program) {
        Ok(chunk) => {
            let out_path = file_path.with_extension("fgc");
            let bytes = match vm::serialize::serialize_chunk(&chunk) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("{}", errors::format_simple_error(&e.message));
                    process::exit(1);
                }
            };
            if let Err(e) = fs::write(&out_path, &bytes) {
                eprintln!(
                    "{}",
                    errors::format_simple_error(&format!(
                        "could not write '{}': {}",
                        out_path.display(),
                        e
                    ))
                );
                process::exit(1);
            }
            println!(
                "Compiled {} -> {}\n  {} instructions\n  {} constants\n  {} prototypes\n  {} max registers\n  {} bytes",
                filename,
                out_path.display(),
                chunk.code.len(),
                chunk.constants.len(),
                chunk.prototypes.len(),
                chunk.max_registers,
                bytes.len(),
            );
        }
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    }
}

fn compile_to_native_launcher(
    source: &str,
    filename: &str,
    file_path: &PathBuf,
    strict: bool,
    allow_run: bool,
) {
    let (_, warnings) = match prepare_program(source, filename, strict) {
        Ok(prepared) => prepared,
        Err(err) => print_frontend_error(source, filename, err),
    };
    emit_type_warnings(source, filename, &warnings);

    match native::build_native_launcher(source, file_path, allow_run) {
        Ok(output) => {
            let runtime_msg = match output.runtime {
                native::NativeRuntimeKind::StandaloneSourceRuntime => {
                    "standalone source runtime (libforge linked; source embedded)"
                }
                native::NativeRuntimeKind::CliLauncher => {
                    "Forge CLI launcher required at execution time"
                }
            };
            println!(
                "Built native binary {} -> {}\n  runtime: {}",
                filename,
                output.path.display(),
                runtime_msg,
            );
        }
        Err(message) => {
            eprintln!("{}", errors::format_simple_error(&message));
            process::exit(1);
        }
    }
}

fn compile_to_native_aot(source: &str, filename: &str, file_path: &PathBuf, strict: bool) {
    let (program, warnings) = match prepare_program(source, filename, strict) {
        Ok(prepared) => prepared,
        Err(err) => print_frontend_error(source, filename, err),
    };
    emit_type_warnings(source, filename, &warnings);

    if let Err(message) = ensure_vm_compatible(&program, "AOT build", Serving::Rejected) {
        let message = if message.contains("decorator-driven runtime features") {
            format!(
                "{message}\n  hint: decorator-driven servers are not bytecode AOT yet; use `forge build --native` for a standalone source-runtime server binary"
            )
        } else {
            message
        };
        eprintln!("{}", errors::format_simple_error(&message));
        process::exit(1);
    }

    let chunk = match vm::compiler::compile(&program) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    };

    let bytecode = match vm::serialize::serialize_chunk(&chunk) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{}", errors::format_simple_error(&e.message));
            process::exit(1);
        }
    };

    match native::build_native_aot(&bytecode, file_path) {
        Ok(output_path) => {
            let standalone = native::find_libforge_dir().is_some();
            let runtime_msg = if standalone {
                "standalone (libforge linked)"
            } else {
                "Forge VM required at execution time"
            };
            println!(
                "Built AOT binary {} -> {}\n  bytecode embedded ({} bytes, no source exposure)\n  runtime: {}",
                filename,
                output_path.display(),
                bytecode.len(),
                runtime_msg
            );
        }
        Err(message) => {
            eprintln!("{}", errors::format_simple_error(&message));
            process::exit(1);
        }
    }
}

fn run_bytecode_file(file_path: &PathBuf, profile: bool) {
    let bytes = match fs::read(file_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_simple_error(&format!(
                    "could not read '{}': {}",
                    file_path.display(),
                    e
                ))
            );
            process::exit(1);
        }
    };

    let chunk = match vm::serialize::deserialize_chunk(&bytes) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "{}",
                errors::format_simple_error(&format!(
                    "invalid bytecode file '{}': {}",
                    file_path.display(),
                    e.message
                ))
            );
            process::exit(1);
        }
    };

    let mut vm = if profile {
        vm::machine::VM::with_profiling()
    } else {
        vm::machine::VM::new()
    };
    if let Err(e) = vm.execute(&chunk) {
        // No source to show: the stack trace names the function and line.
        report_vm_error("", &file_path.display().to_string(), &e);
        process::exit(1);
    }
    if profile {
        vm.profiler.print_report();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jit_help_documents_numeric_limit_and_vm_fallback() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains("--jit"));
        assert!(help.contains("JIT-compile numeric leaf functions"));
        assert!(help.contains("falls back to the bytecode interpreter automatically"));
    }

    #[test]
    fn ffi_is_opt_in_everywhere_but_interactive_use() {
        use permissions::Capability::Ffi;
        let parse = |args: &[&str]| Cli::try_parse_from(args).expect("parse").perms;
        let plain = parse(&["forge", "run", "a.fg"]);
        // `forge run`: denied unless granted.
        assert!(!build_policy(&plain, None, false, false, None).is_granted(Ffi));
        // REPL / -e: allowed, unless sandboxed.
        assert!(build_policy(&plain, None, false, true, None).is_granted(Ffi));
        let sandboxed = parse(&["forge", "--sandbox", "run", "a.fg"]);
        assert!(!build_policy(&sandboxed, None, false, true, None).is_granted(Ffi));
        // An explicit grant wins, including a scoped one under --sandbox.
        let granted = parse(&["forge", "--sandbox", "--allow-ffi=./plugins", "run", "a.fg"]);
        assert!(build_policy(&granted, None, false, false, None).is_granted(Ffi));
        // forge.toml can grant it too.
        let toml = manifest::PermissionsConfig {
            allow_ffi: Some(manifest::GrantSpec::Flag(true)),
            ..Default::default()
        };
        assert!(build_policy(&plain, Some(toml), false, false, None).is_granted(Ffi));
        // forge mcp: deny-all unless --allow-ffi.
        assert!(!build_mcp_policy(&plain, None, false).is_granted(Ffi));
        let mcp = parse(&["forge", "--allow-ffi", "mcp"]);
        assert!(build_mcp_policy(&mcp, None, false).is_granted(Ffi));
    }

    #[test]
    fn build_allow_run_is_native_only() {
        assert!(
            Cli::try_parse_from(["forge", "build", "--native", "--allow-run", "app.fg"]).is_ok()
        );
        assert!(Cli::try_parse_from(["forge", "build", "--aot", "--allow-run", "app.fg"]).is_err());
    }

    #[test]
    fn parity_corpus_supported_cases() {
        let cases = crate::testing::parity::load_supported_cases();
        assert!(!cases.is_empty(), "expected supported parity fixtures");
        for case in &cases {
            crate::testing::parity::assert_supported_case(case);
        }
    }

    #[test]
    fn parity_corpus_vm_rejection_cases() {
        let cases = crate::testing::parity::load_vm_rejection_cases();
        assert!(!cases.is_empty(), "expected VM rejection parity fixtures");

        for case in &cases {
            let (program, _) = prepare_program(&case.source, "<test>", false)
                .unwrap_or_else(|err| panic!("{} should parse: {:?}", case.path.display(), err));
            let error = ensure_vm_compatible(&program, "parity corpus", Serving::Rejected)
                .expect_err(&format!("{} should be rejected by VM", case.path.display()));
            assert!(
                error.contains(&case.expected_error),
                "{} rejection mismatch: expected substring '{}', got '{}'",
                case.path.display(),
                case.expected_error,
                error
            );
        }
    }

    #[test]
    fn prepare_program_rejects_strict_type_errors() {
        let source = r#"
        fn needs_int(x: Int) { return x }
        needs_int("oops")
        "#;

        match prepare_program(source, "<test>", true) {
            Err(FrontendError::Type(warnings)) => {
                assert!(warnings.iter().any(|w| w.is_error()));
                assert!(warnings.iter().any(|w| w.message.contains("expected Int")));
            }
            other => panic!("expected type error, got {:?}", other),
        }
    }

    #[test]
    fn prepare_program_keeps_non_strict_warnings_non_fatal() {
        let source = r#"
        fn needs_int(x: Int) { return x }
        needs_int("oops")
        "#;

        let (_, warnings) =
            prepare_program(source, "<test>", false).expect("program should prepare");
        assert!(warnings.iter().any(|w| !w.is_error()));
        assert!(warnings.iter().any(|w| w.message.contains("expected Int")));
    }

    #[test]
    fn vm_incompatibilities_allow_interface_and_impl_blocks() {
        let source = r#"
        thing Robot { id: Int }
        power Speakable { fn speak() -> String }
        give Robot {
            fn speak(it) { return "beep" }
        }
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        let issues = vm_incompatibilities(&program);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn vm_incompatibilities_allow_type_definitions() {
        let source = r#"
        type Color = Red | Green | Blue
        let color = Red
        color
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        let issues = vm_incompatibilities(&program);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn vm_incompatibilities_ignore_basic_programs() {
        let source = r#"
        fn add(a, b) { return a + b }
        let sum = add(20, 22)
        println(sum)
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_object_destructuring() {
        let source = r#"
        let user = { name: "Forge", age: 4 }
        unpack { name, age } from user
        name
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_try_catch() {
        let source = r#"
        let status = "ok"
        try {
            let crash = 1 / 0
        } catch err {
            status = err.type
        }
        status
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_array_rest_destructuring() {
        let source = r#"
        let items = [1, 2, 3]
        unpack [first, ...rest] from items
        first
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_safe_blocks() {
        let source = r#"
        let mut status = "ok"
        safe {
            let crash = 1 / 0
            status = "bad"
        }
        status
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_retry_blocks() {
        let source = r#"
        let mut attempts = 0
        retry 3 times {
            attempts += 1
            if attempts < 3 {
                let crash = 1 / 0
            }
        }
        attempts
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_timeout_blocks() {
        let source = r#"
        timeout 1 seconds {
            println("slow")
        }
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_file_imports() {
        let import_path = format!("/tmp/forge_vm_import_check_{}.fg", std::process::id());
        std::fs::write(&import_path, r#"let meaning = 42"#).expect("write import fixture");

        let source = format!(
            r#"
            import "{}"
            meaning
            "#,
            import_path
        );

        let (program, _) = prepare_program(&source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());

        std::fs::remove_file(&import_path).ok();
    }

    #[test]
    fn vm_incompatibilities_allow_where_filters() {
        let source = r#"
        let users = [{ age: 17 }, { age: 30 }]
        users where age >= 18
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_pipe_chains() {
        let source = r#"
        let users = [{ name: "Bob", active: true }]
        users >> keep where active >> sort by name >> take 1
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_prompt_definitions() {
        let source = r#"
        prompt summarize(text) {
            system: "You are concise"
            user: "Summarize: {text}"
        }
        let kind = type(summarize)
        kind
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_agent_definitions() {
        let source = r#"
        agent researcher(topic) {
            tools: ["search", "read"]
            goal: "Research {topic}"
            max_steps: 5
        }
        let kind = type(researcher)
        kind
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn vm_incompatibilities_allow_test_decorators() {
        let source = r#"
        @test
        fn smoke() { return 42 }
        smoke()
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
    }

    #[test]
    fn server_programs_run_on_the_vm_only_where_it_serves() {
        let source = r#"
        @server(port: 8080)
        @get("/hello")
        fn hello() { return "hi" }
        "#;

        let (program, _) = prepare_program(source, "<test>", false).expect("program should parse");
        assert!(vm_incompatibilities(&program).is_empty());
        assert!(ensure_vm_compatible(&program, "VM", Serving::Supported).is_ok());
        for mode in ["--jit", "AOT build"] {
            let error = ensure_vm_compatible(&program, mode, Serving::Rejected)
                .expect_err("a non-serving VM entry point must reject @server");
            assert!(
                error.contains("decorator-driven runtime features"),
                "{error}"
            );
        }
    }

    #[test]
    fn unsupported_decorators_fall_back_to_the_interpreter() {
        for source in [
            "@server(port: compute_port())\n@get fn a() { return 1 }",
            "@server(8080)\n@get fn a() { return 1 }",
            "@cache\nfn a() { return 1 }",
            "@get(\"/a\", auth: true)\nfn a() { return 1 }",
            "@get(path)\nfn a() { return 1 }",
            "@patch(\"/a\")\nfn a() { return 1 }",
            "fn outer() {\n  @cache\n  fn inner() { return 1 }\n  return inner()\n}",
        ] {
            let (program, _) =
                prepare_program(source, "<test>", false).expect("program should parse");
            let issues = vm_incompatibilities(&program);
            assert!(
                issues
                    .iter()
                    .any(|issue| issue.contains("decorator-driven runtime features")),
                "{source:?} should fall back, got {issues:?}"
            );
        }
    }
}
