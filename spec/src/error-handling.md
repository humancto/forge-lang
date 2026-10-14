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
- `type` — a category of the code (table below). Prefer `code`.
- `code` — the stable error code (`E0008`), as `forge explain` lists it.
- `hint` — the suggestion the error carries (`"available fields: a, b"`, `"did you mean 'count'?"`), or `null` when it has none.

The `type` is a category of the code, so every error with one code has the
same type:

| Error Type        | Codes                                                                    |
| ----------------- | ------------------------------------------------------------------------ |
| `TypeError`       | E0004 E0005 E0006 E0007 E0011 E0012 E0014 E0015 E0017 E0028 E0029 E0034 |
| `ReferenceError`  | E0003 E0010 E0013 E0019                                                  |
| `IndexError`      | E0009                                                                    |
| `ArithmeticError` | E0008                                                                    |
| `ValueError`      | E0038 (not a number), E0039 (invalid JSON), E0040 (invalid regex)        |
| `AssertionError`  | E0021 E0022                                                              |
| `TimeoutError`    | E0031 E0036                                                              |
| `ConnectionError` | E0037                                                                    |
| `RuntimeError`    | every other code (E0000, E0016, E0023, E0041, ...)                       |

## Subsections

The following subsections define each error handling mechanism in detail:

- [Result Type](error-handling/result.md) — Ok/Err values and inspection functions.
- [Propagation](error-handling/propagation.md) — The `?` operator.
- [Safe and Must](error-handling/safe-must.md) — Error suppression and crash-on-error.
- [Check](error-handling/check.md) — Declarative validation.
