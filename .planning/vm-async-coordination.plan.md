# Plan: VM Async Coordination

## Goal

Port `await_all()` and `await_timeout()` builtins from interpreter to VM.

## Design

### `await_all(handles)` -> Array

- Takes array of task handles
- Iterates each handle, waits on condvar (same as Await opcode)
- Unwraps ResultOk, propagates ResultErr as VMError
- Non-task-handle values pass through unchanged
- Returns array of collected results

### `await_timeout(handle, timeout_ms)` -> value | null

- Takes single task handle + timeout in ms (int or float)
- Uses `cvar.wait_timeout(guard, Duration::from_millis(ms))`
- Returns null on timeout
- Unwraps ResultOk, propagates ResultErr

### Helper method

Extract `extract_task_handle()` similar to `extract_channel()` — returns `Arc<(Mutex<Option<SharedValue>>, Condvar)>`.

### Files

1. `src/vm/builtins.rs` — add `await_all`, `await_timeout` + helper
2. `src/vm/machine.rs` — register builtins
3. `src/vm/async_tests.rs` — TDD tests

## Tests

1. `vm_await_all_basic` — await_all with multiple spawn handles
2. `vm_await_all_mixed` — array with task handles and plain values
3. `vm_await_timeout_completes` — task finishes within timeout
4. `vm_await_timeout_expires` — task takes too long, returns null

## Rollback

Revert builtins.rs, machine.rs, async_tests.rs.
