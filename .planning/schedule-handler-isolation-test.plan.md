# Schedule Handler Isolation Test Plan

## Goal

Close issue #109 by adding an integration regression test that locks in the documented Forge HTTP fork invariant: writes made by `schedule` / `watch` background runtimes must not leak into per-request handler forks.

## Current State

- `Environment::fork_for_background_runtime()` intentionally uses `deep_clone()` so recurring background jobs keep their own state across ticks.
- `Environment::fork_for_serving()` intentionally uses `deep_clone_isolated()` so each HTTP request sees the template snapshot and gets isolated closure state.
- `tests/server_concurrency.rs` already boots real Forge HTTP servers on ephemeral ports and polls `/ping` for readiness.
- There are strong per-request parallelism tests, but no end-to-end server test proving scheduled background mutation remains invisible to handlers.

## Implementation

1. Add a focused test to `tests/server_concurrency.rs`.
2. Reuse `spawn_test_server()` so the test runs the same lexer/parser/interpreter/server path as the existing server integration tests.
3. Test program:
   - top-level `let mut state = 0`
   - `schedule every 1 seconds { state = state + 1; fs.write("__SENTINEL__", "ran") }`
   - `@get("/read") fn read() -> Json { return { state } }`
   - existing `/ping` route for readiness
4. Generate a unique sentinel path under `std::env::temp_dir()` and replace `__SENTINEL__` in the program source before booting the server.
5. After readiness, sleep long enough for at least one 1-second schedule tick, then assert the sentinel file exists. This proves the schedule actually ran, avoiding a vacuous pass where handlers see `state == 0` because the schedule never fired.
6. Issue several `/read` requests and deserialize each response with `serde_json::Value`; assert `body["state"] == serde_json::json!(0)`.
7. Keep the assertion about handler visibility, not exact schedule tick counts. The test should fail only if handler forks start sharing background runtime state.
8. Document that the existing `spawn_test_server()` path leaves `Interpreter::defer_host_runtime` at its default `false`, so schedules start during `interp.run()`. This differs from the production `main.rs` orchestration path but exercises the same background-runtime vs serving-fork semantics needed for this regression.

## Tests

- `cargo fmt`
- `cargo test --test server_concurrency schedule_mutations_do_not_leak_into_handler_forks`
- `cargo test --test server_concurrency`
- `cargo test`

## Rollback

Remove the added integration test and this plan file. No runtime behavior changes expected.
