# Spawn

The `spawn` keyword creates a concurrent task that runs in a separate OS thread. It can be used as a statement (fire-and-forget) or as an expression (returning a task handle).

## Syntax

```
SpawnStmt = "spawn" Block
SpawnExpr = "spawn" Block
```

When `spawn` appears as a statement, the result is discarded. When it appears as an expression (e.g., assigned to a variable), it returns a `TaskHandle`.

## Statement Form (Fire-and-Forget)

When `spawn` is used as a statement, the block is executed concurrently and its result is discarded:

```forge
spawn {
    say "running in background"
}
say "continues immediately"
```

The parent does not wait for the spawned task to complete. The spawned block runs independently.

## Expression Form (Task Handle)

When `spawn` is used as an expression, it returns a `TaskHandle` that can be awaited:

```forge
let handle = spawn {
    return 42
}

let result = await handle    // 42
```

The task handle is an opaque value that represents the running task. Its type name is `"TaskHandle"`.

## Execution Model

When `spawn` is executed:

1. The block's statements are cloned.
2. A new `Interpreter` is created and its environment is cloned from the parent.
3. A shared result slot is created: `Arc<(Mutex<Option<Value>>, Condvar)>`.
4. A new OS thread is spawned via `std::thread::spawn`.
5. The thread executes the block. When it completes:
   - `Signal::Return(v)` or `Signal::ImplicitReturn(v)` stores `v` in the result slot.
   - `Signal::None` or other signals store `null`.
   - Errors print to stderr and store `null`.
6. The `Condvar` is notified, unblocking any `await` on the handle.
7. The `TaskHandle` value is returned to the parent.

## Environment Cloning

The spawned task receives a clone of the parent's environment at the point of the `spawn` call. This means:

- Variables defined before `spawn` are accessible in the spawned block.
- Modifications to the environment inside the spawned block do not affect the parent.
- Modifications in the parent after `spawn` do not affect the spawned block.

```forge
let x = 10
spawn {
    say x        // 10 — sees parent's x
    let x = 20   // shadows, does not affect parent
}
say x            // 10 — parent's x unchanged
```

## Return Values

The spawned block may use `return` to provide a result. This value is stored in the task handle's result slot:

```forge
let h = spawn {
    return "hello from spawn"
}
let msg = await h    // "hello from spawn"
```

If no `return` is used, the result is the last expression value (implicit return) or `null`:

```forge
let h = spawn {
    1 + 1
}
let result = await h    // may be 2 or null depending on block signal
```

## Error Isolation

Errors in spawned tasks are isolated from the parent. A runtime error in the spawned block:

1. Prints the error message to stderr: `spawn error: <message>`.
2. Stores `null` in the result slot.
3. Does **not** crash or affect the parent task.

```forge
spawn {
    let x = 1 / 0    // error: division by zero
}
// Parent continues normally
say "still running"
```

When the handle is awaited, the result is `null`:

```forge
let h = spawn {
    return 1 / 0
}
let result = await h    // null (error was caught internally)
```

## Multiple Spawns

Multiple tasks can be spawned and awaited:

```forge
let a = spawn { return 10 }
let b = spawn { return 20 }

let va = await a    // 10
let vb = await b    // 20
say va + vb         // 30
```

Tasks run concurrently in separate threads. The order of completion is non-deterministic.

## Spawn with Channels

Spawn and channels work together for structured concurrency:

```forge
let ch = channel()

spawn {
    let result = expensive_computation()
    send(ch, result)
}

// Do other work...

let result = receive(ch)    // blocks until computation completes
```
