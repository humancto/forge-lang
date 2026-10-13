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

The bytecode VM is the default engine (`--vm` is accepted but is a no-op). A tree-walking interpreter (`--interp` flag) is available for full feature coverage (programs the VM cannot run faithfully, e.g. unknown decorators, auto-fallback). `@server` programs are served by the VM. A JIT compiler (`--jit`) is available for maximum performance on numeric workloads.

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

Edition 2026 only: edition 2027 (`--edition 2027`, `rfcs/0008-edition-2027.md`) removes every natural form in this table (E0035, `src/parser/removed.rs`; the lexer stops reserving the words, `lexer::token::REMOVED_IN_2027`) and the builtins `say`/`yell`/`whisper` and the GenZ kit (`builtins_registry::REMOVED`, filtered per edition by `globals_in`). `forge fix --edition 2027` (`src/fix.rs`) rewrites 2026 code. Destructuring is `let {a, b: c} = obj` / `let [x, ...rest] = arr` in every edition.

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
| Destructure | `let (a, b) = tuple`, `let {a, b} = obj`, `let [x, ...rest] = arr` | `unpack {a, b} from obj` / `unpack [x, ...rest] from arr` |
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

run, check, explain, repl, version, fmt, fix, test, new, build, install, add, update, publish, yank, search, lsp, dap, mcp, learn, chat, watch, doc, help, plus `-e` for inline eval.

`forge mcp` (`src/mcp/`, test `tests/mcp_stdio.rs`) is a stdio MCP server for AI agents: tools `run_forge` (optional `session_id`, `src/mcp/session.rs`) / `reset_session` / `check_forge` / `forge_reference`; `forge mcp serve tools.fg` serves `@tool`/`@resource` functions written in Forge (`src/mcp/tools.rs`). Scripts run in `Sandbox` (VM by default, `--engine interp`) under deny-all plus the `--allow-*` / `[permissions]` grants (`build_mcp_policy` in `main.rs`). Both the binary and the lib compile `mcp/` and `sandbox.rs`.

