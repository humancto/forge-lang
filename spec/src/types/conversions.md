# Type Conversions

Forge is dynamically typed and does not perform implicit type coercion between incompatible types (with the exception of `Int` to `Float` promotion in mixed arithmetic). Explicit conversion functions are provided for converting between types.

## Conversion Functions

### `str(value)` — Convert to String

Converts any value to its string representation.

```forge
say str(42)       // "42"
say str(3.14)     // "3.14"
say str(true)     // "true"
say str(null)     // "null"
say str([1, 2])   // "[1, 2]"
```

`str()` never fails. Every Forge value has a string representation.

### `int(value)` — Convert to Int

Converts a value to a 64-bit signed integer.

```forge
say int("42")     // 42
say int("100")    // 100
say int(3.14)     // 3 (truncates toward zero)
say int(true)     // 1
say int(false)    // 0
```

If the input string cannot be parsed as an integer, `int()` produces a runtime error. Always validate user input before converting.

| Input Type | Behavior                                 |
| ---------- | ---------------------------------------- |
| `String`   | Parses decimal integer; error if invalid |
| `Float`    | Truncates toward zero                    |
| `Bool`     | `true` = 1, `false` = 0                  |
| `Int`      | Returns the value unchanged              |
| Other      | Runtime error                            |

### `float(value)` — Convert to Float

Converts a value to a 64-bit floating-point number.

```forge
say float("3.14")  // 3.14
say float(42)      // 42.0
say float(true)    // 1.0
say float(false)   // 0.0
```

If the input string cannot be parsed as a float, `float()` produces a runtime error.

| Input Type | Behavior                                     |
| ---------- | -------------------------------------------- |
| `String`   | Parses decimal float; error if invalid       |
| `Int`      | Promotes to float (lossless for most values) |
| `Bool`     | `true` = 1.0, `false` = 0.0                  |
| `Float`    | Returns the value unchanged                  |
| Other      | Runtime error                                |

## Type Inspection Functions

### `typeof(value)` — Get Type Name

Returns a string describing the runtime type of a value.

```forge
say typeof(42)                  // Int
say typeof(3.14)                // Float
say typeof("hello")             // String
say typeof(true)                // Bool
say typeof(null)                // Null
say typeof([1, 2, 3])           // Array
say typeof({ name: "Alice" })   // Object
```

For struct instances, `typeof()` returns the struct name:

```forge
thing Point { x: Int, y: Int }
let p = Point { x: 1, y: 2 }
say typeof(p)   // Point
```

For functions:

```forge
fn f() { return 1 }
say typeof(f)   // Function
```

### `type(value)` — Alias for `typeof`

The `type()` function is an alias for `typeof()`. Both return identical results:

```forge
let value = 3.14
if type(value) == "Float" {
    say "It's a float"
}
```

## Implicit Conversions

Forge performs very few implicit conversions:

### Int-to-Float Promotion

When an arithmetic operator has one `Int` operand and one `Float` operand, the integer is implicitly promoted to a float. The result is a `Float`:

```forge
say 5 + 2.0    // 7.0 (Int promoted to Float)
say 10 / 3.0   // 3.3333333333333335
```

### String Interpolation

Inside string interpolation (`{expr}`), the expression result is implicitly converted to a string using the same logic as `str()`:

```forge
let n = 42
say "The answer is {n}"  // "The answer is 42"
```

### Truthiness

When a value is used in a boolean context (e.g., `if` condition), it is evaluated for truthiness (see [Types](../types.md)). This is not a type conversion — the value itself is not changed. It is a contextual interpretation.

## No Other Implicit Coercion

Operations between incompatible types (e.g., adding a string and an integer) produce a runtime error. Explicit conversion is required:

```forge
// Error: cannot add String and Int
// say "age: " + 30

// Correct: convert explicitly
say "age: " + str(30)
```
