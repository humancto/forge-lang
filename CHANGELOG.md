# Changelog

All notable changes to Forge will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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

### Fixed

- **HTTP server no longer single-threaded** — the server previously wrapped the entire interpreter in `Arc<Mutex<Interpreter>>` and held the lock for the full handler body, so throughput on any non-trivial handler collapsed to ~10 req/sec regardless of CPU count and p99 latency exploded under concurrency. Replaced with a per-request fork model: each request gets a fresh interpreter forked from a read-only `InterpreterTemplate` and runs on `tokio::task::spawn_blocking`. Empirical impact on a 16-core machine, `/cpu` handler ~96ms: throughput at C=16 went from 9.8 → 34.6 req/sec (3.5×); at C=100 from 10.0 to 38.4 req/sec with p99 latency dropping from 18.8s → 2.9s (6.5× p99). Also adds a backpressure semaphore (default 512 in-flight, returns 503 with `Retry-After` when saturated), client-disconnect cancellation via Drop guard, panic capture with no payload leak, graceful shutdown on SIGINT/SIGTERM, and a ratio-based regression test in `tests/server_concurrency.rs`. **Behavior change:** handler mutations to top-level globals no longer persist across requests; handlers no longer see writes from concurrent `schedule`/`watch` blocks. The previous "persistence" was a race condition the global mutex hid. A future `shared { }` block will provide explicit cross-request state.

- **`fork_for_background_runtime` shared scope storage by Arc** — the existing fork primitive used `env.clone()` (shallow over `Vec<Arc<Mutex<HashMap>>>`), so `schedule`/`watch` blocks shared scope locks with the parent interpreter. Latent because background tasks did not run concurrently with the foreground or each other. Switched to `env.deep_clone()`, the same primitive `spawn_task` already uses for squad blocks.

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
