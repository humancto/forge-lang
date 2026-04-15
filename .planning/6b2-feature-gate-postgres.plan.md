# 6B.2 Feature-gate PostgreSQL behind `postgres` cargo feature

## Goal

Make `tokio-postgres`, `tokio-postgres-rustls`, `rustls`, and `webpki-roots` optional dependencies, gated behind a `postgres` cargo feature (enabled by default).

## Approach

Same pattern as 6B.1 (JIT feature gate).

## Files to change

### 1. `Cargo.toml`

- Make 4 deps optional: `tokio-postgres`, `tokio-postgres-rustls`, `rustls`, `webpki-roots`
- Add `postgres = ["tokio-postgres", "tokio-postgres-rustls", "rustls", "webpki-roots"]` to `[features]`
- Add `"postgres"` to `default`

### 2. `src/stdlib/mod.rs`

- Gate `pub mod pg;` with `#[cfg(feature = "postgres")]`
- Gate `create_pg_module()` with `#[cfg(feature = "postgres")]`

### 3. `src/stdlib/pg.rs`

- No changes needed (entire module gated at parent)

### 4. `src/interpreter/mod.rs`

- Gate the `pg` module registration (line ~484) with `#[cfg(feature = "postgres")]`
- Gate `"pg"` in module name lists with `#[cfg(feature = "postgres")]`

### 5. `src/vm/machine.rs`

- Gate pg module registration (line ~727-737) with `#[cfg(feature = "postgres")]`

### 6. `src/vm/builtins.rs`

- Gate the `n if n.starts_with("pg.")` dispatch arm (line ~1510-1514)

### 7. `src/vm/compiler.rs`

- Conditionally include `"pg"` in module name list (line ~1485)

### 8. `src/lsp/mod.rs`

- Conditionally include `"pg"` in 3 module lists (lines ~373, 468, 1082)

### 9. `src/repl/mod.rs`

- Conditionally include `"pg"` in module list (line ~171)

## Edge cases

- `rustls` is shared with reqwest and tungstenite via rustls-tls feature flags, BUT those pull in their own rustls transitively. Our direct `rustls = "0.23"` dep is only needed for pg TLS config construction. Making it optional behind `postgres` is correct.
- Module lists are string arrays — need conditional compilation or runtime filtering

## Test strategy

- `cargo test` — all tests pass with default features
- `cargo test --no-default-features` — passes without pg or jit
- `cargo test --features jit` — passes with jit but no pg

## Rollback

- Revert the single commit on the feature branch
