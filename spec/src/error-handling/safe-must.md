# Safe and Must

`safe` and `must` provide two ends of the error handling spectrum: `safe` suppresses errors silently, while `must` crashes on error with a clear message.

## Safe Blocks

### Syntax

```
SafeBlock = "safe" "{" Statement* "}"
```

The `safe` keyword introduces a block whose errors are silently suppressed.

### Semantics

A `safe` block executes its body statements. If any statement raises a runtime error, the error is caught and the block evaluates to `null`. If the block completes successfully, its result is returned normally.

```forge
safe {
    let data = json.parse("invalid json")
    say data
}
// No error — block silently returns null
```

### Behavior

| Block Outcome               | Result                                      |
| --------------------------- | ------------------------------------------- |
| Body completes successfully | Signal passes through (value, return, etc.) |
| Body raises a runtime error | `null` (error suppressed)                   |

The `safe` block is a **statement**, not an expression. It does not produce a value that can be assigned directly. When used for its side effects, it simply prevents errors from propagating:

```forge
// Attempt to write a file; ignore errors
safe {
    fs.write("log.txt", "entry")
}

// Execution continues regardless
say "done"
```

### Use Cases

- Optional side effects (logging, caching) where failure is acceptable.
- Defensive code around external operations (file I/O, network) that may fail intermittently.
- Quick prototyping where error handling is deferred.

### Caution

`safe` blocks suppress **all** errors indiscriminately, including programming bugs, type errors, and logic errors. Overuse of `safe` can hide real problems. Prefer explicit error handling with `Result`/`?` for production code.

## Must Expression

### Syntax

```
MustExpr = "must" Expression
```

The `must` keyword is a prefix operator applied to an expression.

### Semantics

`must` evaluates its operand and asserts that the result is a successful value:

- If the value is `Ok(v)`, returns `v` (unwrapped).
- If the value is `Err(e)`, raises a runtime error: `"must failed: <e>"`.
- If the value is `null`, raises a runtime error: `"must failed: got null"`.
- For any other value, returns it unchanged.

```forge
// Succeeds — unwraps Ok
let value = must Ok(42)       // 42

// Crashes — Err inside must
let value = must Err("oops")  // runtime error: must failed: oops

// Crashes — null inside must
let value = must null          // runtime error: must failed: got null

// Passes through — non-Result, non-null
let value = must 42            // 42
```

### Comparison with unwrap

| Function    | On `Ok(v)` | On `Err(e)` | On `null`        | On other     |
| ----------- | ---------- | ----------- | ---------------- | ------------ |
| `unwrap(r)` | `v`        | Error       | Error (for None) | Error        |
| `must expr` | `v`        | Error       | Error            | Pass through |

The key difference: `must` passes through non-Result, non-null values unchanged, while `unwrap` requires a Result or Option value. `must` is designed for contexts where the expression might return a Result, a plain value, or null.

### Use Cases

- Asserting that a critical operation succeeds:

  ```forge
  let db = must db.open("app.db")
  ```

- Unwrapping configuration that should always be present:

  ```forge
  let key = must env.get("API_KEY")
  ```

- Failing fast on unexpected null values:
  ```forge
  let user = must find_user(id)
  ```

## Combining Safe and Must

`safe` and `must` can be used together for fallback patterns:

```forge
// Try the primary source, fall back to default
safe {
    let config = must load_config("primary.json")
    apply_config(config)
}
// If must fails, safe catches it and continues

apply_config(default_config())
```

However, this pattern is generally better expressed with Result types:

```forge
let config = unwrap_or(load_config("primary.json"), default_config())
apply_config(config)
```
