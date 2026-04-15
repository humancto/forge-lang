# Plan: 9B.1 — Task groups

## Goal

Provide a way to wait for multiple spawned tasks and collect their results. Cancel remaining on first error.

## Current state

`spawn { }` returns a `TaskHandle(Arc<(Mutex<Option<Value>>, Condvar)>)`. `await handle` waits for a single task. No way to wait for multiple tasks at once.

## Approach — `await_all` builtin

Instead of new `task_group` syntax (which would require parser changes and implicit spawn tracking), implement `await_all(handles)` as a builtin:

```forge
let h1 = spawn { compute1() }
let h2 = spawn { compute2() }
let results = await_all([h1, h2])
// results = [result1, result2]
```

This covers the core use case without parser changes.

### Implementation

1. Add `await_all(handles)` builtin that takes an array of TaskHandles
2. Wait for each handle using Condvar (same pattern as existing `await`)
3. Collect results into an array
4. If any task returned an error (currently tasks eprintln errors and return Null), return the error

### Error handling

Currently `spawn_task` catches errors and prints them, returning Null. For `await_all`, we should propagate errors. Modify `spawn_task` to wrap errors in `Value::ResultErr` so callers can detect failures.

Actually, changing spawn_task's error handling is a bigger change. For now, keep simple: `await_all` just collects all results (including Null for errored tasks). The roadmap item 9B.3 will add proper Result wrapping.

### Files to touch

1. **`src/interpreter/builtins.rs`** — add `await_all` builtin
2. **`src/interpreter/mod.rs`** — register `await_all`

### Edge cases

- Empty array → return empty array
- Non-TaskHandle in array → error
- Single handle → same as await, but returns [result]
- Already-completed tasks → returns immediately

## Test strategy

- `await_all` with two tasks returns both results
- `await_all` with empty array returns []
- `await_all` with non-handle errors

## Rollback

Revert builtins.rs and mod.rs changes.
