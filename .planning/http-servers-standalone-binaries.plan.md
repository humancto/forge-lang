# HTTP Servers as Standalone Binaries Plan

## Roadmap Item

- `ROADMAP.md`: `HTTP servers work as standalone binaries`

## Production-Readiness Status

Forge is now serious beta for language/runtime development: CI covers Rust tests on Linux/macOS/Windows, Forge integration tests, backend parity, audit, OTel build, and a real `fork_for_serving` benchmark. The server runtime is no longer blocked by global interpreter serialization.

This item closes a visible production gap: a Forge HTTP service should be buildable into a single executable that does not shell out to the `forge` CLI at runtime. This is a **source-runtime standalone binary**: it statically links the Forge runtime and interpreter, embeds source, and boots the existing server runtime in-process. It does not make handlers true native-code functions yet; startup and 2-5x Rust performance are separate roadmap items.

## Current State

- `src/native.rs::build_native_aot` can build a standalone executable only when `libforge_lang.a` is discoverable. That executable embeds bytecode and calls `forge_execute_bytecode`.
- `src/lib.rs::forge_execute_bytecode` deserializes bytecode and runs the VM. It does not know how to launch decorator-driven HTTP servers.
- `src/main.rs::compile_to_native_aot` rejects programs with decorators because `ensure_vm_compatible` marks `@server` / `@get` runtime metadata as VM-incompatible. This should stay true: decorated servers are not bytecode AOT today.
- `src/main.rs::compile_to_native_launcher` accepts source programs, but the generated binary writes a temp `.fg` and execs the `forge` CLI. That is not standalone.
- `src/runtime/host.rs::launch` already knows how to spawn schedules/watchers and start HTTP servers from a parsed program plus an initialized `Interpreter`.

## Scope

### In Scope

1. Add a standalone source-execution FFI entrypoint in `src/lib.rs` for embedded source programs.
2. Teach `src/native.rs` to emit a C wrapper that embeds Forge source and calls the new source-execution entrypoint when `libforge_lang.a` is available.
3. Route `forge build --native` decorated server programs through the standalone source path instead of a CLI-shellout launcher when `libforge_lang.a` is available.
4. Add focused tests proving:
   - the source FFI path can run non-server source,
   - a decorated server can be built into a standalone executable when `libforge_lang.a` is available,
   - launcher fallback behavior remains available when no static library is discoverable.

### Out of Scope

- True native route function pointers via `forge_register_route`.
- Cranelift AOT codegen for decorated handlers.
- `forge build --native --aot` support for decorated servers. Keep `--aot` VM-bytecode-only and direct users to `--native` for standalone source-runtime servers.
- Startup-time target `< 10ms`.
- Performance target `2-5x of equivalent Rust`.
- Cross-compilation target support.
- Windows standalone static linking. Existing standalone AOT is Unix-only; keep that boundary unless implementation proves a tiny safe Windows slice is available.

## Approach

### U1. Share the Source Runtime Pipeline

Files:
- `src/lib.rs`
- `src/runtime/embedded.rs` (new) or another small runtime helper module

Extract a shared Rust helper for the same two-phase flow used by `forge run`:

Behavior:
- Parse source with existing `Lexer` / `Parser`.
- Create an `Interpreter`, set `source`, set `source_file` or diagnostic source label, and set `defer_host_runtime = true`.
- Optionally call `permissions::set_allow_run(allow_run)`.
- Run the interpreter once so top-level bindings and functions are installed.
- Extract `RuntimePlan` and call `runtime::host::launch`.
- Create and own a Tokio multi-thread runtime (`Builder::new_multi_thread().enable_all()`) for the embedded entrypoint before calling `host::launch`.
- Enter that runtime with `rt.block_on(async { ... host::launch(...).await ... })`; merely constructing a `Runtime` is not enough because stdlib modules use `tokio::runtime::Handle::try_current()`.
- Let `start_server` keep ownership of OTel/subscriber ordering; it already calls `init_otel()` from inside the Tokio runtime before `init_subscriber()`.
- For non-server source, return after the top-level interpreter run and empty runtime launch.
- Preserve current `forge run` behavior for schedule/watch-only programs: without a server, launch returns after spawning background threads and the process exits when the main thread returns.

