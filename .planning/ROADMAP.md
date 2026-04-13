# Forge Roadmap — v0.8.0 → v1.0

Written: 2026-04-12
Baseline: v0.8.0, ~43k LOC, 972 cargo tests, 20 stdlib modules, 3 backends.
Phases 0–7 complete. This roadmap covers the five evolution tracks toward v1.0.

---

## Phase 8 — Type System

**Goal:** Make the type checker useful enough that `--strict` mode catches real bugs. Currently advisory-only with no generics, no flow narrowing, and limited inference.

### Phase 8A — Inference & Enforcement

- [x] 8A.1 Bidirectional type inference for function return types — if all return paths return `Int`, infer the return type. Currently only forward inference works (literal → type). Touch: `typechecker.rs`.
- [x] 8A.2 Flow-sensitive type narrowing in if/match — `if x != null { x.field }` should know `x` is non-null in the true branch. Enables safe null handling without `unwrap`. Touch: `typechecker.rs`.
- [x] 8A.3 Exhaustive match checking — warn when a match doesn't cover all variants. Essential for Result/Option patterns. Touch: `typechecker.rs`.

### Phase 8B — Generics

- [x] 8B.1 Parse generic type parameters — `fn map<T, U>(arr: [T], f: fn(T) -> U) -> [U]`. Add `TypeParam` to AST, parse `<T, U>` after function names and struct names. Touch: `parser.rs`, `ast.rs`.
- [x] 8B.2 Generic type resolution in type checker — resolve `T` to concrete types at call sites. Monomorphization not needed yet — just validate constraints. Touch: `typechecker.rs`.
- [x] 8B.3 Generic struct definitions — `struct Pair<T> { first: T, second: T }`. Touch: `parser.rs`, `typechecker.rs`.

### Phase 8C — Type Ecosystem

- [x] 8C.1 Union types — `type StringOrInt = String | Int`. Parse `|` in type position, check assignability. Touch: `parser.rs`, `typechecker.rs`.
- [x] 8C.2 Type aliases — `type UserId = Int`. Simple substitution in the checker. Touch: `parser.rs`, `typechecker.rs`.
- [x] 8C.3 Typed collection literals — `let xs: [Int] = [1, 2, 3]` validates element types. Touch: `typechecker.rs`.

---

## Phase 9 — Concurrency Model

**Goal:** Go from "OS threads that work" to "concurrency primitives that are pleasant and safe." Currently: `spawn` creates OS threads, `channel` is bounded MPSC, no select/multiplex.

### Phase 9A — Channel Improvements

- [x] 9A.1 Channel `select` — wait on multiple channels, return first ready value. `select { ch1 -> v1 { ... }, ch2 -> v2 { ... } }`. Touch: `parser.rs`, `interpreter/mod.rs`, `vm/compiler.rs`, `vm/machine.rs`.
- [x] 9A.2 Channel `close` and iteration — `close(ch)` signals no more values; `for msg in ch { }` drains until closed. Touch: `interpreter/builtins.rs`, `vm/builtins.rs`.
- [x] 9A.3 Unbounded channels — `channel()` with no capacity argument creates unbounded. Currently only `sync_channel` is used. Touch: `interpreter/builtins.rs`, `vm/builtins.rs`.

### Phase 9B — Structured Concurrency

- [ ] 9B.1 Task groups — `task_group { spawn { ... }; spawn { ... } }` waits for all children before continuing. Cancels remaining on first error. Touch: `parser.rs`, `interpreter/mod.rs`, `vm/compiler.rs`, `vm/machine.rs`.
- [ ] 9B.2 Spawn with timeout — `spawn timeout 5 seconds { ... }` cancels after deadline. Touch: `interpreter/mod.rs`, `vm/machine.rs`.
- [ ] 9B.3 Spawn return type — `let handle = spawn { 42 }; let result = await handle` with proper Result wrapping on panics. Currently partially works. Touch: `vm/machine.rs`.

---

## Phase 10 — Package Ecosystem

**Goal:** Make `forge install` work with real packages from the internet. Currently local filesystem registry only, no semver, no transitive deps.

### Phase 10A — Version Resolution

- [ ] 10A.1 Semver constraint parsing — support `^1.0`, `~1.5`, `>=1.0.0, <2.0.0`, `*` in `forge.toml` dependency specs. Touch: `manifest.rs`, add `semver` crate.
- [ ] 10A.2 Semver resolution algorithm — given constraints, find the latest compatible version from available candidates. Touch: `package.rs`.
- [ ] 10A.3 Transitive dependency resolution — if A depends on B and B depends on C, install all three. Detect cycles. Touch: `package.rs`.

### Phase 10B — Remote Registry

- [ ] 10B.1 GitHub-based package index — packages listed in a central repo (e.g., `forge-lang/registry`) as TOML manifests. `forge install` fetches the index and resolves URLs. Touch: `package.rs`.
- [ ] 10B.2 `forge search <query>` — search the package index by name/description. Touch: `main.rs`, `package.rs`.
- [ ] 10B.3 Checksum verification — verify downloaded package SHA256 matches index entry. Touch: `package.rs`.

### Phase 10C — Developer Workflow

