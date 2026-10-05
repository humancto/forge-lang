# CLAUDE.md — Forge Language Project Context

## What Is This?

Forge is an internet-native programming language built in Rust (60k+ lines; run `find src -name '*.rs' | xargs wc -l` for the current figure). Dual syntax (classic + natural language). Built-in HTTP, database, crypto, AI, CSV, terminal UI, shell integration, NPC fake data, GenZ debug kit, and 30 interactive tutorials.

## Architecture

```
Source (.fg) → Lexer → Parser → AST → Type Checker → VM / Interpreter → Result
                                                         ↓
                                                  Runtime Bridge
                                              (axum, reqwest, tokio, rusqlite)
```

The bytecode VM is the default engine (`--vm` is accepted but is a no-op). A tree-walking interpreter (`--interp` flag) is available for full feature coverage (decorator-driven HTTP servers auto-fallback). A JIT compiler (`--jit`) is available for maximum performance on numeric workloads.

For a compact, verified description of canonical Forge (written for AI models), see `llms.txt` at the repo root. Keep it in sync when syntax or stdlib signatures change.

## Quick Start

```bash
cargo build
forge learn                  # 30 interactive tutorials
forge run examples/hello.fg  # run a program
forge -e 'say "hello!"'     # eval inline
forge new my-app             # scaffold project
forge test                   # run tests
forge chat                   # AI chat mode
forge fmt                    # format code
```

## Dual Syntax (Classic + Natural)

| Feature     | Classic            | Forge-Unique                |
| ----------- | ------------------ | --------------------------- |
| Variables   | `let x = 5`        | `set x to 5`                |
| Mutable     | `let mut x = 0`    | `set mut x to 0`            |
| Reassign    | `x = 10`           | `change x to 10`            |
| Functions   | `fn add(a, b) { }` | `define add(a, b) { }`      |
| Output      | `println("hi")`    | `say` / `yell` / `whisper`  |
| Else        | `else { }`         | `otherwise { }` / `nah { }` |
| Async fn    | `async fn x() { }` | `forge x() { }`             |
| Await       | `await expr`       | `hold expr`                 |
| Yield       | `yield value`      | `emit value`                |
| Destructure | `let (a, b) = tuple` (tuples only) | `unpack {a, b} from obj` / `unpack [x, ...rest] from arr` |
| Fetch       | `fetch("url")`     | `grab resp from "url"`      |

## Innovation Keywords (unique to Forge)

- `when age { < 13 -> "kid", else -> "senior" }` -- when guards
- `must expr` -- crash on error with clear message
- `safe { risky_code() }` -- null-safe execution (statement only)
- `check name is not empty` -- declarative validation
- `retry 3 times { }` -- automatic retry
- `timeout 5 seconds { }` -- time-limited execution (experimental)
- `schedule every 5 minutes { }` -- cron tasks
- `watch "file" { }` -- file change detection
- `ask "prompt"` -- AI/LLM calls
- `download "url" to "file"` -- file download
- `crawl "url"` -- web scraping
- `repeat 5 times { }` -- counted loop
- `wait 2 seconds` -- sleep with units

## CLI Commands

Source of truth: `forge help` (clap `Command` enum in `src/main.rs`).

run, repl, version, fmt, test, new, build, install, add, update, publish, search, lsp, dap, mcp, learn, chat, watch, doc, help, plus `-e` for inline eval.

`forge mcp` (`src/mcp.rs`, test `tests/mcp_stdio.rs`) is a stdio MCP server for AI agents: tools `run_forge` / `check_forge` / `forge_reference`; scripts run in `Sandbox` under deny-all plus the `--allow-*` / `[permissions]` grants (`build_mcp_policy` in `main.rs`). Both the binary and the lib compile `mcp.rs` and `sandbox.rs`.

Global flags: `--interp`, `--jit`, `--profile`, `--strict`, `--allow-run` (`--vm` is a backwards-compatible no-op). `forge build` takes `--native` (embeds source) or `--aot` (embeds bytecode).

## Standard Library (22 global modules, 200+ module functions)

Source of truth: `src/stdlib/`. Globals on both engines (registered from `src/builtins_registry.rs`): math, fs, io, crypto, db, pg, mysql, jwt, env, json, regex, log, http, csv, term, os, path, time, url, toml, npc, ws. `exec` is not a global object — use the `run_command` builtin.

