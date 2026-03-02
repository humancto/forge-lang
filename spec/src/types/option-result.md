# Option and Result

Forge provides two built-in wrapper types for representing optional values and fallible operations: **Option** and **Result**. These types enable explicit, composable error handling without exceptions.

## Option Type

The `Option` type represents a value that may or may not be present.

### Variants

| Variant       | Meaning             |
| ------------- | ------------------- |
| `Some(value)` | A value is present  |
| `None`        | No value is present |

### Construction

```forge
let x = Some(42)
let y = None
```

Both `Some` and `None` are globally available constructors.

### Inspection

| Function     | Description                          | Example                   |
| ------------ | ------------------------------------ | ------------------------- |
| `is_some(v)` | Returns `true` if `v` is `Some(...)` | `is_some(Some(1))` = true |
| `is_none(v)` | Returns `true` if `v` is `None`      | `is_none(None)` = true    |

```forge
let x = Some(42)
say is_some(x)  // true
say is_none(x)  // false

let y = None
say is_some(y)  // false
say is_none(y)  // true
```

### Pattern Matching

Option values are destructured with `match`:

```forge
let value = Some(42)

match value {
    Some(v) => say "Got: {v}"
    None => say "Nothing"
}
```

### Unwrapping

| Function          | Description                                |
| ----------------- | ------------------------------------------ |
| `unwrap(v)`       | Returns the inner value; crashes if `None` |
| `unwrap_or(v, d)` | Returns the inner value, or `d` if `None`  |

```forge
let x = Some(42)
say unwrap(x)          // 42
say unwrap_or(x, 0)    // 42

let y = None
say unwrap_or(y, 0)    // 0
// unwrap(y) would crash with an error
```

## Result Type

The `Result` type represents the outcome of an operation that may succeed or fail.

### Variants

| Variant        | Meaning                          |
| -------------- | -------------------------------- |
| `Ok(value)`    | Operation succeeded with `value` |
| `Err(message)` | Operation failed with `message`  |

### Construction

```forge
let success = Ok(42)
let failure = Err("something went wrong")
```

Result constructors accept both cases: `Ok(42)` and `ok(42)` are equivalent, as are `Err("msg")` and `err("msg")`.

### Inspection

| Function    | Description                         | Example                   |
| ----------- | ----------------------------------- | ------------------------- |
| `is_ok(v)`  | Returns `true` if `v` is `Ok(...)`  | `is_ok(Ok(1))` = true     |
| `is_err(v)` | Returns `true` if `v` is `Err(...)` | `is_err(Err("x"))` = true |

```forge
let result = Ok(42)
say is_ok(result)   // true
say is_err(result)  // false
```

### Pattern Matching

Result values are destructured with `match`:

```forge
fn parse_number(s) {
    let n = int(s)
    if n == null {
        return Err("invalid number: {s}")
    }
    return Ok(n)
}

match parse_number("42") {
    Ok(n) => say "Parsed: {n}"
    Err(msg) => say "Error: {msg}"
}
```

### Unwrapping

| Function          | Description                               |
| ----------------- | ----------------------------------------- |
| `unwrap(v)`       | Returns the inner value; crashes if `Err` |
| `unwrap_or(v, d)` | Returns the inner value, or `d` if `Err`  |

```forge
let result = Ok(42)
say unwrap(result)          // 42
say unwrap_or(result, 0)    // 42

let err = Err("failed")
say unwrap_or(err, 0)       // 0
```

## The `?` Operator

The `?` postfix operator provides concise error propagation. When applied to a `Result` value:

- If the value is `Ok(v)`, the `?` unwraps it to `v` and execution continues.
- If the value is `Err(e)`, the enclosing function immediately returns `Err(e)`.

```forge
fn read_config(path) {
    if !fs.exists(path) {
        return Err("config file not found")
    }
    return Ok(fs.read(path))
}

fn start_server() {
    let config = read_config("server.toml")?
    say "Starting with config: {config}"
    return Ok(true)
}

match start_server() {
    Ok(_) => say "Server started"
    Err(msg) => say "Failed: {msg}"
}
```

The `?` operator can only be used inside functions that return `Result`. It is syntactic sugar for:

```forge
let result = read_config("server.toml")
if is_err(result) {
    return result
}
let config = unwrap(result)
```

## The `must` Keyword

The `must` keyword is an assertion on Result values. It unwraps an `Ok` value or crashes the program with a clear error message on `Err`:

```forge
let config = must read_config("server.toml")
```

Use `must` for errors that are truly unrecoverable — situations where the program cannot meaningfully continue (e.g., missing configuration, failed database connection).

## The `safe` Block

The `safe` block catches any errors within its body and returns `null` instead of crashing:

```forge
safe {
    let result = risky_operation()
    say result
}
// If risky_operation() fails, execution continues here
```

`safe` is a statement-level construct. It does not return a value and cannot be used as an expression.

## Idiomatic Error Handling

The recommended patterns for error handling in Forge, in order of preference:

1. **`?` for propagation.** Pass errors up the call stack to a centralized handler.
2. **`match` for handling.** Explicitly handle both `Ok` and `Err` at the appropriate level.
3. **`unwrap_or` for defaults.** Provide a fallback when an error is acceptable.
4. **`must` for fatal errors.** Crash with a clear message when recovery is impossible.
5. **`safe` for silencing.** Suppress errors only when the operation is truly optional.
