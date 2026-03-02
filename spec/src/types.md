# Types

This chapter describes the type system of the Forge programming language.

## Overview

Forge is **dynamically typed** at runtime: variables do not have fixed types, and any variable may hold a value of any type at any point during execution. However, Forge supports **optional type annotations** (gradual typing) on variable declarations, function parameters, and return types. These annotations serve as documentation and enable the optional type checker to detect certain errors before execution.

Every value in Forge belongs to exactly one of the following type categories:

| Category        | Types                                         |
| --------------- | --------------------------------------------- |
| Primitive       | `Int`, `Float`, `String`, `Bool`, `Null`      |
| Collection      | `Array`, `Object`                             |
| Struct          | User-defined via `struct` / `thing`           |
| Interface       | User-defined via `interface` / `power`        |
| Function        | Named functions, closures, lambdas            |
| Algebraic (ADT) | User-defined via `type Name = Variant \| ...` |
| Result          | `Ok(value)`, `Err(message)`                   |
| Option          | `Some(value)`, `None`                         |

## Type Annotations

Type annotations use a colon after the name, followed by the type:

```forge
let name: String = "Alice"
let age: Int = 30
let score: Float = 98.5
let active: Bool = true
```

Function parameters and return types may also be annotated:

```forge
fn add(a: Int, b: Int) -> Int {
    return a + b
}
```

When annotations are omitted, types are inferred from the assigned values. Annotations are always optional.

## Type Inspection at Runtime

The built-in `typeof()` function (aliased as `type()`) returns a string describing the runtime type of a value:

```forge
say typeof(42)                 // Int
say typeof(3.14)               // Float
say typeof("hello")            // String
say typeof(true)               // Bool
say typeof(null)               // Null
say typeof([1, 2, 3])          // Array
say typeof({ name: "Alice" })  // Object
```

For struct instances, `typeof()` returns the struct name (e.g., `"Person"`).

## Truthiness

When a value is used in a boolean context (such as an `if` condition), Forge evaluates it as "truthy" or "falsy" according to the following rules:

| Value               | Truthy? |
| ------------------- | ------- |
| `false`             | Falsy   |
| `null`              | Falsy   |
| `0` (integer zero)  | Falsy   |
| `0.0` (float zero)  | Falsy   |
| `""` (empty string) | Falsy   |
| `[]` (empty array)  | Falsy   |
| Everything else     | Truthy  |

## Subsections

The following subsections define each type category in detail:

- [Primitive Types](./types/primitives.md) — Int, Float, String, Bool, Null
- [Collection Types](./types/collections.md) — Array, Object
- [Struct Types](./types/structs.md) — `struct` / `thing` definitions
- [Interface Types](./types/interfaces.md) — `interface` / `power` contracts
- [Function Types](./types/functions.md) — functions as first-class values
- [Algebraic Data Types](./types/adt.md) — `type Name = Variant | ...`
- [Option and Result](./types/option-result.md) — `Option` and `Result` wrapper types
- [Type Conversions](./types/conversions.md) — `str()`, `int()`, `float()`, `type()`, `typeof()`
