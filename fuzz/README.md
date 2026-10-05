# Fuzzing Forge

Coverage-guided fuzz targets for the parts of Forge that consume untrusted
input. They use [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz)
(libFuzzer, nightly toolchain). The same target bodies also run on stable as
an ordinary test, `tests/fuzz_smoke.rs`, so regular CI covers them too.

| Target         | Input                         | Must hold                                                                                       |
| -------------- | ----------------------------- | ----------------------------------------------------------------------------------------------- |
| `parse`        | arbitrary bytes as source     | the lexer and parser never panic                                                                |
| `compile`      | arbitrary bytes as source     | parsed programs compile without panicking; compiler output passes the verifier and round-trips |
| `bytecode`     | arbitrary bytes as `.fgc`     | deserialization never panics or over-allocates; verified bytecode runs on the VM without panics |
| `differential` | bytes steering a generator    | the interpreter and the VM agree on generated programs (value, or both fail)                    |

The target bodies live in `src/harness.rs`; `src/gen.rs` is the
grammar-based program generator (deterministic subset: arithmetic, strings,
arrays, objects, tuples, `if`/`while`/`for`/`match`/`try`, functions,
closures that mutate captures). Executions run under a deny-all permission
policy with a wall-clock budget (`harness::RUN_BUDGET`); runs that exceed it
are discarded rather than reported.

## Running

```bash
rustup toolchain install nightly
cargo install cargo-fuzz

# Seed the source targets with the repository's own programs.
mkdir -p fuzz/corpus/parse && find examples tests -name '*.fg' -exec cp {} fuzz/corpus/parse/ \;
cargo +nightly fuzz run parse fuzz/corpus/parse -- -max_total_time=600

cp fuzz/seeds/bytecode/* fuzz/corpus/bytecode/   # valid .fgc files
cargo +nightly fuzz run bytecode fuzz/corpus/bytecode -- -max_total_time=600

cargo +nightly fuzz run differential -- -max_total_time=600 -timeout=30
```

Build with `--debug-assertions` (`-a`) to catch integer overflow panics, as
the nightly workflow does. Without nightly, run the stable harness:

```bash
cargo test --test fuzz_smoke                                   # fixed seed, CI size
FORGE_FUZZ_ITERS=100000 FORGE_FUZZ_SEED=$RANDOM cargo test --release --test fuzz_smoke -- --nocapture
```

## When a target fails

1. Minimize: `cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/crash-…`
   (for `differential`, also shrink the printed Forge program by hand).
2. Fix the root cause and add a focused unit/regression test next to the code.
3. Commit the minimized input to `fuzz/regressions/<target>/<short-name>`.
   `tests/fuzz_smoke.rs` replays every file there on every `cargo test`, and
   the nightly workflow adds them to the corpus.

A divergence that is a known, deliberately deferred semantic difference goes
in `tests/engine_diff_known.txt` with a reason, never silently ignored.

## Bytecode verifier

`forge run app.fgc` and the `forge_execute_bytecode` C entry point execute
serialized bytecode, which may be crafted. `src/vm/verify.rs` checks every
deserialized chunk (and, in debug builds, every chunk the compiler emits)
before it runs: register, constant, prototype and upvalue indices, branch
targets (back-edges only through `Loop`, which polls cancellation), arity,
line tables, terminal instructions and prototype nesting depth. The loader
(`src/vm/serialize.rs`) checks every length prefix against the remaining
input before allocating.

## Miri

`cargo +nightly miri test --lib -- miri_` runs the tests that exercise the
crate's unsafe code paths Miri can execute: the `extern "C"` JIT bridges in
`src/vm/jit/runtime.rs` (called directly with raw VM and buffer pointers,
see `src/vm/jit/runtime_miri_tests.rs`) and the C-ABI entry points in
`src/lib.rs` (`src/ffi_miri_tests.rs`). Cranelift-generated machine code
itself cannot run under Miri, so the JIT-to-bridge call path (and its
documented `&mut VM` aliasing caveat on `rt_get_global`) is out of reach;
the nightly `miri` job in `.github/workflows/fuzz.yml` covers the rest.
