# Forge Roadmap — Post v0.4.3

Written: 2026-04-10
Baseline: v0.4.3, 37k LOC, 805 cargo tests, production-readiness hardening merged.

---

## Phase 0 — Cleanup + Release (v0.5.0)

**Goal:** Close the 4 follow-up items from the production-readiness review, then cut a release.

### ~~0.1 `crypto::random_bytes` — real CSPRNG~~ ✅ DONE (merged in PR #4)

Already uses `getrandom::getrandom()`. No further work needed.

### ~~0.2 JWT key-confusion defence~~ ✅ DONE (PR #5)

Implemented algorithm pinning via optional third argument `{ algorithm: "RS256" }`.
Header mismatch fails with a clear error. 6 tests including the actual attack vector
(HS256 + RSA pubkey as HMAC secret → succeeds without pin, fails with RS256 pin).
Deprecation warning for unpinned calls deferred — current behaviour (trust header) is
preserved for back-compat; pinning is opt-in.

### ~~0.3 `http.download` / `http.crawl` — thread user options~~ ✅ DONE (PR #5)

Both now accept `timeout`, `max_redirects`, `max_bytes` via options object.
Shared `parse_http_opts` helper. Flexible arg parsing for download:
`(url)`, `(url, dest)`, `(url, opts)`, `(url, dest, opts)`.

### ~~0.4 `drop(&obj)` compiler warning~~ ✅ DONE (merged in PR #4)

Already changed to `let _ = obj;`. No further work needed.

### 0.5 Cut v0.5.0

- Bump `Cargo.toml` version to `0.5.0`
- Move `[Unreleased]` CHANGELOG block to `## [0.5.0] - 2026-04-XX`
- `cargo clean -p forge-lang && cargo build && cargo test`
- Tag `v0.5.0`, push tag
- Update Homebrew formula (SHA256 from release asset)
- Update landing page version string in `docs/index.html`

---

## Phase 1 — Developer Experience

**Goal:** Make Forge pleasant to write real programs in. The LSP is the highest-leverage item because every editor user benefits without changing the language.

### ~~1.1 LSP: go-to-definition~~ ✅ DONE (PR #7)

Deep go-to-definition finds top-level symbols, function params, local variables, for-loop
vars, catch vars, and impl block methods via `collect_all_symbols`.

### ~~1.2 LSP: find-references~~ ✅ DONE (PR #7)

`textDocument/references` with word-boundary matching. Single-file for now.

### ~~1.3 LSP: real diagnostics~~ ✅ DONE (PR #8)

Type checker wired into `get_diagnostics` — runs on every didOpen/didChange. Surfaces
type mismatches, arity errors, return type mismatches as warnings with line numbers.
Source tagged as `forge-typecheck`.

### ~~1.4 LSP: hover + signature help~~ ✅ DONE (PR #7)

Hover shows full signatures for user-defined functions, variables (with mutability/type),
structs (with fields), types (with variants), interfaces (with methods). Recursively
walks into function bodies and impl blocks. Context-aware module completions filter
to the specific module typed before the dot.

### ~~1.5 Error messages with source spans~~ ✅ DONE

Changed all `Vec<Stmt>` body fields in the AST to `Vec<SpannedStmt>` with line+col.
Parser now wraps every inner statement with position from the token stream.
`exec_stmts` patches `RuntimeError.line` from `SpannedStmt` on bubble-up.
Runtime errors now show exact source line with ariadne snippets and carets.
`RuntimeError` gained `col` field (ready for expression-level spans in future).
8 files changed across parser, interpreter, VM compiler, type checker, LSP.

### ~~1.6 `forge fmt` — paren continuation~~ ✅ DONE (PR #9)

Added parenthesis depth tracking alongside existing brace/bracket tracking.
Multi-line function calls now auto-indent correctly.

### ~~1.7 `forge doc` — variable extraction + comments~~ ✅ DONE (PR #9)

Fixed `let`/`let mut` declarations being silently skipped in doc output.
Implemented `extract_preceding_comments` using `SpannedStmt.line` to capture
`//` comments above declarations. Removed unused import.

### ~~1.8 REPL improvements~~ ✅ DONE (PR #9)

- Syntax highlighting: keywords (magenta), builtins (blue), modules (green),
  strings (yellow), numbers (cyan), comments (dim)
- Live tab completion from interpreter environment (user-defined vars/fns)
- `env` command shows all defined variables instead of just `_last`
- Added `Environment::all_names()` for tab completion support

### Order of attack

