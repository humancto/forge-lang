# RFC 0004: Errors as Values

- **Status:** Implemented
- **Author:** Archith Rapaka
- **Date:** 2026-02-01

## Summary

Forge uses `Result<T, E>` types for error handling instead of exceptions. Functions that can fail return `Ok(value)` or `Err(message)`. The `?` operator propagates errors. The `must` keyword asserts success. There is no `throw`, no stack unwinding, and no invisible control flow.

## Motivation

Exception-based error handling has three fundamental problems:

1. **Invisible control flow.** Any function call might throw. You can't tell by reading the code which lines might jump to a catch block.

2. **Forgotten errors.** In Python, JavaScript, and Java, it's easy to forget to wrap code in try/catch. The program crashes at runtime with an unhandled exception.

3. **Performance cost.** Exception handling requires stack unwinding machinery that has runtime overhead even when no exceptions are thrown.

Rust and Go demonstrated that explicit error handling produces more reliable software. Forge follows this path.

## Design

### Result Types

```
fn parse_age(input) {
    let n = int(input)
    if n < 0 { return Err("age cannot be negative") }
    if n > 150 { return Err("unrealistic age") }
    return Ok(n)
}
```

### Inspection

```
let result = parse_age("25")
say is_ok(result)           // true
say unwrap(result)           // 25
say unwrap_or(result, 0)     // 25

let bad = parse_age("-5")
say is_err(bad)              // true
say unwrap_or(bad, 0)        // 0
```

### The ? Operator

The `?` operator propagates errors up the call stack automatically:

```
fn process(input) {
    let age = parse_age(input)?       // returns Err early if parse fails
    return Ok("Age is {age}")
}
```

This replaces verbose `if is_err(result) { return result }` chains.

### The must Keyword

When failure is unexpected and should crash the program:

```
let config = must load_config("app.toml")
```

`must` unwraps the Ok value or terminates with a clear error message. It signals to the reader: "I expect this to succeed."

### try/catch Blocks

For cases where you want to handle errors from a block of statements:

```
try {
    let data = fs.read("config.json")
    let config = json.parse(data)
} catch err {
    say "Failed: {err}"
}
```

### safe Blocks

For cases where you want to ignore errors entirely:

```
safe {
    let data = fs.read("maybe-missing.txt")
    say data
}
// Execution continues even if the file doesn't exist
```

## Alternatives Considered

### "Use try/catch like JavaScript"

Partially adopted. Forge has `try/catch` as an escape hatch, but the primary mechanism is `Result` types with `?`. This makes error paths explicit by default.

### "Use Go-style if err != nil"

Rejected. Go's approach is correct in principle but verbose in practice. The `?` operator achieves the same explicitness with less boilerplate.

### "Use Rust's exact Result<T, E> with generics"

Partially adopted. Forge's Result types don't require generic annotations — they work dynamically. This trades some type safety for simplicity, consistent with Forge's gradual typing philosophy.
