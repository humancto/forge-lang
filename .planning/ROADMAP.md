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

Current status: **Phase 3 complete. Phase 2 complete. Starting Phase 4.**

---

## Phase 4 — Beyond v0.6.0

**Goal:** Expand Forge from a capable scripting language into a production-grade development platform.

### 4.1 VM as default engine

- **What:** Flip the default execution engine from interpreter to VM. Phase 3 made the VM feature-complete (spawn/await, schedule/watch, try/catch, destructuring, full stdlib). Remaining gaps: ask/must/freeze expressions, decorator-driven server routes.
- **Impact:** Very high — users get faster execution without `--vm` flag. The interpreter becomes the fallback for features the VM doesn't support.
- **Prerequisite:** Audit remaining VM incompatibilities, ensure all examples pass under `--vm`.

### 4.2 Cross-file LSP

- **What:** Extend go-to-definition and find-references to work across files. Currently single-file only. Requires building a project-wide symbol index from `forge.toml` or workspace root.
- **Impact:** High — transformative for multi-file projects. Currently the #1 LSP limitation.

### 4.3 `forge test` coverage

- **What:** Add code coverage reporting to `forge test`. Show line/branch coverage percentages. Could use source spans from Phase 1.5 to track executed lines.
- **Impact:** Medium — helps adoption for serious projects, signals production-readiness.

### 4.4 Native compilation (AOT)

- **What:** Expand `forge build` to produce standalone binaries. The JIT (cranelift) already compiles hot functions — extend to full ahead-of-time compilation.
- **Impact:** High — major differentiator vs other scripting languages. Enables deployment without the Forge runtime.
- **Prerequisite:** VM as default (4.1), NaN-boxing runtime for string/object JIT support.

### 4.5 Debugger (DAP)

- **What:** Implement the Debug Adapter Protocol for VS Code step-through debugging. Set breakpoints, inspect variables, step into/over/out.
- **Impact:** Medium — critical for complex program development. Builds on source spans (1.5) and LSP infrastructure.

### Order of attack

1. 4.1 (VM default) — validates Phase 3 completeness, biggest UX win
2. 4.2 (cross-file LSP) — highest developer experience impact
3. 4.3 (coverage) — low effort, high signal for adoption
4. 4.4 (AOT) — ambitious, depends on 4.1
5. 4.5 (debugger) — nice to have, can be done anytime