Package registry (`rfcs/0007-package-registry.md`): `src/registry/` = `index.rs` (sparse-index format, name rules, resolution, ownership; pure), `client.rs` (fetch + ETag cache, offline, archives), `signing.rs` (ed25519, TOFU pins); `src/publish_index.rs` = `forge publish --registry <index-clone>` / `forge yank` (binary only). End-to-end tests: `tests/registry_index.rs` (file:// index + in-process HTTP server). `tools/registry-template/` seeds the hosted index repo; its Python validator is cross-checked by those tests.

Global flags: `--interp`, `--jit`, `--profile`, `--strict`, `--allow-run`, `--allow-ffi[=PATHS]`, `--max-time`, `--max-fuel`, `--max-memory`, `--error-format human|json` (`--vm` is a backwards-compatible no-op). `forge build` takes `--native` (embeds source) or `--aot` (embeds bytecode).

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
| `regex`  | test(text, pattern), find, find_all, captures, captures_all, replace (`$1`/`${name}`), split                                                                     |
| `log`    | info, warn, error, debug                                                                                                                                         |
| `http`   | get, post, put, delete, patch, head, download, crawl                                                                                                             |
| `csv`    | parse, stringify, read, write                                                                                                                                    |
| `term`   | red/green/blue/yellow/bold/dim, table, hr, sparkline, bar, banner, box, gradient, success/error                                                                  |
| `exec`   | run_command                                                                                                                                                      |
| `os`     | hostname, platform, arch, pid, cpus, homedir                                                                                                                     |
| `path`   | join, resolve, relative, is_absolute, dirname, basename, extname, separator                                                                                      |
| `npc`    | name, first_name, last_name, email, username, phone, number, pick, bool, sentence, word, id, color, ip, url, company                                             |

## Core Builtins (beyond modules)

- Output: print, println, eprint, eprintln (stderr), say, yell, whisper
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
tools/bench.sh [--json]    # wall-clock benchmarks (release build); PRs are gated by perf.yml: >15% slower fails unless labelled perf-regression-ok (docs/BENCHMARKS.md)
```

`examples/` holds 20+ programs. Server examples (`api.fg`, `bench_server*.fg`) block until killed; `devops.fg`/`showcase.fg` need `--allow-run`; `bench_client.fg` needs `FORGE_HTTP_ALLOW_PRIVATE=1` and a running bench server. `tools/run_examples.sh` runs every runnable example on the default engine, and `cargo test --test engine_diff` compares all examples, parity fixtures and `tests/*.fg` across both engines.

## Known Limitations (v0.11.0)

- All three database modules (db, pg, mysql) now support parameterized queries — always use them for user input
- The VM is the default engine, including for decorator-driven HTTP servers (`@server`, `@get`, ...); decorators it cannot honor (unknown ones, non-literal `@server` arguments) auto-fallback to the interpreter
- Use `--interp` for full feature coverage, `--jit` for maximum numeric performance
- `forge build --native` / `--aot` produce standalone executables when `libforge_lang.a` is found (via `FORGE_LIB_DIR` or next to the `forge` binary); otherwise they fall back to a launcher that shells into an installed `forge`. `--aot` is VM-only and rejects decorator-driven servers — use `--native` for those.
- Shell builtins (`sh`, `shell`, `run_command`, `pipe_to`, ...) require `--allow-run` for `forge run`; the REPL and `-e` enable it automatically.
- Shell builtins run through `src/runtime/shell.rs`: `/bin/sh -c` on Unix; on Windows a POSIX `sh` on `PATH`, else `cmd /d /s /c "..."` (command passed verbatim); `FORGE_SHELL` overrides. `which` honors `PATHEXT` on Windows.
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

The HTTP server uses **per-request fork**, not a shared engine.

`runtime/server.rs` is engine-neutral: it serves any `ServeEngine` (a
read-only template that forks a `HandlerWorker` per request). Two engines
implement it:

| Engine | Selected by | Template | Per-request fork |
|---|---|---|---|
| Bytecode VM (default) | `forge run app.fg` | `vm::serve::VmTemplate` (frozen heap + globals after the top level) | `VmTemplate::fork` — fresh `VM` with a private heap copy |
| Interpreter | `forge run --interp app.fg`, or auto-fallback | `InterpreterTemplate` | `fork_for_serving` (`deep_clone_isolated`) |

Routing, argument binding (`server::handler_args`), JSON encoding,
status codes, backpressure, cancellation and tracing are shared, and
`tests/server_engine_parity.rs` requires byte-identical responses from
both engines. Route/server metadata is read from the AST
(`runtime::metadata::extract_runtime_plan`) for both engines; on the VM,
`@server(...)` with literal arguments compiles to nothing. Anything the VM
cannot honor exactly (`metadata::vm_unsupported_decorator`: unknown
decorators, non-literal `@server` args, route decorators with extra args)
keeps the program on the interpreter — never silently dropped.

When `forge run app.fg` boots a server, the engine runs the whole top
level first (`schedule`/`watch` start-up is deferred: VM
`defer_host_runtime` / `launch_deferred_host_tasks`, interpreter
`set_defer_host_runtime` + `host::launch`), then the final state becomes
the template. Each incoming request:

1. Acquires a backpressure permit (default 512 in-flight; excess → 503).
2. Forks the template on a `tokio::task::spawn_blocking` thread (VM fork
   is a single linear pass over the frozen heap, measured ~2x cheaper
   than the interpreter's `fork_for_serving`; `cargo bench --bench
   fork_for_serving`), with the capability
   policy captured at server start installed (`permissions::scope`).
3. Runs the handler synchronously on that blocking thread so it cannot
   block an async worker.
4. A `Drop` guard on the response future flips a per-request cancel flag
   when axum drops it (client disconnect, server shutdown). The
   interpreter polls the flag at every safe point; the VM at every
   backward jump and call (`VM::set_cancel_flag`).

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
  invariant `Environment::deep_clone_isolated` provides on the
  interpreter; on the VM every upvalue cell is re-created per fork while
  identity is preserved inside the fork (two closures sharing a cell
  still share it). Cycle handling for recursive closures is built in to
  both.
- WebSocket handlers fork **once per connection**, not per message.
  Connection-scoped state is held in a `parking_lot::Mutex`. Different
  WS connections are fully isolated. VM connection workers run with the
  JIT off (JIT state is not `Send`; `vm::serve::ConnectionWorker`
  asserts it stays empty); HTTP request workers keep the default JIT
  mode, but compiled code is per fork (not shared across requests).
- Channels and task handles in the template are `Arc`-shared by every
  fork on both engines (deliberate cross-request coordination).
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
- `Value::Stream` in the template env is **forbidden** (interpreter:
  debug builds panic at first fork; VM: `VmTemplate::new` refuses to
  start the server). Streams are single-use; sharing across forks
  silently breaks. Construct streams inside handlers, not at module
  top level.

**Spawn vs serve — different fork semantics:**

| Caller | Closure isolation? | Why |
|---|---|---|
| `fork_for_serving` (HTTP requests) | Yes (full deep walk) | Implicit fork; user did not opt in. Must be sound by default. |
| `VmTemplate::fork` (HTTP requests, VM) | Yes (private heap copy) | Same contract as `fork_for_serving`. VM `schedule`/`watch` threads use `fork_for_spawn` (their own VM), so their writes never reach the template either. |
| `spawn_task` (squad `spawn` blocks) | No (shallow on closures) | Squad is opt-in concurrency. `let counter = make_counter(); squad { spawn { counter() } spawn { counter() } }` legitimately wants accumulation. |
| `fork_for_background_runtime` (schedule/watch) | No (shallow on closures) | Schedule blocks want state continuity across iterations. |

Squad-shared state: in the interpreter, `count = count + 1`, `o.n += 1`,
`a[i] = a[i] + 1` (effect-free right operand and indexes) are atomic per
operation, so concurrent spawns calling a shared closure do not lose
updates (#128, `squad_shared_closure_updates_are_atomic`). Other
read-modify-writes (`x = x + f()`) are not. **Known engine divergence:**
the VM's `spawn` gives each task *copies* of globals and captured upvalues
(`fork_for_spawn` / `transfer_closure`; each thread has its own GC heap),
so writes inside a task are invisible to the parent and to other tasks —
three `spawn { bump() }` calling `let bump = fn() { count = count + 1 }`
leave `count` at `3` on `--interp` and `0` on the default VM.
Portable code returns values from `spawn` (the `squad` result array,
`await`) or uses channels; it never relies on mutating captured state.
Making the VM share would need thread-safe shared upvalue cells.

**Observability:**

The HTTP server and the Forge `log` stdlib emit structured events
through `tracing` (see `src/runtime/tracing_init.rs`).

| Env var | Values | Default |
|---|---|---|
| `FORGE_LOG_FORMAT` | `pretty` / `compact` / `json` | `pretty` on TTY, `compact` when piped |
| `FORGE_LOG` | any `tracing_subscriber::EnvFilter` directive | falls back to `RUST_LOG`, then to `forge=info,forge_lang=info,tower_http=info,axum=warn,forge.user=info,forge.runtime=info,forge.panic=error` (`forge=info` matters: the CLI binary compiles the runtime itself, so its module-path targets are `forge::...`) |

Stable target names:
- `forge.server` — server lifecycle (startup, panic, shutdown, cancel-on-drop).
- `forge.user` — user-emitted events from the Forge `log` stdlib module.
- `tower_http::trace::*` — per-request HTTP span and response event from `TraceLayer`.
- `forge.runtime` — CLI runtime notes (e.g. VM-to-interpreter fallback; JSON mode only, text otherwise).
- `forge.panic` — Rust panics, emitted by the hook `init_subscriber` installs
  (`install_panic_hook`). If the filter disables this target the previous
  (default) hook prints the panic instead — a panic is never swallowed.
- `forge_lang::runtime::client` span `http.client.request` — one per
  outbound HTTP request (method + host only; never the full URL).

`tests/observability_cli.rs` pins the JSON contract by running the `forge`
binary (one subscriber per process): every stderr line parses, user events
carry `timestamp`/`level`/`target`/`fields.message`, and a request produces
`tower_http::trace::on_response` with `status`/`latency` inside the `request`
span (`method`/`uri`/`request_id`).

OpenTelemetry/OTLP export (behind `otel` Cargo feature, off by default):
- Build with `cargo build --features otel` (or set in `Cargo.toml` for
  service projects). Adds ~30 transitive crates (tonic, prost, hyper).
- Activate at runtime by setting `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` or
  `OTEL_EXPORTER_OTLP_ENDPOINT` (e.g. `http://localhost:4317`); the
  signal-specific one wins (`tracing_init::traces_endpoint`).
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
- Sampling follows `OTEL_TRACES_SAMPLER` / `OTEL_TRACES_SAMPLER_ARG`
  (`always_on`, `always_off`, `traceidratio`, `parentbased_always_on`
  (default), `parentbased_always_off`, `parentbased_traceidratio`).
  Invalid values fall back to the default with a stderr warning
  (`tracing_init::SamplerConfig`).
- Only gRPC is wired (`OTEL_EXPORTER_OTLP_PROTOCOL=grpc`). Other
  protocols are silently ignored.
- Outbound requests (`client::fetch`, `http.download`, `http.crawl`)
  open `client::request_span` and send its W3C `traceparent` via
  `client::inject_trace_context` (a caller-set `traceparent` header wins).
  Any new outbound HTTP path must do the same. Compute the span on the
  calling thread, before any `block_on`, so it parents under the
  handler's request span. Tested in `tests/otel_propagation.rs`.

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
- **VM collections are values.** `SetIndex`/`SetField` return an updated copy and the compiler stores it back into the place (`compile_store`). Never mutate an `ObjKind::Array`/`Object`/`Set` in place through `gc.get_mut` from user-visible operations — aliases (`let w = z`, arguments, captured values) would observe it. The sanctioned exceptions are the single-owner bits `GcObject::unique` / `slot_owned` (see `src/vm/local_ops.rs`).
- **VM single-owner updates (`GcObject::unique`).** `AddLocal`/`PushLocal`/`PopLocal` (strings/arrays), `UpdateSetLocal` (sets) and `SetIndexLocal`/`SetFieldLocal`/`PushIndexLocal`/`PushFieldLocal` (arrays/objects) mutate in place only when the local's value carries the `unique` bit, which is set solely on a value those opcodes just allocated (or copied) into one local register with no open upvalue; a container becomes owned through `Gc::set_unique_container`. Every copy of a reference *out of* a local register must clear it: `GetLocal`, `Move` and closure capture (`Gc::share`). The opcodes that read a local's register directly are `GetIndexLocal`/`GetFieldLocal` (share the *element* they return), `PeekLocal` (only as an argument of the following `CallPure`, which shares every argument unless the callee is one of `machine::PURE_BORROW_BUILTINS`, and then shares the result) and the in-place opcodes themselves. Every value an in-place opcode stores into an owned container (`PushLocal`, `AddLocal`, `UpdateSetLocal`, `SetIndexLocal`, `PushIndexLocal`, ...) is shared first. If you add another such opcode, call `self.gc.share(v)` on what escapes and on what it stores, or in-place updates become visible through an alias. A top-level `let` that a function forward-references is a global (`let_defines_global`), not a register, so none of these opcodes apply to it: it stays correct and just copies. Unique strings are never interned (`Gc::alloc_unique_string`); interned strings are shared and must never be mutated.
- **VM owned elements (`GcObject::slot_owned`).** `local[k].push(v)` appends in place when the local owns its container *and* the container owns the array (`slot_owned`, set only by `PushIndexLocal`/`PushFieldLocal` on the array copy they store; never on sets or objects, so `g[k].add(x)` and deeper chains take the copying read-modify-write path). The bit is trusted only through an owned container, so: a container becomes owned only via `Gc::set_unique_container` (clears its elements' bits: they are still referenced by the copy's source); every value stored into an owned container is shared first; every element read out of an owned container without sharing the container is shared (`share` clears both bits). The rules are in `src/vm/local_ops.rs`; tests in `src/vm/container_ops_tests.rs` (aliases, GC stress, allocation counts).
- **Nested method updates (`g[k].push(x)`).** The method form of `push`/`pop`/`add`/`remove` (`semantics::is_mutating_method`) on a field/index chain rooted at a variable updates the place on both engines (interpreter `mutate_nested_place`, VM `try_compile_place_method` + `__forge_method_mut_place`): index expressions, then arguments, then the place is read; the value is stored back only when the method changed it (a struct with its own `push` gets a plain method call); a change under an immutable root is the immutable-assignment error (T0017 statically when the receiver is a known array/set). Only the method form: the free `push(o.a, v)` / `pop(o.a)` keep their (pure) meaning.
- **Receivers: plain `it` is read-only, `mut it` is written back (`ast::ParamKind`).** The parser marks the first parameter of an `impl`/`give` method named `it`/`self` as `Receiver` or `MutReceiver` (`mut` anywhere else is a parse error); both engines bind a plain receiver immutably (interpreter `bind_params`, VM `compile_function_body`), and the checker binds it `BindKind::Immutable`, so writes are E0006/T0017. A `mut it` method hands its final `it` to the dispatch that called *that* function: interpreter `call_function_inner` → `receiver_out` (keyed by `Arc` pointer) → `call_user_method`; VM `__forge_receiver_out(it)` before every return (`emit_receiver_out`, also before `?`) → `receiver_out` (keyed by closure) → `call_user_method` → `call_forge_method_full`. Callers must store it into the receiver's place or refuse: `call_forge_method` (no place) raises `semantics::temporary_receiver`; `__forge_call_method_at` + the `TakeReceiver` opcode (`try_compile_receiver_place_call`, the `m.get(k)` peek path) assign it like `place = v` (`compile_store_variable`); `__forge_method_mut(_place)` report it as the new receiver (`method_mut_apply`), so the `PushLocal`/`UpdateSetLocal`/`PushIndexLocal` fallbacks store it too; the interpreter's `assign_receiver_place`. Any new path that calls a user method must take the updated receiver or go through one of these. Builtin-module roots (`math.sqrt(x)`) keep plain `__forge_call_method`, which the JIT understands. `push`/`pop`/`add`/`remove` on a mutable variable evaluate arguments before reading the variable whatever it holds (maps, structs included); all other method calls read the receiver first. Frozen errors for those methods come from `semantics::frozen_method(method, kind)`.
- **VM safe points are budgeted.** `timeout` deadlines are polled every `SAFEPOINT_INTERVAL` instructions (`safepoint_countdown`), not per instruction — reading the clock per instruction cost ~6x on tight loops. Anything that must take effect promptly (like `PushTimeout`) sets `safepoint_countdown = 0`. Cancellation remains a per-back-edge/per-call atomic load.
- **JIT loop tier-up restarts the call.** `CallFrame::entry_args` keeps the call's arguments (GC-rooted) so `try_jit_loop_restart` can re-run a pure function natively after `LOOP_HOT_THRESHOLD` back-edges. This is only sound because the verifier admits pure functions; if the JIT ever accepts side effects (globals, output, bridges with effects), restart must be replaced by real OSR or disabled for those functions. Why OSR was not done yet: `src/vm/jit/mod.rs`.
- **JIT globals are guards, checked at entry.** Compiled code may read a global only to call it (self, another verified pure function, `float`/`int`/`range`, `math.*` via `__forge_call_method`) or to load a `math` Float constant. Each read becomes a `verifier::Guard`; a spec's guard list includes its callees' (transitively) and `VM::try_jit_native` checks all of them before every native entry. That is sound only because native code cannot run Forge code or assign globals — keep it that way, or guards must be re-checked inside native code. A pure builtin added to the JIT needs a `PureOp`, a guard, and lowering that evaluates *the VM's own Rust expression* (bridges in `jit/math_bridges.rs`: Cranelift `fmin`/`fmax`/`nearest` differ from `f64::min`/`max`/`round`).
- **JIT Float registers.** Each VM register is two Cranelift variables (`i64`, `f64`); the verifier's per-ip state picks which one is live. Floats cross the entry ABI as bit patterns; `JitType::decode` re-boxes with `Value::float` (NaN canonicalization). A register that is Int on one path and Float on another (`let mut s = 0` then `s = s + 0.5`) is a Conflict: the function stays in the VM.
- **Counting loops are `ForRangePrep`/`ForRangeNext`.** `for v in range(a[, b])` and `repeat n times` compile to one body with two loop heads: counting (when the callee is the builtin `range` with Int args, decided at run time) and the generic iterate-the-result path. The mode register selects the step jump; the JIT verifier tracks it as `RegState::RangeFast` so the generic head is unreachable in compiled code. `range`'s third argument is accepted and ignored by the builtin (both engines), so three-argument calls stay on the generic path.
- **Direct calls vs callbacks.** `semantics::check_call_arity` applies to direct calls (VM `Call` opcode; interpreter `Expr::Call`/pipelines). Builtins call back through `call_callback`/`call_value` (VM) and `call_function` (interpreter), which stay lenient so `map(xs, fn(x, i) {...})` keeps working.
- **Builtins call Forge functions through `VM::call_callback` (`src/vm/callback.rs`), not `call_value`.** It keeps a small per-VM memo of recent callees (entered from the argument slice; `entry_args` only for functions with a `Loop`) and caches the JIT selection together with the conditions its entry guards were checked under: `VM::exec_epoch` (bumped on every `run_until` entry and every `call_native`), `Globals::version` (bumped by every global write), frame depth and `JitState::instance`. Soundness rests on "only entering bytecode or a builtin can change globals, upvalue cells, `scope_cancels` or frame timeouts while a builtin runs"; a new way to change any of those without entering `run_until`/`call_native` must bump `exec_epoch`. That is also why the `Call` opcode does not use the cache: bytecode in the *same* `run_until` (`SetUpvalue`, `PushTimeout`, ...) does not bump the epoch. A new per-element builtin should use `call_callback`; non-closures and `--profile` fall back to `call_value` inside it. Measured: map/filter/reduce callbacks went from ~100–140 ns to ~25 ns per element (`docs/BENCHMARKS.md`, "Higher-order collection code"). Compiling `map(xs, fn...)` into a loop was rejected: shadowable `map`, callback leniency and identical errors would all need run-time checks, for the last ~25 ns.
- **JIT bridges cannot return `Result`.** A failing `extern "C"` bridge must call `VM::record_jit_bridge_error`; `try_jit_call` raises it. Returning a placeholder without recording swallows the error.
- **VM-interpreter parity is not automatic.** The two share no code. Every interpreter builtin fix must be manually ported to `src/vm/builtins.rs`. Known audit-tracked gaps: `sort()` string support, `split("")` char-splitting, `int(bool)`, `keys({})`, `is_some`/`is_none` — all fixed in March 2026.
- **`sort()` with custom comparator:** `sort_by` closure borrows `self` immutably but calling `self.call_value()` needs `&mut self`. Work around by collecting items first (releasing the `gc` borrow), then sort with `call_value` on cloned items.
- **GC borrow in closures:** Never call `self.alloc_string()` or `self.call_value()` inside a closure that still holds `self.gc.get()`. Always collect into a `Vec<String>` or `Vec<Value>` first to drop the GC borrow.
- **VM `TryCatch` (resolved):** The compiler used to drop the catch block (M1.2.2), so the VM did not catch runtime errors. It is now compiled; `try { } catch e { }` works on the default VM engine.
- **VM `Destructure` (resolved):** Also once dropped by the compiler (M1.2.1). `let {a, b: c} = obj`, `let [x, ...rest] = arr`, `let (a, b) = tuple` (and the 2026 `unpack ... from`) work on the VM; `Stmt::Destructure::mutable` decides mutability on both engines.
- **Soft keywords (all editions, `src/parser/soft.rs`).** Natural/innovation/query words (`order`, `from`, `to`, `limit`, `where`, `safe`, `emit`, `check`, ...) are keywords only where their construct can start. At an operand position the parser calls `soften_keyword` (statement start: statement and expression constructs; unary position: only `hold`/`must`/`freeze`/`ask`/`safe {`/`when`/`craft`), which rewrites the token to `Token::Ident` when `keyword_is_name` says the construct cannot start there; binding/field positions accept them via `soft::name_word` (`expect_ident`). The rule must stay additive: a word may become a name only where its construct would have been a parse error (so `check(x)` at statement start is still the `check` statement in 2026). Words after an expression (`x where`, `>> take`, `times`, `to`) are still matched as tokens. Reserved everywhere: the core words (`let mut fn return if else match for in while loop break continue struct type interface impl pub import spawn squad try catch async await yield true false null`) and type names. `forge fix` uses the same `keyword_is_name`.
- **Edition-gated syntax lives in the lexer's keyword set.** In edition 2027 the removed natural words lex as `Token::Ident` (`Token::keyword_in`), so 2027 programs may use them as names. `parser/removed.rs` recognises the old shapes built from those identifiers (only when `Parser::report_removed`, set from the edition when the parser is created) and raises `semantics::edition::removed_syntax` (E0034). A new removed form needs: the word in `REMOVED_IN_2027`, a shape in `removed.rs`, a rewrite in `src/fix.rs`, a row in docs/editions/2027.md and a case in `tests/edition_one_syntax.rs`.
- **VM GC rooting is structural — keep it that way.** GC only runs at safe points in `run_until`, so the danger is a native builtin holding `Value`s in Rust locals while it calls back into Forge code. `VM::call_native` wraps every builtin in a GC *native scope* (`Gc::enter_native`/`exit_native`): args are pinned and every allocation made by native code is auto-pinned until the builtin returns; `VM::call_value` suspends auto-pinning while bytecode runs and pins the callback's return value into the caller's scope. The one manual duty: if a builtin snapshots values *out of* a heap object and then calls back, pin the snapshot (`self.gc.pin_values(&items)`) because the callback may mutate the source. Never call the private `dispatch_native` directly. Test with `FORGE_GC_STRESS=1` / `vm.gc.set_stress(true)` (collects at every safe point after an allocation); `vm::runtime_safety_tests` runs all parity fixtures under stress. Invariants are documented in `src/vm/gc.rs`.
- **VM integers: only `Value::int(n, gc)`.** There is no public panicking inline-int constructor any more (`NanBoxedValue::from_small_int` is `#[cfg(test)]`). Values outside 48 bits become `ObjKind::BoxedInt`; use `Value::try_inline_int` when no GC is available. Integer overflow policy (both engines): results that don't fit in i64 promote to float, like `i64::MAX + 1` — shared helpers live in `stdlib/math.rs` (`int_pow`, `int_abs`, `rounded_float_to_num`).
- **Recursion limits live in `runtime/recursion.rs`.** Every engine calls `check_call_depth(depth)` per Forge call: a configurable depth limit (default 10000, `FORGE_MAX_DEPTH` / `--max-depth`) plus a native-stack guard (red zone below the thread's stack end), and reports `depth_exceeded_message` so the text is identical everywhere. The CLI runs on a `forge-main` thread with a 1 GiB (lazily committed) stack; unregistered threads are assumed to have 2 MiB. Measured cost per Forge call: ~16 KB interpreter, ~4 KB VM. JIT'd self-recursion receives the remaining depth budget and deopts to the VM before exceeding the limit, so the error is identical.
- **Import cycles: `runtime/imports.rs::enter_import`.** Both engines push the resolved module path (RAII `ImportGuard`) before running an imported file and get `circular import: a.fg -> b.fg -> a.fg` on re-entry. The chain is per-thread and unwinds on drop.
- **VM stdlib tables are backfilled from the interpreter.** `VM::backfill_stdlib_from_interpreter` adds any member of `stdlib::*::create_module()` missing from the hand-written VM module tables (fixed `fs.size`, `term.sparkline`, `math.inf` on the VM). VM `db.*`/`term.*` now use the full `args_to_interp` conversion — a string-only conversion silently dropped array/object arguments.
- **Interpreter calls are lexical.** `call_function_inner` runs every function in `closure` + one fresh parameter scope. The old "global function fast path" pushed the callee's scope onto the *caller's* env: dynamic scoping, and O(call depth) global lookups. Never reintroduce a path that executes a body on top of the caller's scopes.
- **Reading a variable copies it.** `Environment::get` deep-clones (strings, arrays, objects are owned). Hot paths must borrow via `Environment::with_value` / `with_value_mut` / `with_binding_mut` (see `src/interpreter/places.rs`). The callback runs under the scope `Mutex`, which is not re-entrant: it must not touch the environment or run Forge code — evaluate operands first, and only reorder evaluation when the operand `is_effect_free`.
- **Statement bodies don't produce values.** Loop bodies, statement `if` branches, `match` arms and `try`/`catch` run via `exec_body`, which evaluates a trailing expression for effect only (so a trailing `out.push(x)` does not copy `out`). Only `when`/`safe` statement values are consumed by `eval_block_value`; if that changes, `exec_body` callers must change too.
- **Arc-backed `Value::String`/`Array`/`Object` is blocked on the VM.** `src/vm` constructs and destructures `interpreter::Value::{Array, Object, Map, Tuple, String}` with owned payloads, so moving them to `Arc` (copy-on-write, and a hashed Map index) needs a coordinated VM change.
- **Sets are hash-indexed on both engines (`semantics::ordered_set::OrderedSet`).** Interpreter `Value::Set(SetItems)` and VM `ObjKind::Set(VmSet)` are the same insertion-ordered container (tombstones on removal, compacted when they outnumber live items; small sets scan linearly). The container never hashes or compares by itself: each engine hashes with its own `set_hash` and compares with its own equality (`container_eq` / `set_eq`), and **values that compare equal must hash equally** — Int/Float through `hash_number`, NaN alike, `Frozen(v)` like `v` on the interpreter; anything whose equality is not plain structure hashes by kind/size only (a collision costs one comparison, a missed equal value is a bug). A new value kind or equality rule needs a `set_hash` case on that engine (`equal_values_have_equal_set_hashes` in both engines' tests). Equality is not transitive (two large Ints can equal one Float), so removal goes through `remove_matching` (removes every match) and conversions between engines use `*_unchecked` (keep items as they are). Positional access (`get_index`) is O(1) only without tombstones; the VM's `IterGet` calls `make_dense` first. VM hashing needs the `Gc`, so look up with `gc.get` and then update with `gc.get_mut` (see `update_set_local`).
- **Maps are the same container, keyed (`MapEntries` / `VmMap` = `OrderedSet<(key, value)>`).** Slots are hashed and compared by the key alone with the set rules, so everything above applies. With a non-transitive equality several keys can match a probe; `get`/`set` must use the *oldest* (`OrderedSet::find`/`find_mut` walk the whole index chain), `remove` drops every match. Map equality probes the other map's index (equal keys hash alike, so it finds what a scan would). Fast paths: interpreter `x = x.set(k, v)` / `x = x.remove(k)` with effect-free arguments update the binding in place (`try_update_map_ident`), and `m.get/has/len` on a map variable count as effect-free; VM `UpdateMapLocal` (compiler `try_compile_map_update`) and `m.get/has/len` / `s.has` as `PeekLocal` + `CallPure` of `__forge_call_method` (borrowed only when the receiver really is a map/set: `is_pure_borrow_call`). `UpdateMapLocal` keeps the generic evaluation order: `set` reads its receiver first (a `PeekLocal` snapshot) and updates in place only if the local still holds that same owned map, `remove` reads it last. That is sound only because the key/value expressions cannot update an owned container of this frame in place (`has_no_inline_updates`: no blocks, lambdas or mutating-method calls); widening that predicate needs the same argument. Fallbacks run exactly the generic lowering (`__forge_call_method` / `__forge_method_mut`) and own the fresh result.
- **One shell implementation.** Every builtin that runs a command string goes through `runtime::shell` (`command`/`output`/`succeeds`/`pipe`, `which`, `resolve_program`). Never call `Command::new("/bin/sh")` or `/usr/bin/which` directly; cmd.exe needs `raw_arg` (MSVCRT `.arg()` escaping is not undone by cmd). Feeding a child's stdin must happen on a separate thread from reading its output (`shell::pipe`), or large I/O deadlocks.
- **Shared-state atomicity in squads (interpreter).** Spawned tasks share state captured by closures. `x = x op e` / `x op= e` and the field/index forms (`o.f op= e`, `a[i] = a[i] op e`, nested chains) are one read-modify-write under the scope lock *only* when `e` and the indexes are effect-free (`try_update_ident` / `try_update_place` in `places.rs`). Anything else (e.g. `x = x + f()`, `x = g(x)`) reads and writes under separate locks and can lose updates; use channels or the `squad` result array for cross-task aggregation. Never evaluate Forge code while holding a scope lock.
- **Panics go through `tracing`.** `init_subscriber` installs `install_panic_hook`: an `ERROR` event on `forge.panic`, falling back to the previous hook when that target is filtered out. Keep `forge.panic=error` in `DEFAULT_FILTER`.
- **Interpreter children inherit containment: use `Interpreter::child_context`.** Imports, `timeout` bodies and spawned tasks must be created through it so they carry the run's cancel tokens (`is_cancelled` also checks `ancestor_cancels`, which `squad`/`timeout` push), output capture + budget and `defer_host_runtime`. A bare `Interpreter::new()` escaped sandbox deadlines and started `schedule` threads (SEC-02). Blocking waits go through `wait_cancellable` / `take_task_result`, never a bare `recv()`/`cvar.wait()`.
- **Script-sized allocations go through `semantics::alloc`.** Rust aborts the process on a failed infallible allocation, so `s.repeat(n)`, `collect()` of `n` items or `Vec::with_capacity(n)` with a user-chosen `n` is a one-line host kill. Use `alloc::{repeat_str, padding, int_range, vec_with_capacity, string_with_capacity}` (both engines, same error text). Never cast a negative `i64` straight to `usize`.
- **Open the path the permission check returned.** `stdlib::fs::confine_read/confine_write`, `permissions::checked_path` and `require_import` return the resolved path that was approved under a scoped grant; operate on it, never on the caller's original string (a `cd` from another task or a `file:` URI would otherwise redirect the operation). Native libraries that open files or sockets themselves (SQLite ATTACH/URIs, database drivers, proxies from env) need their own gate — see `docs/SECURITY_AUDIT.md`.
- **Native plugins (`src/plugins`, RFC 0006): the C ABI is the contract.** `crates/forge-plugin/include/forge_plugin.h` is normative; `src/plugins/abi.rs` (host) and `crates/forge-plugin/src/abi.rs` (SDK) mirror it with layout tests. Any layout change is an ABI break: bump `ABI_VERSION` in all three. Rules: arguments are borrowed for one call (strings point into the caller's `Vec<Value>`, arrays/objects into a per-call arena); results are copied out and then always handed back to the plugin's `free_value` (the host never frees plugin memory); the `ffi` check runs on the canonical path *before* `dlopen`; libraries are never unloaded (function values are `Value::BuiltIn("native:<lib>:<fn>")` and may outlive any scope). Both engines route the `native:` prefix to `plugins::call` before any other dispatch. The SDK is a standalone crate (own `[workspace]`), not a dependency of `forge-lang`, so publishing the language crate is unaffected; `tests/native_plugins.rs` builds the example plugins with cargo/cc and diffs both engines.
- **Standalone crates carry their own lockfiles.** `bindings/python/Cargo.lock` (and `examples/plugins/hello_rust/Cargo.lock`) pin `forge-lang` by path, and CI builds them with `--locked`. Any change to the main crate's dependencies makes them stale: run `cargo update -p forge-lang` in `bindings/python` (and `cargo metadata --locked` in each standalone crate) in the same commit.
- **Registry entries are immutable; checksums are mandatory.** A published index line never changes except its `yanked` flag, and the lockfile (`archive_checksum`, `signer`) and the index CI both enforce it. Change the entry format only by bumping `index::INDEX_FORMAT_VERSION` (old clients skip newer lines) and updating `tools/registry-template/scripts/validate_index.py` in the same change (`tests/registry_index.rs` runs it). Never send `GITHUB_TOKEN` outside `client::GITHUB_HOSTS`.
- **Resource limits live in `runtime/limits.rs`.** A `Budget` (fuel, memory, handle slots, imports, sticky fatal trip) is scoped per thread like the permission policy and carried into forked threads by `permissions::inherit` — never start an engine thread without it. Engines charge fuel only at safe points (`Meter::safepoint`): VM `safepoint_countdown`/`fuel_window` (anything that zeroes the countdown early must first shrink `fuel_window` by the remaining countdown, as `PushTimeout` does), interpreter `tick()` per statement/call/loop iteration. Fuel/memory errors are fatal (`VMError::fatal`, `RuntimeError::fatal`): handlers skip them, and the trip is sticky so a builtin that swallows one cannot resume the run. Size caps (`Caps`) must be checked *before* building a value; new amplifying builtins (anything whose output size is a numeric argument) need a cap check on both engines. Subprocesses start through `permissions::begin_subprocess()` (permission + slot); hold the returned `Slot` until the process exits. While fuel is limited the VM stays out of the JIT. The `forge` binary installs `CountingAllocator`; it only counts on threads whose budget limits memory.
- **VM globals are indexed slots (`src/vm/globals.rs`).** A global name's `GlobalId` comes from an interner *domain* (`GlobalNames`, owned `Arc<str>` names, held by `Globals`): **ids are only meaningful within their domain.** `VM::new` starts a fresh domain; only VMs that share ids share it (`fork_for_spawn` via `VM::new_in_domain`, `VmTemplate` freeze/fork for HTTP/WS/MCP-tool forks; imports and REPL steps run in the same VM). So a sandbox run gets a fresh domain, charged to its budget and freed with its VM — never reintroduce a process-wide interner or leaked names (SEC-26: untrusted runs could grow the host without bound). `Chunk::global_id(k, names)` fills `Chunk::global_ids` (shared by clones, not serialized; `add_constant` resets it) on the first `GetGlobal`/`SetGlobal` of constant `k`; each entry is `domain tag << 24 | id` in one `AtomicU64`, so the hot path is an atomic load, a tag compare and `Globals::get_id`/`set_id` (a `Vec<Option<Value>>` index), and a chunk run by a VM of another domain re-resolves instead of using a foreign id. `Globals::index` (name → id, copy-on-write `Arc` shared by forks) holds exactly the defined names and serves name-based access (`get`/`insert`/`keys`/`iter`, JIT guards) without the domain lock; keep the two in sync (globals are never removed). Copy globals by id only within a domain (`try_map_values`, `Clone`), otherwise by name; `values()` are GC roots. `GlobalNames::live_counts` is the leak-test hook (`tests/sandbox_global_names.rs`). Top-level `let`s are *not* globals: the main chunk keeps them in registers and top-level functions capture them as upvalues (`OpenUpvalues` is indexed by register for that reason). The exception is a forward reference: a function compiled before a top-level `let` (or destructuring) of a name it uses reads it as a global (`Compiler::toplevel_refs`), so that `let` then defines the global instead of a register (`let_defines_global`); a write to a not-yet-declared one first reads it (`emit_forward_definedness_check`) so it fails as undefined like the interpreter. A `let` redeclaring a *captured* local of the same scope reuses its register (`redeclared_captured_local`). So the remaining gap between a top-level loop and the same loop in a function is the JIT (main/module chunks never tier up), not variable lookup.
- **VM serving: freeze once, fork linearly.** `vm::serve::VmTemplate` copies the heap reachable from globals/method tables into a slot-indexed list in post-order (children before parents). Cycles can only pass through upvalue cells (value semantics keep every other graph acyclic), so cells are allocated empty first and filled last; freezing errors out on any other cycle. A fork is then one pass with a dense slot → `GcRef` table. If you add an `ObjKind` variant, extend `FrozenObj`/`thaw`/`freeze_object` (the matches are exhaustive on purpose) and `value_to_json`. The VM keeps no parameter names in chunks, so handler argument binding uses `metadata::top_level_fn_params`.
- **VM containment: `scope_cancels` + `wait_cancellable`.** `squad` and `timeout` push a cancel flag on `VM::scope_cancels`; forked tasks copy it with `cancelled`, and `VM::is_cancelled` checks all of them. A deadline sets every flag opened inside its block (`cancel_scopes_from`). Any new blocking wait in the VM must go through `wait_cancellable` (or `wait_task_result` / `receive_cancellable`), never a bare `Condvar::wait` / `recv`. Control errors (`is_unwound_to_handler`, `is_fatal`) must pass through builtins unchanged (see the import builtin).
- **Recursive value walkers enter `recursion::enter_value_level()` per container** (limit `MAX_VALUE_DEPTH`, plus the native stack guard). Add it to any new recursive walk over Forge values; fallible callers report `take_value_too_deep()` (the VM does it in `check_stream_boundary`).
- **Bytecode is verified before it runs (`src/vm/verify.rs`).** `serialize::deserialize_chunk` verifies every chunk (registers inside `max(max_registers,1)`, constant/prototype/upvalue indices, name constants are strings, branch targets in range, back-edges only via `Loop`, last instruction terminal, `min_arity <= arity <= registers`, nesting ≤ `parser::MAX_NESTING`); debug builds also verify every chunk `compile*` returns and panic with `BUG:` on violation. A new opcode must get a rule in `ChunkVerifier::instruction` (the match is exhaustive). The loader checks each length prefix against the remaining bytes before allocating. Compiler branches that do not fit sBx set `branch_overflow` → a `CompileError`, never a silent wrong jump.
- **Fuzzing (`fuzz/`, `tests/fuzz_smoke.rs`).** Target bodies live in `fuzz/src/harness.rs` and are shared with the stable smoke test via `#[path]`. Commit minimized crashers to `fuzz/regressions/<target>/`; the smoke test replays them on every `cargo test`. Durations from user values go through `semantics::{seconds_f64, schedule_interval_secs, timeout_deadline}` (saturating) — `Duration::from_secs_f64`/`Instant + Duration` panic on huge inputs.
- **Threads that run Forge code need a registered stack.** Use `recursion::spawn_worker` (std threads) or `recursion::configure_runtime` (tokio runtimes); an unregistered thread is assumed to have 2 MiB and the guard stops recursion early.
- **Runtime error codes are derived from the message.** `semantics::errors::classify(message)` maps a message to its stable code (`E0000`–`E0041`; `e.type` is `errors::error_type(code)`) via the one table in `src/semantics/errors.rs`; both engines stay plain-string errors. So: raise errors through a helper in `src/semantics/` (both engines call it, so text and code match), keep the headline's stable prefix when rewording (the `helpers_classify_to_their_codes` test pins helper → code), put hints on a `\n  hint: ` line, and never reuse or renumber a code — append. New codes need a fixture in `tests/errors/<CODE>_*.fg` (`tests/error_codes.rs` runs each on both engines and requires the same code and message). `catch e` objects come from `errors::caught_error_fields` on both engines: `message` (without the hint line), `type`, `code` and `hint` (or null).
- **Builtin argument errors are annotated centrally.** `call_builtin` (interpreter) and `call_native` (VM) append `(got Int, String)` to a builtin's own E0015 error via `semantics::errors::annotate_builtin_error`; type names go through `user_type_name` (the VM maps Option ADT objects and frozen values to the interpreter's names). Write builtin argument errors as `name() requires ...` and do not mention the argument types yourself; for the common string/collection/functional/object builtins the text is `semantics::builtin_args::bad_args(name)` on both engines (one table of accepted shapes; the VM maps `get_string_arg` failures of those builtins to it), and `tests/parity/supported/builtin_arg_errors.fg` calls each with wrong kinds on both engines. An interpreter fast path (`places.rs`) that cannot handle a value falls back to the ordinary call instead of raising its own error, so the error is annotated. Both dispatchers also call `builtins_registry::warn_if_deprecated` (a no-op while `DEPRECATED` is empty).
- **VM "did you mean" for locals uses `Chunk::global_hints`.** The VM has no local names at run time, so the compiler records, per `GetGlobal` of an identifier, the closest visible local (`Compiler::visible_name_hint`). The field is debug-only and not serialized (no `.fgc` format change). Suggestions on both engines use `semantics::errors::suggest_name` (innermost scope first, then alphabetical) — never iterate a `HashMap` to pick one.
- **Program diagnostics render through `errors::ProgramDiagnostic`.** Syntax, type and runtime errors in `main.rs` become `ProgramDiagnostic`s and `render()` in the process's `--error-format` (human snippet or one JSON line). Do not print program errors with `format_error`/`format_simple_error` in new CLI paths, or `--error-format json` consumers get unparseable stderr.
- **Interpreter scope cycles are collected, not refcounted (`src/interpreter/heap.rs`).** A closure stored in a scope it captures (`fn f() { f() }`, a lambda bound in a loop body) is an `Arc` cycle. Every `Interpreter` is attached to a `ScopeHeap`; closures must capture through `Interpreter::capture_env()`, which registers the captured scopes as candidates (a new closure-creation site that clones `self.env` directly is a leak). The collector is trial deletion: a scope is reclaimed only if every strong reference to it comes from other garbage, so anything held from outside (a host, a Rust local, a running task, another interpreter) survives. Collections run only when one interpreter is attached (periodic, from `track`) or none (teardown in `Drop for Interpreter`, after it clears its own roots). Children that reach the same scopes must share the heap: `child_context()` (imports, `timeout`, `spawn`) and `fork_for_background_runtime` do; an HTTP fork gets its own heap via `deep_clone_isolated_into`. Per-call hosts (Sandbox, MCP, Python, request forks, sessions) need no explicit teardown: dropping the interpreter is it. Limits: cycles through a channel buffer are not seen (kept alive), and a value that outlives its heap keeps its scopes uncollected after the host drops it. Leak tests: `src/interpreter/leak_tests.rs` (`heap::probe` counts live scopes).
- **Name positions come from the syntax index, not the AST.** Expressions carry no spans (both engines match on `Expr`), so `Parser::with_index` records every name occurrence (role, exact span, scope, enclosing statement) in `parser::index::SyntaxIndex`. When you add syntax that binds or reads a name, record it there (`note_def` / `note_prev`, and `scoped` for new scopes) or the checker reports it as unknown and the LSP cannot rename it. Name resolution over the index is `typechecker::resolve`; it mirrors run-time scoping (a `let` is visible after its statement, function bodies see later definitions of enclosing scopes).
- **The type checker only errs against declared types or engine rules.** Inferred types type results and callbacks but never reject a later use (arrays are heterogeneous, unannotated variables change type; mutable bindings get the join of all assignments from a silent first pass). Builtin signatures in `typechecker/builtins.rs` type results only — they do not reject arguments. Run-time-rule diagnostics call the shared rules (`semantics::binary`, `check_call_arity`, `builtins_registry::check_arity`). `typechecker::corpus_tests` fails on any strict-mode error in `tests/`, `examples/` and parity fixtures not listed (with a reason) in `tests/typecheck_allowlist.txt`.
- **`--strict` runtime checks are AST instrumentation.** `typechecker::enforce::instrument` inserts `__types.check(value, "<canonical type>", "<context>")` calls; `__types` is a hidden stdlib module (registry), so both engines run the same rule (`semantics::types`) with no engine changes. Instrumentation must never change a correct program's result except by the one deliberate conversion: a value checked against a declared `Float` is widened Int → Float (`semantics::types::widen`; parameters whose type mentions Float are reassigned from their check, annotated `let`s and struct fields/defaults declared Float go through `__types.widen`, which never raises). The VM mirrors the rule on its own values (`fits_vm`/`widen_vm`/`widens_vm` in `vm/builtins.rs`) and the JIT lowers a widening check to `ToFloat` (`kind_widens`); the VM's `TypeCheck` fast path (`vm/type_check.rs`) skips the check's call only when the value certainly fits and would not be widened, so its type codes must never accept a value `fits_vm` rejects or `widen_vm` changes; keep the four in sync. `Object<T>` (`RuntimeType::ObjectOf`, checker `Ty::ObjectOf`) constrains an object's values except its internal `__` keys (`types::is_object_value_key`, used by both engines); `Map<K, V>` is only ever the `Map` value, so the checker never lets an object (or `Object<T>`) satisfy it. Argument checks go before the body (an empty body gets an explicit `null` tail), and tails are only wrapped where they are the function's value.
- **The core builds without an OS (`host` feature off, `wasm32-unknown-unknown`).** Host-only code (tokio/axum/reqwest/rusqlite/tungstenite/libloading/registry/LSP/DAP/MCP/CLI) is behind the default `host` feature, gated at module boundaries: `runtime::no_host` stands in for `runtime::{client, host}`, `stdlib::unavailable` registers the OS-bound modules (`http`, `ws`, `db`, `pg`, `mysql`, `os`) with their real member lists (a `host`-build test compares them), and without `host` the default permission policy is deny-all and `PermissionError` reads "… is not available in the browser playground". Crates that depend on `forge-lang` with `default-features = false` must add `features = ["host"]` (bindings/python, fuzz). CI (`playground.yml`) checks `cargo check --lib --no-default-features` natively and for wasm32.
- **Core code reads the clock, sleeps and blocks on channels only through `crate::clock`.** On wasm32 `std::time::Instant::now()`/`SystemTime::now()` and `thread::sleep` panic and `mpsc::recv_timeout` uses the std clock; `clock` re-exports `web-time` there, busy-waits for `sleep`, and makes cancellable waits fail with `WAITS_FOREVER` instead of spinning. Host-only modules may keep `std::time`.
- **Program output goes through `runtime::stdio` (`out`/`out_line`/`err`/`err_line`), never `print!`/`eprintln!`.** That covers `say`/`print`/`yell`/`whisper` on both engines, `io.print`, the GenZ kit, `log.*` and the `color::c*print*!` chrome macros. Without a capture it is exactly `print!`/`eprint!`. A capture is a shared `stdio::Sink` installed per thread (`stdio::scope`; `stdio::capture(limit)` for the browser playground, `Sink::bounded` for `Sandbox::run_contained`: stdout and stderr under one budget) and **inherited by `permissions::inherit`** together with the policy and the budget, so output from spawned tasks on either engine lands in the same capture. A thread started any other way writes to the real stdout: start engine threads with `permissions::spawn` / `recursion::spawn_worker`. While a capture takes stderr (`stdio::captures`), `log.*` writes a `[level] message` line into it instead of a `tracing` event (the subscriber writes to the host's stderr, outside the run's budget) and `color::enabled` is false for captured streams; anything new a program can write must go through `stdio` too, or it escapes the sandbox's output budget.
- **The browser runner uses the shared resource limits.** `bindings/wasm` runs each program under `limits::scope(Budget::new(Limits { max_fuel, max_memory, .. }))` and installs `CountingAllocator`; there is no wasm-specific budget code in the engines. Both engines' `wait_cancellable` return `clock::WAITS_FOREVER` when `!clock::HAS_THREADS` and the first poll is not ready, so a single-threaded wait never spins.

- **Top-level `fn` hoisting is a statement reordering (`semantics::hoist::plan`).** Both engines run a file's/REPL step's top-level statements in `plan(...).order` (interpreter `run`/`run_repl`, VM `compile_top_level`/`compile_repl_with`), and the type checker moves the hoisted definition's `visible_from` (`apply_hoisting`). A definition moves up only past statements that bind none of its free names, and only when its name is bound exactly once and not predefined — so it captures exactly what it did at its written position and no program that ran before changes meaning. The analysis must over-approximate: `collect_binders` (what a statement may bind) and `Free` (what a function may read) match every `Stmt`/`Expr` exhaustively; a new variant that binds names must be added there (when unsure, bind more / read more).
- **One containment point for untrusted code: `Sandbox::run_contained`, for both engines.** Whole programs (`run_source`), `run_forge` calls and session steps, MCP tool calls (a `VmTemplate::fork_in_current_budget` or `fork_for_serving` of the tool file's template) and tool-file loading all run through it: worker thread + policy scope + a fresh resource `Budget` (fuel, memory, handles; `make_state` runs under it, so a template fork is charged to the call; `memory_baseline` carries a session's retained bytes into the next step) + a stdout+stderr `Sink` with the output budget (overflow sets the run's cancel token), deadline/cancel polling, and the run's cancel token set when the job ends (stops unawaited `spawn`s). The engine-side part is `RunScope::contain_interpreter` / `RunScope::contain_vm` (cancel token, `defer_host_runtime`, and for the VM run-wide memory polling): a job must pass every engine instance it runs through one of them. `run_interpreter` is the interpreter wrapper (it hands the interpreter back, which is how sessions persist). A new sandbox limit belongs in `run_contained`, not in one caller or one engine.
- **Sandboxed code runs on the VM by default.** `Sandbox::engine(Engine::Vm | Engine::Interpreter)`, `forge mcp --engine vm|interp`, Python `Sandbox(engine=...)`. Every sandbox, MCP and security test runs on both engines (`ENGINES` in each suite); keep it that way when adding one. Programs the VM cannot run faithfully (`runtime::metadata::vm_incompatibilities`, or an `Unsupported` compile error) run on the interpreter inside the same `run_contained` (`vm::embed::compile_program`). Compile on the worker: compiling reads imported modules under the run's policy. Exceptions: `run_forge` **sessions** always use the interpreter — the VM keeps top-level `let`s in the step chunk's registers/upvalues and only copies them to globals when defined, so state cannot carry over faithfully between separately compiled steps (needs a compiler mode where top-level bindings are globals; the state could then persist as a frozen `VmTemplate`). Fuel counts each engine's own steps (VM instructions are ~2x interpreter steps), so `forge mcp` defaults differ per engine (`ServerConfig::with_engine`). `@tool`/`@param`/`@resource` are pure metadata (`MCP_DECORATORS` in `runtime/metadata.rs`).
- **HTTP requests and responses are engine-neutral (`src/runtime/http_response.rs`, `server::handler_args`).** `read_request` reads at most `MAX_BODY_BYTES` (2 MiB, else 413), refuses a non-UTF-8 body (415, any content type) and decodes it with `http_response::read_body` (JSON → value, 400 if invalid; url-encoded form → object of Strings; anything else → String). `HandlerRequest` holds path params, query, lower-case headers, the decoded `body` and the exact `raw_body`; `handler_args` binds them by parameter name for both engines (`body`/`data`, `raw_body`, `query`/`qs`, `headers`, `req`/`request`, a path param, else a query param) — add a binding there, never in one engine, and extend the `tests/server_engine_parity.rs` corpus. Responses: a handler's value reaches the server as JSON; `http.response(status, body?, headers?)` (`make_response`) builds an object tagged `__type__: "HttpResponse"`, and `ResponseSpec::from_json` validates it when built *and* again in `response_for` (handlers may edit it): status 200–599, no body on 204/304, no CR/LF or invalid header names, no framing headers. Anything else is a 200 JSON body; errors the server answers itself are JSON `{"error": ...}`.
- **Databases are shared per program, transactions per request (`src/stdlib/db_share.rs`).** `db.open`/`pg.connect` make a `Handle` (an `Arc<Shared>`: one mutex, so statements run one at a time, plus the open transaction's owner id) current on the thread. The server snapshots `Databases::current()` after the top level, and each request runs under `enter_request()`: the same connections under a fresh owner, so a transaction it opens is private (other owners wait up to `TX_WAIT`, 30 s, then "database is busy") and one left open is rolled back when the request's `DatabaseScope` drops; `last_insert_rowid` is per owner. `permissions::inherit` carries the parent's handles with the *same* owner into spawned tasks and `schedule`/`watch` threads. `db.open`/`db.close` in a handler change only that thread. A new database module must keep its connection in a `db_share::Handle` and join `Databases`.
- **A module runs once per program, keyed by canonical path.** Interpreter `Interpreter::modules` (`load_module`) and VM `VM::loaded_modules` (`__forge_import_module`) key a module by `fs::canonicalize` of its resolved file: the first import runs it, every later import (from any module, `import "m.fg"` or `import { f }`) binds from that instance (interpreter: its scopes and type tables; VM: its globals under the private prefix `compiler::module_global_prefix`), so importers share its state. Across threads the rule is `runtime::imports::ModuleRegistry`, one per program, shared by both engines' children (`child_context`, `fork_for_background_runtime`; VM `fork_for_spawn` shares `VM::module_registry` and copies `loaded_modules`): the first importer *claims* a module (`Claim::Run`, which also holds its `enter_import` chain entry) and publishes it with `RunClaim::finish`; a concurrent importer waits cancellably (engine `interrupted` callback) and gets the published instance; a dropped claim (error, panic) releases the module so the next import runs it; a cross-thread cycle (task 1 runs a.fg waiting for b.fg, which task 2 runs waiting for a.fg) is found from the registry's wait-for edges and reported as `circular import`, never a deadlock. The interpreter publishes its shared `LoadedModule`; the VM (`vm::module_share`) publishes a `ModuleSnapshot`: `SharedValue` copies of the globals/methods/struct metadata the module's run defined (diffed against a `Baseline`) and the modules it imported (`deps`, recorded through `VM::module_imports`), which another VM installs instead of running the module (value semantics: copies as of the end of the module's top level, like everything else a VM task gets). The snapshot is built only when another VM of the program exists at that moment (registry `strong_count`), since any VM forked later already has the module in `loaded_modules`. HTTP request forks (`fork_for_serving`, `VmTemplate::fork`) start with an empty cache and registry, so a handler that imports a file runs it inside its own fork. Never run a module's top level without a claim from the registry (the VM's fallback for a snapshot without state, unreachable by the argument above, is the one exception).
- **A module exports every name its top level binds, re-exports included.** Functions, variables (destructured too), structs, interfaces, `type` names (a value: the type description, unless a variant has the type's name, `semantics::type_name_is_value`), variants, and the names its own imports bind (`semantics::import_bindings`). On the VM those names live under the module's global prefix (`compiler::module_top_level_names`; a bare `import "x"` inside a module is parsed when the module is compiled so its names get the prefix too, `Compiler::wildcard_imports`); structs and interfaces are registered under their own name, so `VM::module_export` looks there second. A name that a module's top level binds but that is missing from `module_top_level_names` leaks into the program's globals unprefixed and cannot be imported from the module on the VM. The checker follows a selective import of a re-export to its definition (`typechecker::reexported_declarations`). A bare `import "m.fg"` binds only the module's own definitions on both engines.
- **`import { a as b }`: one rule, three places.** `ImportName { name, alias }` in the AST; `semantics::check_import_alias` allows renaming functions and variables only (a struct, `type`, variant or interface keeps its name: values, methods and patterns refer to it) and gives the text for the engines' E0019 and the checker's T0021 (reported at the alias, `typechecker::import_rename_diagnostics`). The syntax index records the alias as the definition and the module's name as a `Field` reference, so an LSP rename of the module's function follows. Names inside an imported module's bodies resolve in that module (`typechecker::infer` `ForeignModule`), not in the importer.
- **`import "m.fg" as m` builds one object from one list.** `Stmt::ImportNamespace`; both engines build the namespace from `semantics::namespace_members` (the module's exports: top-level `fn`/`let`/destructured names/`type` names/variants/interfaces and the names its imports bind as values, structs as the VM's struct marker `{__kind__: "struct", name}`) plus `__module__` (path as written), so it prints and compares alike; the interpreter binds it immutable, the VM through `__forge_import_namespace` (module loading shared with `__forge_import_module` via `import_module_once`). Anything that recognises a namespace keys on `__module__`: a missing member is `semantics::missing_member`/`no_module_member` (both engines' field read and method call; no builtin-method fallback). `m.S { }` is `Expr::StructInit { namespace: Some(m) }` and `m.V(x)` in a pattern is `Pattern::Constructor` — the parser produces both only after it parsed `import ... as m` (`Parser::namespaces`), so `a.B { x: 1 }` elsewhere keeps its meaning. The syntax index links `m.f`'s field occurrence to its receiver (`SyntaxIndex::receivers`); the checker keeps the module's functions in `NamespaceInfo` (not in `functions`), and the LSP resolves `Resolved::Member` through the receiver's `DefKind::Namespace`.
- **`io.args()` is the program's command line, never the host's (`runtime::program_args`).** `forge run file.fg args...` (clap `trailing_var_arg`) sets it once for the process (`set_process`: the script path, then the arguments; flags after the file belong to the program), the standalone entry points (`forge_execute_*`) set it from their own argv, and a `Sandbox` scopes `Sandbox::args` (default empty) on its worker (`scope`), which `permissions::inherit` carries into spawned tasks. `io.args*` read only that, so they need no capability. Never read `std::env::args()` from code a program can reach.
- **Escaped braces in string tokens are noncharacters (`lexer::LITERAL_LBRACE`/`LITERAL_RBRACE`, U+FDD0/U+FDD1).** `\{`, `\}`, `\u{7B}`, `\u{7D}` must not start an interpolation, but the parser splits `{expr}` out of a `Token::StringLit`'s *value*, so the lexer marks escaped braces and the parser turns them back with `lexer::literal_braces` (interpolation literal parts, plain strings, patterns, import paths, decorator strings). Any new consumer of a `Token::StringLit` value must call it too. A literal cannot contain the two noncharacters (lex error). Escapes are decoded by one function, `lexer::decode_escape`, which `escape_source_len` reuses for the syntax index's value-to-column map.

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
