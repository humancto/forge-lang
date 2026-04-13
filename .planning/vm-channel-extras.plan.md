# Plan: VM Channel Extras

## Goal

Port `try_send()`, `try_receive()`, `select()` builtins from interpreter to VM for full channel API parity.

## Current State

- VM has `channel()`, `send()`, `receive()`, `close()` (PR #87)
- Interpreter has all 7 channel builtins
- `try_send`, `try_receive`, `select` missing from VM

## Design

### `try_send(ch, value)` -> Bool

Non-blocking send. Returns `true` on success, `false` if channel full/closed.

- Bounded: use `SyncSender::try_send()` — returns `TrySendError` on full/disconnected
- Unbounded: use `Sender::send()` (unbounded never blocks) — returns `false` only on disconnect

Matches interpreter: `Ok(()) => Ok(Value::Bool(true))`, `Err(_) => Ok(Value::Bool(false))`.

### `try_receive(ch)` -> Some(value) | None

Non-blocking receive. Returns `Some(value)` if available, `None` if empty/closed.

- Use `Receiver::try_recv()` — returns `TryRecvError` on empty/disconnected

VM difference: interpreter returns `Value::Some(Box::new(val))` / `Value::None`. VM has `ObjKind::ResultOk(v)` / `Value::Null`. Need to check interpreter's `Some`/`None` mapping.

Actually looking at interpreter: `Value::Some(Box::new(val))` and `Value::None`. The VM doesn't have `Value::Some`/`Value::None` — these are interpreter-only types. In VM, we need to match equivalent semantics. The natural VM mapping:

- Success: wrap in `ObjKind::ResultOk(value)` (maps to `Some`)
- Empty/closed: `Value::Null` (maps to `None`)

This matches how `is_some()`/`is_none()` work in the VM — `is_some` checks `ResultOk`, `is_none` checks Null.

### `select(channels, timeout_ms?)` -> [index, value] | null

Poll multiple channels, return first available message with its channel index.

- Takes array of channels + optional timeout in ms
- Spin-loop with `try_recv()`, round-robin offset to avoid starvation
- Returns `[channel_index, received_value]` on success
- Returns `null` on timeout or all channels closed
- Sleeps 1ms between polls (matches interpreter)

### Files to touch

1. `src/vm/builtins.rs` — add `try_send`, `try_receive`, `select` handlers
2. `src/vm/machine.rs` — register the 3 builtins
3. `src/vm/async_tests.rs` — TDD tests

## Test Strategy (TDD — tests first)

1. `vm_try_send_success` — try_send to buffered channel returns true
2. `vm_try_send_full` — try_send to full bounded channel returns false
3. `vm_try_send_closed` — try_send to closed channel returns false
4. `vm_try_receive_available` — try_receive with pending message returns value
5. `vm_try_receive_empty` — try_receive on empty channel returns null
6. `vm_select_single_channel` — select with one channel returns [0, value]
7. `vm_select_multiple_channels` — select picks the ready channel
8. `vm_select_timeout` — select with timeout returns null when no messages
9. `vm_select_all_closed` — select returns null when all channels closed

## Rollback

Revert changes to `builtins.rs`, `machine.rs`, `async_tests.rs`.
