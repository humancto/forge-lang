mod builtins_registry;
mod chat;
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

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process;

#[cfg(test)]
use clap::CommandFactory;
use clap::{Parser, Subcommand};

use interpreter::Interpreter;
use parser::ast::{Expr, Program, Stmt};

const VERSION: &str = env!("CARGO_PKG_VERSION");

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

    /// Allow shell execution (sh, shell, run_command, sh_lines, sh_json, sh_ok, pipe_to).
    /// Without this flag, these builtins return a permission error.
    #[arg(long = "allow-run")]
    allow_run: bool,

    /// Maximum Forge call depth before "maximum recursion depth exceeded"
    /// (default 10000; also settable with FORGE_MAX_DEPTH).
    #[arg(long = "max-depth", value_name = "N")]
    max_depth: Option<usize>,

    #[command(flatten)]
    perms: PermissionFlags,
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

/// `forge mcp`: serve until the client closes stdin, then exit.
fn run_mcp(caps: permissions::Capabilities, max_time: Option<f64>) -> ! {
    let mut config = mcp::ServerConfig::new(caps);
    if let Some(secs) = max_time {
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
    eprintln!(
        "forge mcp {}: {}",
        env!("CARGO_PKG_VERSION"),
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
enum Command {
    /// Run a Forge source file (.fg) or compiled bytecode (.fgc)
    Run {
        /// Path to a .fg or .fgc file (reads entry from forge.toml if omitted)
        file: Option<PathBuf>,
        /// Allow shell execution (same as the top-level --allow-run)
        #[arg(long = "allow-run")]
        allow_run: bool,
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
    /// forge_reference). Scripts are denied everything (files, network,
    /// env, db, subprocesses, AI) unless granted with --allow-* flags or
    /// [permissions] in forge.toml; --max-time caps each call (default 30s).
    #[command(
        after_help = "Example (Claude Desktop / Claude Code config):\n  {\"command\": \"forge\", \"args\": [\"mcp\", \"--allow-net=api.example.com\"]}"
    )]
    Mcp {
        /// Let scripts run subprocesses (sh, run_command, ...). Never granted
        /// by default.
        #[arg(long = "allow-run")]
        allow_run: bool,
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
    if let Some(Command::Mcp { allow_run }) = cli.command {
        // No process watchdog for the server: the limit applies to each
        // script instead.
        let caps = build_mcp_policy(&cli.perms, toml_perms, cli.allow_run || allow_run);
        run_mcp(caps, max_time);
    }
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

fn render_diagnostic(source: &str, filename: &str, d: &typechecker::Diagnostic) -> String {
    let len = if d.span.end.line == d.span.start.line {
        d.span.end.col.saturating_sub(d.span.start.col).max(1)
    } else {
        1
    };
    errors::format_diagnostic(
        &errors::display_path(filename),
        source,
        &errors::DiagnosticView {
            code: d.code.as_str(),
            message: &d.message,
            help: d.help.as_deref(),
            line: d.line(),
            col: d.col().max(1),
            len,
            is_error: d.is_error(),
        },
    )
}

fn print_frontend_error(source: &str, filename: &str, err: FrontendError) -> ! {
    match err {
        FrontendError::Lex { line, col, message } | FrontendError::Parse { line, col, message } => {
            let filename = &errors::display_path(filename);
            eprintln!(
                "{}",
                errors::format_error(filename, source, line, col, &message)
            );
        }
        FrontendError::Type(diagnostics) => {
            for d in &diagnostics {
                eprintln!("{}", render_diagnostic(source, filename, d));
            }
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
        eprintln!("{}", render_diagnostic(source, filename, d));
    }
}

fn collect_vm_incompatible_stmt(stmt: &Stmt, issues: &mut BTreeSet<String>) {
    match stmt {
        Stmt::TypeDef { .. } => {}
        Stmt::InterfaceDef { .. } => {}
        Stmt::ImplBlock { methods, .. } => {
            for method in methods {
                collect_vm_incompatible_stmt(&method.stmt, issues);
            }
        }
        Stmt::Destructure { pattern: _, value } => {
            collect_vm_incompatible_expr(value, issues);
        }
        Stmt::TryCatch {
            try_body,
            catch_body,
            ..
        } => {
            for s in try_body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
            for s in catch_body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::SafeBlock { body } => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::TimeoutBlock { body, .. } => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::RetryBlock { count, body } => {
            collect_vm_incompatible_expr(count, issues);
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::ScheduleBlock { body, .. } => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::WatchBlock { body, .. } => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::PromptDef { .. } => {}
        Stmt::AgentDef { .. } => {}
        Stmt::DecoratorStmt(decorator) => {
            issues.extend(runtime::metadata::vm_unsupported_decorator(decorator, true));
        }
        Stmt::Import { .. } | Stmt::ImportNative { .. } => {}
        Stmt::FnDef {
            body, decorators, ..
        } => {
            issues.extend(
                decorators
                    .iter()
                    .filter_map(|d| runtime::metadata::vm_unsupported_decorator(d, false)),
            );
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
            if let Some(else_body) = else_body {
                for s in else_body {
                    collect_vm_incompatible_stmt(&s.stmt, issues);
                }
            }
        }
        Stmt::Match { arms, .. } => {
            for arm in arms {
                for s in &arm.body {
                    collect_vm_incompatible_stmt(&s.stmt, issues);
                }
            }
        }
        Stmt::For { body, .. }
        | Stmt::While { body, .. }
        | Stmt::Loop { body }
        | Stmt::Spawn { body }
        | Stmt::Squad { body } => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Stmt::Let { value, .. } | Stmt::Expression(value) | Stmt::YieldStmt(value) => {
            collect_vm_incompatible_expr(value, issues)
        }
        Stmt::Assign { target, value } => {
            collect_vm_incompatible_expr(target, issues);
            collect_vm_incompatible_expr(value, issues);
        }
        Stmt::Return(Some(expr)) | Stmt::CheckStmt { expr, .. } => {
            collect_vm_incompatible_expr(expr, issues)
        }
        Stmt::When { subject, arms } => {
            collect_vm_incompatible_expr(subject, issues);
            for arm in arms {
                if let Some(value) = &arm.value {
                    collect_vm_incompatible_expr(value, issues);
                }
                collect_vm_incompatible_expr(&arm.result, issues);
            }
        }
        Stmt::Return(None) | Stmt::Break | Stmt::Continue | Stmt::StructDef { .. } => {}
    }
}

fn collect_vm_incompatible_expr(expr: &Expr, issues: &mut BTreeSet<String>) {
    match expr {
        Expr::BinOp { left, right, .. } => {
            collect_vm_incompatible_expr(left, issues);
            collect_vm_incompatible_expr(right, issues);
        }
        Expr::UnaryOp { operand, .. } | Expr::Try(operand) => {
            collect_vm_incompatible_expr(operand, issues)
        }
        Expr::FieldAccess { object, .. } => collect_vm_incompatible_expr(object, issues),
        Expr::Index { object, index } => {
            collect_vm_incompatible_expr(object, issues);
            collect_vm_incompatible_expr(index, issues);
        }
        Expr::Call { function, args } => {
            collect_vm_incompatible_expr(function, issues);
            for arg in args {
                collect_vm_incompatible_expr(arg, issues);
            }
        }
        Expr::Pipeline { value, function } => {
            collect_vm_incompatible_expr(value, issues);
            collect_vm_incompatible_expr(function, issues);
        }
        Expr::Lambda { body, .. } | Expr::Block(body) => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Expr::Object(fields) | Expr::StructInit { fields, .. } => {
            for (_, value) in fields {
                collect_vm_incompatible_expr(value, issues);
            }
        }
        Expr::Array(items) => {
            for item in items {
                collect_vm_incompatible_expr(item, issues);
            }
        }
        Expr::StringInterp(parts) => {
            for part in parts {
                if let parser::ast::StringPart::Expr(expr) = part {
                    collect_vm_incompatible_expr(expr, issues);
                }
            }
        }
        Expr::MethodCall { object, args, .. } => {
            collect_vm_incompatible_expr(object, issues);
            for arg in args {
                collect_vm_incompatible_expr(arg, issues);
            }
        }
        Expr::WhereFilter { source, value, .. } => {
            collect_vm_incompatible_expr(source, issues);
            collect_vm_incompatible_expr(value, issues);
        }
        Expr::PipeChain { source, steps } => {
            collect_vm_incompatible_expr(source, issues);
            for step in steps {
                match step {
                    parser::ast::PipeStep::Keep(expr)
                    | parser::ast::PipeStep::Take(expr)
                    | parser::ast::PipeStep::Apply(expr) => {
                        collect_vm_incompatible_expr(expr, issues);
                    }
                    parser::ast::PipeStep::Sort(_) => {}
                }
            }
        }
        Expr::Must(expr) | Expr::Ask(expr) | Expr::Freeze(expr) | Expr::Await(expr) => {
            collect_vm_incompatible_expr(expr, issues);
        }
        Expr::Spread(expr) => collect_vm_incompatible_expr(expr, issues),
        Expr::Spawn(body) | Expr::Squad(body) => {
            for s in body {
                collect_vm_incompatible_stmt(&s.stmt, issues);
            }
        }
        Expr::Tuple(items) => {
            for item in items {
                collect_vm_incompatible_expr(item, issues);
            }
        }
        Expr::Int(_) | Expr::Float(_) | Expr::StringLit(_) | Expr::Bool(_) | Expr::Ident(_) => {}
    }
}

fn vm_incompatibilities(program: &Program) -> Vec<String> {
    let mut issues = BTreeSet::new();
    for stmt in &program.statements {
        collect_vm_incompatible_stmt(&stmt.stmt, &mut issues);
    }
    issues.into_iter().collect()
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
    let filename = &errors::display_path(filename);
    let main_frame = error
        .stack_trace
        .iter()
        .rev()
        .find(|frame| frame.function == "<main>" && frame.line > 0);
    match main_frame {
        Some(frame) if source.lines().count() >= frame.line => {
            eprintln!(
                "{}",
                errors::format_error(
                    filename,
                    source,
                    frame.line,
                    frame.col.max(1),
                    &error.message
                )
            );
            if error.stack_trace.len() > 1 {
                for frame in &error.stack_trace {
                    eprintln!("  at {} (line {})", frame.function, frame.line);
                }
            }
        }
        _ => eprintln!("{}", errors::format_simple_error(&error.to_string())),
    }
}

/// Run a package-manager operation on a plain OS thread.
///
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
                        eprintln!("  Info: falling back to interpreter ({})", e.message);
                    }
                    Err(e) => {
                        eprintln!("{}", errors::format_simple_error(&e.message));
                        process::exit(1);
                    }
                }
            }
            Err(message) => {
                eprintln!("  Info: falling back to interpreter ({})", message);
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
                let filename = &errors::display_path(filename);
                if e.line > 0 {
                    eprintln!(
                        "{}",
                        errors::format_error(
                            filename,
                            source,
                            e.line,
                            if e.col > 0 { e.col } else { 1 },
                            &e.message
                        )
                    );
                } else {
                    eprintln!(
                        "{}",
                        errors::format_simple_error(&format!("[{}] {}", filename, e.message))
                    );
                }
                process::exit(1);
            }
        }

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
        Ok(_) => {}
        Err(e) => {
            // Use the full Display impl so the stack trace (function +
            // source line) gets printed, not just the bare message.
            eprintln!("{}", errors::format_simple_error(&e.to_string()));
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
    match vm.execute(&chunk) {
        Ok(_) => {}
        Err(e) => {
            // Use the full Display impl so the stack trace (function +
            // source line) gets printed, not just the bare message.
            eprintln!("{}", errors::format_simple_error(&e.to_string()));
            process::exit(1);
        }
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