| Module   | Key Functions                                                                                                                                                    |
| -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `math`   | sqrt, pow, abs, max, min, floor, ceil, round, pi, e, sin, cos, random_int, clamp                                                                                 |
| `fs`     | read, write, append, exists, list, remove, mkdir, copy, rename, size, ext, read_json, write_json, lines, dirname, basename, join_path, is_dir, is_file, temp_dir |
| `io`     | prompt, print, args, args_parse, args_get, args_has                                                                                                              |
| `crypto` | sha256, md5, base64_encode/decode, hex_encode/decode                                                                                                             |
| `db`     | open, query, execute, close (SQLite)                                                                                                                             |
| `pg`     | connect, query, execute, close (PostgreSQL)                                                                                                                      |
| `mysql`  | connect, query, execute, close (MySQL — parameterized queries, connection pooling)                                                                               |
| `jwt`    | sign, verify, decode, valid (HS256/384/512, RS256, ES256)                                                                                                        |
| `env`    | get, set, has, keys                                                                                                                                              |
| `json`   | parse, stringify, pretty                                                                                                                                         |
| `regex`  | test(text, pattern), find, find_all, replace, split                                                                                                              |
| `log`    | info, warn, error, debug                                                                                                                                         |
| `http`   | get, post, put, delete, patch, head, download, crawl                                                                                                             |
| `csv`    | parse, stringify, read, write                                                                                                                                    |
| `term`   | red/green/blue/yellow/bold/dim, table, hr, sparkline, bar, banner, box, gradient, success/error                                                                  |
| `exec`   | run_command                                                                                                                                                      |
| `os`     | hostname, platform, arch, pid, cpus, homedir                                                                                                                     |
| `path`   | join, resolve, relative, is_absolute, dirname, basename, extname, separator                                                                                      |
| `npc`    | name, first_name, last_name, email, username, phone, number, pick, bool, sentence, word, id, color, ip, url, company                                             |

## Core Builtins (beyond modules)

- Output: print, println, say, yell, whisper
- Types: str, int, float, type, typeof
- Collections: len, push, pop, keys, values, contains, range, enumerate, sum, min_of, max_of, unique, zip, flatten, group_by, chunk, slice, partition
- Functional: map, filter, reduce, sort (with custom comparator), reverse, find, flat_map, any, all, sample, shuffle
- Streams: `.stream()` on arrays/tuples/sets/maps/strings → lazy pull-based iterator. Combinators: filter, map, take, skip, chain, zip, enumerate. Terminals: collect/to_array, count, for_each, first, reduce, sum, find, any, all. Single-use (drained streams yield empty terminals), iterative (no recursion depth limit), poisons on closure error.
- Enum methods: `impl MyType { fn foo(it, ...) { ... } }` attaches instance methods to algebraic `type` definitions; dispatch walks through the ADT value's `__type__` field into the method table. Supports method bodies with `match it { Variant(f) => ... }`, returning new ADT instances, chained calls, and dispatch via collection lambdas. Known gap: `TypeName.method()` static dispatch on algebraic types is not resolved today (works only for `struct`).
- Objects: has_key, get (with dot-paths), pick, omit, merge, entries, from_entries, diff
- Strings: split, join, replace, starts_with, ends_with, lines, substring, index_of, last_index_of, pad_start, pad_end, capitalize, title, repeat_str, count, slugify, snake_case, camel_case
- Results: Ok, Err, is_ok, is_err, unwrap, unwrap_or
- Options: Some, None, is_some, is_none
- Shell: sh, shell, sh_lines, sh_json, sh_ok, which, cwd, cd, pipe_to
- System: time, uuid, exit, input, wait, run_command
- Validation: assert, assert_eq, assert_ne, assert_throws, satisfies
- GenZ Debug Kit: sus (inspect), bruh (panic), bet (assert), no_cap (assert_eq), ick (assert-false)
- Execution: cook (profiling), yolo (fire-and-forget), ghost (silent exec), slay (benchmarking)
- Concurrency: channel, send, receive, try_send, try_receive, select, close

## Build & Test

```bash
cargo build          # 0 errors
cargo test           # 1,600+ Rust tests pass
forge --allow-run test --engine both # 640+ Forge tests on VM and interpreter (shell tests need --allow-run)
forge test --coverage # with line coverage report
```

`examples/` holds 20+ programs. Server examples (`api.fg`, `bench_server*.fg`) block until killed; `devops.fg`/`showcase.fg` need `--allow-run`; `bench_client.fg` needs `FORGE_HTTP_ALLOW_PRIVATE=1` and a running bench server. `tools/run_examples.sh` runs every runnable example on the default engine, and `cargo test --test engine_diff` compares all examples, parity fixtures and `tests/*.fg` across both engines.

