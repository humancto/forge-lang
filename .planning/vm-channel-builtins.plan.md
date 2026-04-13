# Plan: VM Channel Builtins

## Goal

Port `channel()`, `send()`, `receive()`, `close()` builtins from interpreter to VM so channel-based concurrency works in `--vm` mode.

## Current State

- Interpreter: `channel()` creates `Value::Channel(Arc<ChannelInner>)` using `std::sync::mpsc`
- VM: Has `Spawn` opcode + `ObjKind::TaskHandle`, but no channel support at all
- No `ObjKind::Channel` variant in `src/vm/value.rs`
- No channel builtins in `src/vm/builtins.rs`

## Design

### Value Representation

Add `ObjKind::Channel(Arc<VmChannelInner>)` to the GC. The `Arc` ensures the channel survives even if one GC's copy is swept — spawned threads hold their own `Arc` reference via `SharedValue::Channel`.

```rust
pub struct VmChannelInner {
    pub sender: Mutex<Option<VmChannelSender>>,
    pub receiver: Mutex<Option<Receiver<SharedValue>>>,
}

pub enum VmChannelSender {
    Bounded(SyncSender<SharedValue>),
    Unbounded(Sender<SharedValue>),
}
```

**Key safety decision (from expert review):** The `Arc` inside `ObjKind::Channel` ensures the channel isn't destroyed when one GC sweeps its entry — other threads' `Arc` refs keep it alive. This is the same pattern used by `ObjKind::TaskHandle(Arc<...>)`.

### Builtins

1. **`channel(capacity?)`** — if Int arg, create bounded `sync_channel(n)`; no args creates unbounded `channel()`. Alloc into GC as `ObjKind::Channel(Arc::new(...))`.
2. **`send(ch, value)`** — extract `Arc<VmChannelInner>`, convert value via `value_to_shared()`, lock sender mutex (handle `PoisonError`), send. Return Null on success, Err on closed.
3. **`receive(ch)`** — lock receiver mutex, `recv()`. Convert `SharedValue` back via `shared_to_value()`. Return Null if channel closed.
4. **`close(ch)`** — lock sender mutex, set to `None`. Existing buffered values remain receivable.

### SharedValue conversion

Add `SharedValue::Channel(Arc<VmChannelInner>)` so channels survive across spawn boundaries. Update `value_to_shared()` and `shared_to_value()` in `machine.rs`.

### GC handling

- `trace()`: explicit match arm for `ObjKind::Channel` — no inner GcRefs to trace (channel contents are `SharedValue`, not `Value`)
- `sweep()`: dropping an `ObjKind::Channel` just decrements the `Arc` refcount — safe even if other threads hold refs
- `display()`: render as `"<channel>"`

### Out of scope (separate roadmap items)

- `try_send()`, `try_receive()`, `select()` — next roadmap item
- Channel iteration (`for msg in ch { }`) — requires compiler support
- `await_all()`, `await_timeout()` — separate roadmap item

### Files to touch

1. `src/vm/value.rs` — add `VmChannelInner`, `VmChannelSender`, `ObjKind::Channel`
2. `src/vm/builtins.rs` — register `channel`, `send`, `receive`, `close` builtins
3. `src/vm/machine.rs` — add `SharedValue::Channel` + conversion logic
4. `src/vm/gc.rs` — explicit Channel handling in trace/sweep/display

## Test Strategy (TDD — tests first)

1. `vm_channel_create` — `let ch = channel(1)` returns non-null value
2. `vm_channel_send_receive` — send(ch, 42), receive(ch) == 42
3. `vm_channel_unbounded` — `channel()` creates unbounded, send/recv works
4. `vm_channel_close_prevents_send` — after close(ch), send returns error
5. `vm_channel_receive_after_close` — buffered values still receivable after close
6. `vm_channel_cross_spawn` — channel passes data between spawned tasks

## Rollback

Revert changes to `value.rs`, `builtins.rs`, `machine.rs`, `gc.rs`.
