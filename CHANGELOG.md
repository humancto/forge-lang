# Changelog

All notable changes to Forge will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

---

## [0.5.0] - 2026-04-10

### Added

- **`db.begin` / `db.commit` / `db.rollback`** — explicit transaction control for the SQLite module, sharing the existing thread-local connection.
- **`pg.begin` / `pg.commit` / `pg.rollback`** — same trio for the PostgreSQL module, backed by `client.batch_execute`.
- **Opt-in filesystem confinement** — setting `FORGE_FS_BASE=/path` confines every `fs.*` operation that touches a path to that subtree (with symlink resolution). Pure path manipulation helpers (`dirname`, `basename`, `ext`, `join_path`, `temp_dir`) are exempt; `exists`/`is_dir`/`is_file` return `false` instead of erroring on confinement failure so script branches still work.
- **VM source-line stack traces** — `VMError` now carries real `(function, line)` frames populated from the bytecode line table, and the CLI prints them via the `Display` impl rather than dropping them on the floor. Makes `--vm` errors actionable.
- **69 new unit tests** for `crypto`, `regex`, `json`, and `time` stdlib modules — these had **zero** prior coverage despite living on the security-critical / format-correctness paths. Includes RFC 4231 HMAC-SHA256 vector, century-rule leap year cases, and JSON deep-merge round trip.
- **`PRODUCTION_READINESS.md`** — internal punch list tracking all v0.4.3+ hardening work.

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