1. 1.5 (spans) — foundation for everything else
2. 1.1 + 1.3 (go-to-def + diagnostics) — the two most impactful LSP features
3. 1.2 + 1.4 (find-refs + hover) — build on the symbol table from 1.1
4. 1.6, 1.7, 1.8 (fmt, doc, REPL) — polish

---

## Phase 2 — Package Ecosystem

**Goal:** Enable multi-file, multi-author Forge projects. This is what turns Forge from a scripting tool into a language people build real things with.

### ~~2.1 `forge.toml` project manifest~~ ✅ DONE (pre-existing)

Already implemented in `src/manifest.rs`: `Manifest`, `ProjectConfig`, `DependencySpec`
(version/git/path/branch), `TestConfig`, `Lockfile`, `LockedPackage`. 10 tests.
`forge run` now reads `entry` from `forge.toml` when no file argument is given.

### ~~2.2 Module resolution across packages~~ ✅ DONE

`resolve_import_from()` resolves relative to the importing file's directory first,
then falls back to CWD-relative, `forge_modules/`, and `.forge/packages/`. The interpreter
tracks `source_file` and passes the base directory to the resolver. Wildcard imports
now copy struct defs, type defs, and impl block methods alongside functions and variables.

### ~~2.3 `forge install <pkg>`~~ ✅ DONE (pre-existing)

Already implemented in `src/package.rs`: `install()` handles git URLs, local paths,
and registry sources. `install_from_manifest()` reads `forge.toml` dependencies,
installs to `forge_modules/`, manages `forge.lock`. 4 tests.

### ~~2.4 `forge publish`~~ ✅ DONE (PR #16)

Local filesystem registry: `forge publish` packages the project and copies to
`~/.forge/registry/<name>/<version>/`. SHA-256 checksums, symlink protection,
manifest validation, `--dry-run` and `--registry` flags. Integrates with
existing `forge install` via `default_registry_roots()`.

### Order of attack

1. 2.1 (manifest) — define the format, parse it in `forge build`/`forge run`
2. 2.2 (resolution) — make imports work across package boundaries
3. 2.3 (install) — git-based first, registry later
4. 2.4 (publish) — only after the registry exists

---

## Phase 3 — VM Parity

**Goal:** Make `--vm` capable enough to be the default execution engine. This unlocks the performance story (and eventually the JIT story).

### ~~3.1 Async runtime in VM~~ ✅ DONE (PR #14)

Real threading for spawn/await in `--vm` mode. Spawn launches an OS thread with
a forked VM (`SendableVM` + `fork_for_spawn`), await blocks via `Condvar`.
Cross-thread values use `SharedValue` enum (no GcRef leaks). `ObjKind::TaskHandle`

- `OpCode::Await` added. Upvalue capture in spawn closures. 13 tests.
  `schedule`/`watch` deferred to 3.5.

### ~~3.2 `try-catch` in compiler~~ ✅ DONE (pre-existing)

Already implemented — compiler emits TryCatch/EndTry opcodes, machine handles error recovery.

### ~~3.3 `destructure` in compiler~~ ✅ DONE (pre-existing)

Already implemented — both object and array destructuring compile to GetField/GetIndex ops.

### ~~3.4 Remaining stdlib in VM~~ ✅ DONE

Added 4 missing module namespaces (npc, url, toml, ws) and 43 standalone builtins.
Collections: first, last, zip, flatten, chunk, slice, compact, partition, group_by,
sort_by, for_each, take_n, skip, frequencies, sample, shuffle.
Strings: typeof, substring, index_of, last_index_of, capitalize, title, upper, lower,
trim, pad_start, pad_end, repeat_str, count, slugify, snake_case, camel_case.
Plus GenZ debug kit (sus, bruh, bet, no_cap, ick) and execution helpers (cook, yolo, ghost, slay).

### ~~3.5 `schedule` / `watch` in VM~~ ✅ DONE (PR #15)

Both compile to dedicated opcodes (`Schedule`, `Watch`) and spawn background
threads using `fork_for_spawn` + `SendableVM`. Schedule supports seconds/minutes/hours
units with interval validation. Watch polls file mtime at 1s intervals. Upvalue
capture in closures. 9 tests.

### ~~3.6 JIT expansion~~ ✅ DONE (PR #13)

Fixed And/Or from bitwise to logical semantics (result is 0/1, not the raw operand value).
Extended JIT dispatch from max 3 arguments to 8 for both integer and float functions.
24 new tests covering logical operators, multi-arg functions, float operations, recursive
algorithms (fib(30), GCD, Collatz), and comparison operators. String operations and
inter-function calls deferred — requires NaN-boxing runtime (Milestone 2).

