# 6B.1 — Feature-gate Cranelift JIT behind `jit` cargo feature

## Problem

5 cranelift crates add significant compile time and binary size to every build, even though most users never use `--jit`. Feature-gating makes the default build faster and smaller.

## Scope

### Cargo.toml

- Move cranelift-\* deps under `[dependencies]` with `optional = true`
- Add `[features]` section: `jit = ["cranelift-codegen", "cranelift-frontend", "cranelift-jit", "cranelift-module", "cranelift-native"]`
- Default features: empty (no JIT by default)

### src/vm/jit/ (entire module)

- Wrap `pub mod jit;` in `vm/mod.rs` with `#[cfg(feature = "jit")]`
- All files in `src/vm/jit/` remain unchanged but are conditionally compiled

### src/vm/machine.rs

- `JitEntry`, `jit_call_i64`, `jit_call_f64`: wrap with `#[cfg(feature = "jit")]`
- `jit_cache` and `jit_modules` fields: wrap with `#[cfg(feature = "jit")]`
- JIT compilation block (lines ~2025-2095): wrap with `#[cfg(feature = "jit")]`
- `Profiler` import and field: Profiler is in jit/profiler.rs — need to either move it out of jit/ or cfg-gate it too. Since profiling is useful without JIT, move `profiler.rs` out of `jit/` to `vm/profiler.rs`.
- `SendableVM` debug_assert: conditionally check jit fields
- Both constructors: conditionally init jit fields

### src/vm/mod.rs

- `#[cfg(feature = "jit")]` on `pub mod jit;`
- JIT-related tests: wrap with `#[cfg(feature = "jit")]`
- Parity tests that use JIT: skip JIT backend when feature disabled

### src/main.rs

- `--jit` CLI flag: keep it always visible but error at runtime if feature not enabled
- `run_jit()` function: wrap with `#[cfg(feature = "jit")]`

### src/testing/parity.rs

- JIT backend in parity tests: conditionally compile, skip when feature disabled

## Test strategy

- `cargo test` (without jit feature) — must pass, JIT tests skipped
- `cargo test --features jit` — all 950 tests pass including JIT
- `cargo build` — builds without cranelift deps
- `cargo build --features jit` — builds with cranelift

## Risk

- Forgetting a `#[cfg]` guard → compile error without feature (caught immediately)
- Profiler move may break imports in other files

## Rollback

Feature flag is additive. Remove `optional = true` and `#[cfg]` guards to revert.
