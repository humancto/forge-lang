# Benchmarks

How Forge's performance is measured, how regressions are caught in CI, and
where Forge stands against Python, Node.js and Lua on the same workloads.

## Tools

| Tool | What it does |
|---|---|
| `tools/bench.sh` | The benchmark runner. Runs suites of `.fg` programs as whole `forge run` processes and prints a table, or JSON with `--json`. |
| `tools/bench_compare.py` | A/B gate. Runs `bench.sh --json` against a base and a head binary in interleaved rounds, compares medians, and writes a Markdown table. Exits 1 on a regression. |
| `.github/workflows/perf.yml` | CI job that runs the gate on every pull request. |
| `tools/bench_vm.sh`, `tools/bench_interp.sh` | Wrappers for `bench.sh --suite vm` / `--suite interp` (kept for muscle memory). |
| `benches/*.rs` (criterion) | Micro-benchmarks of Rust internals (`cargo bench`): `fork_for_serving`, `interpreter_hot_paths`. Report-only. |
| `tools/startup_time.rs` | Start-up harness for source, bytecode, `--native` and `--aot` binaries ([performance/startup.md](performance/startup.md)). Report-only. |

```bash
cargo build --release
tools/bench.sh                          # vm, interp, startup suites; table
tools/bench.sh --json > results.json    # machine-readable
tools/bench.sh --suite vm -- --jit      # eager JIT instead of tier-up
tools/bench.sh --suite peers            # Python / Node / Lua ports
tools/bench.sh --list                   # benchmark ids
tools/bench_compare.py --base /path/to/old/forge --head target/release/forge
```

### Suites

| Suite | Programs | Engine |
|---|---|---|
| `vm` | `benchmarks/vm/*.fg`: `loop` (20M-iteration `while`), `fib` (recursive fib(30)), `string_build` (200k `s = s + "x"`), `array_push` (100k `push`), `map_filter` (map/filter/reduce over 1M items), `mandelbrot` (200×200 grid, 50 iterations, pure Float loops), `spectral_norm` (n = 100, Float helper called from array loops), `nbody` (5 bodies, 20k steps over Float arrays), `repeat_loop` / `range_loop` (5M-iteration top-level `repeat` / `for i in range(..)`) | default (VM with JIT tier-up) |
| `interp` | `benchmarks/interp/*.fg`: array reads, deep recursion, fib, push via method and reassignment, set building, string building | `--interp` |
| `startup` | `benchmarks/startup/hello.fg` run 20 times per sample (`STARTUP_BATCH`) | default |
| `peers` | `benchmarks/peers/*.{py,js,lua}`: line-for-line ports of the `vm` suite | `python3`, `node`, `lua`/`luajit` |

`peers` is opt-in and never gated. Interpreters that are not installed are
reported as `missing`, not as failures.

### JSON schema (`schema: 1`)

```json
{
  "schema": 1,
  "forge": "target/release/forge",
  "version": "Forge v0.9.0",
  "host": {"os": "Linux", "arch": "x86_64", "cpus": 4},
  "runs": 3, "warmup": 1,
  "benchmarks": [
    {"id": "vm/fib", "suite": "vm", "runner": "forge", "status": "ok",
     "times_s": [0.21, 0.20, 0.22], "median_s": 0.21, "min_s": 0.20, "output": "832040"}
  ]
}
```

`status` is `ok`, `failed`, `timeout` or `missing`. Startup entries also
carry `"batch"`. `bench.sh` exits 1 if any benchmark failed or timed out.

## Methodology

- **Whole-process wall clock.** Each sample times complete `forge run`
  processes, so start-up, parsing and compilation are included, as a user
  experiences them. Timing uses bash's `$EPOCHREALTIME` (µs), with GNU
  `date` or perl as fallbacks. Each program prints its result, and the
  last output line is recorded so a wrong answer is visible.
- **Warm-up, then repeated samples.** By default there is one untimed
  run (page cache, CPU frequency) and then `--runs` timed ones. The
  reported value is the **median**. The minimum is shown as well.
- **Workloads are sized for 0.05–3 s per run** on a release build. That
  is long enough to rise above process noise, and short enough that a
  quadratic regression shows up as seconds or a timeout.
  `BENCH_TIMEOUT` (default 120 s) bounds each run.
- **Peers are ports, not idioms.** The Python, Node and Lua programs use
  the same algorithm, loop shape and sizes as the Forge program, with no
  `sum(range(n))` shortcuts. That measures the engines on the same work.

### The CI regression gate

`perf.yml` runs on every pull request to `main`:

1. Build the head and the PR base in release mode (the shipping profile,
   with fat LTO) on the same runner. The base is built from a
   `git worktree`, sharing one target directory so dependencies compile
   once.
2. Run `tools/bench_compare.py --rounds 5 --runs 2 --threshold 0.15`.
   Both binaries run the **head's** benchmark programs, so only the
   engine differs. The rounds are **interleaved** (base/head, head/base,
   ...), so thermal or neighbour drift on the runner hits both sides
   equally.
3. A benchmark is **flagged** when the head is more than 15% slower in
   **both** its median and its best sample, **and** more than 10 ms
   slower (`--min-delta`). Contention only ever adds time, so a real
   slowdown moves the best sample too, while a median dragged up by a
   noisy neighbour does not. The absolute floor keeps millisecond-scale
   jitter from failing short benchmarks.
