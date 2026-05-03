# Startup Time Under 10ms Measurement Plan

## Roadmap Item

- `ROADMAP.md`: `Startup time: < 10ms (vs ~100ms for interpreter)`

## Scope Decision

This PR does **not** claim the `<10ms` target is achieved. It establishes repeatable startup measurement and report-only CI visibility so the next optimization PR can be judged against real data. The roadmap checkbox must stay unchecked until the measured target is met.

The target should apply to the standalone/native execution path, not ordinary `forge run app.fg`. A source file run still has to start the Rust CLI, parse CLI args, read source, lex, parse, typecheck, initialize runtime state, and execute. The native/standalone path is the only realistic place for `<10ms`.

## Current State

- `forge run app.fg` goes through the full CLI/frontend/interpreter path.
- `forge run app.fgc` skips lex/parse but still starts the CLI and VM.
- `forge build --native` can now produce a standalone source-runtime binary when `libforge_lang.a` is present.
- Existing `benches/fork_for_serving.rs` measures per-request fork cost, not process startup.
- There is no repeatable startup benchmark, no CI trend signal, and no agreed measurement definition.

## Measurement Definition

Measure cold-ish process startup wall time from parent process spawn to child process exit for short-lived programs.

Initial modes:

1. `source-run`: `forge run hello.fg`
2. `bytecode-run`: `forge run hello.fgc`
3. `native-source-runtime`: generated `forge build --native hello.fg` binary when `libforge_lang.a` is available
4. `aot-bytecode`: generated `forge build --aot hello.fg` binary when `libforge_lang.a` is available

Short-lived fixture:

```forge
println("ok")
```

The harness must assert correctness on every run. A child process that exits nonzero, segfaults, times out, or prints unexpected output must fail the measurement instead of looking like a fast startup.

Use a small `println("ok")` fixture for every mode so the harness can assert stdout-based correctness. Avoid server startup, networking, shell builtins, or filesystem writes in the measured child program.

## Implementation Units

### U1. Startup Measurement Harness

Files:
- Create: `tools/startup_time.rs` or `tests/startup_time.rs` as a small Rust harness binary/test helper
- Modify: `Cargo.toml` only if using a cargo bench/bin target is necessary

Do **not** use Criterion for process startup measurement. Criterion is optimized for in-process function benchmarking and its warmup/statistical model is a poor fit for fork/exec wall time.

Add a custom wall-time harness (or a thin wrapper around `hyperfine` only if introducing that dependency/tool is cleaner) that:
- Locates the `forge` binary under test.
- Creates an isolated temp fixture directory.
- Writes `hello.fg`.
- Builds `hello.fgc`.
- Requires the caller/CI job to provide `FORGE_LIB_DIR` pointing at an existing `libforge_lang.a`.
- Builds native artifacts with `FORGE_LIB_DIR` set so standalone modes are actually measured.
- Measures process spawn-to-exit wall time for each mode using `std::process::Command` and `Instant`.
- Runs enough repetitions to report min/median/p95 or min/mean/p95.
- Asserts every child exits successfully and emits expected output where applicable.
- Times out child processes so hangs fail fast.

Harness output should be simple, line-oriented, and easy to paste into PRs, for example:

```text
startup.source_run median=...
startup.bytecode_run median=...
startup.native_source_runtime median=...
startup.aot_bytecode median=...
```

### U2. Report-Only CI Job

Files:
- Modify: `.github/workflows/ci.yml`

Add a startup benchmark job that:
- Builds the Forge binary in release mode.
- Builds `libforge_lang.a` explicitly.
- Sets `FORGE_LIB_DIR` to the directory containing `libforge_lang.a`.
- Runs the startup measurement harness.

Keep this report-only for now:
- The job should fail if the harness does not compile/run or any measured child fails/times out.
- It should not fail because the measured value is above 10ms yet.

Rationale: shared CI runners are noisy; the first step is a trend signal.

### U3. Budget Documentation

Files:
- Create: `docs/performance/startup.md` or update an existing performance doc if one exists
- Modify: `CHANGELOG.md`

Document:
- Measurement modes and what each means.
- Why `<10ms` applies to standalone/native startup, not `forge run`.
- Current status: report-only startup harness exists; hard gate follows after optimization.
- Future hard-gate proposal: native startup p50/p95 budget once stable baseline is known.
- CI explicitly builds and measures the standalone native path; native modes must not be silently skipped.

### U4. Local Developer Command

Files:
- Optional create: `scripts/measure_startup.sh`

Add a script only if it materially improves developer ergonomics by wrapping the Rust harness with the right release-build and `FORGE_LIB_DIR` setup. Avoid duplicating measurement logic between shell and Rust.

## Risks

- Process startup benchmarks are noisy on GitHub-hosted runners.
- Harness setup must not accidentally measure build time.
- Native source-runtime binaries embed the interpreter and may not get close to `<10ms`; if so, the next item may require a bytecode/native runner fast path rather than optimizing the source-runtime path.
- Launcher-mode native binaries must be labeled separately from standalone source-runtime binaries; the roadmap target cares about standalone.
- Without storing historical baselines, CI output is visibility-only; this PR should not pretend to provide trend analysis yet.
- The native measurements require a working C compiler (`cc`) and static library; CI must install/use the available platform toolchain explicitly.

## Verification

- `cargo fmt -- --check`
- `cargo test`
- `cargo clippy --all-targets -- -A clippy::approx_constant -A clippy::result_large_err -A clippy::only_used_in_recursion -A clippy::len_zero`
- The new startup measurement command/harness
- Existing Forge integration tests remain green.

## Success Criteria

- Developers can run one command to see startup timings for source, bytecode, and available native modes.
- CI exposes startup timing regressions as benchmark output.
- The roadmap item remains unchecked, with a clear next optimization target based on measured data.
