# Plan: 9A.3 — Unbounded channels

## Goal

`channel()` with no args creates an unbounded channel (using `mpsc::channel()`). `channel(n)` creates a bounded channel (using `mpsc::sync_channel(n)` as before).

## Current state

`ChannelInner` stores `SyncSender<Value>` and `Receiver<Value>`. Only `sync_channel` is used. Default capacity is 32.

## Approach

### 1. Add a sender enum

`SyncSender` and `Sender` are different types. Create an enum:

```rust
enum ChannelSender {
    Bounded(SyncSender<Value>),
    Unbounded(Sender<Value>),
}
```

Both implement `send(val)` with the same signature (returns `Result<(), SendError>`).

### 2. Update `ChannelInner`

```rust
pub struct ChannelInner {
    pub tx: Mutex<Option<ChannelSender>>,
    pub rx: Mutex<Option<Receiver<Value>>>,  // same type for both
    pub capacity: Option<usize>,  // None = unbounded
}
```

`Receiver<Value>` is the same type for both `channel()` and `sync_channel()`.

### 3. Update `channel()` builtin

- `channel()` → `mpsc::channel()`, wraps in `ChannelSender::Unbounded`
- `channel(n)` → `mpsc::sync_channel(n)`, wraps in `ChannelSender::Bounded`

### 4. Update `send()` and `try_send()` builtins

Match on the sender enum:

- `Bounded(tx)` → `tx.send(val)` / `tx.try_send(val)`
- `Unbounded(tx)` → `tx.send(val)` / `tx.send(val)` (unbounded send never blocks, so try_send = send)

### Files to touch

1. **`src/interpreter/mod.rs`** — update `ChannelInner`, add `ChannelSender` enum
2. **`src/interpreter/builtins.rs`** — update `channel`, `send`, `try_send`

### Edge cases

- `channel(0)` → rendezvous channel (sync_channel(0)), valid
- `channel()` → unbounded, send never blocks
- `try_send` on unbounded → always succeeds (unless closed)

## Test strategy

- `channel()` creates unbounded (no capacity)
- `channel(5)` creates bounded
- Send many values to unbounded without blocking
- Existing channel tests still pass

## Rollback

Revert changes to mod.rs and builtins.rs.