## Known Limitations (v0.9.0)

- All three database modules (db, pg, mysql) now support parameterized queries — always use them for user input
- The VM is the default engine; programs using decorator-driven HTTP servers (`@server`, `@get`, etc.) auto-fallback to the interpreter
- Use `--interp` for full feature coverage, `--jit` for maximum numeric performance
- `forge build --native` / `--aot` produce standalone executables when `libforge_lang.a` is found (via `FORGE_LIB_DIR` or next to the `forge` binary); otherwise they fall back to a launcher that shells into an installed `forge`. `--aot` is VM-only and rejects decorator-driven servers — use `--native` for those.
- Shell builtins (`sh`, `shell`, `run_command`, `pipe_to`, ...) require `--allow-run` for `forge run`; the REPL and `-e` enable it automatically.
- `regex` functions take `(text, pattern)` order, not `(pattern, text)`
- Result constructors accept both cases: `Ok(42)`/`ok(42)`, `Err("msg")`/`err("msg")`

## Engineering Discipline

These rules are non-negotiable. Follow them on every change.

### Before Every Change

1. **Read the code you're modifying.** Never edit blind.
2. **Run `cargo test` before starting.** Know what passes now.
3. **Understand the dependency chain.** Changing `bytecode.rs` affects `compiler.rs`, `machine.rs`, `ir_builder.rs`, and `serialize.rs`.

### During Changes

4. **Small, atomic commits.** One concern per commit. Never mix features.
5. **Tests before or alongside code.** Risky changes get tests first.
6. **No `unwrap()` in production paths.** Use `?` or proper error handling. If structurally impossible, use `expect("BUG: ...")` with an explanation.
7. **If it compiles but feels wrong, stop.** Check the design.
8. **Never remove a working execution path.** Interpreter, VM, and JIT must all keep working.
9. **VM parity is your responsibility.** Builtin names, arities and stdlib modules come from `src/builtins_registry.rs` (both engines register from it; module members share one implementation). Global builtins still have one match arm per engine — add both (`src/interpreter/builtins.rs`, `src/vm/builtins.rs`); the registry tests fail otherwise. Shared language rules live in `src/semantics/`.

### After Every Change

10. **Run `cargo test`.** If tests fail, fix before committing.
11. **Run the examples.** `forge run examples/hello.fg` and `forge run examples/functional.fg` must pass.
12. **Check for regressions.** The VM is the default engine, so plain `forge run` exercises it; also run with `--interp` when you touch shared semantics, and with `--jit` when you change the JIT.
13. **Update CHANGELOG.md.** Every PR that ships user-facing changes must have an entry under `[Unreleased]`. Format: `- Description of change ([#PR](link))`. On release, `[Unreleased]` is cut into a version block.
14. **Bump the version together.** When cutting a release, update `Cargo.toml` version, add CHANGELOG heading (e.g. `## [0.5.0] - 2026-03-06`), and tag the commit.

### Release Checklist (`/release-verify`)

Run this checklist after every version bump. Grep for stale version strings and verify all targets. **Every item must pass before shipping.**

#### Version touchpoints (update ALL to new version)

| # | File | What to update |
|---|------|----------------|
| 1 | `Cargo.toml` | `version = "X.Y.Z"` |
| 2 | `CHANGELOG.md` | Cut `[Unreleased]` into `[X.Y.Z] - YYYY-MM-DD` |
| 3 | `README.md` | Version output example + project status section |
| 4 | `CLAUDE.md` | `## Known Limitations (vX.Y.Z)` |
| 5 | `docs/index.html` | Hero badge text |
| 6 | `docs/spec/theme/forge-9e99b408.js` | `badge.textContent` |
| 7 | `docs/spec/index.html` | Spec version label (`version <strong>X.Y.Z</strong>`) |
| 8 | `docs/spec/introduction.html` | Same spec version label |
| 9 | `docs/spec/print.html` | Same spec version label |
| 10 | `docs/PROGRAMMING_FORGE.md` | Front-matter `version:` + example output |
| 11 | `docs/FORGE_BOOK.md` | Front-matter `version:` + example output |
| 12 | `docs/BOOK_FRONT_MATTER.md` | Front-matter `version:` |
| 13 | `docs/book/template.tex` | Title page edition line |
| 14 | `docs/part4_internals.md` | Example output `# Output: Forge vX.Y.Z` |

#### Publish targets

