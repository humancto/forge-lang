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

### 1.5 Error messages with source spans

- **Where:** `src/errors.rs`, `src/interpreter/mod.rs`, `src/parser/parser.rs`
- **What:** Rust-style error messages with source snippets, underline carets, and context. Currently errors are just strings. This is a cross-cutting change: parser errors, type errors, and runtime errors all need span info threaded through.
- **Note:** This is the biggest item in Phase 1. Consider doing it early since 1.1-1.4 depend on good span coverage.

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

### 2.4 `forge publish`

- **What:** Package the current project and push to a registry. Requires a registry service (hosted or self-hosted).
- **Note:** This is the most ambitious item. Defer until 2.1-2.3 are solid.

### Order of attack

1. 2.1 (manifest) — define the format, parse it in `forge build`/`forge run`
2. 2.2 (resolution) — make imports work across package boundaries
3. 2.3 (install) — git-based first, registry later
4. 2.4 (publish) — only after the registry exists

---

## Phase 3 — VM Parity

**Goal:** Make `--vm` capable enough to be the default execution engine. This unlocks the performance story (and eventually the JIT story).

### 3.1 Async runtime in VM

- **What:** Wire up `await`, `spawn`, `hold` in the VM. Requires integrating tokio into the VM's execution loop or compiling async blocks into state machines.
- **Note:** This is the hardest item on the entire roadmap. The interpreter uses Rust's native async; the VM would need coroutine-style suspend/resume or a similar mechanism.

### 3.2 `try-catch` in compiler

- **Where:** `src/vm/compiler.rs` — currently logged as TODO (M1.2.2), catch block is dropped
- **What:** Compile the catch block, emit `TryCatch` / `EndTry` opcodes, wire up the error handler in `machine.rs`.

### 3.3 `destructure` in compiler

- **Where:** `src/vm/compiler.rs` — currently logged as TODO (M1.2.1), silently skipped
- **What:** Compile `let {a, b} = obj` and `unpack {a, b} from obj` into register loads from object fields.

### 3.4 Remaining stdlib in VM

- **What:** Audit which stdlib functions the VM can't call. Port the dispatch tables from `vm/builtins.rs`. Track parity with the interpreter's `call_builtin.rs`.

### 3.5 `schedule` / `watch` in VM

- **What:** These are runtime features (cron + file watcher). Currently rejected by the compiler. Implementing them requires the async runtime from 3.1.

### 3.6 JIT expansion

- **What:** Extend JIT beyond integer loops. Float arithmetic, string operations, function calls. This is Cranelift IR work in `src/vm/jit/ir_builder.rs`.
- **Note:** Only valuable after VM parity is solid. JIT is a performance optimization on top of a working VM.

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

Current status: **Phase 2 — items 2.1-2.3 complete. Remaining: 2.4 (publish — deferred until registry exists). Phase 1 item 1.5 (source spans) also deferred. Ready for Phase 3.**
