# Changelog

All notable changes to Forge will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Deterministic resource limits for untrusted code** (`src/runtime/limits.rs`) — `--max-fuel <n>` / `Sandbox::max_fuel` is a step budget (VM: bytecode instructions; interpreter: statements, calls and loop iterations) that runs out at exactly the same step on every run; `--max-memory <bytes|K|M|G>` / `Sandbox::max_memory` caps memory (VM: GC-heap accounting with a collection before failing; interpreter: the new `forge_lang::CountingAllocator`, scoped per run). Both fail with a fatal, uncatchable error (`fuel exhausted` / `memory limit exceeded`; `SandboxError::{FuelExhausted, MemoryLimit}`) and the host keeps running. `forge_lang::Limits` adds caps on concurrently open files, sockets, subprocesses and tasks, on the size of one string or collection (checked before allocating, so `repeat_str("x", 1e12)` or `range(1e12)` fail fast), and on imports (`SandboxError::ResourceLimit`). Budgets are per run and inherited by forked threads. Fuel limits keep the VM out of JIT code. Overhead is within noise: on a 20M-iteration dispatch-bound loop (A/B vs. the previous build, median of 7) the VM ran 3.74 s → 3.70 s unlimited and 3.58–3.63 s with fuel/memory limits, the interpreter 12.73 s → 12.70 s unlimited and 12.0–12.7 s limited; `tools/bench_compare.py` on the `vm`/`interp` suites shows no regression beyond the machine's noise. Fuel limits do turn off the JIT, so JIT-bound programs run at bytecode speed under `--max-fuel`.
- **`forge mcp` resource limits** — every `run_forge` call runs under 200M steps of fuel, 256 MiB of memory and handle/size/import caps by default (`--max-fuel`, `--max-memory` to change; an agent's `max_fuel` argument can only lower the fuel); new error kinds `fuel_exhausted`, `memory_limit`, `resource_limit`.
- **A real type checker** — local inference (variables, function results, generic type arguments, lambda parameters from the function type they are passed as, struct fields, array/map element types, `Option`/`Result` payloads, `match` bindings), checked against declared types and against the engines' own run-time rules. New diagnostics: invalid operators (`"a" - 1`, via the shared arithmetic rules), unknown names, types, struct fields and module members with "did you mean" suggestions, wrong argument counts, calls of non-functions, assignments to immutable bindings, use before definition, missing returns, unreachable code, struct literals with unknown or missing fields, and non-exhaustive `match` on algebraic types. Every diagnostic has a stable code (`T0001`–`T0018`), a precise span and, where possible, a quick fix; the CLI prints them with source snippets. Default mode reports warnings and runs the program; `--strict` makes them errors. The repository's test and example corpus is checked by a test that allows no strict-mode errors beyond a justified allowlist (`tests/typecheck_allowlist.txt`).
- **Function and tuple type annotations** — `fn(Int, String) -> Bool`, `fn()` and `(Int, String)` are valid annotations; `Option<Option<Int>>` (closing `>>`) parses.
- **Runtime enforcement under `--strict`** — annotated function and lambda arguments and declared return values are checked on every call, identically on the VM and the interpreter (`type error: argument 'n' of 'double' must be Int, got String`); correct programs behave the same with and without the flag.
- **LSP features on the type checker** — hover shows inferred types and signatures, go-to-definition and references follow imports across files, scope-aware rename (`textDocument/rename` + `prepareRename`) edits every file that uses the symbol, quick-fix code actions apply "did you mean" suggestions, semantic tokens classify every name, and inlay hints show the inferred type of unannotated `let` bindings. Diagnostics carry their codes.
- **`check_forge` (MCP) reports diagnostic codes** — each diagnostic has a `code` field.
- **Python package `forge-lang`** (`bindings/python`, PyO3 + maturin, abi3 wheels for CPython 3.9+ on Linux/macOS/Windows) — `forge_lang.Sandbox(allow=..., allow_read=..., allow_write=..., allow_net=..., max_time=..., max_output=...)` runs untrusted Forge code in-process under the deny-by-default sandbox. `run()` releases the GIL, is safe to call from many threads, honours Ctrl-C and a `CancelToken`, and raises typed exceptions (`ForgeSyntaxError`, `ForgePermissionError`, `ForgeRuntimeError`, `ForgeTimeoutError`, `ForgeOutputLimitError`, `ForgeCancelledError`, all `ForgeError`) carrying `kind`, `stdout`, `line`/`column` and `limit`; `check()` returns the same diagnostics as `forge mcp`'s `check_forge`. Ships type stubs and `py.typed`. A new `Python` workflow builds wheels and an sdist, runs pytest on Linux, and publishes to PyPI on `py-v*` tags via trusted publishing once enabled. A Node.js (napi-rs) design is in `bindings/node/README.md`.
- **Native plugins: call Rust and C from Forge** — `import native "path/libfoo" [as foo]` and `import { add } from native "path/libfoo"` load a shared library (platform suffix resolved) that implements Forge's versioned C plugin ABI (v1, `crates/forge-plugin/include/forge_plugin.h`) and call its functions with typed values (null/bool/int/float/string/bytes/array/object) on both engines. Plugin errors and panics become catchable Forge errors. New SDK crate `crates/forge-plugin` with `#[forge_fn]` and `export!`; examples in `examples/plugins/hello_rust` and `examples/plugins/hello_c`. Design: `rfcs/0006-native-plugins.md`.
- **`ffi` capability and `--allow-ffi[=PATHS]`** (also `allow-ffi` in `forge.toml`, `Sandbox::allow_ffi`) — loading native code is full trust, so it is opt-in for `forge run`/`forge test`, allowed in the REPL and `-e`, and denied under `--sandbox`, in `forge mcp` and in embedded sandboxes unless granted.
- **Sparse package registry** ([RFC 0007](rfcs/0007-package-registry.md)) — `forge add`/`install`/`update` resolve remote packages from a Git-hosted sparse index (one JSON-lines file per package) at `FORGE_REGISTRY_URL` (default `https://raw.githubusercontent.com/humancto/forge-registry/main`; `file://` and mirrors supported). Index files are cached per registry and revalidated with ETags (`FORGE_CACHE_TTL`), `FORGE_OFFLINE=1` works from the cache, archives are cached content-addressed, and a mirror's `config.json` can redirect downloads (`dl`).
- **Mandatory checksums and optional ed25519 signatures** — every archive is verified against its index SHA-256 before extraction. `forge publish --sign` signs entries with `~/.forge/keys/publish.key` (`$FORGE_SIGNING_KEY`). Installs pin each package's publisher key on first use (`~/.forge/trusted-keys.toml`) and refuse key changes or signature stripping. `FORGE_REQUIRE_SIGNATURES=1` refuses unsigned packages. `forge.lock` records `archive_checksum` and `signer` and refuses a locked version whose content or signer changed.
- **`forge publish --registry <index-clone>`** — builds a deterministic `dist/<name>-<version>.tar.gz`, appends the entry (deps, checksum, URL, signature) and commits it on a `publish/<name>-<version>` branch, ready for a pull request (`--download-url`, `--out-dir`, `--no-commit`). It enforces name rules (lowercase, reserved and look-alike names), immutable versions, `owners.toml` namespace/key ownership and signing-key continuity.
- **`forge yank <name@version> --registry <index-clone> [--undo]`** — yanked versions are skipped by new resolutions but stay installable from a lockfile, and `forge install` keeps lockfile versions that still match the manifest.
- **Benchmark regression gate** — `tools/bench.sh` is the single benchmark runner (suites `vm`, `interp`, `startup`, plus `peers` for Python/Node/Lua ports; human table or `--json`). `tools/bench_compare.py` runs it against two binaries in interleaved A/B rounds and compares medians. The new `Performance` workflow builds the PR base and head in release mode on one runner and fails when a benchmark is more than 15% slower, with the table in the job summary. The `perf-regression-ok` label accepts an intended slowdown. `tools/bench_vm.sh` / `bench_interp.sh` are now wrappers. Methodology and current numbers: `docs/BENCHMARKS.md`.
- **JIT Float tier** — functions over Float (and mixed Int/Float, with the language's promotion rules) now compile: arithmetic, comparisons, equality and truthiness with the VM's exact IEEE semantics (NaN, infinities, signed zero), plus pure builtins `math.sqrt/abs/floor/ceil/round/sin/cos/tan/log/pow/min/max/clamp`, `math.pi/e/inf`, `float()` and `int()`. Int overflow and results the VM would box differently (e.g. `math.floor(1e30)`) still deopt to the VM. `benchmarks/vm/mandelbrot.fg`: 0.55 s → 0.03 s; `spectral_norm`: 0.41 s → 0.18 s (release, whole process).
- **JIT calls between compiled functions** — a hot function that calls another pure function (through a global or a captured top-level `fn`) compiles to a direct native call of the callee's specialization instead of staying in the VM. Every global or captured binding the code relies on is a guard re-checked at each native entry, so rebinding it sends the next call to the VM.
- **Benchmarks** — `mandelbrot`, `spectral_norm`, `nbody`, `repeat_loop` and `range_loop` in `benchmarks/vm` (Python/Node/Lua ports in `benchmarks/peers`).
- **`tools/registry-template/`** — seed for the hosted index repository: README, `config.json`, `owners.toml`, CODEOWNERS placeholder and a CI validator (`scripts/validate_index.py`: format, names, semver, ownership, signatures, append-only history, archive checksums), cross-checked against `forge publish` output in `tests/registry_index.rs`.
- Outbound HTTP requests (`fetch`, `http.*`, `download`, `crawl`) run in an `http.client.request` span and, when OpenTelemetry export is active, send the W3C `traceparent` of that span, so downstream services join the caller's trace. A `traceparent` header the script sets itself is never replaced (#134)
- `OTEL_TRACES_SAMPLER` / `OTEL_TRACES_SAMPLER_ARG` configure head sampling (`always_on`, `always_off`, `traceidratio`, `parentbased_always_on` (default), `parentbased_always_off`, `parentbased_traceidratio`); invalid values fall back to the default with a warning (#135)
- `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` is honored and takes precedence over `OTEL_EXPORTER_OTLP_ENDPOINT` (#132)
- Rust panics are reported as structured `ERROR` events on the `forge.panic` target (payload, location, thread, backtrace when `RUST_BACKTRACE` is set) once Forge's tracing subscriber is installed, inside the current request span; if the active filter drops that target the standard panic message is printed instead (#121)
- **Write MCP tools in Forge: `forge mcp serve tools.fg`** — top-level functions annotated `@tool(description: ...)` become MCP tools whose JSON Schema comes from their parameter types (`Int`→integer, `Float`→number, `String`, `Bool`, `[T]`→array, `Object`/`Map<String, T>`/structs→object, `?T`/`Option<T>`/defaults→optional), documented with `@param(name: "...")`; a declared return type becomes the `outputSchema` and is checked. `@resource(uri: ...)` functions serve read-only resources (`resources/list`, `resources/read`). Arguments are validated before any Forge code runs (problems come back as `isError` results naming each field), `Err(x)` is a tool error, and every call runs in a fresh sandboxed fork of the file's top level under the server's grants, time and output limits. Load errors stop the server with `file:line` messages. `--with-code-tools` also serves `run_forge` & co. Example: `examples/mcp/weather_tools.fg`.
- **`run_forge` sessions** — an optional `session_id` keeps top-level variables, functions and types across calls (bounded by `--max-sessions`, default 16, and `--session-idle`, default 900s; one call at a time per session; each step gets fresh fuel and the memory limit covers the session's whole state); the new `reset_session` tool forgets one. Forge tool calls run under the same per-call resource limits as `run_forge`, and their runtime errors carry the stable error `code` and `hint`. Calls without `session_id` stay stateless.
- Windows shell support, completed: when no POSIX `sh` is on `PATH`, shell builtins run through `cmd /d /s /c "<command>"` with the command passed verbatim, so quoted arguments survive (`.arg()` escaping mangled them); `which` searches `PATH` in-process and honors `PATHEXT` (`which("npm")` finds `npm.cmd`); `run_command` resolves programs through `PATHEXT`
- **Bytecode verifier** — `src/vm/verify.rs` checks every deserialized chunk (`forge run app.fgc`, AOT binaries) before it runs: register, constant, prototype and upvalue indices, string name operands, branch targets (back-edges only via `Loop`, which polls cancellation), arity, line tables, terminal instructions and prototype nesting depth. Malformed bytecode is rejected with `invalid bytecode in '<chunk>' at instruction N: ...` instead of misbehaving. Debug builds also verify every chunk the compiler emits.
- **Fuzzing** — cargo-fuzz targets in `fuzz/` (`parse`, `compile`, `bytecode`, and a grammar-based `differential` target comparing the interpreter and the VM), a nightly `Fuzz` workflow (plus a Miri job for the C-ABI and JIT bridge code), and `tests/fuzz_smoke.rs`, which runs the same targets on stable in every `cargo test` and replays committed crashers from `fuzz/regressions/`.
- `VM::cancel_flag()` exposes the VM's cooperative cancellation flag to embedders.

- **Stable runtime error codes** — every runtime and syntax error has a code (`E0000`–`E0033`) from one table shared by both engines (`src/semantics/errors.rs`), so the same failure reports the same code and message on the VM and the interpreter. Errors print as `[E0009] Error: ...` with a one-line hint and a pointer to `forge explain`; caught errors expose `e.code`. Every code has a fixture in `tests/errors/` checked on both engines (`tests/error_codes.rs`).
- **`forge explain [CODE]`** — explanation, example and fix for any `E`/`T` code; without a code it lists them all. Each type-checker explanation's example is verified to produce its code.
- **Machine-readable diagnostics** — `--error-format json` (global flag) prints syntax, type and runtime diagnostics as one JSON object per line (`code`, `severity`, `message`, `file`, `line`, `col`, `hint`, `phase`). New `forge check [--format json] [file]` parses and type-checks without running (exit 1 on errors; `--strict` makes type diagnostics errors). `forge mcp`: `check_forge` diagnostics always carry a `code` (`E0001`/`E0002` for syntax errors) and a `hint`; `run_forge` runtime errors carry `code` and `hint`.
- **Better runtime messages** — "did you mean" for undefined names now also suggests locals on the VM (and prefers the innermost scope on both engines, deterministically); unknown object fields suggest a close field or list the fields; index errors give the valid range; field access on `null` explains where null usually comes from; builtin argument errors name the types that were passed (`len() requires ... (got Int)`); calling a non-function says `cannot call a value of type Int` on both engines.
- **Editions** — `edition = "2026"` under `[project]` in `forge.toml` is parsed and validated (unknown editions are refused by `run`, `test`, `check` and `mcp`); `forge new` writes it. Only one edition exists; `docs/STABILITY.md` describes how future breaking changes are gated on editions.
- **Deprecation mechanism for builtins** — a registry (`DEPRECATED` in `src/builtins_registry.rs`) whose entries warn once per process on both engines. Nothing is deprecated yet.
- **`docs/STABILITY.md`** — the 1.0 stability policy: syntax, semantics, stdlib signatures, error codes, JSON diagnostics, CLI, bytecode format versioning, `forge_lang::Sandbox` semver, plugin ABI v1, editions and the deprecation policy.

### Changed

- Runtime error output is headed by the error code and shows the hint as `Help:`; `e.type` of a caught `modulo by zero` is now `ArithmeticError` on the interpreter too (it already was on the VM).
- A `Float` value is no longer accepted where an `Int` is declared (an `Int` still widens to `Float`).
- The default registry is now `humancto/forge-registry` (the old `forge-lang/registry` URL never existed). A registry without `config.json` is reported as "not a Forge registry" with guidance, instead of every package looking missing.
- Registry archive extraction rejects symlinks, hard links and device entries as well as absolute and `..` paths.
- Updates of the form `x = x op e`, `x op= e`, `o.f = o.f op e`, `a[i] op= e` (any field/index chain) with an effect-free `e` and indexes are a single atomic read-modify-write in the interpreter, so squad `spawn`s that update state shared through a captured closure no longer lose updates (#128)

### Fixed

- Decorator named arguments accept soft keywords as keys, like object literals do (`@tool(timeout: 5)` was a parse error)
- `pad_start`/`pad_end` with a negative width no longer try to allocate an astronomically large string (the width is treated as 0).
- The VM ignored a statement `match` whose arms all failed; it now raises `non-exhaustive match` (E0026) like the interpreter.
- `?` on an `Err` at the top level of a program ended it silently with exit status 0 on the VM; it now fails with `unhandled error: ...` (E0024) like the interpreter.
- VM string builtins given a non-string said `expected string argument`; they now name the builtin (`upper() requires a string (got Int)`), as on the interpreter.
- `pipe_to` no longer deadlocks when both its input and the command's output exceed the OS pipe buffer
- `which` no longer depends on `/usr/bin/which` being installed
- `forge run` logs: the default filter now enables the CLI binary's own targets (`forge=info`), so the server's per-request `request` span (`method`, `uri`, `request_id`) and the `forge.server` startup event are no longer filtered out; under `FORGE_LOG_FORMAT=json` the VM-to-interpreter fallback note is a `forge.runtime` JSON event instead of plain text, so stderr stays line-delimited JSON (#119, #120)
- The debug-build check that rejects a `Value::Stream` in a server's top-level environment now also finds streams captured by closures, and names the binding path (#115)
- `==` / `!=` are total on both engines: comparing values of different types is `false` instead of an interpreter error, and numbers compare numerically inside collections (`[1] == [1.0]`). `match` literal patterns follow the same rule (`3` matches `3.0`). `Ok(1) == Ok(1)` is now `true` on the VM.
- A `match` with no matching arm is a runtime error (`non-exhaustive match`) on the VM too (it silently did nothing).
- `is_ok()` / `is_err()` / `unwrap_or()` on a non-Result value are an error on the VM too.
- `contains()` on the VM finds object keys (`contains({a: 1}, "a")` was `false`) and rejects unsearchable arguments like the interpreter.
- Assigning a field on a non-object (`b.a = 1` with `b = false`) is an error on the interpreter too (it was silently ignored).
- Anonymous functions display as `<lambda>` (and `"<Lambda>"` inside objects) on both engines (the VM printed `<fn <lambda>>` / `"<Function>"`).
- Function values compare by identity on both engines (`f == f` was `false` on the interpreter).
- `min_of()` / `max_of()` on the VM reject empty and non-numeric arrays and promote mixed Int/Float to Float, like the interpreter.
- `out.push(f())` on the VM evaluates the argument before reading `out`, so mutations `f` makes to a captured `out` are kept (they were lost).
- Built-in string methods (`chars`, `bytes`, `words`, `char_at`, `is_alpha`, `encode_uri`, ...) are shared by both engines; `"ab".chars()` and friends now work on the VM.
- `range()`, `sample()` and `slay()` counts above 100,000,000, and `repeat_str()` / `pad_start()` / `pad_end()` results above 1 GiB, are runtime errors instead of a crash.
- **`repeat n times` and `for i in range(..)` no longer allocate the range** on the VM: they compile to a counting loop (falling back to the old path when `range` is not the builtin or the arguments are not Ints) and are JIT-eligible. A function running `repeat 2000000 times` went from 0.29 s to 0.02 s; top-level `repeat`/`range` loops (never JIT-compiled) are 1.5-1.8x faster.

### Fixed

- VM: `range(1, 2.5)` raised no error and returned `[0]`; it now fails with "range() requires integer arguments", like the interpreter.
- The interpreter no longer leaks memory through closure reference cycles. A function or lambda stored in a scope it captures (every recursive function, closures kept in loop bodies, inner functions, methods, imported modules) formed an `Arc` cycle that was never freed: each interpreter that defined a function leaked its whole global scope (~88 KB, stdlib modules included), and loops that created such closures leaked on every iteration. A cycle collector now reclaims them when the last interpreter of a run is dropped (each `Sandbox` run, `forge mcp` call, Python `Sandbox.run`, `--interp` HTTP request fork) and periodically during long runs. Values that outlive the interpreter, such as a lambda the host keeps, are unaffected.

### Security

- Sandbox: tasks a script `spawn`s and never awaits are now stopped when the run ends. Before, `spawn { while true { } }` returned immediately and left the task spinning in the host (`forge mcp`, embedders, the Python package) after the call finished.
- `GITHUB_TOKEN` is sent only over HTTPS to GitHub hosts. It was previously attached to requests for any `FORGE_REGISTRY_URL` and archive URL.
- `cargo bench --bench server_throughput` boots `forge run` on `examples/bench_server*.fg` on both engines and reports requests per second and latency percentiles; `benches/fork_for_serving.rs` also measures the VM's per-request fork.
- `tests/server_engine_parity.rs` runs one `@server` program on the VM and on the interpreter and requires identical responses (routes, path/query/body binding, every method, 404/405/body rejections, handler errors, panics, isolation of globals and closures, deep recursion, WebSocket echo).

### Changed

- **`@server` programs now run on the bytecode VM.** `forge run` no longer falls back to the interpreter for decorator-driven HTTP servers: the VM runs the top level, freezes the result into a read-only template (`vm::serve::VmTemplate`) and forks a private VM per request (and once per WebSocket connection), with the same isolation contract as the interpreter — handler mutations of globals, collections and captured closure state never leak into other requests. Backpressure, cancel-on-disconnect, request-id tracing, large handler stacks and `schedule`/`watch` start-up after the top level are unchanged. `--interp` still serves on the interpreter. Decorators the VM cannot honor (unknown decorators, `@server` arguments that are not literals, route decorators with extra arguments) keep the program on the interpreter. CPU-bound handlers get the VM's speed: on one loaded 4-core machine, `fib(25)` per request went from 5 to 1,030 req/s and `/ping` from 4.6k to 12.8k req/s (`cargo bench --bench server_throughput`).
- Server handlers now run under the capability policy that was in force when the server started, on every blocking-pool thread (previously they used the process-wide policy).

### Security

Findings and fixes from the sandbox audit (`docs/SECURITY_AUDIT.md`); every fix has a regression test in `tests/security/` (`cargo test --test security`).

- SQLite can no longer open files outside the filesystem grant: under a restricted `fs` policy `ATTACH DATABASE`, `VACUUM INTO` and `file:` URI names are refused, and `db.open` files need both `fs.read` and `fs.write` (SEC-01).
- A sandbox's deadline and cancel now reach everything the script started: `squad` bodies and tasks, `timeout` bodies, imported modules, blocking waits (`receive`, iterating a channel, `await`, `await_all`, `select`, `time.sleep`) and loops with an empty body (`while true { }`). Imported modules and tasks can no longer start `schedule`/`watch` threads inside a sandbox (SEC-02).
- `pg.connect` and `mysql.connect` require `net` for every server they connect to, not just `db` (SEC-03).
- Script-sized allocations (`repeat_str`, `range`, `pad_start`/`pad_end`, `sample`, `slay`, `crypto.random_bytes`) are fallible: an impossible size is a runtime error instead of aborting the host process; a negative pad length pads nothing; `time.sleep` with an out-of-range duration is an error instead of a panic (SEC-04).
- Under a restricted policy, `env.set`/`env.load` cannot change Forge's own configuration (the `FORGE_*` variables the runtime reads, `OPENAI_API_KEY`, `OTEL_*`, HTTP proxy variables, `RUST_LOG`), closing net-allowlist and SSRF-guard bypasses; invalid variable names are errors instead of panics (SEC-05, SEC-12).
- `env.load` loads exactly the checked file instead of searching parent directories outside the grant (SEC-06).
- `term.confirm`/`term.menu` and `io.args*` need the `process` capability, like `input()`: embedded scripts can no longer read the host's stdin or command line (SEC-07, SEC-13).
- `which()` requires `--allow-run` (it spawns a subprocess) (SEC-08).
- `watch`, `path.resolve` and `path.relative` require `fs.read` for the path (SEC-09).
- `ws.connect` applies the HTTP client's private-address (SSRF) guard (SEC-10).
- MySQL and WebSocket handles are unguessable, so one sandbox cannot use another's connection in the same host process (SEC-11).
- `Sandbox::max_output` is enforced on every write instead of by polling (SEC-14).
- Filesystem operations, imports and `watch` act on the resolved path that passed the permission check, so a concurrent `cd` cannot redirect them (SEC-15).
- VM: a `timeout` block (and the host's cancel) now stops everything started inside it — `squad` tasks, spawned tasks, blocked `receive`/`await`/`await_all`/`select`/`for x in channel` — and a deadline inside an imported module reports `timeout: ...` instead of `internal control transfer to catch handler` (SEC-02, VM part).
- Values nested more than 10,000 levels deep (`a = [a]` in a loop) are an error (`value nested too deeply`) in conversions, `json.stringify`/`json.pretty`, display and equality, instead of aborting the process with a native stack overflow (SEC-16).

### Fixed

- Bytecode loading: length prefixes are checked against the remaining input before allocating, prototype nesting is bounded, and trailing bytes are rejected — a crafted `.fgc` can no longer panic, overflow the stack or exhaust memory while loading.
- The compiler fails cleanly for functions whose jumps exceed the 16-bit branch offset instead of emitting wrong jumps.
- JIT code memory is freed when a VM is dropped (every VM that tiered a function up leaked its code pages).
- The parser looped forever on a `prompt` block entry that is not `name: "string"` (found by fuzzing).
- Panics on extreme inputs: `-(-9223372036854775807 - 1)` on the interpreter, `substring(s, start, end)` with `start > end`, `range()`/`sample()`/`slay()`/`repeat_str()` with huge counts, `wait()`/`time.sleep()` with huge or infinite durations, `timeout` with a huge duration on the VM, and `schedule` intervals that overflow.

## [0.9.0] - 2026-10-05

Highlights: the default VM is now trustworthy (a guarded, verified JIT tier; GC rooting; full VM/interpreter parity on the test suite), much faster (VM and interpreter performance passes), and Forge gains a capability-based sandbox (`--sandbox`, `--allow-*`, `--max-time`, `forge_lang::Sandbox`) plus `forge mcp`, an MCP server that lets AI agents run sandboxed Forge code.

### Added

- **`/* ... */` block comments** — the lexer now accepts block comments anywhere whitespace is allowed (non-nesting, as the spec describes); line numbers after multi-line comments stay correct, a multi-line block comment separates statements like a newline, and an unclosed comment is an error pointing at its `/*`. `Lexer::tokenize_with_comments` returns the comments for tools such as the formatter.
- **Single builtin registry** — `src/builtins_registry.rs` lists every global builtin (with its arity) and every stdlib module; both engines register and dispatch from it, so a builtin or module can no longer exist on one engine only (tests fail if it does). Builtin arity errors are identical on both engines (`len() expects 1 argument, got 2`); `upper`/`lower`/`trim` are global functions on both engines.
- **Default parameters on the VM and call arity checks** — `fn g(a, b = 10)` works on the VM (defaults may use earlier parameters; an explicit `null` is kept). Calling a user function or lambda directly with too few or too many arguments is a catchable runtime error on both engines (`fn add expects 2 arguments, got 1`); callbacks invoked by builtins (`map`, `filter`, ...) stay lenient. Bytecode format is now v1.3.

- **Capability-based permissions** — one policy, checked identically by the VM and the interpreter, covering `fs.read`, `fs.write` (path-scoped; `..` and symlink escapes denied), `net` (host allowlist, redirects re-checked, server listen gated), `env`, `db`, `run`, `ai` and `process` (`exit`/`cd`). Denials read `permission denied: <cap> (<detail>) — run with --allow-<cap> or grant it in the host policy`. Defaults for `forge run`, `-e` and the REPL are unchanged.
- **`--sandbox` and Deno-style `--allow-read[=paths]`, `--allow-write[=paths]`, `--allow-net[=hosts]`, `--allow-env`, `--allow-db`, `--allow-ai`** — usable before or after the subcommand; `forge run --allow-run` now works too. The same policy can be set in `forge.toml` under `[permissions]` (a malformed table is an error, not ignored).
- **`--max-time <secs>`** — wall-clock limit for any program on any engine; exits with code 124.
- **`forge mcp` — Model Context Protocol server for AI agents** — a stdio MCP server ("code mode") with `run_forge` (run a script in the sandbox; typed errors, 64 KiB output cap, structured results), `check_forge` (parse + type-check diagnostics) and `forge_reference` (the `llms.txt` guide). Deny-all by default; grants come from the `--allow-*` flags and `forge.toml` `[permissions]`, `--max-time` caps each call (default 30s), and shell access needs an explicit `--allow-run`. Each call runs on its own thread (stuck scripts time out, `notifications/cancelled` stops them), malformed input gets JSON-RPC errors, and script I/O can never reach the protocol stream. Speaks MCP `2026-07-28` (stateless, `server/discover`) and the `initialize` handshake for `2025-11-25`–`2024-11-05`. Also embeddable as `forge_lang::mcp`.
- **Sandbox cancellation and output limit** — `Sandbox::run_source_cancellable` with a `CancelHandle`, `Sandbox::max_output`, and `SandboxError::{Cancelled, OutputLimit}` plus `kind()`/`stdout()` accessors.
- **Embedding API: `forge_lang::Sandbox`** — run untrusted Forge source from a Rust host under a default-deny policy with scoped grants, a wall-clock limit and captured stdout; returns `Output` or a typed `SandboxError`. Policies are per worker thread and inherited by every thread the engines fork, so concurrent sandboxes are independent.

- **Interpreter/VM differential test** — `cargo test --test engine_diff` runs all examples, parity fixtures and `tests/*.fg` suites on both engines and fails on any new output divergence; known gaps are tracked in `tests/engine_diff_known.txt`.
- **VS Code extension: language server and debugger** — `editors/vscode` now starts `forge lsp` via `vscode-languageclient` and contributes a `forge` debug type backed by `forge dap` (F5 on a `.fg` file works without `launch.json`). New settings `forge.path`, `forge.lsp.enabled`, `forge.trace.server` and a **Forge: Restart Language Server** command. The extension is plain JavaScript (no build step) and is ready to publish (`npm run package` produces a `.vsix`; icon, license and extension changelog included).
- **LSP formatting and signature help** — `forge lsp` now advertises `textDocument/formatting` (whole-document, via the `forge fmt` formatter) and `textDocument/signatureHelp` (builtins and user-defined functions, with active-parameter tracking).
- **`forge test --engine vm|interp|both`** — Forge tests now run on the same engine as `forge run` by default (the bytecode VM, with the same automatic interpreter fallback for programs the VM cannot run yet; the global `--interp` flag selects the interpreter). `--engine both` runs every file on both engines and reports a summary per engine. `--coverage` is collected from the interpreter.
- **`forge test --timeout <secs>`** — per-test time limit (default 60s, `0` disables). A test that hangs (for example an engine bug that loops forever) now aborts the run with a failure naming the test instead of stalling CI indefinitely.
- **Standalone source-runtime native binaries for Forge servers** — `forge build --native` now links against `libforge_lang.a` when available and emits a single executable that embeds Forge source and starts interpreter-only runtime features like `@server` without shelling out to the `forge` CLI. `--aot` remains bytecode/VM-only and continues to reject decorator-driven servers with guidance to use `--native`.
- **Startup time measurement harness** — `tools/startup_time.rs` measures source, bytecode, native source-runtime, and bytecode AOT process startup with correctness checks. CI runs it as a report-only signal before the `<10ms` native startup target becomes a hard gate.
- **Structured concurrency with `squad` blocks** — `squad { spawn { } spawn { } }` runs tasks concurrently with automatic join, cooperative cancellation on failure, and error propagation. Returns an array of results in spawn order. Works in both interpreter and VM engines.
- **First-class `Set` type** — `set([1, 2, 3])` or `set((1, 2, 3))` builds a deduplicated set. Methods: `.has(x)`, `.add(x)`, `.remove(x)`, `.union(other)`, `.intersect(other)`, `.diff(other)`, `.to_array()`. Supports `len()`, `contains()`, iteration, order-independent equality, and is truthy when non-empty. Works across interpreter, VM, bytecode round-trip, and JIT.
- **First-class `Map` type** — `map([("a", 1), ("b", 2)])` or `map()` builds an ordered key/value map with any-type keys. Methods: `.get(k)`, `.set(k, v)`, `.has(k)`, `.remove(k)`, `.keys()`, `.values()`, `.len()`, `.to_array()`. Insertion order is preserved on overwrite. Key equality uses container semantics (int/float collision, NaN self-match). Supports `for k, v in m` iteration (which also unlocks `for k, v in obj` parity for plain objects under the VM), `len()`, `contains()`, order-independent equality, and is truthy when non-empty. `json.stringify` emits JSON objects for maps with string keys and errors on non-string keys. Works across interpreter, VM, bytecode round-trip, and JIT.
- **`select(channels, timeout?)` builtin** — wait on multiple channels, returns `[index, value]` for the first ready channel. Optional timeout in ms. ([#69](https://github.com/humancto/forge-lang/pull/69))
- **`close(ch)` builtin and channel iteration** — close a channel to signal no more values; `for msg in ch { }` drains until closed. ([#70](https://github.com/humancto/forge-lang/pull/70))
- **Unbounded channels** — `channel()` with no args creates an unbounded channel; `channel(n)` creates bounded as before. ([#71](https://github.com/humancto/forge-lang/pull/71))
- **`await_all(handles)` builtin** — wait for multiple spawned tasks and collect results into an array. ([#72](https://github.com/humancto/forge-lang/pull/72))
- **`await_timeout(handle, ms)` builtin** — wait for a task with a deadline; returns Null on timeout. ([#73](https://github.com/humancto/forge-lang/pull/73))
- **Spawn Result wrapping** — spawned tasks wrap results in Ok/Err; `await` auto-unwraps Ok and propagates Err as runtime errors. ([#74](https://github.com/humancto/forge-lang/pull/74))
- **Semver constraint parsing** — `forge.toml` dependency specs now support `^1.0`, `~1.5`, `>=1.0.0, <2.0.0`, `*` version constraints via the `semver` crate. ([#75](https://github.com/humancto/forge-lang/pull/75))
- **Semver resolution algorithm** — `forge install` now resolves the latest compatible version from the registry instead of requiring exact version matches. Includes directory traversal protection and helpful error messages listing available versions. ([#76](https://github.com/humancto/forge-lang/pull/76))
- **Transitive dependency resolution** — `forge install` now recursively resolves and installs transitive dependencies from `forge.toml`. Detects circular dependencies and handles diamond dependency patterns. ([#77](https://github.com/humancto/forge-lang/pull/77))
- **GitHub-based remote package registry** — `forge install` now falls back to a remote GitHub-based package index when local registry lookup fails. Supports TOML-based package entries, semver resolution, tarball download/extraction, local caching with TTL, and `GITHUB_TOKEN` authentication. ([#78](https://github.com/humancto/forge-lang/pull/78))
- **`forge search <query>`** — search the remote package registry by name or description. Case-insensitive substring matching with cached index. ([#79](https://github.com/humancto/forge-lang/pull/79))
- **`forge add <pkg>`** — add a dependency to `forge.toml` and install it. Supports `forge add router` (any version) and `forge add router@^1.0` (with constraint). ([#80](https://github.com/humancto/forge-lang/pull/80))
- **`forge update`** — update all dependencies to latest compatible versions by re-resolving from registries. ([#81](https://github.com/humancto/forge-lang/pull/81))
- **Lockfile integrity checking** — `forge install` computes directory-content SHA-256 checksums and stores them in `forge.lock`. On subsequent installs, verifies installed packages haven't been tampered with. Backwards-compatible with existing lockfiles via `checksum_kind` field. ([#82](https://github.com/humancto/forge-lang/pull/82))
- **String interning in GC** — identical strings (≤128 bytes) are deduplicated via hash-consing in the garbage collector. Reduces memory usage for programs with repeated string values. ([#83](https://github.com/humancto/forge-lang/pull/83))
- **Interned string fast equality** — `==` on interned strings short-circuits via GcRef pointer comparison, skipping byte-by-byte comparison. ([#84](https://github.com/humancto/forge-lang/pull/84))
- **Interned field name lookups** — `GetField` opcode avoids cloning field name strings from the constant pool, using `&str` references directly for object map lookups. ([#85](https://github.com/humancto/forge-lang/pull/85))
- **JIT string operations** — JIT compiler now supports string concat, length, and equality via runtime bridge calls. Functions with string-only operations (no float mixing) can be JIT-compiled. ([#86](https://github.com/humancto/forge-lang/pull/86))
- **VM channel builtins** — `channel()`, `send()`, `receive()`, `close()` now work in `--vm` mode. Supports bounded and unbounded channels with cross-spawn communication. ([#87](https://github.com/humancto/forge-lang/pull/87))
- **VM channel extras** — `try_send()`, `try_receive()`, `select()` now work in `--vm` mode. Non-blocking channel operations and multi-channel select with optional timeout. ([#88](https://github.com/humancto/forge-lang/pull/88))
- **VM async coordination** — `await_all()` and `await_timeout()` now work in `--vm` mode. Await multiple task handles or a single handle with a deadline. ([#89](https://github.com/humancto/forge-lang/pull/89))
- **VM `time()` builtin** — `time()` now works in `--vm` mode, returning a datetime object with iso, unix, year, month, day, hour, minute, second, weekday, and timezone fields. Module-as-function via `__call__` dispatch. ([#90](https://github.com/humancto/forge-lang/pull/90))
- **NaN-boxed value encoding** — New `nanbox` module implements 8-byte NaN-boxed value representation (vs current 16-byte enum). Encodes Int/Float/Bool/Null/Obj in a single u64 using IEEE 754 quiet NaN payload bits. 40 unit tests. ([#91](https://github.com/humancto/forge-lang/pull/91))
- **NaN-boxed VM migration** — VM `Value` type migrated from 16-byte enum to 8-byte NaN-boxed newtype, halving memory for all registers, arrays, and objects. Integers >48 bits transparently heap-allocate via `BoxedInt`. Three-tier arithmetic overflow: inline → BoxedInt → float.
- **Verified enum methods via `impl` blocks on algebraic `type` definitions (M9.5)** — `impl MyEnum { fn foo(it, ...) { ... } }` attaches instance methods to ADTs; dispatch walks through the value's `__type__` field into the registered method table. Supports multi-arg methods, returning new ADT instances, chained calls, predicate methods with wildcard patterns, recursive traversal (nested ADTs on the VM), closures capturing `it`, method-to-method dispatch, and collection builtins (`map`, `filter`, `reduce`, `sort`) dispatching via lambda. Feature was latent in both backends and is now locked in by 63 new tests (32 interpreter + 31 VM) plus 10 parity fixtures across interpreter / VM / bytecode round-trip / JIT.
- **First-class `Stream` type (M9.4 iterator protocol)** — `[1,2,3].stream()` produces a lazy, pull-based iterator. Source: arrays, tuples, sets, maps, strings (chars), empty. Combinators: `.filter(fn)`, `.map(fn)`, `.take(n)`, `.skip(n)`, `.chain(other)`, `.zip(other)`, `.enumerate()`. Terminals: `.collect()` / `.to_array()`, `.count()`, `.for_each(fn)`, `.first()`, `.reduce(init, fn)`, `.sum()`, `.find(fn)`, `.any(fn)`, `.all(fn)`. Pipelines are iterative (no recursion depth limit), single-use (drained streams yield empty terminals on re-drain), and poison on closure errors. Short-circuit terminals (`any`, `all`, `find`, `first`, `take`) stop at the first matching element. `sum` promotes to float when any element is a float. Works across interpreter, VM, bytecode round-trip, and JIT. JIT auto-compilation now skips anonymous `<lambda>` functions to avoid cache collisions across distinct lambdas sharing the same name.
- **`mysql.begin` / `mysql.commit` / `mysql.rollback`** — MySQL now has explicit transaction handles: `mysql.begin(conn_id)` returns an opaque transaction id, and `mysql.query` / `mysql.execute` accept that id to run statements on the pinned physical connection. Supersedes the earlier v0.5.0 deferral where pooled one-shot calls made raw transaction control unsafe. ([#140](https://github.com/humancto/forge-lang/pull/140))

### Changed

- **`forge fmt` is a token-based formatter** — it used to only re-indent lines. It now works on the lexer's token stream: one space around binary operators and after commas/colons/keywords, `{ x }` inside non-empty braces, no space inside `( )`/`[ ]`, around `.`/`..` or after unary operators, a lone `}` + `else {`/`catch e {` line joined into `} else {`, one indent level per line of open brackets, blank-line runs collapsed. Comments are preserved (trailing `//` alignment kept), string and number literals are copied verbatim, and ambiguous tokens (`<`/`>`, `?`, `|`) keep their original spacing. Output is idempotent and keeps the same tokens and AST; `cargo test --test fmt_corpus` formats every example and test file and checks identical behaviour on both engines. Source that does not lex is only re-indented.
- **One color policy for all CLI output** — `src/color.rs` decides color per stream (`NO_COLOR` > `FORCE_COLOR`/`CLICOLOR_FORCE` > is-a-terminal). Diagnostics, `forge test`, the REPL (prompt highlighting and results), `forge learn`, `forge watch`/`doc`/`install`/`publish`/`chat`, the server banner, `log.*` prefixes, `term.success/error/table` chrome and the tracing subscriber all follow it, so piped output and `NO_COLOR` get plain text. Values a program builds on purpose are untouched: `term.red("x")` still returns ANSI-colored text.
- **Tree-walking interpreter performance** — no more quadratic loops: `a.push(x)` / `a = push(a, x)` (100k items: >150s → 0.05s), `s = s + t` / `s += t` (200k chars: 1.8s → 0.08s), `a[i]`, `obj.k`, `len(a)`, `s.has(x)`, `m.get(k)` and `a[i] = v` now work on the variable in place instead of deep-copying it. Function values are shared by `Arc` (fib(30): 7.2s → 1.4s), and scopes keep values and mutability in one compact table. Benchmarks: `cargo bench --bench interpreter_hot_paths`, `tools/bench_interp.sh`.
- **Bytecode VM dispatch** — `timeout` deadlines are polled every 1024 instructions (and immediately after a `timeout` scope starts) instead of reading the clock and walking every frame before each instruction; the executing chunk is no longer `Arc`-cloned per instruction and string constants load without allocating. A 20M-iteration `while` loop with the JIT off: 15.9s → 2.1s.
- **VM string building and array push/pop are amortized O(1)** — `s = s + x` / `s += x`, `xs.push(v)` / `push(xs, v)` and `xs.pop()` on a mutable local update the value in place when the local is its only owner, and copy (as before) otherwise, so value semantics are unchanged. 200k-char string build: 11.9s → 0.04s; 100k pushes: 22.2s → 0.02s.
- **JIT loop tier-up** — a function whose loop runs 1000 iterations is JIT-compiled and re-run natively even if it is called only once (verified functions are pure, so restarting the call is unobservable); a 20M-iteration loop in a function called once now takes 0.04s instead of 15.9s. Benchmarks: `tools/bench_vm.sh`.
- **Shell-permission error text** — denied `sh`/`shell`/`run_command`/... now report `permission denied: run (shell execution) — run with --allow-run or grant it in the host policy` (was `Shell execution denied. Use --allow-run ...`).
- **`FORGE_FS_BASE` covers more file access** — `csv.read`/`csv.write`, `toml.read`, `env.load`, SQLite `db.open` files and `http.download` destinations are now confined like `fs.*`.
- **Documentation refreshed for v0.8.x and `llms.txt` added** — README, CLAUDE.md, ROADMAP.md (new Phase 0 hardening section), SECURITY.md (0.8.x support, `--allow-run`, SSRF guard, `FORGE_FS_BASE`) and the book's CLI sections now match current behavior: VM is the default engine, engine flags go before the subcommand, native builds are standalone when `libforge_lang.a` is available, and performance numbers are re-measured. New `llms.txt` is a compact, verified guide to canonical Forge for AI models.
- **Public library surface expanded** — `forge_lang::interpreter`, `forge_lang::lexer`, `forge_lang::parser`, and `forge_lang::runtime` are now `pub` (previously private modules behind the C ABI entry point). Embedders can now drive the language end-to-end from Rust. Required by the new `tests/server_concurrency.rs` integration test; also matches the AOT-binary embedding story.
- **New direct dependency: `parking_lot = "0.12"`** — used by the WS handler for per-connection state (no poisoning, no Send-across-await hazard with the way the lock is held). Already a transitive dep via `tokio-postgres`, now promoted to direct.

### Fixed

- **`break`/`continue` inside an if-expression now leave the loop in the interpreter** — `let q = if c { break } else { 1 }` used to end only the block (yielding `null`) while the loop carried on; it now unwinds to the enclosing loop like the VM, passes through `try`/`safe`/`retry`, and is still a "break outside of loop" error when it would cross a function boundary.
- **Method calls check arity** — `obj.m(args)` reaching a user-defined function (instance methods from `impl`/`give`, static `Type.m()`, functions stored in object fields) skipped the arity check that direct calls get, so missing arguments silently became null. Both engines now apply `semantics::check_call_arity`; instance methods report the explicit arguments only (`method add expects 1 argument, got 0`).
- **Nested assignment in the interpreter** — `g[0][1] = v`, `o.a.b = v`, `rows[i].name = v` failed with "can only assign to variable indices" on the interpreter while the VM supported them. Any chain of fields and indexes on a variable is now updated in place with value semantics (copies made earlier are unaffected), with the usual bounds/key/frozen/immutability errors.
- **A trailing `if`/`when`/`match`/`safe` is the body's value** — `fn sign(x) { if x < 0 { -1 } else { 1 } }` returned null on both engines. A function, lambda or `spawn` body that ends in one of these block statements now returns the value of the branch that ran (null when none ran); side effects and explicit `return`s are unchanged. The rule is shared (`semantics::is_value_tail`). `match` is also usable as a block value on the VM.
- **DAP breakpoints match by file** — `forge dap` never set the program's source file, so a breakpoint on line N of any file stopped every program at line N. Breakpoints are now keyed by canonical path and only stop in the file they were set in.
- **Error snippets name the file** — source snippets for lex, parse and runtime errors (interpreter, VM, `forge watch`, native source runtimes) showed `<source>`; the header now shows the path relative to the current directory (`examples/x.fg:12:5`) and the message no longer repeats it. Snippet columns are counted in characters, so carets line up after non-ASCII text.
- Output from `spawn` tasks, `timeout` blocks, imported modules and `io.print` now goes to the sandbox/debugger capture instead of the process stdout.
- Pathologically nested source (`((((...`, `----x`, deeply nested blocks) is a parse error ("code is nested too deeply") instead of a native stack overflow that aborted the process.
- **Interpreter calls are lexically scoped** — calling a top-level function used to push its scope on top of the caller's, so a callee could read its caller's locals, deeply recursive ADT `match` methods failed with "non-exhaustive match", and every global lookup cost one scope per active call (recursion slowed down linearly with depth: 20× depth-9000 recursion went from ~29s to ~0.3s).
- **Recursion inside HTTP handlers** — handlers ran on tokio blocking threads with 2 MiB stacks, so the stack guard stopped them at ~150 frames. The CLI and standalone binaries now give runtime threads (and interpreter `spawn`/`timeout` threads) a 256 MiB reserved stack registered with the guard.
- **`a.push(f())` no longer loses changes `f` makes to `a`** — the in-place mutating methods now apply to the variable's value after the argument is evaluated.
- **VM was missing stdlib modules and members** — `npc`, `url`, `toml`, `ws`, `io.args_has` / `args_get` / `args_parse` were undefined on the VM and `io.args()` / `io.prompt()` returned null; VM module calls no longer drop non-string arguments (`regex`, `log`, `crypto`, `os`, ...).
- **VM collections have value semantics** — `let w = z; z[0] = 9` no longer changes `w`, functions and methods no longer mutate their caller's arrays/objects, and `a[0] = 1` on an immutable binding is a runtime error, as on the interpreter.
- **`return` inside an if-expression returns from the function on the interpreter** — `let y = if c { return 1 } else { 2 }` used to make `1` the value of the if-expression; it now returns from the enclosing function (as on the VM) and is not caught by `try`.
- **`check x between lo and hi`** — `check x between 1 && 10` parsed `1 && 10` as the lower bound; mixed int/float bounds now work on both engines.
- **VM `for v in channel`** — iterates until the channel is closed (it printed nothing).
- **VM `spawn` can call top-level functions and captured closures** (was "cannot call non-function").
- **JIT runtime bridges no longer swallow errors** — an error raised inside a bridge call is reported as the call's error instead of a null result.
- **Type checker accepts calls that omit default parameters** (no more "expects 2 argument(s), got 1" warning for `g(1)`).
- **VM closures created in loops capture a fresh binding per iteration** — `for i in range(0, 3) { fs = push(fs, fn() { return i }) }` now yields `[0, 1, 2]` on the VM (was `[0, 0, 0]`). The compiler closes captured upvalues when a scope ends (new `CloseUpvalues` opcode, also on `break`/`continue` and catch paths), so a local read after being captured is no longer stale, and closures can capture variables from a grandparent function.
- **VM `continue` inside `for` loops no longer hangs** — it jumped back to the loop test without advancing the index.
- **VM block expressions and implicit returns produce values** — `let x = if c { 1 } else { 2 }`, `when` / `safe` expressions and `fn f() { x * 2 }` evaluated to `null` on the VM. The interpreter no longer evaluates expression statements inside `if` expressions twice.
- **Mutating methods on mutable variables work on the VM** — `a.push(x)`, `a.pop()`, `s.add(x)`, `s.remove(x)` (and `push(a, x)` / `pop(a)`) update the variable as on the interpreter; `pop()` returns the removed item on both engines.
- **Shared operator, indexing and truthiness rules** — a new `semantics` module is used by both engines: string ordering (`"abc" < "abd"`), `[1, 2] + [3]` is a type error (the VM produced a string), negative indexing for reads and writes, `index out of bounds: index 10 on array of length 2`-style messages, missing object keys error, Option truthiness, `reverse("str")`, `i64::MIN / -1` no longer panics.
- **VM destructuring, array spread, `check` and `when` match the interpreter** — `unpack {missing} from obj` binds `null`, `[...xs, 1]` flattens, `check` statements are enforced (they were dropped), `when` arms use the interpreter's comparison rules, `match` on `Ok(v)` / `Err(e)` works.
- **Nothing is silently dropped by the VM compiler** — `yield`/`emit` raises a clear runtime error on both engines (generators are not implemented), immutable reassignment is a catchable runtime error, and constructs the VM cannot run (e.g. `break` outside a loop) are reported as `Unsupported`, making `forge run` fall back to the interpreter.
- **Imports resolve relative to the importing file and only bind requested names** — on both engines; importing a name a module does not define is an error, VM imports no longer leak a module's private functions into the importer, and the interpreter's imported functions can call their module's private helpers. `forge_modules/` resolution is unchanged.
- **Errors raised inside closures called by builtins reach the builtin first** — `yolo(...)` / `assert_throws(...)` inside `try` no longer jump to the outer `catch` on the VM.
- **VM runtime errors show the source snippet** — `forge run` prints the same annotated source excerpt as the interpreter, followed by the VM stack trace.
- **JIT no longer changes program semantics** — auto-JIT (and `--jit`, and `forge build --aot` binaries) cached compiled code by function *name* with no argument guards, so a function called 100+ times could start returning wrong values (`"x" + "y"` → `593`, float arguments → raw bit patterns, same-name nested functions sharing code, wrong-arity calls reading garbage). The JIT is rebuilt as a guarded tier: code is cached per prototype id and argument-type signature, an explicit verifier proves a function compilable (or records a structured reject reason), entry guards check arity and argument kinds, results are re-boxed with their real type, and integer overflow, division by zero, VM stack-depth limits and cancellation deoptimize back to the VM. The current tier compiles Int/Bool functions with self-recursion; everything else runs in the VM. `fib(30)` under the default VM is ~40x faster as a side effect (self-calls no longer go through a runtime bridge).
- **VM GC no longer frees values held by builtins during callbacks** — `map`/`filter`/`reduce`/`sort`/streams and other callback builtins could have their intermediate results collected mid-call (values printed as `<freed>` or objects corrupted by slot reuse). Native builtins now run inside a GC root scope; a `FORGE_GC_STRESS=1` mode collects at every safe point for testing.
- **Large integers no longer panic the VM** — `math.pow(2, 50)`, `math.abs(-140737488355329)`, `[2^47].stream().sum()`, `range()` above 2^47 and other builtins aborted with "integer too large for inline NaN-boxing". Integer results that overflow i64 (`math.pow(2, 63)`, `math.abs(i64::MIN)`, `sum()`) now promote to float in both engines instead of wrapping or panicking.
- **Deep recursion raises a catchable error instead of crashing** — both engines report `maximum recursion depth exceeded` (default limit 10000, configurable with `FORGE_MAX_DEPTH` or `--max-depth`) and guard the native stack; programs run on a large-stack thread so ordinary deep recursion works.
- **Circular imports are detected** — `a.fg` importing `b.fg` importing `a.fg` now fails with `circular import: a.fg -> b.fg -> a.fg` on both engines instead of aborting (interpreter) or printing a huge nested error (VM).
- **VM parity for examples** — `match` on `Ok`/`Err` values, `term.table()`/`term.sparkline()`, parameterized `db.query`/`db.execute`, and `fs.size()` (and other stdlib members missing from the VM module tables) now work on the default VM engine.
- **`forge lsp` no longer deadlocks on the first message** — the server re-locked the non-reentrant stdin mutex while iterating it, so editors never received an `initialize` response. The transport is now built on `lsp-server` + `lsp-types` (rust-analyzer's synchronous LSP stack), which provides correct `Content-Length`/`Content-Type` framing, the initialize/shutdown/exit handshake (exit code 0 only after `shutdown`), clean EOF handling and `InvalidParams`/`MethodNotFound` errors. `textDocument/didClose` now clears diagnostics. An end-to-end stdio test (`tests/lsp_stdio.rs`) spawns the real binary as a regression guard.
- **`forge dap` works with real clients** — breakpoints and stepping now apply to top-level statements (previously only statements inside blocks were stop points, so `stopOnEntry` and top-level breakpoints never fired); the program starts on `configurationDone` so breakpoints set during configuration take effect; `stopped`, `output`, `exited` and `terminated` events are pushed as they happen instead of only when the client sends another request; message framing tolerates extra headers such as `Content-Type`; and a lost-wakeup race that could hang the debuggee after a fast resume is fixed. Covered by `tests/dap_stdio.rs`.
- **Error output respects the terminal** — diagnostics now use ANSI color only when stderr is a terminal and `NO_COLOR` is unset (`FORCE_COLOR` / `CLICOLOR_FORCE` force it on), so piped output and log files get plain text. Source snippets are colored once per span instead of once per character.
- **`forge fmt` preserves `/* block comment */` contents** — the re-indenter no longer strips leading indentation from lines inside multi-line block comments, no longer collapses blank lines inside them, and ignores braces inside block comments when computing indentation. Formatting is covered by an idempotence test.
- **Release assets match the installer again** — the release workflow now publishes `forge-<tag>-<target>.tar.gz` for x86_64/aarch64 Linux and macOS plus `forge-<tag>-x86_64-pc-windows-msvc.zip`, each smoke-tested before upload, with a `SHA256SUMS.txt`. Unix archives bundle `libforge_lang.a` next to `forge`, so `forge build --native` / `--aot` produce standalone binaries for installed users. `install.sh` verifies the archive's SHA-256 (opt out with `FORGE_INSTALL_SKIP_VERIFY=1`), accepts versions with or without the leading `v`, and installs `libforge_lang.a` alongside the binary.
- **`forge search`, `forge add`, `forge install` and `forge update` no longer panic when they reach the remote registry** — the blocking HTTP client was driven from inside the async runtime and aborted with "Cannot drop a runtime in a context where blocking is not allowed". Package-manager commands now run off the runtime.
- **`forge search` falls back to the local registry** — results from `~/.forge/registry` (and `.forge/registry`, `FORGE_REGISTRY_PATH`) are listed alongside remote results with a `SOURCE` column; when the remote index is unreachable (e.g. 404) a clear warning names the URL and local results are still shown.
- **`forge add` only edits `forge.toml` after a successful install** — previously it wrote the dependency (e.g. `router = "*"`) first and left it behind when the install failed. A failed add now leaves `forge.toml` and `forge.lock` untouched, and an unparseable `forge.toml` is never overwritten.
- **Native/AOT build failures now surface the C compiler's diagnostics** — `forge build --native` / `--aot` previously swallowed `cc` output and reported only a generic "compilation failed" string. Failures now include the compiler's exit status plus its stderr (head and tail, bounded at 8 KiB) so root causes like cross-architecture `libforge_lang.a` mismatches or missing libraries are visible directly in the error.
- **WebSocket handlers now observe client disconnect cancellation** — WS connections install a connection-scoped cancellation token, run message handlers on the blocking pool, and keep polling the socket for close/error while handlers run so long-running loops exit at the next interpreter safe point. ([#146](https://github.com/humancto/forge-lang/pull/146))
- **VM error traces now include columns when available** — bytecode chunks carry source columns alongside line tables, old v1.1 bytecode still deserializes with zero columns, and standalone decorator statements now fail VM compilation instead of being silently ignored.
- **OpenTelemetry feature path is now CI-tested and cheaper when inactive** — CI builds `--features otel`, the OTel export path has a smoke test, and request-span traceparent extraction is skipped unless OTel export was activated at runtime.
- **Empty request IDs no longer produce blank span fields** — inbound `X-Request-Id: ` now records as `"unknown"` with a warning, and request-id extraction is covered for empty, non-ASCII, and oversized header values.
- **HTTP server no longer single-threaded** — the server previously wrapped the entire interpreter in `Arc<Mutex<Interpreter>>` and held the lock for the full handler body, so throughput on any non-trivial handler collapsed to ~10 req/sec regardless of CPU count and p99 latency exploded under concurrency. Replaced with a per-request fork model: each request gets a fresh interpreter forked from a read-only `InterpreterTemplate` and runs on `tokio::task::spawn_blocking`. Empirical impact on a 16-core machine, `/cpu` handler ~96ms: throughput at C=16 went from 9.8 → 34.6 req/sec (3.5×); at C=100 from 10.0 to 38.4 req/sec with p99 latency dropping from 18.8s → 2.9s (6.5× p99). Also adds a backpressure semaphore (default 512 in-flight, returns 503 with `Retry-After` when saturated), client-disconnect cancellation via Drop guard, panic capture with no payload leak, graceful shutdown on SIGINT/SIGTERM, and a ratio-based regression test in `tests/server_concurrency.rs`. **Behavior change:** handler mutations to top-level globals no longer persist across requests; handlers no longer see writes from concurrent `schedule`/`watch` blocks. The previous "persistence" was a race condition the global mutex hid. A future `shared { }` block will provide explicit cross-request state.
- **`fork_for_background_runtime` shared scope storage by Arc** — the existing fork primitive used `env.clone()` (shallow over `Vec<Arc<Mutex<HashMap>>>`), so `schedule`/`watch` blocks shared scope locks with the parent interpreter. Latent because background tasks did not run concurrently with the foreground or each other. Switched to `env.deep_clone()`, the same primitive `spawn_task` already uses for squad blocks.
- **OpenTelemetry/OTLP exporter** (behind `otel` Cargo feature, off by default) — when built with `--features otel` and `OTEL_EXPORTER_OTLP_ENDPOINT` is set at runtime, every Forge `tracing` span (HTTP request spans from `TraceLayer`, the `forge.handler` span, user `log.info` events, server lifecycle events) is exported via OTLP/gRPC to an OpenTelemetry collector (Jaeger, Tempo, Honeycomb, Datadog, OTel Collector, etc.). Honors standard OTel env vars: `OTEL_SERVICE_NAME` (default `"forge"`), `OTEL_RESOURCE_ATTRIBUTES`. Inbound W3C `traceparent` headers are extracted and set as the parent context on the request span, so distributed traces connect end-to-end across services. Spans are flushed on graceful shutdown (after `axum::serve` returns) and on CLI script exit, wrapped in `spawn_blocking` so the synchronous `SdkTracerProvider::shutdown()` doesn't pin a tokio worker. **Sampling default is "send everything"** — for high-RPS services, configure your collector to sample, or wait for the `OTEL_TRACES_SAMPLER` follow-up. **Outbound `traceparent` injection in the HTTP client is not yet wired** — outbound requests Forge makes won't have the parent context attached. **Only gRPC is wired** — `OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf` is silently ignored. Adds ~30 transitive crates when enabled (tonic, prost, hyper, h2, ...). Pinned to `opentelemetry 0.31` / `tracing-opentelemetry 0.32`. **Planned:** a CI step running `cargo build --features otel` so future incompatible upgrades fail fast (queued as a follow-up).
- **Per-request `X-Request-Id`** — the HTTP server now uses `tower_http::request_id::SetRequestIdLayer` + `PropagateRequestIdLayer` to assign a UUID v4 to every request that doesn't already carry an `X-Request-Id` header, and to echo the resolved id back in the response. The id is recorded as a span field (`request_id`) on both the outer `tower_http` `request` span (so the per-request `on_response` event carries it: `latency=... status=... request_id="<uuid>"`) and the inner `forge.handler` span (so events emitted from inside the handler body, including user `log.info` via the propagated `Span::current()`, also carry it). Inbound headers are length-capped at 64 chars and warned-about-then-substituted-with-"unknown" if non-ASCII, defending against log amplification and exotic header values. WebSocket per-message events run detached from the trace span and do not carry `request_id` (documented as out of scope; the WS upgrade request itself does carry one). Performance: ~1µs per request from the `getrandom` syscall when no inbound header is present.
- **Structured logging via `tracing`** — the HTTP server and the `log` Forge stdlib module now emit structured events through the `tracing` crate. `tower_http::trace::TraceLayer` wraps every request in a span carrying `method`, `uri`, `version`, `status`, and `latency`. The Forge `log` stdlib module emits events with the stable `target = "forge.user"` so users can filter their own log volume independently from runtime noise (`FORGE_LOG=forge_lang=warn,forge.user=info`). Span context is propagated across the `spawn_blocking` boundary via `Span::current()` so user `log.info` events from inside a handler inherit the HTTP request fields. Configurable via two env vars: `FORGE_LOG_FORMAT=json|pretty|compact` (default: pretty for TTY, compact when piped) and `FORGE_LOG=<env-filter>` with **precedence `FORGE_LOG` > `RUST_LOG` > default** (`forge_lang=info,tower_http=info,axum=warn,forge.user=info`). ANSI escape codes are emitted only when stderr is a TTY — never when piped or redirected to a file. The `forge run app.fg` colorful banner is preserved on TTY but a structured `tracing::info!("Forge server listening")` event always fires. Forge's `log` stdlib keeps its colored TTY output for interactive scripting too. Replaces direct `eprintln!` calls in the server (panic capture, shutdown signal) and the `log` module. Performance: adds ~1 INFO-level tracing event per HTTP request (`tower_http::trace::on_response`) plus user `log.*` events; filtered-out levels cost one atomic load per call site. Precondition for Prometheus metrics, OTel spans, and distributed tracing.
- **Per-request HTTP handlers no longer share captured closures** — PR #108 made `fork_for_serving` deep-clone the env's scope storage, but `Value::Function::closure` and `Value::Lambda::closure` were still shallow-cloned, so concurrent requests calling a captured-closure helper raced on the shared closure scope `Arc<Mutex>`. Top-level `fn` handlers were already isolated (the `is_global_fn` fast path skips the closure), but captured-closure handlers and Lambdas had a silent throughput cap and a lost-update race in the Lambda writeback path. New `Environment::deep_clone_isolated` walks `Value`s during the fork and gives every closure its own scope graph, with cycle handling for the recursive-function pattern (`fn f() { f() }` whose closure captures the env that holds it). `spawn_task` (squad blocks) and `fork_for_background_runtime` (schedule/watch) keep their existing shallow-on-closures behavior on purpose — those callers want closure-state continuity across spawns and iterations. The new ratio test in `tests/server_concurrency.rs` measures closure-handler scaling at 1.68× wall-time C=8 vs C=1 (vs ~6-8× under the pre-fix model). Debug builds also panic at fork time if a `Value::Stream` is found in the template env, since streams are single-use and silently break under cross-fork sharing.

## [0.8.0] - 2026-04-12

### Added

- **`os` stdlib module** — `hostname()`, `platform()`, `arch()`, `pid()`, `cpus()`, `homedir()` for runtime OS introspection. ([#59](https://github.com/humancto/forge-lang/pull/59))
- **`path` stdlib module** — `join()`, `resolve()`, `relative()`, `is_absolute()`, `dirname()`, `basename()`, `extname()`, `separator` for cross-platform path manipulation. ([#59](https://github.com/humancto/forge-lang/pull/59))
- **`--allow-run` permission flag** — shell execution (`sh`, `shell`, `run_command`, `pipe_to`) now requires explicit opt-in via `--allow-run`. REPL and `-e` mode auto-enable for convenience. ([#57](https://github.com/humancto/forge-lang/pull/57))
- **VS Code extension enhanced** — full TextMate grammar covering all 20 modules and 80+ builtins, 24 code snippets, extension README. ([#60](https://github.com/humancto/forge-lang/pull/60))

### Changed

- **Cranelift JIT is now an optional cargo feature** — enabled by default. Build without it via `cargo install forge-lang --no-default-features` for faster compile times and broader platform support. ([#41](https://github.com/humancto/forge-lang/pull/41))
- **PostgreSQL is now an optional cargo feature** — enabled by default. ([#42](https://github.com/humancto/forge-lang/pull/42))
- **MySQL is now an optional cargo feature** — enabled by default. ([#43](https://github.com/humancto/forge-lang/pull/43))
- **Trimmed tokio features** from `"full"` to 7 specific features actually used. ([#44](https://github.com/humancto/forge-lang/pull/44))
- **VM `Value` implements `Copy`** — eliminates 51 unnecessary clone calls in the dispatch hot path. ([#47](https://github.com/humancto/forge-lang/pull/47))
- **Removed dead `NativeFn.func` field** — unused function pointer placeholder cleaned up. ([#48](https://github.com/humancto/forge-lang/pull/48))
- **Variable-width VM frames** — frames now use `max_registers` instead of fixed 256 slots, reducing stack memory usage for simple functions. ([#55](https://github.com/humancto/forge-lang/pull/55))
- **Unified async runtime** — HTTP stdlib reuses existing Tokio handle via `Handle::try_current()` instead of creating a new runtime per call. ([#53](https://github.com/humancto/forge-lang/pull/53))

### Refactored

- **Extracted interpreter tests** to `interpreter/tests.rs` — mod.rs reduced from 7,907 to 3,239 lines. ([#45](https://github.com/humancto/forge-lang/pull/45))
- **Extracted VM tests** to 5 dedicated files — mod.rs reduced from 2,058 to 50 lines. ([#46](https://github.com/humancto/forge-lang/pull/46))

### Fixed

- **`len()` and `count("")` use char count across all backends** — interpreter and VM now return Unicode character count consistently. ([#38](https://github.com/humancto/forge-lang/pull/38))
- **JIT memory leak fixed** — replaced `mem::forget(jit)` with owned `Vec<JitCompiler>` to keep code pages alive without leaking. ([#39](https://github.com/humancto/forge-lang/pull/39))
- **Short-circuit `&&`/`||` in VM** — logical operators now skip right-hand evaluation when unnecessary, matching interpreter behavior. ([#40](https://github.com/humancto/forge-lang/pull/40))
- **Eliminated 16 compiler warnings** — dead code annotations, unused imports, and redundant patterns cleaned up. ([#51](https://github.com/humancto/forge-lang/pull/51))
- **Converted 3 user-reachable panics to error returns** — `alloc_reg`, `add_local` overflow, and JIT dispatch now return proper errors instead of crashing. ([#52](https://github.com/humancto/forge-lang/pull/52))
- **String `.len` returns char count, not byte count** — consistent across interpreter, VM, and JIT. ([#54](https://github.com/humancto/forge-lang/pull/54))
- **Proper JSON string escaping** — `json.stringify` and `json.pretty` now escape control characters, newlines, tabs, and backslashes correctly. ([#56](https://github.com/humancto/forge-lang/pull/56))

### Security

- **SSRF protection on by default** — HTTP client denies requests to private/loopback IPs unless `FORGE_HTTP_ALLOW_PRIVATE=1` is set. ([#58](https://github.com/humancto/forge-lang/pull/58))

## [0.7.1] - 2026-04-12

### Fixed

- **Eliminated undefined behavior in VM dispatch** — replaced 3 `unsafe { transmute(op) }` sites with safe `TryFrom<u8>` conversion. Invalid opcodes now produce clean errors instead of UB. Compile-time assertion guards against enum drift. ([#22](https://github.com/humancto/forge-lang/pull/22))
- **Fixed GC use-after-free risk** — added `method_tables`, `static_methods`, `struct_defaults`, and `open_upvalues` to GC root scanning. These structures hold live GcRefs that were previously invisible to the collector. ([#23](https://github.com/humancto/forge-lang/pull/23))
- **Fixed DAP message corruption** — replaced mixed `stdin.lock().lines()` + separate `io::stdin()` reads with a single `BufReader<Stdin>`, preventing buffer desync under pipelined messages. ([#24](https://github.com/humancto/forge-lang/pull/24))
- **Fixed deflated coverage numbers** — added coverage line tracking to the interpreter `run()` method. Top-level statements were previously invisible to `forge test --coverage`. ([#25](https://github.com/humancto/forge-lang/pull/25))
- **VM `len()` returns char count** — `len("emoji")` now returns Unicode character count instead of byte count, matching interpreter behavior. ([#31](https://github.com/humancto/forge-lang/pull/31))
- **VM object equality** — `==` on objects now compares by key-value equality instead of always returning false. ([#32](https://github.com/humancto/forge-lang/pull/32))
- **AOT/native uses TMPDIR** — generated C launchers now respect the `TMPDIR` environment variable instead of hardcoding `/tmp`. ([#33](https://github.com/humancto/forge-lang/pull/33))
- **Improved coverage heuristic** — excludes `} else {`, lone `{`, decorator lines, and `otherwise` from executable line count for more accurate percentages. ([#35](https://github.com/humancto/forge-lang/pull/35))
- **DAP breakpoints keyed by file** — breakpoints are now stored per source file, preventing cross-file false triggers during multi-file debugging. ([#36](https://github.com/humancto/forge-lang/pull/36))
- **Compiler register overflow check** — `alloc_reg()` now panics with a clear message at 255 registers instead of silently wrapping to 0. ([#37](https://github.com/humancto/forge-lang/pull/37))

### Changed

- **Lazy register allocation** — VM starts with 256 registers (~6KB) instead of 65,536 (~1.5MB), growing on demand at call sites. ([#28](https://github.com/humancto/forge-lang/pull/28))
- **Cached chunk lookup in dispatch loop** — the `Arc<Chunk>` is now cached across dispatch iterations, avoiding redundant GC lookups when the closure hasn't changed. ([#27](https://github.com/humancto/forge-lang/pull/27))
- **`debug_assert!` on SendableVM** — forked VMs now assert that `jit_cache` is empty in debug builds, guarding the `unsafe impl Send` invariant. ([#26](https://github.com/humancto/forge-lang/pull/26))
- **Deduplicated native.rs** — extracted shared `compile_launcher()` and `launcher_c_template()`, reducing the file by ~120 lines. ([#34](https://github.com/humancto/forge-lang/pull/34))
- **Fair benchmarks with internal timing** — all benchmarks now use self-reported timing, eliminating 30-80ms process spawn noise. Array benchmark Python uses `append` loop instead of `list(range())`. Runner tests both VM and interpreter modes. ([#29](https://github.com/humancto/forge-lang/pull/29))
- **Cross-language benchmarks** — added Rust, Go, and Node.js fib(30) benchmark files for landing page verification. ([#30](https://github.com/humancto/forge-lang/pull/30))

## [0.7.0] - 2026-04-12

### Added

- **`forge test --coverage`** — line coverage reporting for Forge test files. Tracks executed lines during test runs and displays per-file and overall coverage percentages with color-coded output (green ≥80%, yellow ≥50%, red <50%).
- **`forge publish`** — package and publish Forge projects to the local filesystem registry (`~/.forge/registry/<name>/<version>/`). Supports `--dry-run` to preview without publishing and `--registry` to specify a custom registry path. Validates manifest fields, computes SHA-256 checksums, and excludes non-source files (forge_modules, .git, tests, etc.).
- **VM as default engine** — the bytecode VM is now the default execution engine for `forge run`. The interpreter is available via `--interp` flag. Programs using decorator-driven HTTP servers (`@server`, `@get`, etc.) automatically fall back to the interpreter.
- **VM `must` expression** — `must Ok(42)` unwraps to `42`, `must Err("x")` crashes with clear error, `must null` crashes. Full parity with interpreter semantics.
- **VM `ask` expression** — `ask "prompt"` calls the LLM API (OpenAI-compatible) in VM mode. Requires `FORGE_AI_KEY` or `OPENAI_API_KEY` environment variable.
- **VM `freeze` expression** — `freeze expr` wraps values as immutable in VM mode. `SetField` on frozen values returns a runtime error.
- **Cross-file LSP** — go-to-definition and find-references now work across files. Imported symbols resolve to their source file via `import` statement following. Find-references searches imported files and sibling `.fg` files in the same directory. Import statements now appear in document symbols.
- **`forge build --aot`** — compiles Forge source to bytecode and embeds it in a native binary. Unlike `--native` (which embeds raw source), `--aot` embeds serialized bytecode for faster startup and no source exposure. The binary still requires the Forge VM runtime at execution time.
- **`forge dap` — Debug Adapter Protocol server** for VS Code step-through debugging. Supports breakpoints, step over/in/out, continue, pause, variable inspection, and call stack traces. The interpreter pauses at breakpoints via shared debug state with timeout-based cooperative waiting. Output from `print`/`say`/`yell`/`whisper` is captured and sent as DAP output events to prevent stdout corruption.

## [0.6.0] - 2026-04-11

### Added

- **LSP `textDocument/references`** — find all references to any identifier in the current document with word-boundary matching
- **LSP hover for user-defined symbols** — functions show full signature, variables show mutability/type, structs show fields, types show variants, interfaces show methods
- **LSP deep go-to-definition** — finds function parameters, local variables, for-loop vars, catch vars, and impl block methods — not just top-level symbols
- **LSP context-aware module completions** — typing `math.` now only shows `math` module members instead of all 200+ members from every module
- **LSP type-check diagnostics** — the gradual type checker now runs on every edit, surfacing type mismatches, arity errors, and return type mismatches as editor warnings with line numbers
- **REPL syntax highlighting** — keywords (magenta), builtins (blue), modules (green), strings (yellow), numbers (cyan), comments (dim)
- **REPL live tab completion** — user-defined variables and functions now appear in tab completion alongside builtins
- **REPL `env` command** — now shows all defined variables and their values instead of just `_last`
- **`forge doc` variable extraction** — `let`/`let mut` declarations now appear in doc output (previously silently skipped)
- **`forge doc` comment extraction** — `//` comments preceding functions, structs, and variables are now captured and displayed
- **`forge fmt` paren continuation** — multi-line function calls with open parens now auto-indent correctly (previously only braces and brackets were tracked)
- **`forge run` with manifest entry** — `forge run` without a file argument now reads the `entry` field from `forge.toml`, enabling project-level `forge run` workflows
- **Relative import resolution** — `import "helper"` now resolves relative to the importing file's directory first, then falls back to CWD and `forge_modules/`. Enables packages with internal imports.
- **Import struct/type/impl definitions** — wildcard imports (`import "lib"`) now copy struct definitions, type definitions, and impl block methods in addition to functions and variables
- **Source spans in AST** — all inner statement bodies (`if`, `for`, `while`, `fn`, `match`, `try/catch`, etc.) now carry per-statement line and column info via `SpannedStmt`. Runtime errors report the exact source line, even inside deeply nested blocks.
- **VM stdlib parity: 47 new builtins** — added 4 missing module namespaces (`npc`, `url`, `toml`, `ws`) and 43 standalone builtins to the VM: collections (`first`, `last`, `zip`, `flatten`, `chunk`, `slice`, `compact`, `partition`, `group_by`, `sort_by`, `for_each`, `take_n`, `skip`, `frequencies`, `sample`, `shuffle`), strings (`typeof`, `substring`, `index_of`, `last_index_of`, `capitalize`, `title`, `upper`, `lower`, `trim`, `pad_start`, `pad_end`, `repeat_str`, `count`, `slugify`, `snake_case`, `camel_case`), GenZ debug kit, and execution helpers
- **Line-accurate runtime errors** — errors inside nested blocks now show the correct inner line with source snippets via ariadne, instead of pointing at the top-level statement
- **JIT: logical And/Or** — `&&`/`||` in JIT-compiled functions now use logical semantics (result is 0 or 1) instead of bitwise AND/OR which produced wrong results for non-boolean integers (e.g. `2 && 3` was `2`, now correctly `1`)
- **JIT: support up to 8 function arguments** — JIT dispatch previously silently dropped arguments beyond 3; now supports 0–8 arguments for both integer and float functions
- **VM async: spawn/await** — `spawn { }` now runs on a real OS thread in `--vm` mode (previously ran synchronously inline). `await` blocks on the spawned task's result via `Condvar`. Cross-thread value transfer uses `SharedValue` enum to avoid GC reference leaks. Supports nested spawn, variable capture via upvalues, string/object/array return values, and error isolation. 12 new tests.
- **JIT: 24 new tests** — comprehensive coverage for logical operators, multi-argument functions, float arithmetic, recursive algorithms, and comparison operators
- **VM: schedule/watch blocks** — `schedule every N seconds/minutes/hours { }` and `watch "path" { }` now work in `--vm` mode. Both compile to dedicated opcodes and spawn background threads using the same `fork_for_spawn` + `SendableVM` infrastructure from spawn/await. Includes interval validation and upvalue capture. 9 new tests.

---

## [0.5.0] - 2026-04-10

### Added

- **`db.begin` / `db.commit` / `db.rollback`** — explicit transaction control for the SQLite module, sharing the existing thread-local connection.
- **`pg.begin` / `pg.commit` / `pg.rollback`** — same trio for the PostgreSQL module, backed by `client.batch_execute`.
- **Opt-in filesystem confinement** — setting `FORGE_FS_BASE=/path` confines every `fs.*` operation that touches a path to that subtree (with symlink resolution). Pure path manipulation helpers (`dirname`, `basename`, `ext`, `join_path`, `temp_dir`) are exempt; `exists`/`is_dir`/`is_file` return `false` instead of erroring on confinement failure so script branches still work.
- **VM source-line stack traces** — `VMError` now carries real `(function, line)` frames populated from the bytecode line table, and the CLI prints them via the `Display` impl rather than dropping them on the floor. Makes `--vm` errors actionable.
- **69 new unit tests** for `crypto`, `regex`, `json`, and `time` stdlib modules — these had **zero** prior coverage despite living on the security-critical / format-correctness paths. Includes RFC 4231 HMAC-SHA256 vector, century-rule leap year cases, and JSON deep-merge round trip.
- **`PRODUCTION_READINESS.md`** — internal punch list tracking all v0.4.3+ hardening work (through v0.7.1).

### Fixed

- **`http.get/post/...` had no redirect limit** — could be steered around localhost guards with a 302 chain. Now capped at **5 redirects** (down from reqwest's default of 10) via a custom `redirect::Policy` that re-validates every hop's URL through the same scheme + private-address checks the initial URL went through. Open-redirect → `file://`, `ftp://`, or an internal host gets rejected at the policy callback.
- **`http.download` / `http.crawl` had no body-size cap** — single response could OOM the host. Added a streaming size cap that fast-fails on advertised `Content-Length` _and_ enforces during read.
- **HTTP SSRF / scheme bypasses** — every HTTP entrypoint now rejects non-`http(s)` schemes and (when `FORGE_HTTP_DENY_PRIVATE=1` is set) refuses RFC1918 / loopback / link-local / ULA / multicast destinations. The guard is **opt-in** via env var because allowing localhost is the right default for dev tooling; production deployments should set `FORGE_HTTP_DENY_PRIVATE=1`. `http.download` and `http.crawl` go through the same validator, not just `http.get`/`post`.
- **HTTP DNS-rebinding window on the initial connection** — Forge resolves the host itself, validates the address, then pins it into reqwest via `Client::builder().resolve(host, addr)` so the TCP connect uses the exact address that passed the check. Closes the TOCTOU window between Forge's DNS check and reqwest's own connect-time lookup. Note: this protection is **only for the initial URL** — redirected hops are re-validated via DNS (closing the open-redirect class) but not pinned, so a microsecond-scale rebind window remains on redirect targets. Treat untrusted redirect chains as untrusted.
- **HTTP IPv4-mapped IPv6 bypass** — `ip_is_private` previously matched only on the IPv6 segment pattern, so `http://[::ffff:127.0.0.1]/` slipped past the loopback guard. Now mapped addresses are unwrapped and classified against the inner IPv4. Test fixtures cover `::ffff:{127.0.0.1, 10.0.0.1, 169.254.169.254}`.
- **`jwt.verify` accepted `alg: none` tokens** — header parser now rejects `none` (and case variants) before any signature verification path runs.
- **`jwt.verify` key-confusion vulnerability** — an attacker could sign a token with HS256 using an RSA public key as the HMAC secret, and `jwt.verify` would accept it because it trusted whatever algorithm the token header claimed. `jwt.verify` now accepts an optional third argument `{ algorithm: "RS256" }` that pins the expected algorithm; if the token header claims a different algorithm, verification fails with a clear mismatch error.
- **`pg.connect` defaulted to plaintext** — now defaults to TLS with full server certificate verification using webpki roots. Plaintext requires an explicit `"disable"` (or `"none"`/`"no-tls"`/`"plain"`) mode argument. `"tls-no-verify"` opts out of cert verification for dev.
- **`pg.query` / `pg.execute` raw-pointer client extraction** — replaced with a clean `Arc::clone` checkout from the thread-local `RefCell`, eliminating the `unsafe` block and its lifetime hazards. Functionally equivalent under load tests.
- **VM silently dropped `must` / `ask` / `await` / `freeze` / `spawn` expressions** — the compiler stripped them and ran the inner expression with no error. Now `--vm` rejects programs containing these constructs up front with a specific message naming the unsupported feature.
- **LSP returned malformed responses for unknown methods** — now responds with proper `MethodNotFound` error per LSP spec.
- **Two production-path `unwrap()` calls** — `jwt.sign` re-fetched a matched argument via `args.first().unwrap()` (replaced with `Some(v @ Value::Object(_))` binding); `crypto::rand_byte` could panic on a pre-1970 system clock (replaced with `unwrap_or(0)`). Every other `unwrap()` in the tree (309 total) is now confirmed to live in `#[cfg(test)]` modules.

### Security

- HTTP SSRF/scheme/redirect/size hardening (see Fixed).
- JWT `alg=none` rejection (see Fixed).
- JWT key-confusion defence via algorithm pinning (see Fixed).
- PostgreSQL TLS-by-default (see Fixed).
- Filesystem `FORGE_FS_BASE` confinement (see Added).

### Changed

- **`http.download` / `http.crawl` now accept an options object** — `timeout`, `max_redirects`, `max_bytes` can be passed via `http.download(url, dest, { timeout: 60, max_bytes: 10000000 })` and `http.crawl(url, { timeout: 10 })`. Previously these functions used hardcoded defaults and ignored user options.
- `--vm` and `--jit` CLI help text rewritten to spell out exact limitations: VM rejects `ask`/`await`/`must`/`freeze`/`spawn` and decorator-driven runtime features; JIT supports only the integer-loop subset and falls back to the bytecode VM for everything else.
- `mysql.begin`/`commit`/`rollback` are intentionally **not** added — `mysql_async`'s pool returns a fresh physical connection on every `get_conn()`, so transaction control across separate calls would silently target different connections. A note in `mysql::create_module` documents the limitation.

---

## [0.4.3] - 2026-03-06

### Fixed

- **VM `is_some()` / `is_none()` were stubs that always returned `false`** — restored real ADT-aware logic for `Option<T>` values in `--vm` mode
- **VM `keys({})` returned an error on empty objects** — now correctly returns `[]` matching interpreter behaviour
- **VM `split(str, "")` did not split into characters** — empty delimiter now produces a char array (parity with interpreter)
- **VM `int(bool)` raised an error** — `true` → `1`, `false` → `0` now works in `--vm` mode
- **VM `sort()` only handled Int/Float** — String comparison and custom comparator function now supported
- **VM `ok()`/`err()` lowercase aliases silently fell through** — `"Ok" | "Some"` match arm appeared before `"ok"` alias, making lowercase calls return `unknown builtin`; arm order corrected
- **VM `float()` did not accept strings** — `float("3.14")` now parses correctly (parity with interpreter)
- **VM `entries({})` returned `Null` for empty object** — now returns `[]` (parity fix)
- **VM `find` / `flat_map` spawned a full Interpreter instance per call** — replaced with native VM loop implementations; no more per-call interpreter startup cost
- **VM missing builtins: `any`, `all`, `unique`, `sum`, `min_of`, `max_of`, `assert_ne`** — implemented natively in `vm/builtins.rs` AND registered in `vm/machine.rs` builtin registry (registration was the critical missing step — without it names resolved as `undefined variable`)
- **`pg.query` / `pg.execute` nested `block_on` deadlock** — the previous pattern `block_in_place(|| handle.block_on(async { rt.block_on(client.query) }))` is undefined/deadlock in Tokio; fixed by extracting a raw pointer to the client before `block_in_place`, then awaiting the query directly in the outer async block
- **`sus()` panic on no arguments** — `args.into_iter().next().unwrap()` → `unwrap_or(Value::Null)`
- **Parser `decorators.pop().unwrap()`** — replaced with `ok_or_else(ParseError)` to avoid panic on unexpected empty decorator list
- **8× `Mutex::lock().unwrap()` in interpreter `Environment`** — replaced with poison-recovery `lock().unwrap_or_else(|p| p.into_inner())` to prevent panic propagation if a spawned thread panics while holding the lock
- **Bare `unwrap()` in interpreter method dispatch path** (`mod.rs:1797`) — replaced with `unwrap_or(Value::Null)` to prevent panic on edge-case object mutation
- **Unsafe `unwrap()` in VM GetField handler** (`machine.rs:852`) — replaced with `expect("BUG: ...")` for better crash diagnostics
- **Compiler `loops.pop().unwrap()`** in While/Loop/For compile paths — replaced with `ok_or_else(CompileError)` to avoid panic on malformed AST

### Changed

- JIT `runtime.rs`: added `#![allow(dead_code)]` with explanatory comment — all unused functions are M2 NaN-boxing bridge infrastructure, intentionally kept ready

---

## [0.4.2] - 2026-01-15

### Fixed

- **Closure mutable capture (BUG-005)** — mutable variables captured in closures now persist mutations across invocations instead of resetting to the initial value
- **Unwrap safety sweep** — removed all bare `unwrap()` calls from production execution paths in `interpreter/builtins.rs` and `interpreter/call_builtin.rs`
- **LSP incremental sync** — fixed `textDocument/didChange` handler dropping partial edits in large files
- **REPL multi-line paste** — pasted blocks with embedded newlines no longer trigger premature evaluation

### Changed

- Extracted `call_builtin` and `call_native` into separate files (`interpreter/call_builtin.rs`, `vm/builtins.rs`) for readability — zero behaviour change
- Version bump: `0.4.1` → `0.4.2`

---

## [0.4.1] - 2026-01-08

### Added

- **`mysql` module** — `mysql.connect`, `mysql.query`, `mysql.execute`, `mysql.close` with parameterised queries and connection pooling (mirrors `pg` API)
- **`jwt` module** — `jwt.sign`, `jwt.verify`, `jwt.decode`, `jwt.valid` supporting HS256/384/512, RS256, ES256
- **`time` module** — `time.now`, `time.unix`, `time.format`, `time.parse`, `time.diff`, `time.sleep`
- **`csv` improvements** — `csv.read` / `csv.write` now handle quoted fields with embedded commas and newlines

### Fixed

- `http.post` with JSON body set incorrect `Content-Type` (was `text/plain`, now `application/json`)
- `fs.read_json` panicked on malformed JSON instead of returning `Err`
- `pg.connect` TLS mode `"tls-no-verify"` was not recognised (case sensitivity)

### Changed

- Version bump: `0.4.0` → `0.4.1`

---

## [0.4.0] - 2026-01-01

### Added

- **Bytecode VM** (`--vm` flag) — register-based virtual machine with own compiler, GC, and JIT integration
- **JIT compilation** (`--jit` flag) — Cranelift-backed JIT for numeric hot loops; auto-promotes functions after 100 calls
- **VM serialisation** — compiled bytecode can be serialised to `.fgc` files and loaded without re-parsing
- **`pg` module (PostgreSQL)** — `pg.connect`, `pg.query`, `pg.execute`, `pg.close` with TLS support (`no-tls`, `tls`, `tls-no-verify`)
- **`forge build`** command — produces serialised `.fgc` bytecode artefact
- **`forge lsp`** command — Language Server Protocol skeleton (hover, diagnostics, completion stubs)
- **Gradual type checker** — `--strict` emits type warnings without failing; type annotations in function signatures
- **ADT / enum types** — `type Shape = Circle(f) | Rect(f, f)` with exhaustive `match`
- **`struct` + `give` blocks** — struct definitions with default fields and impl-style method blocks
- **`safe { }` block** — null-safe execution scope; errors inside produce `null` instead of crashing
- **`timeout N seconds { }` block** — time-limited execution (interpreter mode)
- **`retry N times { }` block** — automatic retry up to N attempts on error
- **`spawn { }` + channels** — cooperative concurrency with Tokio; `channel()`, `send()`, `receive()`
- **30 interactive tutorials** (`forge learn`)

### Changed

- Interpreter is now the _default_ engine; VM/JIT are opt-in
- `println` aliased to `say` (both work)
- Version bump: `0.3.0` → `0.4.0`

---

## [0.3.0] - 2026-03-01

### Added

#### Language Features

- **Native Option<T> values** — `Some(x)` and `None` are first-class `Value::Some`/`Value::None` variants. Pattern matching, `unwrap()`, `unwrap_or()`, `is_some()`, `is_none()` all work natively.
- **Task handles from spawn** — `let h = spawn { return 42 }` returns a handle; `await h` gets the value.
- **Interface satisfaction checking** — Go-style structural typing with `satisfies` keyword.
- **Tokio-powered concurrency** — `spawn`, `channel()`, `send()`, `receive()` with real async runtime.
- **Gradual type inference** — `--strict` mode for type validation with warnings.

#### GenZ Debug Kit (5 builtins)

- `sus(val)` — Inspect with attitude, returns value (like Rust's `dbg!` but cooler)
- `bruh(msg)` — Panic with GenZ energy
- `bet(condition, msg?)` — Assert with swagger ("LOST THE BET" on failure)
- `no_cap(a, b)` — Assert equal ("CAP DETECTED" on mismatch)
- `ick(condition, msg?)` — Assert false ("ICK" when unexpectedly true)

#### Execution Helpers (4 builtins)

- `cook(fn)` — Time execution with personality ("speed demon fr" / "bruh that took a minute")
- `yolo(fn)` — Fire-and-forget, swallows ALL errors, returns None on failure
- `ghost(fn)` — Execute silently, capture result
- `slay(fn, n?)` — Benchmark N times, returns `{avg_ms, min_ms, max_ms, p99_ms, runs, result}`

#### NPC Module — Fake Data Generation (16 functions)

- `npc.name()`, `npc.first_name()`, `npc.last_name()`, `npc.email()`, `npc.username()`, `npc.phone()`
- `npc.number(min, max)`, `npc.pick(arr)`, `npc.bool()`, `npc.sentence(n?)`, `npc.word()`
- `npc.id()`, `npc.color()`, `npc.ip()`, `npc.url()`, `npc.company()`

#### String Operations (12 builtins)

- `substring(s, start, end?)`, `index_of(s, substr)`, `last_index_of(s, substr)`
- `pad_start(s, len, char?)`, `pad_end(s, len, char?)`, `capitalize(s)`, `title(s)`
- `repeat_str(s, n)`, `count(s, substr)`
- `slugify(s)` — URL-friendly strings
- `snake_case(s)` — Handles camelCase, PascalCase, consecutive caps (myAPIKey → my_api_key)
- `camel_case(s)` — From snake_case, kebab-case, or spaces

#### Collection Operations (16 builtins)

- `sum(arr)`, `min_of(arr)`, `max_of(arr)` — Numeric aggregates
- `any(arr, fn)`, `all(arr, fn)` — Predicate checks
- `unique(arr)`, `zip(arr1, arr2)`, `flatten(arr)`
- `group_by(arr, fn)`, `chunk(arr, size)`, `slice(arr, start, end?)`
- `partition(arr, fn)` — Split into `[matches, rest]`
- `sort(arr, fn?)` — Now supports custom comparators returning -1/0/1
- `sample(arr, n?)` — Random items from array
- `shuffle(arr)` — Fisher-Yates shuffle
- `diff(a, b)` — Deep object comparison with added/removed/changed tracking

#### Testing Framework Improvements

- `assert_ne(a, b)` — Assert not equal
- `assert_throws(fn)` — Assert function throws error
- `@skip` decorator — Skip tests (shown as SKIP in output)
- `@before` / `@after` hooks — Setup/teardown per test
- `--filter pattern` — Run only matching tests
- **Structured error objects** — `catch err` now binds `{message, type}` instead of plain string
  - Error types: ArithmeticError, TypeError, ReferenceError, IndexError, AssertionError, RuntimeError

#### Stdlib Additions

- `math.random_int(min, max)`, `math.clamp(val, min, max)`
- `fs.lines(path)`, `fs.dirname(path)`, `fs.basename(path)`, `fs.join_path(a, b)`
- `fs.is_dir(path)`, `fs.is_file(path)`, `fs.temp_dir()`
- `io.args_parse()`, `io.args_get(flag)`, `io.args_has(flag)`
- `try_send(ch, val)` — Non-blocking channel send (returns Bool)
- `try_receive(ch)` — Non-blocking channel receive (returns Option)

#### Developer Experience

- `forge doc` — Auto-generate documentation from source
- `forge watch` — File watcher for auto-reload
- Package management with `forge.toml` dependency resolution
- Bytecode serialization (`.fgc` binary format) with `forge build`
- Function profiler with `--profile` flag
- **30 interactive tutorials** (was 14)
- **7 new language spec chapters** in the book

#### Infrastructure

- VM closure upvalue capture
- VM dispatch for csv, time, pg modules
- Auto-JIT compilation for hot integer functions
- 17 JIT parity tests, 33 VM parity tests
- Production gap fixes: is_truthy consistency, result-type propagation, catch-block isolation

### Changed

- `Some()` builtin returns `Value::Some(Box<Value>)` instead of ADT object wrappers
- `None` in prelude is `Value::None` instead of ADT object
- `Expr::Spawn` added to AST — spawn usable as expression
- `catch err` binds structured error object with `.message` and `.type` (breaking change from plain string)
- `Token::Any` now works as identifier in expression context (fixes `any()` builtin keyword conflict)
- Standard library expanded from 15 to 16 modules (added `npc`)
- Total functions: 160+ → 230+
- Total tests: 287 → **822** (488 Rust + 334 Forge)

---

## [0.2.0] - 2026-02-28

### Added

- **JIT compiler** via Cranelift — `--jit` flag compiles hot functions to native code (fib(30) in 10ms, alongside Node.js/V8)
- **Bytecode VM** with register-based architecture, mark-sweep GC, and green thread scheduler (`--vm` flag)
- **Natural language syntax**: `set`/`to`, `say`/`yell`/`whisper`, `define`, `repeat`, `otherwise`/`nah`, `grab`/`toss`, `for each`
- **15 standard library modules**: math, fs, io, crypto, db (SQLite), pg (PostgreSQL), env, json, regex, log, exec, term, http, csv
- **Terminal UI toolkit**: colors, tables, sparklines, bars, banners, progress, gradients, boxes, typewriter effects
- **HTTP server** with `@server`, `@get`, `@post`, `@put`, `@delete`, `@ws` decorators (powered by axum)
- **HTTP client** with `fetch()`, `http.get/post/put/delete/patch/head`, `download`, `crawl`
- **Shell integration**: `shell()` for full pipe chain support, `sh()` shorthand
- **Innovation features**: `when` guards, `must` keyword, `safe` blocks (usable as expressions), `check` validation, `retry`/`timeout`/`schedule`/`watch` blocks
- **AI integration**: `ask()` for LLM calls, `prompt` templates, `agent` blocks
- **Developer tools**: `forge fmt`, `forge test`, `forge new`, `forge build`, `forge install`, `forge lsp`, `forge learn`, `forge chat`
- **Interactive tutorial system** with 14 lessons (expanded to 30 in v0.3.0)
- **Type checker** with gradual type checking and warnings
- **Algebraic data types** with pattern matching
- **Result/Option types** with `?` operator propagation, both `Ok()`/`ok()` and `Err()`/`err()` supported
- **`null` literal** as a first-class value with proper comparison semantics
- **String keys in objects** — `{ "Content-Type": "json" }` works
- **Implicit return** in closures — `[1,2,3].map(fn(x) { x * 2 })` returns `[2, 4, 6]`
- **LSP server** for editor integration
- **Package manager** for git-based and local package installation
- **GitHub Actions CI/CD** with multi-platform builds (Linux + macOS, x86_64 + aarch64)
- **Install script** for binary installation (`curl | bash`)
- **287 tests** (Rust unit + Forge integration)

### Changed

- Default execution engine switched from VM to interpreter for broader feature support
- VM available via `--vm` flag, JIT via `--jit` flag for performance-critical workloads
- Improved error messages with "did you mean?" suggestions and source context
- REPL upgraded with rustyline (history, completion, multiline)
- `timeout` now enforces deadlines and kills runaway code
- `safe` and `when` work as both statements and expressions
- Spread operator properly flattens: `[...a, 4, 5]` → `[1, 2, 3, 4, 5]`
- Pipeline operator `|>` correctly returns values

## [0.1.0] - 2026-01-15

### Added

- Initial release
- Lexer with string interpolation
- Recursive descent parser
- Tree-walk interpreter
- Basic HTTP server and client
- REPL
- 7 example programs
