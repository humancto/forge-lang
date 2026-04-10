# Forge Roadmap — Post v0.4.3

Written: 2026-04-10
Baseline: v0.4.3, 37k LOC, 805 cargo tests, production-readiness hardening merged.

---

## Phase 0 — Cleanup + Release (v0.5.0)

**Goal:** Close the 4 follow-up items from the production-readiness review, then cut a release.

### ~~0.1 `crypto::random_bytes` — real CSPRNG~~ ✅ DONE (merged in PR #4)

Already uses `getrandom::getrandom()`. No further work needed.

### 0.2 JWT key-confusion defence

- **Where:** `src/stdlib/jwt.rs` — `jwt_verify` function
- **Problem:** `jwt.verify(token, secret)` doesn't let the caller pin the expected algorithm. An attacker can forge a token by signing with HS256 using the RSA public key as the HMAC secret. The `jsonwebtoken` crate supports `Validation::set_required_spec_claims` and algorithm whitelisting.
- **Fix:** Accept optional third argument: `jwt.verify(token, secret, { algorithm: "RS256" })`. When provided, set `Validation.algorithms = [algo]`. When omitted, keep current behaviour (accept HS256/384/512 + RS256 + ES256) but log a deprecation warning to stderr on first call.
- **Tests:** Sign with HS256 + RSA pubkey as secret → verify with RS256 pinned → must reject. Sign with RS256 → verify with RS256 pinned → must accept.
- **Effort:** ~1 hr

### 0.3 `http.download` / `http.crawl` — thread user options

- **Where:** `src/stdlib/http.rs:242-364` — `do_download` and `do_crawl`
- **Problem:** These functions accept `(url)` or `(url, dest)` but ignore any options object. `do_request` already parses `timeout`, `max_redirects`, `max_bytes` from an options map. Downloads and crawls should do the same.
- **Fix:** Accept optional trailing `Value::Object` argument. Parse `timeout`, `max_redirects`, `max_bytes` the same way `do_request` does. Pass through to `build_client` / `read_body_capped`.
- **Tests:** Unit test that `do_download` with `{ timeout: 5 }` doesn't panic. Integration test optional.
- **Effort:** ~30 min

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

### 1.1 LSP: go-to-definition

- **Where:** `src/lsp/mod.rs`
- **What:** Resolve symbol under cursor to its definition location. Requires building a symbol table from the AST (variable bindings, function definitions, imports).
- **Depends on:** A `SymbolTable` struct that maps `(name, scope)` → `(file, line, col)`.

### 1.2 LSP: find-references

- **What:** Inverse of go-to-definition. Given a definition, find all uses. Same symbol table, walked in reverse.

### 1.3 LSP: real diagnostics

- **What:** Run the parser + type checker on every `textDocument/didChange` and push diagnostics. Currently the LSP only does basic syntax errors.
- **Depends on:** Parser producing span-annotated AST nodes (partially done — check `Span` coverage).

### 1.4 LSP: hover + signature help

- **What:** Show type/doc info on hover. Show function parameter hints while typing. Requires the symbol table from 1.1 + type info from the type checker.

### 1.5 Error messages with source spans

- **Where:** `src/errors.rs`, `src/interpreter/mod.rs`, `src/parser/parser.rs`
- **What:** Rust-style error messages with source snippets, underline carets, and context. Currently errors are just strings. This is a cross-cutting change: parser errors, type errors, and runtime errors all need span info threaded through.
- **Note:** This is the biggest item in Phase 1. Consider doing it early since 1.1-1.4 depend on good span coverage.

### 1.6 `forge fmt` — full syntax coverage

- **Where:** `src/formatter.rs` (or wherever the formatter lives)
- **What:** Currently handles basic formatting. Extend to cover all syntax forms including natural-language keywords, decorators, destructuring, async, etc.

### 1.7 `forge doc` — auto-generated module docs

- **What:** New CLI command that introspects stdlib modules and prints/generates markdown documentation for each function with its signature and a one-line description.
- **Note:** Lower priority than LSP. Nice-to-have for the website.

### 1.8 REPL improvements

- **What:** Tab completion for builtins/module names, syntax highlighting, multi-line editing improvements. Consider integrating `rustyline` features if not already using them.

### Order of attack

1. 1.5 (spans) — foundation for everything else
2. 1.1 + 1.3 (go-to-def + diagnostics) — the two most impactful LSP features
3. 1.2 + 1.4 (find-refs + hover) — build on the symbol table from 1.1
4. 1.6, 1.7, 1.8 (fmt, doc, REPL) — polish

---

## Phase 2 — Package Ecosystem

**Goal:** Enable multi-file, multi-author Forge projects. This is what turns Forge from a scripting tool into a language people build real things with.

### 2.1 `forge.toml` project manifest

- **What:** Define project name, version, entry point, dependencies. Minimal spec:

  ```toml
  [package]
  name = "my-app"
  version = "0.1.0"
  entry = "src/main.fg"

  [dependencies]
  http-utils = "0.2"
  ```

- **Where:** New `src/manifest.rs` or `src/package.rs` (already exists for scaffolding — extend it).

### 2.2 Module resolution across packages

- **Where:** `src/interpreter/mod.rs` (import resolution), `src/parser/parser.rs` (import statements)
- **What:** `import "http-utils"` resolves to `~/.forge/packages/http-utils/src/mod.fg` (or similar). Define the resolution algorithm: local → project deps → global.

### 2.3 `forge install <pkg>`

- **What:** Fetch a package from a git repo (initially) or a registry (later). Write it to a local `forge_packages/` directory. Update a lock file.
- **Note:** Start with git-based packages. A central registry is a Phase 3+ concern.

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

Current status: **Phase 0 — not started**
