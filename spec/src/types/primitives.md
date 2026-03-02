# Primitive Types

Forge has five primitive types. Primitive values are immutable and compared by value.

## Int

The `Int` type represents a **64-bit signed integer**. Its range is -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807 (i.e., the range of a Rust `i64`).

```forge
let x = 42
let y = -7
let z = 0
say typeof(x)  // Int
```

Integer arithmetic follows standard rules. Division between two integers produces an integer result (truncating toward zero):

```forge
say 7 / 2    // 3
say -7 / 2   // -3
```

Integer overflow behavior is implementation-defined. The interpreter wraps on overflow (Rust's default for `i64` in release mode).

## Float

The `Float` type represents a **64-bit IEEE 754 double-precision floating-point number**. This provides approximately 15-17 significant decimal digits of precision.

```forge
let pi = 3.14159
let temp = -0.5
let one = 1.0
say typeof(pi)  // Float
```

When an arithmetic operation involves both an `Int` and a `Float`, the integer is implicitly promoted to a float, and the result is a `Float`:

```forge
say 5 + 2.0    // 7.0
say 10 / 3.0   // 3.3333333333333335
```

Special float values (`NaN`, `Infinity`, `-Infinity`) may arise from operations like division by zero on floats, but there is no literal syntax for these values.

## String

The `String` type represents an immutable sequence of **UTF-8 encoded characters**. Strings are created using double-quoted literals and support interpolation and escape sequences (see [Literals](../lexical-structure/literals.md)).

```forge
let greeting = "Hello, World!"
let empty = ""
let multiline = """This is
a raw string."""
say typeof(greeting)  // String
```

### Key Properties

- **Immutable.** String operations always return new strings; the original is never modified.
- **UTF-8.** All strings are valid UTF-8. The `len()` function returns the number of bytes, not Unicode code points.
- **Interpolation.** Double-quoted strings support `{expr}` interpolation. Raw strings (`"""..."""`) do not.
- **Concatenation.** The `+` operator concatenates two strings.

```forge
let name = "Forge"
let version = 3
say "Welcome to {name} v{version}!"  // Welcome to Forge v3!
say "Hello" + ", " + "World!"         // Hello, World!
```

### String Comparison

Strings are compared lexicographically (byte-by-byte) using the standard comparison operators:

```forge
say "apple" < "banana"   // true
say "hello" == "hello"   // true
```

## Bool

The `Bool` type has exactly two values: `true` and `false`.

```forge
let active = true
let deleted = false
say typeof(active)  // Bool
```

Boolean values result from comparison operators (`==`, `!=`, `<`, `>`, `<=`, `>=`) and logical operators (`&&`, `||`, `!`). They are the natural type for `if` conditions, `while` conditions, and other control flow predicates.

### Logical Operations

| Expression        | Result  |
| ----------------- | ------- |
| `true && true`    | `true`  |
| `true && false`   | `false` |
| `false \|\| true` | `true`  |
| `!true`           | `false` |

Both `&&` and `||` use short-circuit evaluation.

## Null

The `Null` type has exactly one value: `null`. It represents the absence of a meaningful value.

```forge
let nothing = null
say typeof(nothing)  // Null
```

`null` is returned by functions that have no explicit `return` statement. It is falsy in boolean contexts.

### Null vs. None

Forge distinguishes between `null` and `None`:

- `null` is a bare value representing "no value." It is a primitive.
- `None` is a variant of the `Option` type representing "intentionally absent." It is a wrapper.

In practice, `null` appears in dynamic code and untyped contexts, while `None` is used in the `Option`/`Result` error-handling pattern. See [Option and Result](./option-result.md) for details.

## Type Comparison Summary

| Type     | Size     | Default Value | Falsy Values | Mutable |
| -------- | -------- | ------------- | ------------ | ------- |
| `Int`    | 64 bits  | —             | `0`          | No      |
| `Float`  | 64 bits  | —             | `0.0`        | No      |
| `String` | Variable | —             | `""`         | No      |
| `Bool`   | 1 bit    | —             | `false`      | No      |
| `Null`   | 0 bits   | `null`        | `null`       | No      |

All primitive types are compared by value. Two integers with the same numeric value are equal regardless of how they were computed.
