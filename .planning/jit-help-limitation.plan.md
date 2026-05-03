# JIT Help Limitation Plan

## Goal

Close production-readiness item E3 by making the JIT limitation explicit and tested.

## Current State

- `src/main.rs` already documents `--jit` as numeric-leaf-function focused in the clap help text.
- There is no test pinning that wording, so future CLI edits can accidentally erase the warning.
- Runtime `run_jit` already prints per-function `JIT compiled` / `JIT skip` diagnostics, so adding a new runtime notice would likely add noisy stderr churn.

## Implementation

Expert review approved this as test-only hardening. Do not add a runtime notice; `run_jit` already emits per-function compile/skip diagnostics and extra stderr would be noisy.

1. Add `CommandFactory` import from `clap`.
2. Add a focused unit test in `src/main.rs` tests that renders long help (`Cli::command().render_long_help().to_string()`).
3. Assert the help includes:
   - `--jit`
   - `JIT-compile numeric leaf functions`
   - `falls back to the bytecode interpreter automatically`
4. Keep the test unconditional because the help text is unconditional.
5. Update `CHANGELOG.md` only if the help text itself changes. If this is test-only hardening, skip changelog.

## Tests

- `cargo test jit_help --lib` if the test is in lib-visible code, otherwise `cargo test jit_help`
- `cargo test`

## Rollback

Remove the test/import. No runtime behavior changes expected.
