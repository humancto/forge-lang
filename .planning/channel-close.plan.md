# Plan: 9A.2 — Channel close and iteration

## Goal

Add `close(ch)` to signal no more values, and support `for msg in ch { }` to drain a channel until closed.

## Current state

`ChannelInner` wraps `tx` and `rx` in `Mutex<Option<...>>`. No explicit close — channels only "close" when senders are dropped. `for...in` only supports arrays and objects. `close` is NOT a bare builtin — db/pg/mysql close functions are module-qualified (`db.close()`).

## Threading model

Forge's `spawn` creates OS threads (not green threads). The interpreter is single-threaded — `for msg in ch` blocks the calling thread while producers run on spawned OS threads. This is the intended usage pattern.

## Approach

### 1. Add `close(ch)` builtin

Set `tx` to `None` (drops the sender, causing receivers to get `Err` after buffer drains).

### 2. Add `Value::Channel` to `for...in` iteration

Loop calling `recv()` until it returns `Err` (closed + drained) or receiver is `None`. The MutexGuard on `rx` is held during `recv()` — this is acceptable since Forge's interpreter is single-threaded and producers are on OS threads.

### 3. Existing behaviors after close

- `send()` after `close()`: tx is None → "channel closed" error (already handled)
- `receive()` after `close()`: drains buffered values, then returns Null (already handled)
- `break` works inside `for...in` body (already supported by the For codepath)

### Files to touch

1. **`src/interpreter/builtins.rs`** — add `close` builtin
2. **`src/interpreter/mod.rs`** — register `close` in builtin list, add `Value::Channel` arm to `Stmt::For`

### Edge cases

- Close already-closed channel → no-op
- Close non-channel → error
- Close with no args → error
- Iterate empty closed channel → exits immediately
- `break` inside channel iteration → works normally

## Test strategy

- `close(ch)` then `receive(ch)` returns null
- `for msg in ch` collects all sent values then exits after close
- Close non-channel errors
- Close with no args errors
- `send()` after `close()` errors

## Rollback

Revert changes to builtins.rs and mod.rs.
