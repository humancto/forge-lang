# Await

The `await` keyword (or its natural alias `hold`) suspends the current execution until an asynchronous operation completes, then returns the result. It is primarily used with task handles from `spawn`.

## Syntax

```
AwaitExpr = ("await" | "hold") Expression
```

## Semantics

The `await` expression evaluates its operand and inspects the result:

### TaskHandle

If the operand is a `TaskHandle` (returned by `spawn`), `await` blocks the current thread until the spawned task completes, then returns the task's result value.

```forge
let h = spawn { return 42 }
let result = await h    // blocks until task completes, returns 42
```

The blocking mechanism uses a condition variable:

1. Lock the `Mutex<Option<Value>>` inside the task handle.
2. While the value is `None`, wait on the `Condvar`.
3. When notified (the spawned task stored its result), extract the value.
4. Return the extracted value, or `null` if the slot was empty.

### Non-Handle Values (Pass-Through)

If the operand is not a `TaskHandle`, `await` returns the value unchanged. This provides backward compatibility and allows `await` to be used uniformly:

```forge
await 42         // 42
await "hello"    // "hello"
await null       // null
await Ok(10)     // Ok(10)
```

This pass-through behavior means `await` is always safe to call, even on values that are not async results.

## Natural Syntax: hold

The `hold` keyword is the natural-syntax alias for `await`:

```forge
let h = spawn { return "data" }
let result = hold h    // "data"
```

`hold` and `await` are parsed to the same `Expr::Await` AST node and behave identically.

## Awaiting Multiple Tasks

Multiple task handles can be awaited sequentially:

```forge
let a = spawn { return 10 }
let b = spawn { return 20 }
let c = spawn { return 30 }

let va = await a    // 10
let vb = await b    // 20
let vc = await c    // 30

say va + vb + vc    // 60
```

Each `await` blocks until its specific task completes. Tasks run concurrently, so the total time is approximately the duration of the slowest task, not the sum.

## Awaiting Errored Tasks

If a spawned task encountered a runtime error, its result slot contains `null`. Awaiting such a handle returns `null`:

```forge
let h = spawn {
    return 1 / 0    // runtime error
}

let result = await h    // null
```

The error is printed to stderr by the spawned task. The parent receives `null` and must check for it if error detection is needed.

## Await in Functions

`await` can be used inside regular and async functions:

```forge
fn parallel_sum(a, b) {
    let ha = spawn { return a * a }
    let hb = spawn { return b * b }
    return await ha + await hb
}

parallel_sum(3, 4)    // 25
```

## Error Handling

The `await` expression can raise runtime errors in two cases:

- `"await: task handle lock poisoned"` — The `Mutex` guarding the result slot was poisoned (the spawned thread panicked while holding the lock).
- `"await: condvar wait failed"` — The condition variable wait failed.

Both are exceptional conditions that indicate a serious runtime problem.

## Comparison with hold

| Syntax  | Keyword      | Parsing             | Behavior           |
| ------- | ------------ | ------------------- | ------------------ |
| Classic | `await expr` | `Expr::Await(expr)` | Block until result |
| Natural | `hold expr`  | `Expr::Await(expr)` | Block until result |

There is no semantic difference. Use whichever style matches your code's convention.
