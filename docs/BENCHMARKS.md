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
| `vm` | `benchmarks/vm/*.fg`: `loop` (20M-iteration `while`), `fib` (recursive fib(30)), `string_build` (200k `s = s + "x"`), `array_push` (100k `push`), `map_filter` (map/filter/reduce over 1M items), `mandelbrot` (200×200 grid, 50 iterations, pure Float loops), `spectral_norm` (n = 100, Float helper called from array loops), `nbody` (5 bodies, 20k steps over Float arrays), `repeat_loop` / `range_loop` (5M-iteration top-level `repeat` / `for i in range(..)`), `toplevel_loop` / `toplevel_loop_fn` (the same 5M-iteration `while` at the top level and inside a function), `toplevel_string` (`string_build` at the top level), `toplevel_captured` (2M calls of a top-level `fn` that updates a captured top-level variable), `global_calls` (2M iterations reading two builtins as globals), and the higher-order collection shapes: `reduce_lambda` (`reduce` over 1M items with an arithmetic lambda), `sort_comparator` (`sort` of 200k pseudo-random ints with a comparator lambda), `stream_chain` (a lazy `.stream()` filter/map/reduce over 1M items), `records_pipeline` (300k objects built with `map`, then `.filter`/`.map`/`.reduce` reading their fields), and container building: `word_count` (100k words, 5,000 distinct, `counts[w] = get(counts, w, 0) + 1` at the top level), `object_build` (an object with 50k computed keys, `ob[k] = v`), `push_records` (100k `recs.push({ id: i, name: ... })`) | default (VM with JIT tier-up) |
| `interp` | `benchmarks/interp/*.fg`: array reads, deep recursion, fib, push via method and reassignment, set building, string building, and the `vm` suite's `word_count`, `object_build`, `push_records` | `--interp` |
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
   engine differs. With `--base-root` (the base worktree) only programs
   that exist in both checkouts are compared: a benchmark a PR adds has
   no baseline, and the code it was written for may be pathological on
   the base (a quadratic loop the PR fixes can exhaust the runner's
   memory). New benchmarks are listed in the report. The rounds are **interleaved** (base/head, head/base,
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
- `map_filter` was the VM's weakest case in this table (about 2.3× Node
  and 1.25× CPython). The callbacks did tier up, but every call from a
  native builtin re-did the whole call setup and JIT selection. Since
  callbacks go through `VM::call_callback` it runs below CPython and
  Node; see [Higher-order collection code](#higher-order-collection-code).
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

## Higher-order collection code

`map`/`filter`/`reduce`/`sort`/`find`/`any`/`all`/`flat_map`, their method
forms (`xs.map(f)`) and `.stream()` stages call a Forge function once per
element. That is what most real Forge programs are made of, so it has its
own benchmarks (`map_filter`, `reduce_lambda`, `sort_comparator`,
`stream_chain`, `records_pipeline`) with Python and Node ports.

### Where the time went

Profiled with callgrind on a symbolized release build (`map_filter`, and
`records_pipeline` for the non-JIT path):

- The lambdas *did* tier up: `forge --jit` showed `<lambda>(Int) -> Int`
  compiled. The cost was the call path around them, ~100–140 ns per
  element: `call_value` allocated an argument `Vec` per call, cloned the
  function's name, and the JIT tier then hashed the function id (SipHash),
  built a `TypeSig` `Vec`, searched the specializations, re-checked every
  entry guard and encoded the arguments into another `Vec`. On top of that,
  a disabled profiler still allocated and hashed a `String` per named call.
- For callbacks that cannot be compiled (field reads, objects), the call
  setup was a smaller share; the rest is object work: `{...}` literals
  (an intern-table lookup per key constant, a `String` per key, an
  `IndexMap` insert with SipHash) and `r.field` reads (a SipHash lookup).
- `forge run` spent ~10% of a 100k-record program freeing the heap object
  by object at exit.

### What changed

- `VM::call_callback` (`src/vm/callback.rs`): a per-VM memo of recently
  called-back closures; the frame is entered from the argument slice
  (`entry_args` only kept for functions with a loop); the JIT selection is
  cached together with the conditions under which its entry guards held
  (`exec_epoch`, bumped on every entry into bytecode or into a builtin;
  `Globals::version`; frame depth; the `JitState` instance), so a hot
  callback re-enters native code directly.
- The general call path allocates less (no name clone, `TypeSig::matches`,
  stack-encoded JIT arguments, an `FnId` hasher), the disabled profiler is
  free, and `forge run` no longer frees the heap before exiting.
- `rows.push({...})` with an object or tuple literal appends in place
  (`PushLocal`); it used to copy the array on every push.

Compiling `map(xs, fn(x) { ... })` into an inline loop was considered and
not done: it must re-check at run time that `map` is still the builtin
(user code can shadow it), keep callback leniency (`fn(x, i)` gets
`null`), error messages, stack traces and evaluation order identical, and
it would only remove the remaining ~20–30 ns of call setup per element
now that hot callbacks run natively.

### Numbers

Measured 2026-10-12, release build (fat LTO), Linux x86_64, 4 vCPU,
**heavily loaded** (load average 10–14 from parallel builds). Best of 9
interleaved runs (base, head, Python, Node in turn), whole process, in
milliseconds. Contention only adds time, so the best sample is the most
stable figure on this host; ratios matter more than absolute values.

| Workload (`benchmarks/vm/`) | before | after | speed-up | Python 3.11 | Node 22 |
|---|---:|---:|---:|---:|---:|
| `map_filter`: map/filter/reduce, 1M items | 313 | 85 | 3.7× | 239 | 133 |
| `reduce_lambda`: reduce, 1M items | 138 | 42 | 3.3× | 146 | 107 |
| `sort_comparator`: sort 200k ints, comparator | 519 | 167 | 3.1× | 388 | 148 |
| `stream_chain`: lazy stream filter/map/reduce, 1M | 209 | 76 | 2.8× | 131 | 121 |
| `records_pipeline`: 300k objects, filter/map/reduce | 577 | 512 | 1.1× | 207 | 305 |

The JIT-able shapes now run at or below Node. `records_pipeline` is still
about 2.5× CPython: its time is object creation and field lookup, not
callbacks (see above); a faster object representation is the next step
there.

On the regression gate (`tools/bench_compare.py`, 5 interleaved rounds,
same host) nothing else in `vm`, `interp` or `startup` changed beyond
noise; re-running the two benchmarks the noisy first pass flagged with 8–10
rounds gave `toplevel_string` +2.2% best and `global_calls` −1.2% best.

## Building objects, records and groups

The natural way to build a dictionary, a list of records or groups —
`counts[w] = get(counts, w, 0) + 1`, `ob[k] = v`, `recs.push({ ... })`,
`groups[k].push(x)` — was quadratic (eval tier v2, findings 10–14):
collections are values, and on the VM `SetIndex`/`SetField` returned an
updated copy of the whole container, a push of an object literal took the
copying method-call path, a nested `groups[k].push(x)` updated nothing, and
any read of the local (`get(counts, ...)`, `counts[w]`) shared it so the next
update copied again. On the interpreter `get`/`has_key` copied the object
for every call. Benchmarks: `word_count`, `object_build`, `push_records` in
`benchmarks/vm/` and `benchmarks/interp/` with Python/Node/Lua ports.

### What changed

- VM opcodes that read and update a local's container without copying its
  reference (`GetIndexLocal`, `GetFieldLocal`, `SetIndexLocal`,
  `SetFieldLocal`, `PushIndexLocal`, `PushFieldLocal`, and `PeekLocal` +
  `CallPure` for `len`/`get`/`has_key`/`contains`): an owned container
  (`GcObject::unique`) is updated in place, and an array owned by an owned
  container (`GcObject::slot_owned`) is appended in place. The rules that
  keep this unobservable are in `src/vm/local_ops.rs`.
- `xs.push(e)` takes the in-place `PushLocal` path for any `e`.
- String concatenation (`"r" + str(i)`) builds its result once and no
  longer interns it: a hash, a table entry and a second copy per computed
  string (100k distinct strings: 230 → 120 ms).
- Interpreter: `get`/`has_key` borrow the object; `g[k].push(x)` updates
  the place in place.

### Numbers

Measured 2026-10-12, release build, the same loaded 4-vCPU host as above
(load average 4–10). `tools/bench.sh`, 5–7 runs, median, whole process in
seconds; "before" is v0.11.0.

| Workload | VM before | VM after | interp before | interp after | Python 3.11 | Node 22 |
|---|---:|---:|---:|---:|---:|---:|
| `word_count`: 100k words, 5,000 distinct | 57.5 | 0.125 | 28.1 | 0.31 | 0.053 | 0.16 |
| `object_build`: 50k computed keys | 81.2 | 0.045 | 0.11 | 0.074 | 0.032 | 0.083 |
| `push_records`: 100k object literals | killed (memory) | 0.161 | 0.33 | 0.27 | 0.057 | 0.11 |

On the VM these are now 1.4–2.8× CPython. What remains in `push_records`
is object creation (a `String` per key and an `IndexMap` per record) and
`str(i)`, shared with `records_pipeline`. On the regression gate
(`tools/bench_compare.py`, interleaved rounds against v0.11.0) no other
`vm`/`interp`/`startup` benchmark changed beyond this host's noise.