| # | Target | Command / Action |
|---|--------|------------------|
| 1 | GitHub release | `gh release create vX.Y.Z --title "..." --notes-file /tmp/release-notes.md` + attach binary |
| 2 | crates.io | `cargo publish` (run `cargo clean -p forge-lang` first to bust `env!` cache) |
| 3 | Homebrew | Update `humancto/homebrew-tap/Formula/forge.rb` — version, URL, sha256 |

#### Verification command

After updating, run this to catch strays (excludes `target/`, `.git/`, changelogs, and "as of vX.Y.Z" historical markers):

```bash
# Replace OLD with the previous version (e.g., 0.7.0)
rg --glob '!target' --glob '!.git' --glob '!Cargo.lock' --glob '!*.d' \
   "v?OLD" --type-not lock \
   | grep -v 'as of v' | grep -v 'CHANGELOG\|changelog' | grep -v '\[0\.' | grep -v 'ROADMAP'
```

Zero output = clean release. Any remaining hits are stale references to fix.

### CHANGELOG Format (Keep-a-Changelog)

```markdown
## [Unreleased]

### Added

- New feature X

### Fixed

- Bug Y in module Z

### Changed

- Behaviour of W

## [0.4.2] - 2026-01-15

...
```

Categories in order: `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`.

### Server Concurrency Model

The HTTP server uses **per-request fork**, not a shared interpreter.

When `forge run app.fg` boots a server (`@server` decorator), the
program's `Interpreter` is wrapped in a read-only
`Arc<InterpreterTemplate>`. Each incoming request:

1. Acquires a backpressure permit (default 512 in-flight; excess → 503).
2. Calls `template.fork()` (~0.06ms) to get a fresh `Interpreter` with
   a deep-cloned environment.
3. Runs the handler synchronously on `tokio::task::spawn_blocking` so
   it cannot block an async worker.
4. A `Drop` guard on the response future flips a per-request cancel flag
   when axum drops it (client disconnect, server shutdown). The
   interpreter polls the flag at every safe point.

**Implications for handler authors:**

- Handlers may **read** any top-level binding. Mutations made during a
  request do not persist to the template or other requests — each
  fork starts from the template snapshot.
- A handler that reads a top-level variable mutated by a
  `schedule`/`watch` block will read the **template snapshot value**,
  not the schedule's writes. Future `shared { }` blocks will provide
  explicit cross-request state.
- **Captured closures are now isolated per request.** A captured-counter
  helper (`fn make_counter() { return fn() { count = count + 1 } }`)
  returns a Lambda whose closure is fully isolated per fork — two
  concurrent requests get independent counter state. This is the
  invariant `Environment::deep_clone_isolated` provides; cycle handling
  for recursive functions is built in.
- WebSocket handlers fork **once per connection**, not per message.
  Connection-scoped state is held in a `parking_lot::Mutex`. Different
  WS connections are fully isolated.
- Large top-level state (`let huge = read_file("100mb.json")`) is
  copied on every request fork. `Value::String` is `String`, not
  `Arc<str>` (only `Value::Function` bodies are `Arc`-shared). Keep
  top-level data small or load it lazily inside the handler.
- Handlers run on the blocking pool, whose threads get
  `recursion::WORKER_STACK_SIZE` (256 MiB reserved) when the runtime is
  built with `recursion::configure_runtime` (the CLI and standalone
  binaries do this). A host embedding `start_server` in its own runtime
  should call it too, or handler recursion is capped by the 2 MiB
  default (~150 frames).
- `Value::Stream` in the template env is **forbidden** (debug builds
  panic at first fork). Streams are single-use; sharing across forks
  silently breaks. Construct streams inside handlers, not at module
  top level.

**Spawn vs serve — different fork semantics:**

| Caller | Closure isolation? | Why |
|---|---|---|
| `fork_for_serving` (HTTP requests) | Yes (full deep walk) | Implicit fork; user did not opt in. Must be sound by default. |
| `spawn_task` (squad `spawn` blocks) | No (shallow on closures) | Squad is opt-in concurrency. `let counter = make_counter(); squad { spawn { counter() } spawn { counter() } }` legitimately wants accumulation. |
| `fork_for_background_runtime` (schedule/watch) | No (shallow on closures) | Schedule blocks want state continuity across iterations. |

**Observability:**

The HTTP server and the Forge `log` stdlib emit structured events
through `tracing` (see `src/runtime/tracing_init.rs`).

