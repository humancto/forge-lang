# Error Handling

This chapter defines Forge's error handling mechanisms. Forge uses a multi-layered approach: Result types for explicit error values, the `?` operator for propagation, `safe` blocks for error suppression, `must` for crash-on-error semantics, and `check` for declarative validation.

## Overview

Forge provides five complementary error handling mechanisms:

| Mechanism     | Purpose                             | Behavior on Error                       |
| ------------- | ----------------------------------- | --------------------------------------- |
| `Result` type | Represent success/failure as values | Carries error as data                   |
| `?` operator  | Propagate errors up the call stack  | Returns `Err` from enclosing function   |
| `safe { }`    | Suppress errors silently            | Returns `null`                          |
| `must expr`   | Assert success or crash             | Raises a runtime error                  |
| `check expr`  | Declarative validation              | Raises a runtime error with description |

These mechanisms serve different use cases:

- **Result + ?** — For functions that can fail and callers that want to handle failures explicitly.
- **safe** — For optional operations where failure is acceptable and the value can be `null`.
- **must** — For operations that should never fail in correct code.
- **check** — For input validation with readable error messages.

## Runtime Errors

All Forge runtime errors are represented by `RuntimeError`, which contains a `message` string and an optional `propagated` value. When an error is not caught, it terminates the program with an error message.

Errors can be caught with `try`/`catch`:

```forge
try {
    let x = 1 / 0
} catch e {
    println(e.message)    // "division by zero"
    println(e.type)       // "ArithmeticError"
    println(e.code)       // "E0008"
    println(e.hint)       // "check that the divisor is not zero before dividing"
}
```

The catch variable receives an object with four fields, in this order:

- `message` — the error message without its hint line (usually one line; an assertion's `left:`/`right:` lines are kept).
- `type` — a legacy classification (table below). Prefer `code`.
- `code` — the stable error code (`E0008`), as `forge explain` lists it.
- `hint` — the suggestion the error carries (`"available fields: a, b"`, `"did you mean 'count'?"`), or `null` when it has none.

The `type` is derived from the message's first line:

| Error Type        | Triggered By                                |
| ----------------- | ------------------------------------------- |
| `TimeoutError`    | An HTTP request timed out (`E0036`)         |
| `ConnectionError` | An HTTP connection failed (`E0037`)         |
| `TypeError`       | Message contains "type" or "Type"           |
| `ArithmeticError` | Message contains "division by zero"         |
| `AssertionError`  | Message contains "assertion"                |
| `IndexError`      | Message contains "index" or "out of bounds" |
| `ReferenceError`  | Message contains "not found" or "undefined" |
| `RuntimeError`    | All other errors                            |

## Subsections

The following subsections define each error handling mechanism in detail:

- [Result Type](error-handling/result.md) — Ok/Err values and inspection functions.
- [Propagation](error-handling/propagation.md) — The `?` operator.
- [Safe and Must](error-handling/safe-must.md) — Error suppression and crash-on-error.
- [Check](error-handling/check.md) — Declarative validation.
