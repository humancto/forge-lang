# CLAUDE.md — Forge Language Project Context

## What Is This?

Forge is an internet-native programming language built in Rust. ~26,000 lines. Dual syntax (classic + natural language). Built-in HTTP, database, crypto, AI, CSV, terminal UI, shell integration, NPC fake data, GenZ debug kit, and 30 interactive tutorials.

## Architecture

```
Source (.fg) → Lexer → Parser → AST → Type Checker → VM / Interpreter → Result
                                                         ↓
                                                  Runtime Bridge
                                              (axum, reqwest, tokio, rusqlite)
```

The bytecode VM is the default engine. A tree-walking interpreter (`--interp` flag) is available for full feature coverage (decorator-driven HTTP servers auto-fallback). A JIT compiler (`--jit`) is available for maximum performance on numeric workloads.

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
| Destructure | `let {a, b} = obj` | `unpack {a, b} from obj`    |
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

## CLI Commands (16)

run, repl, version, fmt, test, new, build, install, publish, lsp, dap, learn, chat, doc, help, -e

## Standard Library (20 modules, 250+ functions)

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
- Enum methods: `impl MyType { fn foo(it, ...) { ... } }` attaches instance methods to algebraic `type` definitions; dispatch walks through the ADT value's `__type__` field into the method table. Supports method bodies with `match it { Variant(f) => ... }`, returning new ADT instances, chained calls, and dispatch via collection lambdas. Known gaps: `TypeName.method()` static dispatch on algebraic types is not resolved today (works only for `struct`), and a preexisting interpreter bug in `match_pattern` rejects deeply recursive ADT matches when outer-frame bindings collide with field pattern names (the VM is unaffected).
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
cargo test           # 948+ Rust tests pass
forge test           # integration tests pass (run after cargo build)
forge test --coverage # with line coverage report
```

All 18 example files run successfully.

## Known Limitations (v0.8.0)

- All three database modules (db, pg, mysql) now support parameterized queries — always use them for user input
- The VM is the default engine; programs using decorator-driven HTTP servers (`@server`, `@get`, etc.) auto-fallback to the interpreter
- Use `--interp` for full feature coverage, `--jit` for maximum numeric performance
- `forge build --aot` embeds bytecode in a native binary but still requires the Forge runtime at execution time
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
9. **VM parity is your responsibility.** When adding or fixing a builtin in the interpreter, port the same fix to `src/vm/builtins.rs`. The VM is not automatically in sync.

### After Every Change

10. **Run `cargo test`.** If tests fail, fix before committing.
11. **Run the examples.** `forge run examples/hello.fg` and `forge run examples/functional.fg` must pass.
12. **Check for regressions.** If you changed the VM, test with `--vm`. If you changed the JIT, test with `--jit`.
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

- Handlers must be **pure functions of `(request) → response`**.
  Top-level mutations made during a request do not persist to the
  template or other requests.
- A handler that reads a top-level variable mutated by a
  `schedule`/`watch` block will read the **template snapshot value**,
  not the schedule's writes. Future `shared { }` blocks will provide
  explicit cross-request state.
- A handler that captures outer state through a closure (`fn outer() {
  let mut count = 0; @get fn ... }` style) **shares** that captured
  state across concurrent requests via `Arc<Mutex>`. Writes race.
  Don't do this; use a `shared {}` block when it lands.
- WebSocket handlers fork **once per connection**, not per message.
  Connection-scoped state is held in a `parking_lot::Mutex`. Different
  WS connections are fully isolated.
- Large top-level state (`let huge = read_file("100mb.json")`) is
  copied on every request fork. `Value::String` is `String`, not
  `Arc<str>`. Keep top-level data small or load it lazily inside the
  handler.

**Authoring fork primitives:**

- Always use `env.deep_clone()`, never `env.clone()`. `Environment` is
  `Vec<Arc<Mutex<HashMap>>>` — derived `Clone` is shallow and shares
  scope storage. Concurrent forks that share scope `Arc`s would
  silently serialize on the per-scope `Mutex`, defeating the whole
  goal of per-request isolation.

### Learnings (Append Here)

- **`fork_*` env must `deep_clone`, never plain `.clone()`.** `Environment` is `Vec<Arc<std::sync::Mutex<HashMap<String, Value>>>>`, so a derived `Clone` bumps `Arc` refcounts but shares scope storage. Two concurrently-forked interpreters would then serialize on per-scope mutexes — invisible until you actually call the fork concurrently. The HTTP server's per-request fork (`fork_for_serving`) and the schedule/watch fork (`fork_for_background_runtime`) both depend on this. `spawn_task` got it right from day one; the other two were latent until the server fix.
- **JIT jump offsets:** The VM pre-increments IP before applying jump offsets. JIT target = `ip + 1 + sbx`, not `ip + sbx`. This caused fib(30) to return wrong values.
- **Builtin shadowing:** Registering a `BuiltIn("time")` after a `time` module object shadows the module. Register modules last, or remove the simple builtin.
- **Value PartialEq:** The interpreter's `Value` enum needs a manual `PartialEq` impl because `Function`/`Lambda` variants contain non-comparable closures. Never derive it.
- **GitHub Actions runners:** `macos-13` is deprecated. Use `macos-latest` for both ARM and x86_64 targets.
- **Bytecode encoding:** Instructions are 32-bit. Format: `[op:8][a:8][b:8][c:8]` or `[op:8][a:8][bx:16]` or `[op:8][a:8][sbx:16]`. The `sbx` field is signed 16-bit stored as unsigned.
- **Constant dedup:** `Chunk::add_constant()` deduplicates via `identical()`. Don't add the same constant twice — it wastes the constant pool.
- **VM-interpreter parity is not automatic.** The two share no code. Every interpreter builtin fix must be manually ported to `src/vm/builtins.rs`. Known audit-tracked gaps: `sort()` string support, `split("")` char-splitting, `int(bool)`, `keys({})`, `is_some`/`is_none` — all fixed in March 2026.
- **`sort()` with custom comparator:** `sort_by` closure borrows `self` immutably but calling `self.call_value()` needs `&mut self`. Work around by collecting items first (releasing the `gc` borrow), then sort with `call_value` on cloned items.
- **GC borrow in closures:** Never call `self.alloc_string()` or `self.call_value()` inside a closure that still holds `self.gc.get()`. Always collect into a `Vec<String>` or `Vec<Value>` first to drop the GC borrow.
- **VM `TryCatch`:** The compiler currently drops the catch block (logs as TODO, M1.2.2). Until it's implemented, `--vm` mode does not catch runtime errors.
- **VM `Destructure`:** Similarly dropped by the compiler (M1.2.1). Any `let {a, b} = obj` in `--vm` mode is silently skipped.

## Module Dependency Map

```
main.rs → lexer, parser, interpreter, vm, runtime, errors, typechecker, ...
vm/mod.rs → compiler, machine, bytecode, frame, gc, green, jit, value
vm/machine.rs → bytecode, frame, gc, value (2483 lines — largest VM file)
vm/compiler.rs → bytecode, parser::ast (927 lines)
vm/jit/ir_builder.rs → bytecode, cranelift (276 lines)
vm/jit/jit_module.rs → ir_builder (47 lines)
interpreter/mod.rs → parser::ast, runtime, stdlib (8153 lines — largest file)
runtime/server.rs → interpreter, parser::ast, axum (354 lines)
runtime/client.rs → reqwest (100 lines)
```
