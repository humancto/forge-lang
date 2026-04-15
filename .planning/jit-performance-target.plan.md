# Plan: Performance target — 10-20x over tree-walk for JIT-compiled code

## Goal

Measure and demonstrate that JIT-compiled code achieves 10-20x speedup over the tree-walking interpreter. If the target isn't met, identify bottlenecks and optimize.

## Approach

### Step 1: Rust-level benchmark harness

Add a `#[cfg(feature = "jit")]` benchmark test in `src/vm/jit_tests.rs` that:

- Compiles a compute-intensive function (fib, sum_to, mandelbrot)
- Runs it via JIT, VM, and interpreter
- Uses `std::time::Instant` for high-resolution timing
- Includes warmup runs (discard first 3 iterations)
- Asserts correctness across all three modes before measuring
- Reports wall-clock ratios

### Benchmarks

- `fib(35)` — recursive int-only (JIT supports self-recursion natively via PR #94)
- `sum_to(1000000)` — loop accumulator
- `mandelbrot_pixel(cr, ci, max_iter)` — float arithmetic (I64-everywhere ABI)

### Step 2: Run and report

Run with `cargo test --features jit --release bench_jit_performance -- --nocapture --ignored`

Target: JIT should be 10-20x faster than interpreter for numeric workloads.

### Step 3: Optimize if needed

If target isn't met:

- Count `rt_call_native` bridge invocations per benchmark
- Profile with `cargo flamegraph` to find hotspots
- Consider: constant folding, reducing bridge overhead, inlining known pure functions

## Test Strategy

- All existing tests must still pass
- Benchmark test is `#[ignore]` so it doesn't run in CI (manual benchmark)
- Each benchmark asserts correct results before timing

## Rollback

Measurement only — no production code changes unless optimization is needed.
