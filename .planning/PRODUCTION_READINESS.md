# Production Readiness Punch List

Branch: `production-readiness`
Baseline: 697 cargo tests passing, build clean (19 warnings), v0.4.3.
Goal: Close concrete production-grade gaps identified in deep-dive audit (engine, stdlib, tooling agents).

Each item lists the **file:line** verified during inspection, the **fix**, and the **tests** required.
Every item must end with `cargo test` green at 697+ before commit.

---

## Group A — Network safety (HTTP)

### A1. http.rs / runtime/client.rs: redirect limit

- **Where:** `src/runtime/client.rs::fetch` (no redirect config), `src/stdlib/http.rs::do_download` (line 244 builder), `src/stdlib/http.rs::do_crawl` (line 287 builder).
- **Fix:** Add `.redirect(reqwest::redirect::Policy::limited(10))` to all three builders. Expose a `max_redirects` option in the Forge HTTP API (defaults to 10, set 0 to disable).
- **Tests:** Add `tests/http_safety.rs` (or extend existing) using a small in-process HTTP server (httptest crate is already a dev-dep candidate, otherwise hand-roll a tokio listener) that 302s in a loop. Assert 11th hop fails with redirect-limit error.

### A2. http.rs: download size cap

- **Where:** `src/stdlib/http.rs::do_download` line 261 (`.bytes()` reads full body), `src/runtime/client.rs::fetch` (no cap).
- **Fix:** Stream the response body via `.bytes_stream()` and accumulate up to a `max_bytes` cap (default 100 MiB). Error if exceeded. Expose Forge-level option.
- **Tests:** Spin small server returning 200 MiB, verify error. Verify 50 MiB succeeds.

### A3. http.rs: URL scheme/host validation (SSRF)

- **Where:** `do_request` (line 97), `do_download` (line 228), `do_crawl` (line 278), `runtime/client.rs::fetch` (line 17).
- **Fix:** Single guard `validate_url(url) -> Result<Url, String>` in `runtime/client.rs`:
  - Reject non-`http`/`https` schemes.
  - When env `FORGE_HTTP_DENY_PRIVATE=1` (default off, but on for tests of "safe mode"), reject loopback, link-local, private, multicast, ULA addresses. Resolve via `url.host()` plus an explicit `to_socket_addrs()` check; fail closed if resolution fails.
- **Tests:** Unit-test the validator with `file://`, `ftp://`, `http://127.0.0.1`, `http://10.0.0.1`, `http://169.254.169.254`, `http://example.com`. Confirm scheme rejection unconditional, host rejection only when env is set.

---

## Group B — Auth/crypto

### B1. jwt.rs: alg=none dead code

- **Where:** `src/stdlib/jwt.rs` lines 219-221 (empty `if header.alg == Algorithm::default()` block; `Algorithm::default()` is HS256, so the check is meaningless). Note: `parse_algorithm` at line 76 already rejects "none" on the _user-supplied_ alg, but a forged token's header is decoded by `jsonwebtoken::decode_header`, which silently maps unknown `alg` to default (or errors).
- **Fix:** Before `decode_header`, parse the raw header JSON (`serde_json::from_slice` on the base64-decoded segment), and reject if `alg` field is `"none"` / missing / not in the allowlist. Delete the dead empty `if`.
- **Tests:** Forge a token with header `{"alg":"none","typ":"JWT"}` and empty signature; assert `jwt_verify` returns an error. Existing `test_reject_alg_none` (line 456) should still pass.

---

## Group C — Postgres

### C1. pg.rs: default to TLS

- **Where:** `src/stdlib/pg.rs` line 145: `.unwrap_or(PgTlsMode::NoTls)`.
- **Fix:** Change default to `PgTlsMode::Tls`. Document explicit `tls=disable` for unencrypted local sockets. Update CHANGELOG (Changed).
- **Tests:** Existing 24 tests must still pass. Add a connection string parser test verifying default mode without an explicit `sslmode`.

### C2. pg.rs: raw `*const Client` pointer pattern

- **Where:** `src/stdlib/pg.rs` lines 214-220, 279-284 (extract `*const tokio_postgres::Client` from `RefCell` then dereference inside `block_in_place`).
- **Fix:** Hold the `RefCell::borrow()` _across_ the `block_in_place` call. The borrow lives on the stack of the same thread the entire time; `block_in_place` only blocks the runtime, not the thread, so the borrow is sound. Remove `unsafe` and `*const` entirely.
- **Tests:** Existing pg tests cover this code path; add one that does `pg_connect → pg_query → pg_query → pg_close` to exercise repeat borrows.

---

## Group D — Filesystem

### D1. fs.rs: opt-in confined base mode

- **Where:** `src/stdlib/fs.rs` (read+write paths in both `call` and `call_vm`).
- **Fix:** Add `FORGE_FS_BASE` env var. When set, every `read/write/append/remove/mkdir/rename/copy/list` resolves the path, canonicalizes it, and rejects if not under the base. No-op if unset (back-compat).
- **Tests:** Set `FORGE_FS_BASE` to a tempdir, verify reads inside succeed, reads outside (`/etc/passwd`, `../escape`) fail. Verify symlinks pointing outside the base are rejected.

---

## Group E — VM honesty

### E1. VM: error loudly on unsupported constructs