| Env var | Values | Default |
|---|---|---|
| `FORGE_LOG_FORMAT` | `pretty` / `compact` / `json` | `pretty` on TTY, `compact` when piped |
| `FORGE_LOG` | any `tracing_subscriber::EnvFilter` directive | falls back to `RUST_LOG`, then to `forge_lang=info,tower_http=info,axum=warn,forge.user=info` |

Stable target names:
- `forge.server` — server lifecycle (startup, panic, shutdown, cancel-on-drop).
- `forge.user` — user-emitted events from the Forge `log` stdlib module.
- `tower_http::trace::*` — per-request HTTP span and response event from `TraceLayer`.

OpenTelemetry/OTLP export (behind `otel` Cargo feature, off by default):
- Build with `cargo build --features otel` (or set in `Cargo.toml` for
  service projects). Adds ~30 transitive crates (tonic, prost, hyper).
- Activate at runtime by setting `OTEL_EXPORTER_OTLP_ENDPOINT`
  (e.g. `http://localhost:4317`).
- Honors standard OTel env vars via `Resource::builder()`:
  - `OTEL_SERVICE_NAME` (default `"forge"`)
  - `OTEL_RESOURCE_ATTRIBUTES` (parsed by `EnvResourceDetector`)
- Inbound W3C `traceparent` is extracted in `TraceLayer::make_span_with`
  and set as the parent context on the request span. Distributed
  traces connect end-to-end across services.
- Spans flush on graceful shutdown (after `axum::serve` returns) and
  on CLI script exit, wrapped in `spawn_blocking`.
- `init_otel()` MUST be called from the main tokio runtime (not from
  a nested runtime created by a stdlib helper). The valid call sites
  are `start_server` and `main` — both use `#[tokio::main]`.
- `init_subscriber()` consults `OTEL_PROVIDER` and attaches the OTel
  layer at the Registry level (innermost). Layers can't be added
  after `try_init`, so `init_otel` MUST run before `init_subscriber`.
- Default sampling is "send everything." For production high-RPS
  services, configure your collector to sample. `OTEL_TRACES_SAMPLER`
  support is a follow-up.
- Only gRPC is wired (`OTEL_EXPORTER_OTLP_PROTOCOL=grpc`). Other
  protocols are silently ignored.
- Outbound `traceparent` injection in the HTTP client is not yet
  wired (separate follow-up).

Per-request `X-Request-Id`:
- `tower_http::request_id::SetRequestIdLayer` assigns a UUID v4 to
  every request that doesn't already carry `X-Request-Id`, or
  passes through the inbound value if present.
- `PropagateRequestIdLayer` echoes the resolved id back in the
  response `X-Request-Id` header.
- The id is recorded as the `request_id` field on the outer
  `tower_http` `request` span (so `on_response` carries it) and the
  inner `forge.handler` span (belt-and-suspenders for handler-body
  events including user `log.info` via the propagated `Span::current()`).
- Inbound `X-Request-Id` is capped at 64 chars and `to_str()` failures
  are warned-then-substituted-with-`"unknown"` (`extract_request_id`
  helper in `runtime/server.rs`).
- WS per-message events do NOT carry `request_id` (the on_upgrade
  closure runs detached from the trace span); the upgrade request
  itself does. Documented limitation.

Layer order in `start_server` (axum's `Router::layer` makes the LAST
`.layer()` call the OUTERMOST → runs FIRST on the request path):

```
.layer(cors_layer)                                       // innermost
.layer(trace_layer)                                      // middle (uses make_span_with)
.layer(PropagateRequestIdLayer::x_request_id())          // 3rd
.layer(SetRequestIdLayer::x_request_id(MakeRequestUuid)) // outermost (runs first)
```

Critical: `Set` must run before `Propagate` on the request path so
`Propagate` can capture the populated header into its response future.
This ordering is fragile under cargo incremental compilation -- a
`cargo clean -p forge-lang` is sometimes needed when changing
middleware to see the effect.

Per-request span context (`method`, `uri`, `version`, `handler`) is
propagated across the `spawn_blocking` boundary via `Span::current()`,
so a user `log.info` from inside a handler inherits the HTTP request
fields automatically. JSON output is parseable by any log aggregator;
ANSI escape codes are emitted only when stderr is a TTY.

Init is idempotent and lazy: `start_server` and the `log` stdlib both
call `tracing_init::init_subscriber()` on first use.

**Authoring fork primitives:**

- Always use `env.deep_clone()`, never `env.clone()`. `Environment` is
  `Vec<Arc<Mutex<HashMap>>>` — derived `Clone` is shallow and shares
  scope storage. Concurrent forks that share scope `Arc`s would
  silently serialize on the per-scope `Mutex`, defeating the whole
  goal of per-request isolation.