- [ ] 10C.1 `forge add <pkg>` — shorthand that adds to `forge.toml` and installs. Touch: `main.rs`, `manifest.rs`, `package.rs`.
- [ ] 10C.2 `forge update` — update all dependencies to latest compatible versions. Touch: `main.rs`, `package.rs`.
- [ ] 10C.3 Lockfile integrity check — `forge install` verifies `forge.lock` checksums match installed packages, warns on tampering. Touch: `package.rs`.

---

## Phase 11 — Performance

**Goal:** Make Forge competitive with Node.js/Lua on benchmarks. Currently 16-byte tagged enum values, no string interning, JIT limited to numeric ops.

### Phase 11A — String Interning

- [ ] 11A.1 Intern strings in GC — deduplicate identical strings via hash-consing. Short strings (≤23 bytes) stored inline. Touch: `vm/gc.rs`, `vm/value.rs`.
- [ ] 11A.2 Interned string comparison — `==` on interned strings becomes pointer comparison. Touch: `vm/machine.rs`.
- [ ] 11A.3 Intern field names — object field lookups use interned keys, eliminating hash computation on hot paths. Touch: `vm/machine.rs`, `vm/gc.rs`.

### Phase 11B — JIT Expansion

- [ ] 11B.1 JIT string operations — compile string concat, length, and comparison to native code. Requires calling into runtime for GC allocation. Touch: `vm/jit/ir_builder.rs`.
- [ ] 11B.2 JIT function calls — compile inter-function calls to native code instead of falling back to interpreter. Touch: `vm/jit/ir_builder.rs`, `vm/jit/jit_module.rs`.
- [ ] 11B.3 JIT object field access — compile `obj.field` to native code with inline caching for monomorphic access sites. Touch: `vm/jit/ir_builder.rs`.

### Phase 11C — Value Representation

- [ ] 11C.1 Compact object representation — replace `IndexMap<String, Value>` with a shape-based hidden class system. Objects with the same field layout share a shape, field access becomes array index. Touch: `vm/value.rs`, `vm/machine.rs`, `vm/gc.rs`.
- [ ] 11C.2 SmallVec for short arrays — arrays with ≤8 elements stored inline (no heap allocation). Touch: `vm/value.rs`, add `smallvec` crate.
- [ ] 11C.3 Tagged pointer values — compress `Value` from 16 bytes to 8 bytes using tagged pointers (NaN-boxing or pointer tagging). Touch: `vm/value.rs`, `vm/machine.rs`, `vm/gc.rs`, `vm/compiler.rs`.

---

## Phase 12 — Real-World Hardening

**Goal:** Everything needed to stamp "1.0" on the language. Error recovery, watch mode, CI targets, backwards compatibility commitment.

### Phase 12A — Parser Resilience

- [ ] 12A.1 Multi-error parser recovery — on parse error, skip to next statement boundary (`;` or newline + keyword) and continue. Accumulate errors, report all at once. Touch: `parser.rs`.
- [ ] 12A.2 Error context messages — "expected '}' to close block started at line 5" instead of just "expected '}'". Track opening tokens. Touch: `parser.rs`.
- [ ] 12A.3 Suggestion engine — "did you mean 'let' instead of 'lat'?" for common typos of keywords. Touch: `parser.rs`.

### Phase 12B — Testing & CI

- [ ] 12B.1 `forge test --watch` — re-run tests on file changes using the existing `watch` infrastructure. Touch: `main.rs`, `testing/mod.rs`.
- [ ] 12B.2 Cross-compile Linux x86_64 binary in GitHub Actions — CI produces release binaries for both macOS ARM64 and Linux x86_64. Touch: `.github/workflows/`.
- [ ] 12B.3 Integration test suite — `.fg` files that exercise every stdlib module end-to-end, run in CI. Touch: `tests/`.

### Phase 12C — Stability & Polish

- [ ] 12C.1 VS Code extension LSP client — wire up the `forge lsp` language server with `vscode-languageclient`. Full go-to-def, diagnostics, hover in the editor. Touch: `editors/vscode/`.
- [ ] 12C.2 Backwards compatibility test suite — snapshot of programs that must continue to work across versions. Breakage = regression. Touch: `tests/compat/`.
- [ ] 12C.3 `forge upgrade` self-updater — check GitHub releases for newer version, download and replace binary. Touch: `main.rs`.

---

## Ordering & Dependencies

Phases are mostly independent. Recommended interleaving:

1. **12A** (parser recovery) — foundation, makes everything else easier to develop
2. **8A** (type inference) — biggest developer experience win
3. **9A** (channel select) — unblocks real concurrent programs
4. **11A** (string interning) — low-risk, high-impact performance
5. **10A** (semver) — enables real package ecosystem
6. **8B** (generics) — builds on 8A
7. **12B** (CI/testing) — can be done anytime
8. **9B** (structured concurrency) — builds on 9A
9. **10B** (remote registry) — builds on 10A
10. **11B** (JIT expansion) — builds on 11A
11. **8C, 10C, 11C, 12C** — polish within each track

## How to resume

Each phase is independent. When picking up work:

1. Read this file to find the first unchecked `- [ ]` item
2. Run `cargo test` to confirm baseline
3. Follow the CLAUDE.md roadmap-driven workflow (plan → expert review → branch → implement → PR → expert review → merge → mark done → loop)

Current status: **Starting Phase 8 — all prior phases complete.**
