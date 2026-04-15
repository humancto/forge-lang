# Plan: 9B.2 — Spawn with timeout

## Goal

Allow waiting for a spawned task with a deadline. If the task doesn't complete in time, return a timeout indicator.

## Approach — `await_timeout(handle, ms)` builtin

True OS thread cancellation isn't possible in safe Rust. Instead, add `await_timeout` that uses `Condvar::wait_timeout`:

```forge
let h = spawn { slow_computation() }
let result = await_timeout(h, 5000)  // 5 second timeout
// result = task value on success, or Null on timeout
```

### Implementation

1. Add `await_timeout(handle, ms)` builtin
2. Use `Condvar::wait_timeout` with the specified duration
3. Return the task value if completed, or `Null` if timed out
4. The underlying thread continues running (but its result is abandoned)

### Files to touch

1. **`src/interpreter/builtins.rs`** — add `await_timeout`
2. **`src/interpreter/mod.rs`** — register `await_timeout`

### Edge cases

- Timeout of 0 → check immediately, return Null if not done
- Non-TaskHandle → error
- Already-completed task → return immediately
- Negative timeout → treat as 0

## Test strategy

- `await_timeout` with task that completes before timeout returns value
- `await_timeout` with very short timeout returns Null (task still running)
- Non-handle errors

## Rollback

Revert builtins.rs and mod.rs.
