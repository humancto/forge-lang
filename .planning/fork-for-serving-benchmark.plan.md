# Fork For Serving Benchmark Plan

## Goal

Close issue #112 by adding a real Criterion benchmark for `Interpreter::fork_for_serving()` and wiring it into CI for visibility.

## Current State

- There is no `benches/` directory or Criterion setup.
- `src/interpreter/tests.rs` has a diagnostic `fork_for_serving_is_under_50ms` test, but its threshold is intentionally loose and not a useful performance signal.
- `fork_for_serving()` is public and benchmarkable from an external bench target through the `forge_lang` library crate.

## Implementation

1. Add `criterion` as a dev-dependency using Cargo.
2. Add `[[bench]] name = "fork_for_serving" harness = false` to `Cargo.toml`.
3. Add an explicit `[profile.bench] opt-level = 3` so benchmark builds are release-like even if future profile edits change defaults.
4. Add `benches/fork_for_serving.rs`:
   - build an `empty` fixture from `Interpreter::new()` to show the lower bound,
   - build a `with_closures` fixture by lexing/parsing/running a small representative program that creates top-level objects, arrays, functions, and captured lambdas,
   - benchmark `criterion::black_box(interp.fork_for_serving())` for each fixture.
5. Add a lightweight CI job on Ubuntu that runs:
   - `cargo bench --bench fork_for_serving -- --warm-up-time 3 --measurement-time 5 --sample-size 50`
6. Keep the CI job visibility-only: do not parse timing or fail on noisy performance thresholds. Compilation and successful benchmark execution are the gate.
7. Remove the existing diagnostic wall-clock test in `src/interpreter/tests.rs`; Criterion replaces it with a statistically meaningful benchmark.

## Tests

- `cargo fmt`
- `cargo bench --bench fork_for_serving -- --warm-up-time 3 --measurement-time 5 --sample-size 50`
- `cargo test`

## Rollback

Remove the bench target, Criterion dev-dependency, CI job, and this plan file.
