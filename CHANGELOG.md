# Changelog

All notable changes to Forge will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Native Option<T> values** — `Some(x)` and `None` are now first-class `Value::Some`/`Value::None` variants instead of ADT object wrappers. Pattern matching (`match Some(42) { Some(v) => v, None => 0 }`), `unwrap()`, `unwrap_or()`, `is_some()`, `is_none()` all work natively. Nested options supported.
- **Task handles from spawn** — `spawn { ... }` now returns a `TaskHandle` when used as an expression (`let h = spawn { return 42 }`). Fire-and-forget usage (`spawn { ... }` as a statement) remains backward compatible.
- **Await on task handles** — `await h` blocks until the spawned task completes and returns its value. `await` on non-handle values passes through unchanged.
- **35 new tests** — 21 for native Option<T>, 14 for spawn/await task handles, 4 for type checker Option inference. Total: 441 Rust + 26 Forge tests.

### Changed

- `Some()` builtin returns `Value::Some(Box<Value>)` instead of an ADT object with `__type__`/`__variant__` metadata
- `None` in prelude is `Value::None` instead of an ADT object
- `Expr::Spawn` added to AST — spawn is now usable as an expression, not just a statement
- `Expr::Await` rewritten to poll `TaskHandle` result slots (tokio `block_in_place` or spin-wait fallback)
- Type checker infers `Option<T>` for `Some(x)` calls and `None` identifiers

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
