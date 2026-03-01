# Changelog

All notable changes to Forge will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- **Interactive tutorial system** with 14 lessons
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