Rationale:
- Keeps `main.rs` and embedded/native source execution from drifting.
- Makes schedules/watchers/server launch behavior match `forge run`.

Tests:
- Unit-test simple non-server source exits successfully through the helper.
- Unit-test `@server` without routes returns the existing runtime error.
- Unit-test shell builtins remain denied unless `allow_run` is true.
- Unit-test that both `Interpreter::source` and `Interpreter::source_file` / diagnostic label are populated for embedded source execution.

### U2. Add Source Execution FFI

Files:
- `src/lib.rs`

Add `forge_execute_source(source_ptr, source_len, path_ptr, path_len, allow_run) -> i32` or a similarly explicit options-shaped C ABI.

Behavior:
- Validate non-null source pointer and nonzero length.
- Decode UTF-8 source and optional path.
- Call the shared source runtime helper from U1.
- Wrap the call in `panic::catch_unwind(AssertUnwindSafe(...))` so panics never unwind across C.
- Return `0` on success, `1` on user/runtime/frontend errors, and `1` with a stable stderr message on panic.

Safety contract:
- Document that pointer+length pairs must reference valid memory for the duration of the call.
- The C wrapper generated by Forge is the primary caller; arbitrary embedders get a best-effort status code, not a reusable-process guarantee after panic.
- If the Rust signature is `pub unsafe extern "C" fn`, generated C remains unchanged, but Rust callers must acknowledge the raw-pointer contract.
- `allow_run` writes to process-global permission state. This is acceptable for generated standalone binaries, which call the entrypoint once per process, but arbitrary multi-call embedders should not rely on per-call isolation.

Security decision:
- Do not enable shell execution implicitly.
- Thread the existing CLI `--allow-run` flag into `forge build --native` so `forge build --native --allow-run app.fg` bakes `allow_run = true` into the generated wrapper.
- Declare the build subcommand's `allow_run` option as native-only (`requires = "native"` or `conflicts_with = "aot"`) so `forge build --aot --allow-run` is rejected as meaningless.
- Default standalone binaries keep shell builtins denied, matching file execution security.

Tests:
- Unit-test invalid/null input returns failure.
- Unit-test invalid UTF-8 returns failure.
- Unit-test `allow_run = false` rejects a shell builtin and `allow_run = true` permits it.

### U3. Emit Standalone Source Wrappers

Files:
- `src/native.rs`

Add a standalone source builder alongside `build_standalone_aot`:
- Embed source bytes as `FORGE_SOURCE`.
- Embed a diagnostic source label, preferably the source file basename or caller-provided display name rather than an absolute build-machine path.
- Link against `libforge_lang.a`.
- Call `forge_execute_source(FORGE_SOURCE, FORGE_SOURCE_LEN, FORGE_SOURCE_PATH, FORGE_SOURCE_PATH_LEN, FORGE_ALLOW_RUN)`.

Change `build_native_launcher`:
- If `find_libforge_dir()` succeeds, build a standalone source-runtime binary.
- If not, preserve current launcher behavior that shells out to `forge`.
- Make this behavior explicit in CLI output so users know whether the produced binary is standalone or a CLI launcher.

Tests:
- C source generation contains `forge_execute_source` and embeds source bytes, not a temp-file exec path.
- Existing launcher generation tests still cover fallback C source.
- `build_native_launcher` standalone smoke test is gated on Unix + `cc` + discoverable `libforge_lang.a`.
- Tests assert the generated C does not embed absolute source paths by default.

### U4. Keep AOT Honest and Route Native Builds

Files:
- `src/main.rs`
- `src/native.rs`