### Order of attack

1. 3.2, 3.3 (try-catch, destructure) — unblock basic programs
2. 3.4 (stdlib) — make the VM useful for real scripts
3. 3.1 (async) — the hard one, unlocks everything else
4. 3.5 (schedule/watch) — depends on 3.1
5. 3.6 (JIT expansion) — performance polish

---

## How to resume

Each phase is independent. When picking up work:

1. Read this file to remember what's next
2. Check the CHANGELOG `[Unreleased]` section for what's already landed
3. Run `cargo test` to confirm baseline
4. Work through items in the listed order within each phase
5. After each item: `cargo test`, atomic commit, update CHANGELOG
6. After each phase: cut a release

Current status: **Phase 7 in progress — hardening for v1.0.**

---

## Phase 4 — Beyond v0.6.0

**Goal:** Expand Forge from a capable scripting language into a production-grade development platform.

### ~~4.1 VM as default engine~~ ✅ DONE (PR #17)

VM is now the default execution engine. Added Must/Ask/Freeze opcodes. `--interp` flag
for fallback. Auto-detects VM-incompatible features (decorators) and falls back gracefully.

### ~~4.2 Cross-file LSP~~ ✅ DONE (PR #18)

Go-to-definition and find-references now follow imports across files. Uses
`resolve_import_from` for resolution. Searches imported files and sibling .fg files.

### ~~4.3 `forge test` coverage~~ ✅ DONE (PR #19)

`forge test --coverage` reports line coverage per file with color-coded percentages.
Interpreter tracks executed lines via HashSet. Executable line set properly handles
multi-line comments and intersects with executed set for accurate counts.

### ~~4.4 Native compilation (AOT)~~ ✅ DONE (PR #20)

`forge build --aot` compiles source to bytecode and embeds it in a native C launcher.
Unlike `--native` (raw source), `--aot` provides no source exposure and faster startup.
Mutually exclusive flags via clap. Full standalone binary (no forge runtime) deferred
to NaN-boxing milestone.

### ~~4.5 Debugger (DAP)~~ ✅ DONE (PR #21)

`forge dap` starts a DAP server over stdio. Supports breakpoints, step over/in/out,
continue, pause, variable inspection, call stack traces. Output captured via sink
to prevent stdout corruption. Thread-safe shared seq counter.

### Order of attack

1. 4.1 (VM default) — validates Phase 3 completeness, biggest UX win
2. 4.2 (cross-file LSP) — highest developer experience impact
3. 4.3 (coverage) — low effort, high signal for adoption
4. 4.4 (AOT) — ambitious, depends on 4.1
5. 4.5 (debugger) — nice to have, can be done anytime

---

## Phase 5 — v0.7.1 Expert Review Fixes

**Goal:** Resolve critical and high-priority issues identified by 5 parallel expert reviews of the v0.7.0 codebase. Full report: `.planning/v0.7.0-expert-review-report.md`

### Phase 5A — Critical Fixes (blocking for production)