4. **Confirmation:** flagged benchmarks get 5 more interleaved rounds
   (`--confirm-rounds`), and the decision is taken on all samples. Only a
   confirmed regression fails the job. The job also fails if the head can
   no longer run a benchmark the base could. New benchmarks are reported
   as `new`.
5. The comparison table goes to the job summary (`$GITHUB_STEP_SUMMARY`):
   base and head medians, median and best-sample change, spread
   ((max − min) / median of the noisier side) and verdict. Raw samples
   are uploaded as the `bench-compare` artifact.

**Override:** when a slowdown is intended (a correctness fix, a new
safety check), add the **`perf-regression-ok`** label to the pull
request. The workflow re-runs on `labeled`, still prints the table, and
passes. Say in the PR why the slowdown is acceptable. Removing the label
re-arms the gate.

Run the gate locally against any two builds:

```bash
git worktree add /tmp/forge-base origin/main
cargo build --release --manifest-path /tmp/forge-base/Cargo.toml --target-dir /tmp/forge-base-target
cargo build --release
tools/bench_compare.py --base /tmp/forge-base-target/release/forge --head target/release/forge
```

**Validation of the gate itself** (on the deliberately hostile machine
described below):

- *A/A* (the same binary as base and head, 5 rounds × 2 runs): no failure.
  Raw median swings reached ±57% on the 30 ms benchmarks, and the
  median-and-best rule plus confirmation absorbed them. Without the
  confirmation pass, the same A/A run flagged one false regression
  (`interp/push_method`). That is why confirmation is on by default.
- *Positive control* (head = a wrapper adding 30 ms of start-up per
  process): `startup/hello` (+312%), `vm/array_push`, `vm/fib`, `vm/loop`
  and `vm/string_build` were flagged and confirmed, and the job exited 1.
  `vm/map_filter` (+30 ms on 0.65 s, under 15%) correctly passed.

GitHub-hosted runners are far quieter than that machine. If a benchmark
is persistently noisy there, make it bigger rather than raising the
threshold.

## Current numbers

Measured 2026-10-05 with `tools/bench.sh --suite vm,interp,startup,peers
--runs 5` on a release build of Forge v0.9.0 (fat LTO). Host: Linux x86_64,
4 vCPU Intel Xeon @ 2.10 GHz, **heavily shared** (load average 15–18
during the runs, from parallel compile jobs). Absolute times are therefore
pessimistic and noisy. Ratios between columns are more meaningful than
the numbers themselves. Re-run on a quiet machine before quoting them.

Wall-clock seconds per whole process (median of 5, start-up included).
Lower is better.

| Workload (`benchmarks/vm/`) | Forge VM (default) | Forge `--jit` | Forge `--interp` | Python 3.11.15 | Node 22.22.0 | Lua |
|---|---:|---:|---:|---:|---:|---|
| `loop`: 20M-iteration `while` | 0.102 | 0.072 | 18.99 | 1.799 | 0.078 | not installed |
| `fib`: recursive fib(30) | 0.020 | 0.018 | 3.441 | 0.196 | 0.170 | not installed |
| `string_build`: 200k `s = s + "x"` | 0.059 | 0.056 | 0.142 | 1.841 | 0.163 | not installed |
| `array_push`: 100k `push` | 0.039 | 0.018 | 0.136 | 0.047 | 0.086 | not installed |
| `map_filter`: map/filter/reduce, 1M items | 0.568 | 0.536 | 2.815 | 0.454 | 0.251 | not installed |

Forge `--jit` and `--interp` are `tools/bench.sh --suite vm -- --jit` /
`-- --interp` (median of 3). Lua was not installed on the measuring host,
so its column is missing. `benchmarks/peers/*.lua` exist, and
`tools/bench.sh --suite peers` picks up `lua`, `lua5.4`, `lua5.3` or
`luajit` automatically.

Reading the table:

- On the default engine, the hot numeric paths (`loop`, `fib`) tier up
  into the Cranelift JIT. `fib(30)` finishes, start-up included, in about
  a tenth of CPython's time and below Node's. These pure, integer-only
  functions are the JIT's best case.
- `string_build` and `array_push` benefit from the VM's in-place update of
  uniquely owned locals (`GcObject::unique`). CPython 3.11 does not avoid
  the quadratic copy in this loop shape.
- `map_filter` is the VM's weakest case here. Closure callbacks invoked
  from native `map`/`filter`/`reduce` do not tier up, and it is about 2.3×
  Node and 1.25× CPython.
- Start-up: `startup/hello` measured 0.202 s per batch of 20, about
  **10 ms per `forge run`** of a one-line program (including process
  creation on a loaded host). See [performance/startup.md](performance/startup.md)
  for the native and AOT start-up modes.

Interpreter suite (`--interp`, median of 5):

| Benchmark | Seconds | Notes |
|---|---:|---|
| `interp/array_read` | 0.095 | |
| `interp/deep_recursion` | 1.323 | 20 recursions to depth 9000 |
| `interp/fib` | 3.136 | fib(30); the VM is ~150× faster |
| `interp/push_method` | 0.165 | |
| `interp/push_reassign` | 0.128 | |
| `interp/set_build` | 3.707 | 20k `add` + 20k `has`. That is slow for its size and suggests linear-time set operations in the interpreter, worth a look |
| `interp/string_build` | 0.188 | |
