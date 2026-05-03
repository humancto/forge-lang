# Startup Time Measurement

Forge's roadmap target of `<10ms` startup applies to standalone/native execution paths, not to `forge run app.fg`.

`forge run app.fg` intentionally does more work: starts the CLI, reads source, lexes, parses, typechecks, initializes the runtime, and executes. Native and bytecode paths can skip parts of that work and are the realistic target for sub-10ms startup.

## Harness

Startup timing is measured by `tools/startup_time.rs`, a small Rust process-level harness. It measures wall time from parent process spawn to child process exit and verifies each child prints `ok`.

The harness measures:

- `startup.source_run`: `forge run hello.fg`
- `startup.bytecode_run`: `forge run hello.fgc`
- `startup.native_source_runtime`: standalone source-runtime binary from `forge build --native`
- `startup.aot_bytecode`: standalone bytecode binary from `forge build --aot`

The native modes require `FORGE_LIB_DIR` to point at a directory containing `libforge_lang.a`.

## Local Run

```bash
cargo build --release --lib --bin forge
rustc tools/startup_time.rs -O -o target/startup_time
FORGE_LIB_DIR=target/release ./target/startup_time --forge ./target/release/forge --warmups 2 --reps 20
```

## CI Status

CI runs this harness as report-only. The job fails if the harness fails to compile, if fixture builds fail, if any child process exits unsuccessfully, or if output is wrong. It does not yet fail because startup is above 10ms.

The hard `<10ms` gate should be added after we have stable baseline data and an optimization PR that actually reaches the native startup target.