- [x] 5A.1 Replace `transmute(op)` with safe `TryFrom<u8>` in VM dispatch (`machine.rs:1061`, `type_analysis.rs:60`, `ir_builder.rs:88`) — eliminates UB on invalid opcodes (PR #22)
- [x] 5A.2 Add `method_tables`, `static_methods`, `struct_defaults` to GC root scanning (`machine.rs:1940-1957`) — fixes use-after-free risk (PR #23)
- [x] 5A.3 Fix DAP stdin reader to use single `BufReader<Stdin>` (`dap/mod.rs:20,35,39`) — fixes message corruption under pipelining (PR #24)
- [x] 5A.4 Add coverage tracking in interpreter `run()` method (`interpreter/mod.rs:656-676`) — fixes systematically deflated coverage (PR #25)

### Phase 5B — High Priority

- [x] 5B.1 Add `debug_assert!` on `SendableVM` jit_cache emptiness (`machine.rs:15`) (PR #26)
- [x] 5B.2 Hoist `Arc<Chunk>` reference outside VM dispatch loop (`machine.rs:1025`) (PR #27)
- [x] 5B.3 Lazy register allocation or reduced `MAX_REGISTERS` (`machine.rs:295`) (PR #28)
- [x] 5B.4 Add internal timing to benchmarks + fix array benchmark fairness (`benchmarks/`) (PR #29)
- [x] 5B.5 Add Rust/Go/Node.js fib(30) benchmark files for landing page verification (`benchmarks/`) (PR #30)

### Phase 5C — Medium Priority

- [x] 5C.1 Fix `len()` to return char count not byte count for VM parity (`machine.rs:1585`, `builtins.rs:415`) (PR #31)
- [x] 5C.2 Add Object equality to VM `GcObject::equals` (`value.rs:268-276`) (PR #32)
- [x] 5C.3 Use `getenv("TMPDIR")` in AOT generated C code (`native.rs:146,266`) (PR #33)
- [x] 5C.4 Deduplicate AOT/native build functions and C templates (`native.rs:7-366`) (PR #34)
- [x] 5C.5 Improve executable-line heuristic for coverage (`testing/mod.rs:297-325`) (PR #35)
- [x] 5C.6 Key DAP breakpoints by source file (`dap/mod.rs:136-152`) (PR #36)
- [x] 5C.7 Register overflow bounds check in compiler (`compiler.rs:97-104`) (PR #37)

### Order of attack

1. 5A.1 + 5A.2 (VM critical) — most important, eliminates UB and use-after-free
2. 5A.3 (DAP fix) — prevents user-facing crashes
3. 5A.4 (coverage fix) — quick win, accurate numbers
4. 5B.1-5B.3 (VM performance/safety) — polish default engine
5. 5B.4-5B.5 (benchmarks) — defensible landing page claims
6. 5C.\* (medium priority) — parity, AOT, coverage polish

---

## Phase 6 — v0.7.2 Audit Follow-Up

**Goal:** Fix critical bugs, eliminate memory leaks, reduce build bloat, and pay down the highest-impact technical debt identified by the v0.7.1 full-codebase expert audit.

### Phase 6A — Critical Bugs

- [x] 6A.1 Fix interpreter `len()` byte-vs-char parity bug — `interpreter/builtins.rs:28` uses `s.len()` (byte count) while VM uses `s.chars().count()` (char count). Non-ASCII strings return different values between backends.
- [x] 6A.2 Fix `mem::forget(jit)` unbounded memory leak — `machine.rs:2038` leaks every JIT-compiled module. Store JIT modules in a managed `Vec<JitCompiler>` on the VM so they drop with it.
- [x] 6A.3 Fix And/Or short-circuit evaluation in VM — `machine.rs:1176-1184` evaluates both operands eagerly. Compile `&&`/`||` to `JumpIfFalse`/`JumpIfTrue` + conditional right-operand evaluation. Current behavior diverges from interpreter for side-effectful expressions.

### Phase 6B — Build & Binary Size

- [x] 6B.1 Feature-gate Cranelift JIT behind `jit` cargo feature — 5 cranelift crates add significant compile time. Default on, disable with `--no-default-features`. (PR #41)
- [x] 6B.2 Feature-gate PostgreSQL behind `postgres` cargo feature — `tokio-postgres`, `tokio-postgres-rustls`, `rustls`, `webpki-roots`. (PR #42)
- [x] 6B.3 Feature-gate MySQL behind `mysql` cargo feature — `mysql_async`. (PR #43)
- [x] 6B.4 Trim tokio features from `"full"` to `["macros", "rt", "rt-multi-thread", "sync", "net", "time", "io-util"]`. (PR #44)

### Phase 6C — Code Health

- [x] 6C.1 Split `interpreter/mod.rs` (7,907→3,239 lines) — extracted 359 test functions to `interpreter/tests.rs`. (PR #45)
- [x] 6C.2 Extract VM tests from `vm/mod.rs` (2,058→50 lines) — 5 test files: parity_tests, async_tests, jit_tests, schedule_watch_tests, must_ask_freeze_tests. (PR #46)
- [x] 6C.3 Make VM `Value` implement `Copy` — removed 51 unnecessary `.clone()` calls in dispatch hot path. (PR #47)
- [x] 6C.4 Remove dead NativeFn func field and native_dispatch placeholder — string dispatch retained (enum conversion deferred as low ROI). (PR #48)

### Order of attack

1. 6A.1 (len parity) — 5-minute fix, live correctness bug
2. 6A.2 (JIT leak) — unbounded memory leak in default engine
3. 6A.3 (short-circuit) — correctness divergence between backends
4. 6B.1-6B.4 (feature gates) — faster builds, smaller binaries
5. 6C.1-6C.2 (file splits) — maintainability
6. 6C.3-6C.4 (VM performance) — hot path optimization

---

## Phase 7 — Hardening for v1.0

**Goal:** Fix every correctness bug, security hole, and code quality issue that would embarrass a production language. Make Forge brutally good — zero panics in production code, zero compiler warnings, zero injection vectors.

### Phase 7A — Critical Safety Fixes

- [x] 7A.1 Fix `to_json_string()` injection — added `escape_json_string()` helper, fixed both interpreter and VM. (PR #49)
- [x] 7A.2 Fix `unsafe impl Send for SendableVM` — promoted debug_assert to assert, enforced in release builds. (PR #50)
- [x] 7A.3 GC root scanning verified correct — `frames.last()` always gives the highest base since frames stack monotonically (+256 each). Not a bug.
- [x] 7A.4 Parser panics verified test-only — all 14 panics are in `#[cfg(test)]` module, zero in production parser code.
- [x] 7A.5 Fix `ask` keyword JSON injection — replaced manual escaping with `serde_json::json!()`. (PR #49)

### Phase 7B — Dead Code & Warning Elimination

- [x] 7B.1 Eliminate all compiler warnings — 19 warnings in release build: unused imports, unused variables. A production language must compile warning-free. (PR #51)
- [x] 7B.2 Audit and remove production `panic!` calls — 92 panics in non-test code. Convert to proper error returns. Target: zero panics reachable from user input. (PR #52)
- [x] 7B.3 Fix `serialize.rs` 38 `unwrap()` calls — all 38 are in `#[cfg(test)]` only; production code already uses `Result` with `?`. No changes needed.
- [x] 7B.4 Fix `npc` module 11 `panic!` calls — all 11 are in `#[cfg(test)]` only. No production panics. No changes needed.
- [x] 7B.5 Fix `time` module 12 `panic!` calls — all 12 are in `#[cfg(test)]` only. No production panics. No changes needed.
- [x] 7B.6 Remove dead interpreter fallback paths — audit found zero dead paths; interpreter is fully utilized (--interp flag, HTTP server fallback). No changes needed.

### Phase 7C — Performance & Simplification

- [x] 7C.1 Unify async runtime — `stdlib/http.rs:407` creates a new Tokio runtime per HTTP call. Unify on the pg/mysql pattern: `Handle::try_current()` with `block_in_place`, fallback to thread-local runtime. (PR #53)
- [x] 7C.2 Fix string `.len` property inconsistency — `machine.rs` GetField returns `s.len()` (bytes) but `Len` opcode returns `s.chars().count()` (chars). Must agree. (PR #54)
- [x] 7C.3 Variable-width VM frames — 256 registers per frame is wasteful (a 3-arg function wastes 253 slots). Use compiler's `max_register` for tighter allocation. (PR #55)
- [x] 7C.4 Remove dispatch loop closure wrapper — assessed: closure is inlined by LLVM, only 3 early returns use it, and refactoring 850-line match risks bugs for negligible gain. Keeping as-is.

### Phase 7D — Security Hardening

- [x] 7D.1 Audit all `format!("\"{}\"", ...)` patterns — find and fix every manual JSON string construction across the codebase. (PR #56)
- [x] 7D.2 Add `--allow-run` permission flag — `sh()`/`shell()`/`run_command()` execute arbitrary commands with no sandboxing. Add Deno-style permission model. (PR #57)
- [x] 7D.3 Default SSRF protection on — `FORGE_HTTP_DENY_PRIVATE` should be the default; opt out with `FORGE_HTTP_ALLOW_PRIVATE=1`. (PR #58)

### Order of attack

1. 7A.1-7A.5 (safety) — every one of these is a live bug or security hole
2. 7B.1 (warnings) — takes 10 minutes, instant credibility
3. 7B.2-7B.5 (panics) — systematic panic elimination
4. 7C.1-7C.2 (performance/correctness) — quick wins
5. 7C.3-7C.4 (VM optimization) — bigger refactors
6. 7D.1-7D.3 (security) — hardening layer
7. 7E.1-7E.3 (distribution & ecosystem) — reach and adoption

### Phase 7E — Distribution & Real-World Readiness

- [ ] 7E.1 Add curl-pipe-sh installer — `curl -fsSL https://forge-lang.dev/install | sh` for Linux/macOS. Detects arch, downloads release binary, installs to `/usr/local/bin`.
- [ ] 7E.2 Add `os` and `path` stdlib modules — hostname, platform, pid, arch, path.normalize, path.resolve, path.relative, path.is_absolute, path.separator. Table-stakes for real programs.
- [ ] 7E.3 VS Code extension with TextMate grammar — proper syntax highlighting, snippets, debugger launch config. The LSP exists but has no extension wrapper for marketplace distribution.
