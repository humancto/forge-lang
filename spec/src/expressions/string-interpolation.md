# String Interpolation

Double-quoted string literals in Forge support **interpolation**: embedding arbitrary expressions inside `{` and `}` delimiters. The expressions are evaluated at runtime and their results are converted to string representations and spliced into the surrounding text.

## Syntax

```
" ... {expression} ... "
```

Any valid Forge expression may appear between the braces.

## Basic Usage

```forge
let name = "Forge"
say "Hello, {name}!"           // "Hello, Forge!"

let x = 10
let y = 20
say "{x} + {y} = {x + y}"     // "10 + 20 = 30"
```

## Arbitrary Expressions

The interpolated expression is not limited to identifiers. Any expression that produces a value is permitted, including function calls, arithmetic, method calls, and field access.

```forge
say "length: {len([1, 2, 3])}"             // "length: 3"
say "upper: {"hello".upper()}"             // "upper: HELLO"
say "sum: {1 + 2 + 3}"                     // "sum: 6"
say "type: {typeof(42)}"                    // "type: int"
```

## Value Conversion

Interpolated values are converted to their string representation using the same rules as the `str()` builtin:

| Value Type | String Representation                   |
| ---------- | --------------------------------------- |
| `string`   | The string itself                       |
| `int`      | Decimal representation (e.g., `"42"`)   |
| `float`    | Decimal representation (e.g., `"3.14"`) |
| `bool`     | `"true"` or `"false"`                   |
| `null`     | `"null"`                                |
| `array`    | `"[1, 2, 3]"`                           |
| `object`   | `"{key: value, ...}"`                   |

## Escape Sequences

The following escape sequences are recognized inside double-quoted strings:

| Escape | Character                              |
| ------ | -------------------------------------- |
| `\n`   | Newline                                |
| `\t`   | Tab                                    |
| `\\`   | Backslash                              |
| `\{`   | Literal `{` (suppresses interpolation) |
| `\"`   | Double quote                           |

To include a literal `{` character without triggering interpolation, escape it:

```forge
say "use \{braces\} for interpolation"
// Output: use {braces} for interpolation
```

## Nesting

Interpolated expressions may themselves contain string literals with interpolation, though this is discouraged for readability:

```forge
let items = ["a", "b", "c"]
say "result: {join(items, ", ")}"
```

## Non-Interpolated Strings

Single-quoted strings do not support interpolation. Use single quotes when the string contains braces that should be treated literally:

```forge
say 'no {interpolation} here'
// Output: no {interpolation} here
```

## Empty Interpolation

An empty interpolation `{}` is not valid and produces a parse error.

## Implementation Notes

String interpolation is parsed into a `StringInterp` AST node containing a sequence of `StringPart` elements, each being either a `Literal` (raw text) or an `Expr` (an evaluated expression). At runtime, all parts are evaluated and concatenated to produce the final string value.
