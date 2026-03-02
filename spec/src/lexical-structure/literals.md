# Literals

A literal is a notation for representing a fixed value in source code. Forge supports integer, float, string, raw string, boolean, null, array, and object literals.

## Integer Literals

> _IntLiteral_ → _Digit_+
>
> _Digit_ → `0`-`9`

An integer literal is a sequence of one or more decimal digits. Integer literals produce values of type `Int` (64-bit signed integer).

```forge
0
42
1000000
```

Negative integer values are expressed using the unary negation operator:

```forge
let x = -42
```

Integer literals do not support underscores as digit separators, hexadecimal, octal, or binary notation in the current version.

## Float Literals

> _FloatLiteral_ → _Digit_+ `.` _Digit_+

A float literal contains a decimal point with digits on both sides. Float literals produce values of type `Float` (64-bit IEEE 754 double-precision).

```forge
3.14
0.5
100.0
```

A leading dot (`.5`) or trailing dot (`5.`) is not valid. Both sides of the decimal point must have at least one digit.

Negative float values use unary negation:

```forge
let temp = -0.5
```

Scientific notation (e.g., `1.5e10`) is not supported in the current version.

## String Literals

> _StringLiteral_ → `"` _StringContent_\* `"`
>
> _StringContent_ → _Character_ | _EscapeSequence_ | _Interpolation_
>
> _EscapeSequence_ → `\n` | `\t` | `\\` | `\"`
>
> _Interpolation_ → `{` _Expression_ `}`

A string literal is a sequence of characters enclosed in double quotes. String literals produce values of type `String` (UTF-8 encoded, immutable).

```forge
"hello, world"
"line one\nline two"
"she said \"hi\""
```

### Escape Sequences

The following escape sequences are recognized within string literals:

| Sequence | Character               |
| -------- | ----------------------- |
| `\n`     | Newline (U+000A)        |
| `\t`     | Horizontal tab (U+0009) |
| `\\`     | Backslash (U+005C)      |
| `\"`     | Double quote (U+0022)   |

### String Interpolation

Curly braces within a string literal delimit an _interpolation expression_. The expression is evaluated at runtime, converted to a string, and inserted at that position:

```forge
let name = "Forge"
let version = 3
say "Welcome to {name} v{version}!"
// Output: Welcome to Forge v3!
```

Interpolation supports arbitrary expressions, not just variable names:

```forge
let x = 7
say "Seven squared is {x * x}"
say "Length: {len("hello")}"
say "Upper: {name}"
```

To include a literal `{` in a string without triggering interpolation, there is no dedicated escape sequence in the current version. Use string concatenation or a variable if needed.

## Raw String Literals

> _RawStringLiteral_ → `"""` _RawContent_\* `"""`

A raw string literal is delimited by triple double quotes. Raw strings preserve their content exactly as written: no escape sequences are processed and no interpolation occurs.

```forge
let sql = """SELECT * FROM users WHERE active = true"""

let html = """<div class="container">
    <h1>Hello</h1>
</div>"""
```

Raw strings may span multiple lines. They are particularly useful for SQL queries, regular expressions, and embedded markup.

## Boolean Literals

> _BoolLiteral_ → `true` | `false`

The boolean literals `true` and `false` produce values of type `Bool`. They are keyword tokens.

```forge
let active = true
let deleted = false
```

## Null Literal

> _NullLiteral_ → `null`

The `null` literal represents the absence of a value. It produces a value of type `Null`. It is a keyword token.

```forge
let nothing = null
say typeof(nothing)  // Null
```

## Array Literals

> _ArrayLiteral_ → `[` ( _Expression_ ( `,` _Expression_ )\* `,`? )? `]`

An array literal is a comma-separated list of expressions enclosed in square brackets. Arrays are ordered, heterogeneous (elements may have different types), and 0-indexed.

```forge
let empty = []
let nums = [1, 2, 3]
let mixed = [1, "two", true, null]
let nested = [[1, 2], [3, 4]]
```

A trailing comma after the last element is permitted.

## Object Literals

> _ObjectLiteral_ → `{` ( _Field_ ( `,` _Field_ )\* `,`? )? `}`
>
> _Field_ → _Identifier_ `:` _Expression_

An object literal is a comma-separated list of key-value pairs enclosed in curly braces. Keys are identifiers (unquoted). Objects maintain insertion order.

```forge
let empty = {}
let user = { name: "Alice", age: 30 }
let config = {
    host: "localhost",
    port: 8080,
    debug: false,
}
```

Object keys are strings at runtime, even though they appear as bare identifiers in the literal syntax. A trailing comma after the last field is permitted.

### Shorthand Field Syntax

When a variable name matches the desired key name, the value may be omitted:

```forge
let name = "Alice"
let age = 30
let user = { name, age }
// Equivalent to: { name: "Alice", age: 30 }
```