Behavior:
- `forge build --native app.fg` builds a standalone source-runtime binary when `libforge_lang.a` is available; otherwise it builds the existing CLI launcher.
- `forge build --native --allow-run app.fg` bakes shell permission into the generated standalone source wrapper.
- `forge build --aot --allow-run app.fg` is rejected at CLI parsing or validation.
- `forge build --aot app.fg` remains bytecode/VM-only.
- `forge build --aot server.fg` still rejects decorators, but the error should clearly say: `decorator-driven servers are not bytecode AOT yet; use forge build --native for a standalone source-runtime server binary`.

Rationale:
- This satisfies the roadmap item without pretending decorated handlers have native codegen.
- It preserves current bytecode AOT behavior for VM-compatible programs.

Tests:
- CLI-level unit or integration test for `--native` decorator program selecting source-standalone path when the static library is available.
- CLI-level test for `--aot` decorator program rejecting with the new honest guidance.

### U5. End-to-End Native Server Smoke

Files:
- `tests/native_server.rs` or existing native test module
- Optional fixture under `tests/fixtures/` or `examples/`

Add a Unix-gated smoke test:
- Build `libforge_lang.a` if needed or skip with a clear message if unavailable.
- Build a tiny server program with `@server(port: <ephemeral>)` and one `@get("/ping")`.
- Start the produced binary as a child process.
- Poll `/ping` until success.
- Send SIGTERM when available, wait briefly for graceful shutdown, then kill as cleanup fallback.
- Capture stderr and assert the server reaches normal startup logging; this protects the embedded Tokio/OTel initialization path from panicking before serving.

Guardrails:
- Use an ephemeral port inserted into the source before build.
- Time out aggressively so CI cannot hang.
- Skip if no `cc` or static library is unavailable in the test environment.
- Record binary size in test output and keep a loose upper ceiling if practical, because source-runtime binaries link the full Forge runtime.

## Edge Cases

- Invalid UTF-8 source passed through FFI returns failure.
- Source path may be omitted by C caller; diagnostics should still work.
- Server programs run forever; the native smoke test must always kill the child.
- Shell builtins remain denied by default in standalone binaries.
- `--allow-run` can be baked into a standalone binary only when explicitly supplied at build time.
- `ALLOW_RUN` remains process-global; standalone generated binaries are one-call processes, but embedding multiple Forge executions with different permissions in one host process is not supported by this slice.
- `@server` without routes should return the existing runtime error.
- If `libforge_lang.a` is absent, existing launcher fallback remains unchanged.
- Source is embedded in plaintext in the binary. This is acceptable for this roadmap slice and must be documented in CLI output or docs; bytecode/source-hiding remains the bytecode AOT path for VM-compatible programs.

## Rollback Plan

- Remove `forge_execute_source`.
- Remove standalone source wrapper generation.
- Remove the improved `--aot` guidance and keep decorator rejection unchanged.
- Remove native server smoke tests.

## Verification

- `cargo fmt -- --check`
- `cargo test`
- `cargo clippy --all-targets -- -A clippy::approx_constant -A clippy::result_large_err -A clippy::only_used_in_recursion -A clippy::len_zero`
- `cargo run -- --allow-run test tests/`
- `cargo build`
- Targeted native/server smoke tests added by this plan
- Update `CHANGELOG.md` under `[Unreleased]` because `forge build --native` behavior changes for users with `libforge_lang.a` available.

## Success Criteria

- A decorated Forge HTTP server can be built into a single executable that does not call the `forge` CLI at runtime when linked with `libforge_lang.a`.
- Existing launcher fallback still works when no static library is available.
- Existing bytecode AOT behavior for VM-compatible programs remains unchanged.
- `--aot` remains honest: decorated servers are rejected with guidance to use `--native`.
- Tests and CLI output make the source-runtime boundary explicit so future true AOT work can replace it deliberately.