- **Where:** `src/vm/compiler.rs`:
  - line 975 `Stmt::DecoratorStmt(_) => Ok(())` (silent)
  - line 1276 `Stmt::ScheduleBlock { .. } => Ok(())` (silent)
  - line 1277 `Stmt::WatchBlock { .. } => Ok(())` (silent)
  - line 1644 `Expr::Await | Must | Freeze | Ask => compile_expr(c, inner, dst)` (Ask is wrong — silently runs inner expression)
  - line 1647 `Expr::Spawn(body)` (compiles synchronously)
- **Fix:** Replace the silent no-ops for `ScheduleBlock`, `WatchBlock`, top-level `DecoratorStmt` (when not a known parity-supported decorator), `Expr::Ask`, and bare `Expr::Spawn` with a `CompileError("VM does not yet support X — use --interpreter")`. Keep `Await`/`Must`/`Freeze` as identity since they are pure pass-throughs in non-async contexts and have parity coverage.
- **Tests:** Add new fixtures under `tests/parity/unsupported_vm/`:
  - `ask_expression.fg` → `expect-error: VM does not support 'ask'`
  - `watch_block.fg` → `expect-error: VM does not support 'watch'`
  - `spawn_expression.fg` → `expect-error: VM does not support 'spawn'`
  - `decorator_unknown.fg` → `expect-error: VM does not support decorator '@unknown'`
- **Care:** Existing `tests/parity/unsupported_vm/{schedule_block,server_decorator}.fg` already expect errors — verify their `expect-error:` strings match the new wording or update them.
- **Care 2:** Existing parity-supported decorators (the metadata ones added in commit 0d357d5) must keep working. Walk every fixture in `tests/parity/supported/` that uses `@` decorators and confirm they still pass.

### E2. VM: source spans in VMError

- **Where:** `src/vm/machine.rs` `VMError` type and all `Err(...)` constructions; `src/vm/compiler.rs` debug-line tables.
- **Fix:** Plumb a `(line, col)` table from compiler into `Chunk` (one per opcode index). On any runtime VM error, look up the current `ip` and attach `at file:line:col` to the error message.
- **Tests:** Construct a fixture that throws at a known line; assert error contains `line N`. Add unit test in `vm::tests`.

### E3. JIT: document limitation in `--jit` help

- **Where:** `src/main.rs` (CLI help), `src/vm/jit/type_analysis.rs` (where unsupported ops are defined).
- **Fix:** When `--jit` is passed, print a one-line notice on first compilation: "JIT compiles only int/float arithmetic functions; everything else falls back to VM." Or document in `forge run --help`.
- **Tests:** Snapshot test of `forge run --help` output.

---

## Group F — DB ergonomics

### F1. db/pg/mysql: transaction APIs

- **Where:** `src/stdlib/db.rs`, `src/stdlib/pg.rs`, `src/stdlib/mysql.rs`.
- **Fix:** Add `db_begin/db_commit/db_rollback`, same for `pg_*` and `mysql_*`. Implementations: SQLite uses `Connection::execute("BEGIN")`; tokio_postgres uses explicit `BEGIN`/`COMMIT` text since true `client.transaction()` requires `&mut`; mysql_async has `pool.start_transaction()` but for consistency use raw text statements.
- **Tests:** For each backend, exercise: begin → insert → rollback → verify row absent; begin → insert → commit → verify row present.

---

## Group G — Test coverage gaps

### G1. Add unit tests for crypto/regex/json/time

- **Where:** `src/stdlib/{crypto,regex,json,time}.rs` — current tests are sparse.
- **Fix:** For each module add a `#[cfg(test)] mod tests` covering happy path + at least one error path per public function. Aim for +30 tests total.
- **Tests:** Must be deterministic. Avoid network/time-of-day flakes.

---

## Group H — Discipline

### H1. Unwrap sweep in hot paths

- **Where:** Anywhere in `src/interpreter/mod.rs`, `src/vm/machine.rs`, `src/stdlib/*.rs` that `unwrap()` runs on user-input-derived data.
- **Fix:** Replace with `?` or `expect("BUG: explanation")`. Per CLAUDE.md rule #6.
- **Tests:** Bench-style: `cargo test` continues to pass. Spot check by feeding malformed input through the parser → interpreter pipeline.

### H2. CHANGELOG

- **Where:** `CHANGELOG.md`.
- **Fix:** Add `[Unreleased]` block listing every change above under appropriate Keep-a-Changelog headers. PR link can be left as `(#TBD)`.
- **Tests:** None; review by eye.

### H3. Final verification

- `cargo test` (must remain ≥697)
- `cargo build --release`
- `forge test` (integration)
- `forge run examples/hello.fg`, `examples/functional.fg`
- `forge run --vm examples/hello.fg`
- `forge run --jit examples/hello.fg`

---

## Order of attack

1. A1, A2, A3 (HTTP safety — small, isolated, well-tested)
2. B1 (jwt — small, isolated)
3. C1, C2 (pg — touch the same file)
4. D1 (fs)
5. E1, E2 (VM honesty — biggest blast radius, most parity care needed)
6. E3 (JIT help)
7. F1 (DB transactions)
8. G1 (test coverage)
9. H1 (unwrap sweep — last so it doesn't churn the diffs above)
10. H2, H3 (changelog + final verify)

After every group: `cargo test`, atomic commit on `production-readiness`.
