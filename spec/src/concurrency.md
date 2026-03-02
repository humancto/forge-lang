# Concurrency

This chapter defines Forge's concurrency primitives. Forge provides three mechanisms for concurrent execution: channels for message passing, `spawn` for task creation, and `async`/`await` (with natural syntax aliases `forge`/`hold`) for asynchronous functions.

## Overview

Forge's concurrency model is built on OS threads (via `std::thread`) with channels for communication:

| Mechanism   | Purpose                                         | Syntax                                 |
| ----------- | ----------------------------------------------- | -------------------------------------- |
| Channels    | Message passing between tasks                   | `channel()`, `send()`, `receive()`     |
| Spawn       | Create concurrent tasks                         | `spawn { body }`                       |
| Async/Await | Asynchronous function definition and invocation | `async fn` / `await`, `forge` / `hold` |

## Execution Model

Forge spawns concurrent tasks as OS threads. Each spawned task receives a clone of the current environment, enabling access to variables defined before the spawn point. Tasks do not share mutable state directly; communication should use channels.

The runtime uses `std::thread::spawn` for task creation and `std::sync::mpsc::sync_channel` for channels. This provides:

- True parallelism on multi-core systems.
- Thread-safe communication through bounded channels.
- Task handle values with condition-variable notification for `await`.

## Task Handles

When `spawn` is used as an expression, it returns a `TaskHandle` value. Task handles are opaque values that can be passed to `await` (or `hold`) to block until the spawned task completes and retrieve its return value.

```forge
let handle = spawn { return 42 }
let result = await handle    // 42
```

Task handles use an `Arc<(Mutex<Option<Value>>, Condvar)>` internally:

- The `Mutex<Option<Value>>` holds the task's return value (initially `None`).
- The `Condvar` is notified when the task writes its result.
- `await` blocks on the `Condvar` until the result is available, then extracts it.

## Error Isolation

Errors in spawned tasks do not crash the parent task. If a spawned task encounters a runtime error, the error is printed to stderr and the task's result is `null`:

```forge
spawn { let x = 1 / 0 }    // prints error to stderr, does not crash parent
say "still running"          // executes normally
```

## Subsections

The following subsections define each concurrency mechanism in detail:

- [Channels](concurrency/channels.md) — Message passing between tasks.
- [Spawn](concurrency/spawn.md) — Creating concurrent tasks.
- [Async Functions](concurrency/async.md) — Defining asynchronous functions.
- [Await](concurrency/await.md) — Waiting for async results.