- For HTTP serving (where forks happen per-request, implicitly), use
  `env.deep_clone_isolated()` instead. This walks `Value`s and gives
  every captured closure its own scope graph, with Arc-pointer-keyed
  cycle handling for the recursive-function case. The plain
  `deep_clone` would still leave `Value::Function::closure` and
  `Value::Lambda::closure` sharing scope `Arc`s with the template.

### Learnings (Append Here)

- **HTTP per-request fork needs `deep_clone_isolated`, not just `deep_clone`.** `deep_clone` duplicates scope `Arc`s but the `Value`s inside the scopes are cloned by `Value::clone`, which is shallow on `Value::Function::closure: Environment` and `Value::Lambda::closure: Arc<Mutex<Environment>>`. Concurrent HTTP requests calling a captured-closure helper would lock-contend on the shared closure scope mutex. The isolated variant walks values, with Arc-identity memoization for the recursive-function cycle case. `spawn_task` and `fork_for_background_runtime` deliberately stay on the shallow `deep_clone` so spawn/schedule callers get closure-state continuity (their semantics opt into sharing).
- **`fork_*` env must `deep_clone`, never plain `.clone()`.** `Environment` is `Vec<Arc<std::sync::Mutex<HashMap<String, Value>>>>`, so a derived `Clone` bumps `Arc` refcounts but shares scope storage. Two concurrently-forked interpreters would then serialize on per-scope mutexes — invisible until you actually call the fork concurrently. The HTTP server's per-request fork (`fork_for_serving`) and the schedule/watch fork (`fork_for_background_runtime`) both depend on this. `spawn_task` got it right from day one; the other two were latent until the server fix.
- **JIT jump offsets:** The VM pre-increments IP before applying jump offsets. JIT target = `ip + 1 + sbx`, not `ip + sbx`. This caused fib(30) to return wrong values.
- **Builtin shadowing:** Registering a `BuiltIn("time")` after a `time` module object shadows the module. Register modules last, or remove the simple builtin.
- **Value PartialEq:** The interpreter's `Value` enum needs a manual `PartialEq` impl because `Function`/`Lambda` variants contain non-comparable closures. Never derive it.
- **GitHub Actions runners:** `macos-13` is deprecated. Use `macos-latest` for both ARM and x86_64 targets.
- **Bytecode encoding:** Instructions are 32-bit. Format: `[op:8][a:8][b:8][c:8]` or `[op:8][a:8][bx:16]` or `[op:8][a:8][sbx:16]`. The `sbx` field is signed 16-bit stored as unsigned.
- **Constant dedup:** `Chunk::add_constant()` deduplicates via `identical()`. Don't add the same constant twice — it wastes the constant pool.
- **Builtin registry.** `src/builtins_registry.rs` is the single list of global builtins (name + arity) and stdlib modules for both engines. Never add a name to only one engine's table; the registry tests (`every_global_is_dispatched_by_both_engines`, `registry_globals_and_modules_exist_on_both_engines`) catch it. VM module calls go through `call_module` (the interpreter's stdlib code) after `args_to_interp`; do not hand-roll per-module argument conversion.
- **VM collections are values.** `SetIndex`/`SetField` return an updated copy and the compiler stores it back into the place (`compile_store`). Never mutate an `ObjKind::Array`/`Object` in place through `gc.get_mut` from user-visible operations — aliases (`let w = z`, arguments, captured values) would observe it. The one sanctioned exception is `GcObject::unique` (see `src/vm/local_ops.rs`).
- **VM single-owner updates (`GcObject::unique`).** `AddLocal`/`PushLocal`/`PopLocal` mutate a string/array in place only when it carries the `unique` bit, which is set solely on a value those opcodes just allocated into one local register with no open upvalue. Every copy of a reference *out of* a local register must clear it: today that is `GetLocal`, `Move` and closure capture (`Gc::share`). If you add an opcode or compiler path that reads a local's register directly as an operand, call `self.gc.share(v)` on what it reads, or in-place updates become visible through the alias. Unique strings are never interned (`Gc::alloc_unique_string`); interned strings are shared and must never be mutated.
- **VM safe points are budgeted.** `timeout` deadlines are polled every `SAFEPOINT_INTERVAL` instructions (`safepoint_countdown`), not per instruction — reading the clock per instruction cost ~6x on tight loops. Anything that must take effect promptly (like `PushTimeout`) sets `safepoint_countdown = 0`. Cancellation remains a per-back-edge/per-call atomic load.
- **JIT loop tier-up restarts the call.** `CallFrame::entry_args` keeps the call's arguments (GC-rooted) so `try_jit_loop_restart` can re-run a pure function natively after `LOOP_HOT_THRESHOLD` back-edges. This is only sound because the verifier admits pure functions; if the JIT ever accepts side effects (globals, output, bridges with effects), restart must be replaced by real OSR or disabled for those functions.
- **Direct calls vs callbacks.** `semantics::check_call_arity` applies to direct calls (VM `Call` opcode; interpreter `Expr::Call`/pipelines). Builtins call back through `call_value`/`call_function`, which stay lenient so `map(xs, fn(x, i) {...})` keeps working.
- **JIT bridges cannot return `Result`.** A failing `extern "C"` bridge must call `VM::record_jit_bridge_error`; `try_jit_call` raises it. Returning a placeholder without recording swallows the error.
- **VM-interpreter parity is not automatic.** The two share no code. Every interpreter builtin fix must be manually ported to `src/vm/builtins.rs`. Known audit-tracked gaps: `sort()` string support, `split("")` char-splitting, `int(bool)`, `keys({})`, `is_some`/`is_none` — all fixed in March 2026.
- **`sort()` with custom comparator:** `sort_by` closure borrows `self` immutably but calling `self.call_value()` needs `&mut self`. Work around by collecting items first (releasing the `gc` borrow), then sort with `call_value` on cloned items.
- **GC borrow in closures:** Never call `self.alloc_string()` or `self.call_value()` inside a closure that still holds `self.gc.get()`. Always collect into a `Vec<String>` or `Vec<Value>` first to drop the GC borrow.
- **VM `TryCatch` (resolved):** The compiler used to drop the catch block (M1.2.2), so the VM did not catch runtime errors. It is now compiled; `try { } catch e { }` works on the default VM engine.
- **VM `Destructure` (resolved):** Also once dropped by the compiler (M1.2.1). `unpack {a, b} from obj`, `unpack [x, ...rest] from arr` and `let (a, b) = tuple` now work on the VM. Note there is no `let {a, b} = obj` form — object destructuring is `unpack` only.
- **VM GC rooting is structural — keep it that way.** GC only runs at safe points in `run_until`, so the danger is a native builtin holding `Value`s in Rust locals while it calls back into Forge code. `VM::call_native` wraps every builtin in a GC *native scope* (`Gc::enter_native`/`exit_native`): args are pinned and every allocation made by native code is auto-pinned until the builtin returns; `VM::call_value` suspends auto-pinning while bytecode runs and pins the callback's return value into the caller's scope. The one manual duty: if a builtin snapshots values *out of* a heap object and then calls back, pin the snapshot (`self.gc.pin_values(&items)`) because the callback may mutate the source. Never call the private `dispatch_native` directly. Test with `FORGE_GC_STRESS=1` / `vm.gc.set_stress(true)` (collects at every safe point after an allocation); `vm::runtime_safety_tests` runs all parity fixtures under stress. Invariants are documented in `src/vm/gc.rs`.
- **VM integers: only `Value::int(n, gc)`.** There is no public panicking inline-int constructor any more (`NanBoxedValue::from_small_int` is `#[cfg(test)]`). Values outside 48 bits become `ObjKind::BoxedInt`; use `Value::try_inline_int` when no GC is available. Integer overflow policy (both engines): results that don't fit in i64 promote to float, like `i64::MAX + 1` — shared helpers live in `stdlib/math.rs` (`int_pow`, `int_abs`, `rounded_float_to_num`).
- **Recursion limits live in `runtime/recursion.rs`.** Every engine calls `check_call_depth(depth)` per Forge call: a configurable depth limit (default 10000, `FORGE_MAX_DEPTH` / `--max-depth`) plus a native-stack guard (red zone below the thread's stack end), and reports `depth_exceeded_message` so the text is identical everywhere. The CLI runs on a `forge-main` thread with a 1 GiB (lazily committed) stack; unregistered threads are assumed to have 2 MiB. Measured cost per Forge call: ~16 KB interpreter, ~4 KB VM. JIT'd self-recursion receives the remaining depth budget and deopts to the VM before exceeding the limit, so the error is identical.
- **Import cycles: `runtime/imports.rs::enter_import`.** Both engines push the resolved module path (RAII `ImportGuard`) before running an imported file and get `circular import: a.fg -> b.fg -> a.fg` on re-entry. The chain is per-thread and unwinds on drop.
- **VM stdlib tables are backfilled from the interpreter.** `VM::backfill_stdlib_from_interpreter` adds any member of `stdlib::*::create_module()` missing from the hand-written VM module tables (fixed `fs.size`, `term.sparkline`, `math.inf` on the VM). VM `db.*`/`term.*` now use the full `args_to_interp` conversion — a string-only conversion silently dropped array/object arguments.
- **Interpreter calls are lexical.** `call_function_inner` runs every function in `closure` + one fresh parameter scope. The old "global function fast path" pushed the callee's scope onto the *caller's* env: dynamic scoping, and O(call depth) global lookups. Never reintroduce a path that executes a body on top of the caller's scopes.
- **Reading a variable copies it.** `Environment::get` deep-clones (strings, arrays, objects are owned). Hot paths must borrow via `Environment::with_value` / `with_value_mut` / `with_binding_mut` (see `src/interpreter/places.rs`). The callback runs under the scope `Mutex`, which is not re-entrant: it must not touch the environment or run Forge code — evaluate operands first, and only reorder evaluation when the operand `is_effect_free`.
- **Statement bodies don't produce values.** Loop bodies, statement `if` branches, `match` arms and `try`/`catch` run via `exec_body`, which evaluates a trailing expression for effect only (so a trailing `out.push(x)` does not copy `out`). Only `when`/`safe` statement values are consumed by `eval_block_value`; if that changes, `exec_body` callers must change too.
- **Arc-backed `Value::String`/`Array`/`Object` is blocked on the VM.** `src/vm` constructs and destructures `interpreter::Value::{Array, Object, Set, Map, Tuple, String}` with owned payloads, so moving them to `Arc` (copy-on-write, and hashed Set/Map indexes) needs a coordinated VM change.
- **Threads that run Forge code need a registered stack.** Use `recursion::spawn_worker` (std threads) or `recursion::configure_runtime` (tokio runtimes); an unregistered thread is assumed to have 2 MiB and the guard stops recursion early.
- **Name positions come from the syntax index, not the AST.** Expressions carry no spans (both engines match on `Expr`), so `Parser::with_index` records every name occurrence (role, exact span, scope, enclosing statement) in `parser::index::SyntaxIndex`. When you add syntax that binds or reads a name, record it there (`note_def` / `note_prev`, and `scoped` for new scopes) or the checker reports it as unknown and the LSP cannot rename it. Name resolution over the index is `typechecker::resolve`; it mirrors run-time scoping (a `let` is visible after its statement, function bodies see later definitions of enclosing scopes).
- **The type checker only errs against declared types or engine rules.** Inferred types type results and callbacks but never reject a later use (arrays are heterogeneous, unannotated variables change type; mutable bindings get the join of all assignments from a silent first pass). Builtin signatures in `typechecker/builtins.rs` type results only — they do not reject arguments. Run-time-rule diagnostics call the shared rules (`semantics::binary`, `check_call_arity`, `builtins_registry::check_arity`). `typechecker::corpus_tests` fails on any strict-mode error in `tests/`, `examples/` and parity fixtures not listed (with a reason) in `tests/typecheck_allowlist.txt`.
- **`--strict` runtime checks are AST instrumentation.** `typechecker::enforce::instrument` inserts `__types.check(value, "<canonical type>", "<context>")` calls; `__types` is a hidden stdlib module (registry), so both engines run the same rule (`semantics::types`) with no engine changes. Instrumentation must never change a correct program's result: argument checks go before the body (an empty body gets an explicit `null` tail), and tails are only wrapped where they are the function's value.

## Module Dependency Map

```
main.rs → lexer, parser, interpreter, vm, runtime, errors, typechecker, ...
vm/mod.rs → compiler, machine, bytecode, frame, gc, green, jit, value
vm/machine.rs → bytecode, frame, gc, value (largest VM file)
vm/compiler.rs → bytecode, parser::ast
vm/jit/ir_builder.rs → bytecode, cranelift
vm/jit/jit_module.rs → ir_builder
vm/jit/runtime.rs → extern "C" helpers called from JIT code (unsafe)
interpreter/mod.rs → parser::ast, runtime, stdlib (largest file)
runtime/server.rs → interpreter, parser::ast, axum
runtime/client.rs → reqwest (SSRF guard)
lib.rs → C ABI entry points used by native/AOT binaries (unsafe FFI)
native.rs → `forge build --native/--aot` (standalone vs launcher)

Line counts drift fast; measure with `wc -l` instead of recording them here.
```
